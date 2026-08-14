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
use std::{env, fs, path::PathBuf};

fn main() {
	let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
	let metadata_path = out_dir.join("state_chain_metadata.scale");
	let metadata = state_chain_runtime::Runtime::metadata_at_version(15)
		.expect("Metadata V15 should be supported by the runtime");
	fs::write(&metadata_path, metadata.as_slice()).expect("Couldn't write runtime metadata");

	// Write out the expression to generate the subxt macro to a file. Since we must pass
	// a string literal to the subxt macro `runtime_metadata_path` arg, we need to write it out here
	// and include it verbatim instead.
	let cf_static_runtime_content = format!(
		r#"
		#[subxt::subxt(
			runtime_metadata_path = "{}",
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
		metadata_path.to_str().expect("Path to metadata should be stringifiable")
	);
	let cf_static_runtime_path = out_dir.join("cf_static_runtime.rs");
	fs::write(cf_static_runtime_path, cf_static_runtime_content)
		.expect("Couldn't write cf_static_runtime.rs");

	// Re-build if this file changes:
	println!("cargo:rerun-if-changed=build.rs");
}
