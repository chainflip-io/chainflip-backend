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
use syn::{parse::Parse, punctuated::Punctuated, token::Comma, Data, DeriveInput, Fields, Type};

/// Maximum number of elements in a single strategy tuple (proptest supports up to 12).
const CHUNK_SIZE: usize = 10;

/// Returns true if a type looks like `PhantomData<...>` (possibly qualified).
fn is_phantom_data(ty: &Type) -> bool {
	match ty {
		Type::Path(type_path) => {
			let last_segment = type_path.path.segments.last();
			last_segment.is_some_and(|seg| seg.ident == "PhantomData")
		},
		_ => false,
	}
}

/// Parsed contents of `#[arbitrary(bound = "...")]`.
struct BoundAttr {
	predicates: Punctuated<syn::WherePredicate, Comma>,
}

impl Parse for BoundAttr {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		let predicates = Punctuated::parse_terminated(input)?;
		Ok(BoundAttr { predicates })
	}
}

/// Parse the `#[arbitrary(...)]` attributes from a derive input.
/// Returns `Ok(Some(predicates))` if an explicit bound was given, `Ok(None)` if not.
///
/// Supports two syntaxes:
/// - `#[arbitrary(bound = "T: Trait, U: Other")]` — string literal (for direct use)
/// - `#[arbitrary(bound(T: Trait, U: Other))]` — raw tokens (for use inside declarative macros)
fn parse_arbitrary_attrs(input: &DeriveInput) -> syn::Result<Option<Vec<syn::WherePredicate>>> {
	let mut bounds: Option<Vec<syn::WherePredicate>> = None;

	for attr in &input.attrs {
		if !attr.path().is_ident("arbitrary") {
			continue;
		}

		let nested = attr.parse_args_with(Punctuated::<syn::Meta, Comma>::parse_terminated)?;

		for meta in nested {
			match &meta {
				syn::Meta::NameValue(nv) if nv.path.is_ident("bound") => {
					// #[arbitrary(bound = "T: Trait, ...")]
					let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(lit), .. }) = &nv.value
					else {
						return Err(syn::Error::new_spanned(
							&nv.value,
							"expected string literal for `bound`",
						));
					};
					let parsed: BoundAttr = lit.parse()?;
					let predicates: Vec<_> = parsed.predicates.into_iter().collect();
					match &mut bounds {
						Some(existing) => existing.extend(predicates),
						None => bounds = Some(predicates),
					}
				},
				syn::Meta::List(list) if list.path.is_ident("bound") => {
					// #[arbitrary(bound(T: Trait, ...))]
					let parsed: BoundAttr = syn::parse2(list.tokens.clone())?;
					let predicates: Vec<_> = parsed.predicates.into_iter().collect();
					match &mut bounds {
						Some(existing) => existing.extend(predicates),
						None => bounds = Some(predicates),
					}
				},
				_ => {
					return Err(syn::Error::new_spanned(
						&meta,
						"unknown arbitrary attribute, expected `bound`",
					));
				},
			}
		}
	}

	Ok(bounds)
}

