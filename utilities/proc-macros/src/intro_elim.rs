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

use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields};

pub fn derive(input: DeriveInput) -> TokenStream {
	match &input.data {
		Data::Struct(_) => derive_struct(input),
		Data::Enum(_) => derive_enum(input),
		Data::Union(_) =>
			syn::Error::new_spanned(&input.ident, "IntroElim cannot be derived for unions")
				.to_compile_error(),
	}
}

fn derive_struct(input: DeriveInput) -> TokenStream {
	let name = &input.ident;
	let generics = &input.generics;
	let fields = match &input.data {
		Data::Struct(data) => match &data.fields {
			Fields::Named(fields) => fields,
			Fields::Unnamed(_) | Fields::Unit => {
				return syn::Error::new_spanned(
					name,
					"IntroElim can only be derived for structs with named fields",
				)
				.to_compile_error();
			},
		},
		Data::Enum(_) | Data::Union(_) => unreachable!(),
	};

	let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
	let intro_args = fields.named.iter().map(|field| {
		let ident = field.ident.as_ref().expect("named fields have identifiers");
		let ty = &field.ty;
		quote! { #ident: #ty }
	});
	let field_inits = fields.named.iter().map(|field| {
		let ident = field.ident.as_ref().expect("named fields have identifiers");
		quote! { #ident }
	});

	quote! {
		impl #impl_generics #name #ty_generics #where_clause {
			#[allow(clippy::too_many_arguments)]
			pub fn intro(#(#intro_args),*) -> Self {
				Self {
					#(#field_inits,)*
				}
			}
		}
	}
}

fn derive_enum(input: DeriveInput) -> TokenStream {
	let enum_ident = input.ident;
	let generics = input.generics;
	let variants = match input.data {
		Data::Enum(data) => data.variants,
		Data::Struct(_) | Data::Union(_) => unreachable!(),
	};

	let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
	let output = format_ident!("__IntroElimOutput");

	let handler_args = variants.iter().enumerate().map(|(variant_index, variant)| {
		let handler = handler_ident(variant_index);
		let field_types = variant.fields.iter().map(|field| &field.ty);
		quote! {
			#handler: impl Fn(#(#field_types),*) -> #output
		}
	});

	let handler_args_ref = variants.iter().enumerate().map(|(variant_index, variant)| {
		let handler = handler_ident(variant_index);
		let field_types = variant.fields.iter().map(|field| {
			let ty = &field.ty;
			quote! { &#ty }
		});
		quote! {
			#handler: impl Fn(#(#field_types),*) -> #output
		}
	});

	let match_arms = variants.iter().enumerate().map(|(variant_index, variant)| {
		let variant_ident = &variant.ident;
		let handler = handler_ident(variant_index);
		let field_bindings = (0..variant.fields.len())
			.map(|field_index| field_ident(variant_index, field_index))
			.collect::<Vec<_>>();

		match &variant.fields {
			Fields::Named(fields) => {
				let patterns =
					fields.named.iter().zip(field_bindings.iter()).map(|(field, binding)| {
						let field_ident =
							field.ident.as_ref().expect("named fields have identifiers");
						quote! { #field_ident: #binding }
					});
				quote! {
					Self::#variant_ident { #(#patterns),* } => #handler(#(#field_bindings),*)
				}
			},
			Fields::Unnamed(_) => {
				quote! {
					Self::#variant_ident(#(#field_bindings),*) => #handler(#(#field_bindings),*)
				}
			},
			Fields::Unit => {
				quote! {
					Self::#variant_ident => #handler()
				}
			},
		}
	});

	let match_arms_ref = variants.iter().enumerate().map(|(variant_index, variant)| {
		let variant_ident = &variant.ident;
		let handler = handler_ident(variant_index);
		let field_bindings = (0..variant.fields.len())
			.map(|field_index| field_ident(variant_index, field_index))
			.collect::<Vec<_>>();

		match &variant.fields {
			Fields::Named(fields) => {
				let patterns =
					fields.named.iter().zip(field_bindings.iter()).map(|(field, binding)| {
						let field_ident =
							field.ident.as_ref().expect("named fields have identifiers");
						quote! { #field_ident: #binding }
					});
				quote! {
					Self::#variant_ident { #(#patterns),* } => #handler(#(#field_bindings),*)
				}
			},
			Fields::Unnamed(_) => {
				quote! {
					Self::#variant_ident(#(#field_bindings),*) => #handler(#(#field_bindings),*)
				}
			},
			Fields::Unit => {
				quote! {
					Self::#variant_ident => #handler()
				}
			},
		}
	});

	quote! {
		impl #impl_generics #enum_ident #ty_generics #where_clause {
			#[allow(clippy::too_many_arguments)]
			pub fn elim<#output>(
				self,
				#(#handler_args,)*
			) -> #output {
				match self {
					#(#match_arms,)*
				}
			}

			#[allow(clippy::too_many_arguments)]
			pub fn elim_ref<#output>(
				&self,
				#(#handler_args_ref,)*
			) -> #output {
				match self {
					#(#match_arms_ref,)*
				}
			}
		}
	}
}

fn handler_ident(index: usize) -> Ident {
	format_ident!("__elim_handler_{}", index)
}

fn field_ident(variant_index: usize, field_index: usize) -> Ident {
	format_ident!("__elim_{}_{}", variant_index, field_index)
}
