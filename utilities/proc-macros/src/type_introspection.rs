use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

pub fn derive(input: DeriveInput) -> TokenStream {
	let name = &input.ident;
	let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

	// Add HasTypeIntrospection bounds for all generic type parameters.
	let mut generics = input.generics.clone();
	for param in generics.type_params_mut() {
		param.bounds.push(syn::parse_quote!(cf_utilities::HasTypeIntrospection));
	}
	let (impl_generics_bounded, _, where_clause_bounded) = generics.split_for_impl();
	// Use the bounded versions for the impl block.
	let _ = (impl_generics, where_clause);

	let body = match &input.data {
		Data::Struct(data) => struct_body(&data.fields),
		Data::Enum(data) => {
			if data.variants.is_empty() {
				// Empty enum (like `!`) is always empty.
				quote! { true }
			} else {
				// An enum is empty if ALL variants are empty.
				let variant_checks: Vec<TokenStream> =
					data.variants.iter().map(|v| variant_is_empty(&v.fields)).collect();
				quote! { #( #variant_checks )&&* }
			}
		},
		Data::Union(_) => {
			return syn::Error::new_spanned(
				name,
				"HasTypeIntrospection cannot be derived for unions",
			)
			.to_compile_error();
		},
	};

	quote! {
		impl #impl_generics_bounded cf_utilities::HasTypeIntrospection for #name #ty_generics #where_clause_bounded {
			fn is_empty_type() -> bool {
				#body
			}
		}
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
		#( <#field_types as cf_utilities::HasTypeIntrospection>::is_empty_type() )||*
	}
}