/// Generates a strategy expression and destructuring pattern for a chunk of fields.
/// Returns (strategy_expr, pattern_bindings) for a flat tuple of `any::<T>()` calls.
fn generate_chunk_strategy(
	field_types: &[&Type],
	binding_idents: &[syn::Ident],
) -> (TokenStream, TokenStream) {
	let strategies: Vec<_> = field_types
		.iter()
		.map(|ty| quote! { proptest::arbitrary::any::<#ty>() })
		.collect();
	let bindings: Vec<_> = binding_idents.iter().map(|id| quote! { #id }).collect();

	(quote! { ( #(#strategies),* ) }, quote! { ( #(#bindings),* ) })
}

pub fn derive(input: DeriveInput) -> TokenStream {
	let name = &input.ident;
	let generics = &input.generics;

	let fields = match &input.data {
		Data::Struct(data) => match &data.fields {
			Fields::Named(fields) => &fields.named,
			Fields::Unnamed(_) | Fields::Unit => {
				return syn::Error::new_spanned(
					name,
					"ArbitraryWithBounds can only be derived for structs with named fields",
				)
				.to_compile_error();
			},
		},
		Data::Enum(_) | Data::Union(_) => {
			return syn::Error::new_spanned(
				name,
				"ArbitraryWithBounds can only be derived for structs",
			)
			.to_compile_error();
		},
	};

	// Parse bound attribute
	let explicit_bounds = match parse_arbitrary_attrs(&input) {
		Ok(b) => b,
		Err(e) => return e.to_compile_error(),
	};

	// Partition fields into "real" (need strategy) and "phantom" (use Default)
	let mut real_fields: Vec<(&syn::Ident, &Type)> = Vec::new();
	let mut phantom_fields: Vec<&syn::Ident> = Vec::new();

	for field in fields.iter() {
		let ident = field.ident.as_ref().expect("named fields have identifiers");
		if is_phantom_data(&field.ty) {
			phantom_fields.push(ident);
		} else {
			real_fields.push((ident, &field.ty));
		}
	}

	// Generate the where clause
	let (impl_generics, ty_generics, existing_where_clause) = generics.split_for_impl();

	let where_predicates: Vec<TokenStream> = if let Some(ref bounds) = explicit_bounds {
		bounds.iter().map(|pred| quote! { #pred }).collect()
	} else {
		// Default: require Arbitrary + 'static for each real field type, 'static for generics
		let mut preds: Vec<TokenStream> = Vec::new();
		for param in generics.type_params() {
			let ident = &param.ident;
			preds.push(quote! { #ident: 'static });
		}
		for (_, ty) in &real_fields {
			preds.push(quote! { #ty: proptest::arbitrary::Arbitrary + 'static });
		}
		preds
	};

	// Combine existing where clause predicates with ours
	let existing_preds: Vec<TokenStream> = existing_where_clause
		.map(|wc| wc.predicates.iter().map(|p| quote! { #p }).collect())
		.unwrap_or_default();

	let all_preds: Vec<&TokenStream> =
		existing_preds.iter().chain(where_predicates.iter()).collect();

	// Generate the strategy body
	let body = if real_fields.is_empty() {
		// No real fields — all phantom. Just return a constant.
		let phantom_inits: Vec<_> =
			phantom_fields.iter().map(|id| quote! { #id: Default::default() }).collect();
		quote! {
			use proptest::strategy::Strategy;
			proptest::strategy::Just(Self { #(#phantom_inits),* }).boxed()
		}
	} else {
		// Generate binding identifiers for each real field
		let binding_idents: Vec<syn::Ident> =
			real_fields.iter().enumerate().map(|(i, _)| format_ident!("__f{}", i)).collect();

		let real_field_types: Vec<&Type> = real_fields.iter().map(|(_, ty)| *ty).collect();

		// Chunk the fields into groups of CHUNK_SIZE
		let type_chunks: Vec<&[&Type]> = real_field_types.chunks(CHUNK_SIZE).collect();
		let ident_chunks: Vec<&[syn::Ident]> = binding_idents.chunks(CHUNK_SIZE).collect();

		// Build the strategy expression and pattern
		let (strategy_expr, pattern) = if type_chunks.len() == 1 {
			// Single chunk — flat tuple
			generate_chunk_strategy(type_chunks[0], ident_chunks[0])
		} else {
			// Multiple chunks — nested tuple of tuples
			let mut strategy_parts = Vec::new();
			let mut pattern_parts = Vec::new();
			for (types, idents) in type_chunks.iter().zip(ident_chunks.iter()) {
				let (s, p) = generate_chunk_strategy(types, idents);
				strategy_parts.push(s);
				pattern_parts.push(p);
			}
			(quote! { ( #(#strategy_parts),* ) }, quote! { ( #(#pattern_parts),* ) })
		};

		// Build the struct construction
		let field_inits: Vec<TokenStream> = real_fields
			.iter()
			.enumerate()
			.map(|(i, (field_name, _))| {
				let binding = &binding_idents[i];
				quote! { #field_name: #binding }
			})
			.collect();
		let phantom_inits: Vec<TokenStream> =
			phantom_fields.iter().map(|id| quote! { #id: Default::default() }).collect();

		quote! {
			use proptest::strategy::Strategy;

			#strategy_expr
				.prop_map(|#pattern| Self {
					#(#field_inits,)*
					#(#phantom_inits,)*
				})
				.boxed()
		}
	};

	quote! {
		impl #impl_generics proptest::arbitrary::Arbitrary for #name #ty_generics
		where
			#(#all_preds),*
		{
			type Parameters = ();
			type Strategy = proptest::strategy::BoxedStrategy<Self>;

			fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
				#body
			}
		}
	}
}
