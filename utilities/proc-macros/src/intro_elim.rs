use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

pub fn derive(input: DeriveInput) -> TokenStream {
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
		Data::Enum(_) | Data::Union(_) => {
			return syn::Error::new_spanned(name, "IntroElim can only be derived for structs")
				.to_compile_error();
		},
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
			pub fn intro(#(#intro_args),*) -> Self {
				Self {
					#(#field_inits,)*
				}
			}
		}
	}
}
