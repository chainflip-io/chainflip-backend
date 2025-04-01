/// This crate implements a derive macro that allows us to work around a bug in the Javascript and
/// Python libraries implementing SCALE decoding for Substrate RPC calls. Usually, for types that
/// need to be represented in SCALE, we use `#[derive(TypeInfo)]`, which is a derive macro provided
/// by Parity. This will automatically implement the `TypeInfo` trait, which is what ultimately
/// generates the type metadata used by Substrate. The problem arises because we have generic
/// pallets, which contain types that depend on the generic type parameter. Even if the names of
/// these generic types are the same, their types may not be. This confuses the SCALE libraries,
/// because they rely on the name of the type to resolve it in the type metadata (they should really
/// use the uniqe type ID, but they don't). In this crate, we provide an alternative macro
/// `#[derive(GenericTypeInfo)]`. It can be used on types that are generic and contain generic
/// types. The requirement is that the first generic parameter contains a static `NAME` element. For
/// example:
///
/// #[derive(GenericTypeInfo)]
/// pub struct MyStruct<T: Config<I>, I: 'static = ()> {
///     pub foo: u32,
///     pub bar: SpecialType<T, I>,
/// }
///
/// Here, `MyStruct` is generic and contains a generic type `SpecialType`. Depending on `T` and `I`,
/// the actual type of `SpecialType` may be different, but the typename will
/// always be `"SpecialType<T, I>"`, because derive macros are executed before generics are
/// expanded. Thus, the current SCALE libraries cannot uniquely determine the correct type
/// based on the type name alone and will usually just fail. Using `#[derive(GenericTypeInfo)]` here
/// will instead assume that `T::NAME` is a static string uniquely determining the type `T` (for
/// example "Bitcoin") and will generate type names like `SpecialType<T, I>ForBitcoin` instead.
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
	let chain_generic_ident = ast.generics.type_params().next().unwrap().ident.clone();
	let (impl_generics, ty_generics, _) = ast.generics.split_for_impl();
	match ast.data {
		Data::Struct(d) => derive_struct(d, ident, impl_generics, ty_generics, chain_generic_ident),
		Data::Enum(d) => derive_enum(d, ident, impl_generics, ty_generics, chain_generic_ident),
		Data::Union(_) => panic!("#[derive(GenericTypeInfo)] not yet implemented for Unions"),
	}
	.into()
}

fn derive_fields(d: Fields, chain_ident: Ident) -> TokenStream2 {
	match d {
		syn::Fields::Named(fields_named) => {
			let fields: Vec<TokenStream2> = fields_named.named.iter().map(|field| {
                let ty = field.clone().ty;
                let typename = clean_type_string(&quote!(#ty).to_string());
                let name = field.clone().ident.unwrap();
                quote!(.field(|f| {
					let full_typename = scale_info::prelude::format!("{}For{}", #typename, #chain_ident::NAME).leak();
					f.ty::<#ty>().type_name(full_typename).name(::core::stringify!(#name))}))
            }).collect();
			quote!(scale_info::build::Fields::named() #( #fields )* )
		},
		syn::Fields::Unnamed(fields_unnamed) => {
			let fields: Vec<TokenStream2> = fields_unnamed
				.unnamed
				.iter()
				.map(|field| {
					let ty = field.clone().ty;
					let typename = clean_type_string(&quote!(#ty).to_string());
					quote!(.field(|f| {
						let full_typename = scale_info::prelude::format!("{}For{}", #typename, #chain_ident::NAME).leak();
						f.ty::<#ty>().type_name(full_typename)}))
				})
				.collect();
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
	chain_ident: Ident,
) -> TokenStream2 {
	let fields = derive_fields(d.fields, chain_ident.clone());
	quote!(impl #impl_generics TypeInfo for #ident #ty_generics {
		type Identity = Self;
		fn type_info() -> scale_info::Type {
			let full_pathname = scale_info::prelude::format!("{}For{}", ::core::stringify!(#ident), #chain_ident::NAME).leak();
			scale_info::Type::builder()
				.path(scale_info::Path::new(full_pathname, ::core::module_path!()))
				.composite(#fields)
		}
	})
}

fn derive_enum(
	d: DataEnum,
	ident: Ident,
	impl_generics: ImplGenerics,
	ty_generics: TypeGenerics,
	chain_ident: Ident,
) -> TokenStream2 {
	let variants: Vec<_> = d
		.variants
		.into_iter()
		.enumerate()
		.map(|(index, variant)| {
			let name = variant.ident;
			let fields = derive_fields(variant.fields, chain_ident.clone());
			quote!(.variant(::core::stringify!(#name), |v| {
            v.index(#index as u8)
                .fields(#fields)}))
		})
		.collect();
	quote!(impl #impl_generics TypeInfo for #ident #ty_generics {
		type Identity = Self;
		fn type_info() -> scale_info::Type {
			let full_pathname = scale_info::prelude::format!("{}For{}", ::core::stringify!(#ident), #chain_ident::NAME).leak();
			scale_info::Type::builder()
			.path(scale_info::Path::new(full_pathname, ::core::module_path!()))
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
