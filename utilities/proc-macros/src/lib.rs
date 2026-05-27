use proc_macro::TokenStream;

/// Attribute macro that wraps the annotated struct in a `cf_utilities::generate_module!`
/// invocation, automatically generating the module name as `_StructName`.
///
/// Usage:
/// ```ignore
/// #[generate_module]
/// struct MyStruct {
///     field: Type,
/// }
/// ```
///
/// Expands to:
/// ```ignore
/// cf_utilities::generate_module! {
///     struct MyStruct {
///         field: Type,
///     }
///     mod _MyStruct { #![migrations] }
/// }
/// ```
#[proc_macro_attribute]
pub fn generate_module(_attr: TokenStream, item: TokenStream) -> TokenStream {
	let item_clone: proc_macro2::TokenStream = item.clone().into();
	let parsed: syn::ItemStruct =
		syn::parse(item).expect("#[generate_module] can only be applied to structs");
	let mod_name = quote::format_ident!("_{}", parsed.ident);
	let output = quote::quote! {
		cf_utilities::generate_module! {
			#item_clone
			mod #mod_name { #![migrations] }
		}
	};
	output.into()
}
