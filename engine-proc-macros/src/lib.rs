extern crate proc_macro;
extern crate proc_macro2;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, ItemForeignMod};

use engine_upgrade_utils::{ENGINE_ENTRYPOINT_PREFIX, ENGINE_LIB_PREFIX, NEW_VERSION, OLD_VERSION};

#[proc_macro_attribute]
pub fn link_engine_library_version(args: TokenStream, item: TokenStream) -> TokenStream {
	let mut item_foreign_mod = parse_macro_input!(item as ItemForeignMod);

	let syn::ForeignItem::Fn(ref mut input_fn) = item_foreign_mod.items.iter_mut().next().unwrap()
	else {
		panic!("Expected a function signature")
	};

	let version = parse_macro_input!(args as syn::LitStr).value();

	if ![OLD_VERSION, NEW_VERSION].contains(&version.as_str()) {
		panic!("Invalid version. Expected either old new version.")
	}

	let underscored_version = version.replace('.', "_");

	let entrypoint_sig = input_fn.sig.clone();
	let new_fn_name = syn::Ident::new(
		&format!("{ENGINE_ENTRYPOINT_PREFIX}{underscored_version}"),
		input_fn.sig.ident.span(),
	);
	input_fn.sig.ident = new_fn_name.clone();

	let versioned_lib_name = format!("{ENGINE_LIB_PREFIX}{underscored_version}");

	TokenStream::from(quote! {
		#[link(name = #versioned_lib_name)]
		#item_foreign_mod

		pub #entrypoint_sig {
			unsafe {
				#new_fn_name(c_args, start_from)
			}
		}
	})
}

#[proc_macro_attribute]
pub fn cfe_entrypoint(_attrs: TokenStream, item: TokenStream) -> TokenStream {
	// Parse the input function
	let input_fn = parse_macro_input!(item as ItemFn);

	// Get the version from your Cargo.toml file
	let version = env!("CARGO_PKG_VERSION").replace('.', "_");

	// Construct the new function name
	let new_fn_name =
		syn::Ident::new(&format!("{ENGINE_ENTRYPOINT_PREFIX}{version}"), input_fn.sig.ident.span());

	let block = input_fn.block;

	let output = quote! {
		#[no_mangle]
		extern "C" fn #new_fn_name(
			c_args: CStrArray,
			start_from: u32,
		) -> ExitStatus {
			// Insert the function body specified in the input function
			#block
		}
	};

	output.into()
}
