extern crate proc_macro;
extern crate proc_macro2;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemForeignMod};

use engine_upgrade_utils::{ENGINE_LIB_PREFIX, NEW_VERSION, OLD_VERSION};

#[proc_macro_attribute]
pub fn link_engine_library_version(args: TokenStream, item: TokenStream) -> TokenStream {
	let item = parse_macro_input!(item as ItemForeignMod);
	let version = parse_macro_input!(args as syn::LitStr).value();

	if ![OLD_VERSION, NEW_VERSION].contains(&version.as_str()) {
		panic!("Invalid version. Expected either old new version.")
	}

	let versioned_name = format!("{ENGINE_LIB_PREFIX}{}", version.to_string().replace('.', "_"));

	TokenStream::from(quote! {
		#[link(name = #versioned_name)]
		#item
	})
}
