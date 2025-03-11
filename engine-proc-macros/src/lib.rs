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

extern crate proc_macro;
extern crate proc_macro2;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, ItemForeignMod};

use engine_upgrade_utils::{ENGINE_ENTRYPOINT_PREFIX, ENGINE_LIB_PREFIX, NEW_VERSION, OLD_VERSION};

#[proc_macro_attribute]
pub fn link_engine_library_version(args: TokenStream, item: TokenStream) -> TokenStream {
	let mut item_foreign_mod = parse_macro_input!(item as ItemForeignMod);

	assert_eq!(
		item_foreign_mod.items.len(),
		1,
		"Only expect one function signature for the entrypoint"
	);
	let syn::ForeignItem::Fn(ref mut input_fn) = item_foreign_mod.items[0] else {
		panic!("Expected a function signature")
	};

	let version = parse_macro_input!(args as syn::LitStr).value();

	if ![OLD_VERSION, NEW_VERSION].contains(&version.as_str()) {
		panic!("Invalid version. Expected either old new version.")
	}

	let underscored_version = version.replace('.', "_");

	let input_fn_sig = input_fn.sig.clone();
	let versioned_fn_name = syn::Ident::new(
		&format!("{ENGINE_ENTRYPOINT_PREFIX}{underscored_version}"),
		input_fn.sig.ident.span(),
	);
	input_fn.sig.ident = versioned_fn_name.clone();

	let versioned_lib_name = format!("{ENGINE_LIB_PREFIX}{underscored_version}");

	TokenStream::from(quote! {
		#[link(name = #versioned_lib_name)]
		#item_foreign_mod

		pub #input_fn_sig {
			unsafe {
				#versioned_fn_name(c_args, start_from)
			}
		}
	})
}

#[proc_macro_attribute]
pub fn cfe_entrypoint(_attrs: TokenStream, item: TokenStream) -> TokenStream {
	// Parse the input function
	let input_fn = parse_macro_input!(item as ItemFn);

	// Get the version from your Cargo.toml file
	let underscored_version = env!("CARGO_PKG_VERSION").replace('.', "_");

	// Construct the new function name
	let versioned_fn_name = syn::Ident::new(
		&format!("{ENGINE_ENTRYPOINT_PREFIX}{underscored_version}"),
		input_fn.sig.ident.span(),
	);

	let block = input_fn.block;

	let output = quote! {
		#[no_mangle]
		extern "C" fn #versioned_fn_name(
			c_args: CStrArray,
			start_from: u32,
		) -> ExitStatus {
			// Insert the function body specified in the input function
			#block
		}
	};

	output.into()
}
