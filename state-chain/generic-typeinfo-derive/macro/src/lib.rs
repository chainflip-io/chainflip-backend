/// This crate implements a derive macro that allows us to work around a bug in the Javascript and
/// Python libraries implementing SCALE decoding for Substrate RPC calls. Usually, for types that
/// need to be represented in SCALE, we use `#[derive(TypeInfo)]`, which is a derive macro provided
/// by Parity. This will automatically implement the `TypeInfo` trait, which is what ultimately
/// generates the type metadata used by Substrate. The problem arises because we have generic
/// pallets, which contain types that depend on the generic type parameter. Even if the names of
/// these generic types are the same, their types may not be. This confuses the SCALE libraries,
/// because they rely on the name of the type to resolve it in the type metadata (they should really
/// use the unique type ID, but they don't). In this crate, we provide an alternative macro
/// `#[derive(GenericTypeInfo)]`. It can be used on types that are generic and contain generic
/// types. Typically you'd want to use it on types that also have
/// #[scale_info(skip_type_params(T, I))]
/// applied to it. All fields in the struct (or variants of an enum) will have their typename
/// changed from "typename" to "typenameXYZ", where you can specify the "XYZ" part by using
/// #[expand_name_with("XYZ")]. The argument of expand_name_with can be any valid expression.
/// If you don't want a field to be renamed, you can suppress it by using the #[skip_name_expansion]
/// attribute. For example:
///
/// #[derive(GenericTypeInfo)]
/// #[expand_name_with(T::NAME)]
/// pub struct MyStruct<T: Config<I>, I: 'static = ()> {
///     #[skip_name_expansion]
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
/// example "Bitcoin") and will generate type names like `SpecialType<T, I>Bitcoin` instead.
///
/// This code calls .leak(), but that is ok!
/// The problem is that #derive macros like the one below get expanded into code before any generics
/// are resolved. That means that by the time this macro is fully executed and the corresponding
/// code is generated, we still don't know what types T and I from the example above represent. The
/// generic resolution happens at a later stage. Because the actual type of T determines the desired
/// typenames and the TypeInfo trait requires this string to be 'static, we can't really construct
/// it in the macro. The first time any code is executed that has all information required to
/// assemble the name is at runtime. The way to create 'static strings at runtime is exactly to call
/// .leak() on them. Normally one would then implement a caching mechanism to ensure that each
/// string is constructed and leaked at most once during the lifetime of the program, but since this
/// code will be running inside the runtime WASM, this is not necessary (or even possible in a
/// straightforward way). The way the Substrate node calls methods on the WASM runtime guarantees
/// that 'static variables don't survive inbetween calls, so we are fine calling .leak() here and
/// not worrying about anything.
extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
	Data, DataEnum, DataStruct, DeriveInput, Expr, Fields, Ident, ImplGenerics, TypeGenerics,
};

#[proc_macro_derive(
	GenericTypeInfo,
	attributes(expand_name_with, skip_name_expansion, replace_typename_with)
)]
pub fn derive(item: TokenStream) -> TokenStream {
	let item2: TokenStream2 = item.into();
	let ast: DeriveInput = syn::parse2(item2).expect("Failed to parse input tokens");
	let name_for_expansion: Expr = ast.attrs.iter()
		.find(|attr| attr.path().is_ident("expand_name_with"))
		.expect("When using the #[derive(GenericTypeInfo)] directive, you must provide the name to be used for expansion via #[expand_name_with(...)]").parse_args().unwrap();
	let ident = ast.ident;
	let (impl_generics, ty_generics, _) = ast.generics.split_for_impl();
	match ast.data {
		Data::Struct(d) => derive_struct(d, ident, impl_generics, ty_generics, name_for_expansion),
		Data::Enum(d) => derive_enum(d, ident, impl_generics, ty_generics, name_for_expansion),
		Data::Union(_) => panic!("#[derive(GenericTypeInfo)] not yet implemented for Unions"),
	}
	.into()
}

fn derive_fields(d: Fields, name_for_expansion: Expr) -> TokenStream2 {
	match d {
		syn::Fields::Named(fields_named) => {
			let fields: Vec<TokenStream2> = fields_named.named.iter().map(|field| {
				let ty = field.clone().ty;
				let typename = match field.attrs.iter().find(|attr| attr.path().is_ident("replace_typename_with")){
					Some(attr) => attr.parse_args::<Ident>().expect("replace_typename_with requires an argument").to_string(),
					None => clean_type_string(&quote!(#ty).to_string())
				};
				let name = field.clone().ident.expect("Named fields must have an identifier");
				if field.attrs.iter().any(|attr| attr.path().is_ident("skip_name_expansion")){
					quote!(.field(|f|
						f.ty::<#ty>().type_name(#typename).name(::core::stringify!(#name))
					))
				} else {
					quote!(.field(|f| {
						let full_typename = scale_info::prelude::format!("{}{}", #typename, #name_for_expansion).leak();
						f.ty::<#ty>().type_name(full_typename).name(::core::stringify!(#name))}))
				}
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
					if field.attrs.iter().any(|attr| attr.path().is_ident("skip_name_expansion")){
						quote!(.field(|f|
							f.ty::<#ty>().type_name(::core::stringify!(#typename))
						))
					} else {
						quote!(.field(|f| {
							let full_typename = scale_info::prelude::format!("{}{}", #typename, #name_for_expansion).leak();
							f.ty::<#ty>().type_name(full_typename)}))
					}
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
	name_for_expansion: Expr,
) -> TokenStream2 {
	let fields = derive_fields(d.fields, name_for_expansion.clone());
	quote!(impl #impl_generics TypeInfo for #ident #ty_generics {
		type Identity = Self;
		fn type_info() -> scale_info::Type {
			let full_pathname = scale_info::prelude::format!("{}{}", ::core::stringify!(#ident), #name_for_expansion).leak();
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
	name_for_expansion: Expr,
) -> TokenStream2 {
	let variants: Vec<_> = d
		.variants
		.into_iter()
		.enumerate()
		.map(|(index, variant)| {
			let name = variant.ident;
			let fields = derive_fields(variant.fields, name_for_expansion.clone());
			let typename = match variant.attrs.iter().find(|attr| attr.path().is_ident("replace_typename_with")){
				Some(attr) => attr.parse_args::<Ident>().expect("replace_typename_with requires an argument").to_string(),
				None => clean_type_string(&quote!(#name).to_string())
			};
			if variant.attrs.iter().any(|attr| attr.path().is_ident("skip_name_expansion")){
				quote!(.variant(#typename, |v|{
					v.index(#index as u8)
					.fields(#fields)}))
			} else {
				quote!(.variant(scale_info::prelude::format!("{}{}", #typename, #name_for_expansion).leak() as &'static str, |v|{
					v.index(#index as u8)
					.fields(#fields)}))
			}
		})
		.collect();
	quote!(impl #impl_generics TypeInfo for #ident #ty_generics {
		type Identity = Self;
		fn type_info() -> scale_info::Type {
			let full_pathname = scale_info::prelude::format!("{}{}", ::core::stringify!(#ident), #name_for_expansion).leak();
			scale_info::Type::builder()
			.path(scale_info::Path::new(full_pathname, ::core::module_path!()))
			.variant(
				scale_info::build::Variants::new() #( #variants )* )
		}
	})
}

// This is taken directly from the Substrate TypeInfo library
// The purpose is to remove spurious whitespace and make the type
// name align with what one would see in the code.
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
