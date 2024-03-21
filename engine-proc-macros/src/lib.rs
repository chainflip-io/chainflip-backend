extern crate proc_macro;
extern crate proc_macro2;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

use engine_upgrade_utils::{NEW_VERSION, OLD_VERSION};

/// Generates a C compatible entrypoint namespacing with the version suffix so that the no_mangle
/// names do not conflict when two shared libraries are used in the same process.
#[proc_macro_attribute]
pub fn cfe_entrypoint(_attrs: TokenStream, item: TokenStream) -> TokenStream {
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
			args: *mut *mut engine_upgrade_utils::c_char,
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
pub fn engine_runner(_input: TokenStream) -> TokenStream {
	let mut versions = [OLD_VERSION, NEW_VERSION]
		.iter()
		.map(|i| i.replace('.', "_"))
		.map(|version| {
			(syn::Ident::new(&format!("cfe_entrypoint_v{}", version), Span::call_site()), version)
		})
		.map(|(ident, version)| (ident, format!("chainflip_engine_v{}", version), version));

	let (old_version_fn_ident, old_dylib_name, old_version) =
		versions.next().expect("should be two versions provided");

	let (new_version_fn_ident, new_dylib_name, new_version) =
		versions.next().expect("should be two versions provided");

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
		fn main() -> anyhow::Result<()>{
			println!("Starting engine runner...");
			let env_args = std::env::args().collect::<Vec<String>>();

			let mut c_str_array = engine_upgrade_utils::CStrArray::default();
			c_str_array.string_args_to_c_args(env_args.clone())?;
			let (c_args, n) = c_str_array.get_args();

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
					let mut old_c_str_args = engine_upgrade_utils::CStrArray::default();
					let compatible_args = engine_upgrade_utils::args_compatible_with_old(env_args);
					old_c_str_args.string_args_to_c_args(compatible_args)?;
					let (old_c_args, old_n) = old_c_str_args.get_args();
					let exit_status_old = unsafe { #old_version_fn_ident(old_c_args, old_n, engine_upgrade_utils::NO_START_FROM) };

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
			Ok(())
		}
	};

	TokenStream::from(output)
}
