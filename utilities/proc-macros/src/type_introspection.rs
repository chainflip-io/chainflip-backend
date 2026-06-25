use proc_macro2::TokenStream;
use quote::quote;
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

	let body = match &input.data {
		Data::Struct(data) => struct_body(&data.fields),
		Data::Enum(data) => {
			if data.variants.is_empty() {
				quote! { true }
			} else {
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
		impl #impl_generics cf_utilities::type_introspection::HasTypeIntrospection for #name #ty_generics #where_clause {
			fn is_empty_type() -> bool {
				#body
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
		Data::Enum(data) => {
			for variant in &data.variants {
				collect_from_fields(&variant.fields, &mut types);
			}
		},
		Data::Union(_) => {},
	}
	types
}

fn collect_from_fields(fields: &Fields, types: &mut Vec<syn::Type>) {
	match fields {
		Fields::Named(f) => {
			for field in &f.named {
				types.push(field.ty.clone());
			}
		},
		Fields::Unnamed(f) => {
			for field in &f.unnamed {
				types.push(field.ty.clone());
			}
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
