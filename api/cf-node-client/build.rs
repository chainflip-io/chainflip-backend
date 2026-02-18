use std::{env, fs, path::Path};

fn main() {
	let out_dir = env::var_os("OUT_DIR").unwrap();
	let wasm_path = Path::new(&out_dir)
		.parent()
		.unwrap()
		.parent()
		.unwrap()
		.parent()
		.unwrap() // target/debug or target/release
		.join("wbuild/state-chain-runtime/state_chain_runtime.wasm");

	// Write out the expression to generate the subxt macro to a file. Since we must pass
	// a string literal to the subxt macro `runtime_path` arg, we need to write it out here and
	// include it verbatim instead.
	let cf_static_runtime_content = format!(
		r#"
		#[subxt::subxt(
			runtime_path = "{}",
			substitute_type(
				path = "primitive_types::U256",
				with = "::subxt::utils::Static<sp_core::U256>"
			),
			substitute_type(
				path = "cf_chains::address::EncodedAddress",
				with = "::subxt::utils::Static<cf_chains::address::EncodedAddress>"
			),
			substitute_type(
				path = "cf_primitives::chains::assets::any::Asset",
				with = "::subxt::utils::Static<cf_primitives::chains::assets::any::Asset>"
			),
			substitute_type(
				path = "cf_primitives::chains::ForeignChain",
				with = "::subxt::utils::Static<cf_primitives::chains::ForeignChain>"
			),
			substitute_type(
				path = "cf_primitives::SwapRequestId",
				with = "::subxt::utils::Static<cf_primitives::SwapRequestId>"
			),
			substitute_type(
				path = "cf_amm::common::Side",
				with = "::subxt::utils::Static<cf_amm::common::Side>"
			),
		)]
		pub mod cf_static_runtime {{}}
	"#,
		wasm_path.to_str().expect("Path to wasm should be stringifiable")
	);
	let cf_static_runtime_path = Path::new(&out_dir).join("cf_static_runtime.rs");
	fs::write(cf_static_runtime_path, cf_static_runtime_content)
		.expect("Couldn't write cf_static_runtime.rs");

	// Re-build if this file changes:
	println!("cargo:rerun-if-changed=build.rs");
}
