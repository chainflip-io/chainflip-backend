extern crate proc_macro;
extern crate proc_macro2;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// Generates a C compatible entrypoint namespacing with the version suffix so that the no_mangle
/// names do not conflict when two shared libraries are used in the same process.
#[proc_macro_attribute]
pub fn cfe_entrypoint(_attr: TokenStream, item: TokenStream) -> TokenStream {
	// Parse the input function
	let input_fn = parse_macro_input!(item as ItemFn);

	// Get the version from your Cargo.toml file
	let version = env!("CARGO_PKG_VERSION").replace('.', "_");

	// Construct the new function name
	let new_fn_name =
		syn::Ident::new(&format!("cfe_entrypoint_v{}", version), input_fn.sig.ident.span());

	let block = input_fn.block;

	let output = quote! {
		#[no_mangle]
		extern "C" fn #new_fn_name(
			args: *mut *mut c_char,
			n_args: usize,
			start_from: u32,
		) -> ExitStatus {
			// Insert the function body specified in the input function
			#block
		}
	};

	output.into()
}

#[proc_macro]
pub fn engine_runner(input: TokenStream) -> TokenStream {
	let input_str = input.to_string();
	let mut versions = input_str
		.split(',')
		.map(|s| s.trim().trim_matches('"'))
		.map(|i| i.replace('.', "_"));

	let old_version = versions.next().expect("should be two versions provided");
	let new_version = versions.next().expect("should be two versions provided");

	let old_func_name = format!("cfe_entrypoint_v{}", old_version);
	let new_func_name = format!("cfe_entrypoint_v{}", new_version);

	let old_version_fn_ident = syn::Ident::new(&old_func_name, Span::call_site());
	let new_version_fn_ident = syn::Ident::new(&new_func_name, Span::call_site());

	let old_dylib_name = format!("chainflip_engine_v{}", old_version);
	let new_dylib_name = format!("chainflip_engine_v{}", new_version);

	let output = quote! {
		// Define the entrypoints into each version of the engine
		#[link(name = #old_dylib_name)]
		extern "C" {
			fn #old_version_fn_ident(args: *mut *mut engine_upgrade_utils::c_char, n_args: usize, start_from: u32) -> engine_upgrade_utils::ExitStatus;
		}

		#[link(name = #new_dylib_name)]
		extern "C" {
			fn #new_version_fn_ident(args: *mut *mut engine_upgrade_utils::c_char, n_args: usize, start_from: u32) -> engine_upgrade_utils::ExitStatus;
		}

		// Define the runner function.
		// 1. Run the new version first - this is so the new version can provide settings that are backwards compatible with the old settings.
		// 2. If the new version is not yet compatible, run the old version. If it's no longer compatible, then this runner is too old and needs to be updated.
		// 3. If the old version is no longer compatible, run the new version, as we've just done an upgrade, making the new version copmatible now.
		// 4. If this new version completes, then we're done. The engine should be upgraded before this is the case.
		fn main() {
			println!("Starting engine runner...");
			let env_args = std::env::args().collect::<Vec<String>>();
			let (c_args, n) = engine_upgrade_utils::string_args_to_c_args(env_args);

			let old_version = #old_version;
			let new_version = #new_version;

			// Attempt to run the new version first
			let exit_status_new_first = unsafe { #new_version_fn_ident(c_args, n, engine_upgrade_utils::NO_START_FROM) };
			println!("The new version has exited with exit status: {:?}", exit_status_new_first);

			match exit_status_new_first.status_code {
				engine_upgrade_utils::NO_LONGER_COMPATIBLE => {
					println!("You need to update your CFE. The current version of the CFE you are running is not compatible with the latest runtime update.");
				},
				engine_upgrade_utils::NOT_YET_COMPATIBLE => {
					// The new version is not compatible yet, so run the old version
					println!("The latest version {new_version} is not yet compatible. Running the old version {old_version}...");
					let exit_status_old = unsafe { #old_version_fn_ident(c_args, n, engine_upgrade_utils::NO_START_FROM) };

					println!("Old version has exited with exit status: {:?}", exit_status_old);

					// Check if we need to switch back to the new version
					if exit_status_old.status_code == engine_upgrade_utils::NO_LONGER_COMPATIBLE {
						println!("Switching to the new version {new_version} after the old version {old_version} is no longer compatible.");
						// Attempt to run the new version again
						let exit_status_new = unsafe { #new_version_fn_ident(c_args, n, exit_status_old.at_block) };
						println!("New version has exited with exit status: {:?}", exit_status_new);
					} else {
						println!("An error has occurred running the old version with exit status: {:?}", exit_status_old);
					}
				},
				_ => {
					println!("An error has occurred running the new version on first run with exit status: {:?}", exit_status_new_first);
				}
			}

			engine_upgrade_utils::free_c_args(c_args, n);
		}
	};

	TokenStream::from(output)
}
