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

fn main() {
	#[cfg(all(feature = "std", any(feature = "proptest", feature = "runtime-integration-tests")))]
	{
		std::fs::write(
			std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo"))
				.join("wasm_binary.rs"),
			"pub const WASM_BINARY_PATH: Option<&str> = None;\
			 pub const WASM_BINARY: Option<&[u8]> = None;\
			 pub const WASM_BINARY_BLOATY: Option<&[u8]> = None;",
		)
		.expect("can write dummy wasm_binary.rs");
	}

	#[cfg(all(
		feature = "std",
		not(feature = "metadata-hash"),
		not(feature = "proptest"),
		not(feature = "runtime-integration-tests")
	))]
	{
		substrate_wasm_builder::WasmBuilder::new()
			.with_current_project()
			.export_heap_base()
			.import_memory()
			.build();
	}

	#[cfg(all(
		feature = "std",
		feature = "metadata-hash",
		not(feature = "proptest"),
		not(feature = "runtime-integration-tests")
	))]
	{
		substrate_wasm_builder::WasmBuilder::new()
			.with_current_project()
			.export_heap_base()
			.import_memory()
			.enable_metadata_hash("FLIP", 18)
			.build();
	}
}
