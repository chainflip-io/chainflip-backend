extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DataEnum, DataStruct, DeriveInput, Fields, Ident, ImplGenerics, TypeGenerics};

#[proc_macro_derive(GenericTypeInfo)]
pub fn derive(item: TokenStream) -> TokenStream {
	let item2: TokenStream2 = item.into();
	let ast: DeriveInput = syn::parse2(item2).unwrap();
	let ident = ast.ident;
	let (impl_generics, ty_generics, _) = ast.generics.split_for_impl();
	match ast.data {
		Data::Struct(d) => derive_struct(d, ident, impl_generics, ty_generics),
		Data::Enum(d) => derive_enum(d, ident, impl_generics, ty_generics),
		Data::Union(_) => panic!("#[derive(GenericTypeInfo)] not yet implemented for Unions"),
	}
	.into()
}

fn derive_fields(d: Fields) -> TokenStream2 {
	match d {
		syn::Fields::Named(fields_named) => {
			let fields: Vec<TokenStream2> = fields_named.named.iter().map(|field| {
                let ty = field.clone().ty;
                let typename = clean_type_string(&quote!(#ty).to_string());
                let name = field.clone().ident.unwrap();
                quote!(.field(|f| {f.ty::<#ty>().type_name(scale_info::prelude::format!("{}For{}", #typename, T::NAME).leak()).name(::core::stringify!(#name))}))
            }).collect();
			quote!(scale_info::build::Fields::named() #( #fields )* )
		},
		syn::Fields::Unnamed(fields_unnamed) => {
			let fields: Vec<TokenStream2> = fields_unnamed.unnamed.iter().map(|field| {
                let ty = field.clone().ty;
                let typename = clean_type_string(&quote!(#ty).to_string());
                quote!(.field(|f| {f.ty::<#ty>().type_name(scale_info::prelude::format!("{}For{}", #typename, T::NAME).leak())}))
            }).collect();
			quote!(scale_info::build::Fields::unnamed() #( #fields )* )
		},
		syn::Fields::Unit => quote!(scale_info::build::Fields::unit()),
	}
}

fn derive_struct(
	d: DataStruct,
	ident: Ident,
	impl_generics: ImplGenerics,
	ty_generics: TypeGenerics,
) -> TokenStream2 {
	let fields = derive_fields(d.fields);
	quote!(impl #impl_generics TypeInfo for #ident #ty_generics {
		type Identity = Self;
		fn type_info() -> scale_info::Type {
			let expanded_ident = scale_info::prelude::format!("{}For{}", ::core::stringify!(#ident), T::NAME).into_boxed_str();
			scale_info::Type::builder()
				.path(scale_info::Path::new(Box::leak(expanded_ident), ::core::module_path!()))
				.composite(#fields)
		}
	})
}

fn derive_enum(
	d: DataEnum,
	ident: Ident,
	impl_generics: ImplGenerics,
	ty_generics: TypeGenerics,
) -> TokenStream2 {
	let variants: Vec<_> = d
		.variants
		.into_iter()
		.enumerate()
		.map(|(index, variant)| {
			let name = variant.ident;
			let fields = derive_fields(variant.fields);
			quote!(.variant(::core::stringify!(#name), |v| {
            v.index(#index as u8)
                .fields(#fields)}))
		})
		.collect();
	quote!(impl #impl_generics TypeInfo for #ident #ty_generics {
		type Identity = Self;
		fn type_info() -> scale_info::Type {
			let expanded_ident = scale_info::prelude::format!("{}For{}", ::core::stringify!(#ident), T::NAME).into_boxed_str();
			scale_info::Type::builder()
			.path(scale_info::Path::new(Box::leak(expanded_ident), ::core::module_path!()))
			.variant(
				scale_info::build::Variants::new() #( #variants )* )
		}
	})
}

fn clean_type_string(input: &str) -> String {
	input
		.replace(" ::", "::")
		.replace(":: ", "::")
		.replace(" ,", ",")
		.replace(" ;", ";")
		.replace(" [", "[")
		.replace("[ ", "[")
		.replace(" ]", "]")
		.replace(" (", "(")
		// put back a space so that `a: (u8, (bool, u8))` isn't turned into `a: (u8,(bool, u8))`
		.replace(",(", ", (")
		.replace("( ", "(")
		.replace(" )", ")")
		.replace(" <", "<")
		.replace("< ", "<")
		.replace(" >", ">")
		.replace("& \'", "&'")
}
