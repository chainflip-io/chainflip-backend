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

pub fn expand(item_clone: TokenStream, parsed: syn::Item) -> TokenStream {
	let (item_for_generate_module, mod_name) = match &parsed {
		syn::Item::Struct(item_struct) => (item_clone, format_ident!("_{}", item_struct.ident)),
		syn::Item::Enum(item_enum) =>
			(enum_with_tuple_field_names(item_enum), format_ident!("_{}", item_enum.ident)),
		_ => {
			return syn::Error::new_spanned(
				parsed,
				"#[generate_module] can only be applied to structs or enums",
			)
			.to_compile_error();
		},
	};

	quote! {
		cf_utilities::generate_module! {
			#item_for_generate_module
			mod #mod_name { #![migrations] }
		}
	}
}

fn enum_with_tuple_field_names(item_enum: &syn::ItemEnum) -> TokenStream {
	let attrs = &item_enum.attrs;
	let vis = &item_enum.vis;
	let enum_ident = &item_enum.ident;
	let generics = &item_enum.generics;
	let variants = item_enum.variants.iter().map(|variant| {
		let attrs = &variant.attrs;
		let variant_ident = &variant.ident;
		let fields = match &variant.fields {
			syn::Fields::Unit => quote! {},
			syn::Fields::Named(fields) => {
				let fields = fields.named.iter().map(|field| {
					let field_ident = field.ident.as_ref().expect("named fields have identifiers");
					let ty = &field.ty;
					quote! { #field_ident: #ty, }
				});
				quote! { { #( #fields )* } }
			},
			syn::Fields::Unnamed(fields) => {
				let fields = fields.unnamed.iter().enumerate().map(|(index, field)| {
					let field_ident = format_ident!("_{}", index);
					let ty = &field.ty;
					quote! { #field_ident: #ty }
				});
				quote! { ( #( #fields ),* ) }
			},
		};
		let discriminant = variant.discriminant.as_ref().map(|(_, expr)| quote! { = #expr });

		quote! {
			#( #attrs )*
			#variant_ident #fields #discriminant,
		}
	});

	quote! {
		#( #attrs )*
		#vis enum #enum_ident #generics {
			#( #variants )*
		}
	}
}
