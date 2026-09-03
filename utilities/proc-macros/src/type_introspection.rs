// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields};

pub fn derive(input: DeriveInput) -> TokenStream {
	let name = &input.ident;
	let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

	// Collect all field types to add HasTypeIntrospection bounds on them.
	let field_types = collect_field_types(&input.data);

	// Build the where clause: existing bounds + HasTypeIntrospection for each field type.
	let mut where_clause = where_clause.cloned().unwrap_or_else(|| syn::parse_quote!(where));
	for ty in &field_types {
		where_clause
			.predicates
			.push(syn::parse_quote!(#ty: cf_utilities::type_introspection::HasTypeIntrospection));
	}

	let is_empty_body = match &input.data {
		Data::Struct(data) => struct_body(&data.fields),
		Data::Enum(data) =>
			if data.variants.is_empty() {
				quote! { true }
			} else {
				let variant_checks: Vec<TokenStream> =
					data.variants.iter().map(|v| variant_is_empty(&v.fields)).collect();
				quote! { #( (#variant_checks) )&&* }
			},
		Data::Union(_) => {
			return syn::Error::new_spanned(
				name,
				"HasTypeIntrospection cannot be derived for unions",
			)
			.to_compile_error();
		},
	};
	let sample_all_shapes_body = match &input.data {
		Data::Struct(data) => struct_sample_all_shapes(&data.fields),
		Data::Enum(data) => enum_sample_all_shapes(data),
		Data::Union(_) => unreachable!(),
	};

	quote! {
		impl #impl_generics cf_utilities::type_introspection::HasTypeIntrospection for #name #ty_generics #where_clause {
			fn is_empty_type() -> bool {
				#is_empty_body
			}

			fn sample_all_shapes() -> sp_std::vec::Vec<Self> {
				#sample_all_shapes_body
			}
		}
	}
}

/// Collect all field types from the data structure (struct or enum).
fn collect_field_types(data: &Data) -> Vec<syn::Type> {
	let mut types = Vec::new();
	match data {
		Data::Struct(data) => {
			collect_from_fields(&data.fields, &mut types);
		},
		Data::Enum(data) =>
			for variant in &data.variants {
				collect_from_fields(&variant.fields, &mut types);
			},
		Data::Union(_) => {},
	}
	types
}

fn collect_from_fields(fields: &Fields, types: &mut Vec<syn::Type>) {
	match fields {
		Fields::Named(f) =>
			for field in &f.named {
				types.push(field.ty.clone());
			},
		Fields::Unnamed(f) =>
			for field in &f.unnamed {
				types.push(field.ty.clone());
			},
		Fields::Unit => {},
	}
}

/// A struct is empty if ANY field's type is empty.
fn struct_body(fields: &Fields) -> TokenStream {
	field_types_any_empty(fields)
}

/// A variant is empty if ANY of its field types is empty.
/// A unit variant (no fields) is never empty — it's always constructible.
fn variant_is_empty(fields: &Fields) -> TokenStream {
	field_types_any_empty(fields)
}

/// Returns an expression that is `true` if any field type is empty.
/// For no fields (unit struct/variant), returns `false`.
fn field_types_any_empty(fields: &Fields) -> TokenStream {
	let field_types: Vec<_> = match fields {
		Fields::Named(f) => f.named.iter().map(|f| &f.ty).collect(),
		Fields::Unnamed(f) => f.unnamed.iter().map(|f| &f.ty).collect(),
		Fields::Unit => return quote! { false },
	};

	if field_types.is_empty() {
		return quote! { false };
	}

	quote! {
		#( <#field_types as cf_utilities::type_introspection::HasTypeIntrospection>::is_empty_type() )||*
	}
}

fn enum_sample_all_shapes(data: &syn::DataEnum) -> TokenStream {
	let variant_samples = data.variants.iter().map(|variant| {
		let ident = &variant.ident;
		let constructor = construct_value(&variant.fields, quote! { Self::#ident });
		append_field_samples(&variant.fields, constructor)
	});

	quote! {
		let mut __samples: sp_std::vec::Vec<Self> = Default::default();
		#( #variant_samples )*
		__samples
	}
}

fn struct_sample_all_shapes(fields: &Fields) -> TokenStream {
	let constructor = construct_value(fields, quote! { Self });
	let field_samples = append_field_samples(fields, constructor);

	quote! {
		let mut __samples: sp_std::vec::Vec<Self> = Default::default();
		#field_samples
		__samples
	}
}

fn append_field_samples(fields: &Fields, constructor: TokenStream) -> TokenStream {
	let field_types: Vec<_> = match fields {
		Fields::Named(f) => f.named.iter().map(|f| &f.ty).collect(),
		Fields::Unnamed(f) => f.unnamed.iter().map(|f| &f.ty).collect(),
		Fields::Unit => Vec::new(),
	};

	if field_types.is_empty() {
		return quote! {
			__samples.push(#constructor);
		};
	}

	let value_idents: Vec<_> = (0..field_types.len())
		.map(|index| format_ident!("__shape_value_{index}"))
		.collect();

	let baseline_sample = quote! {
		match (
			#(
				<#field_types as cf_utilities::type_introspection::HasTypeIntrospection>::sample_all_shapes()
					.into_iter()
					.next(),
			)*
		) {
			( #( Some(#value_idents), )* ) => __samples.push(#constructor),
			_ => {},
		}
	};

	let field_variations = field_types.iter().enumerate().map(|(field_index, field_type)| {
		let varied_value_ident = &value_idents[field_index];
		let other_field_types = field_types
			.iter()
			.enumerate()
			.filter_map(|(index, ty)| (index != field_index).then_some(*ty));
		let other_value_idents = value_idents
			.iter()
			.enumerate()
			.filter_map(|(index, ident)| (index != field_index).then_some(ident));

		quote! {
			{
				let mut __shape_iter = <#field_type as cf_utilities::type_introspection::HasTypeIntrospection>::sample_all_shapes().into_iter();
				let _ = __shape_iter.next();
				for #varied_value_ident in __shape_iter {
					match (
						#(
							<#other_field_types as cf_utilities::type_introspection::HasTypeIntrospection>::sample_all_shapes()
								.into_iter()
								.next(),
						)*
					) {
						( #( Some(#other_value_idents), )* ) => __samples.push(#constructor),
						_ => {},
					}
				}
			}
		}
	});

	quote! {
		{
			#baseline_sample
			#( #field_variations )*
		}
	}
}

fn construct_value(fields: &Fields, prefix: TokenStream) -> TokenStream {
	let value_idents: Vec<_> = match fields {
		Fields::Named(f) =>
			(0..f.named.len()).map(|index| format_ident!("__shape_value_{index}")).collect(),
		Fields::Unnamed(f) => (0..f.unnamed.len())
			.map(|index| format_ident!("__shape_value_{index}"))
			.collect(),
		Fields::Unit => Vec::new(),
	};

	match fields {
		Fields::Named(f) => {
			let field_names = f.named.iter().map(|field| &field.ident);
			quote! { #prefix { #( #field_names: #value_idents ),* } }
		},
		Fields::Unnamed(_) => quote! { #prefix( #( #value_idents ),* ) },
		Fields::Unit => quote! { #prefix },
	}
}
