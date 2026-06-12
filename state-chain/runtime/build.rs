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
		std::env::set_var("SKIP_STATE_CHAIN_RUNTIME_WASM_BUILD", "1");
	}

	#[cfg(feature = "std")]
	{
		let builder = substrate_wasm_builder::WasmBuilder::new()
			.with_current_project()
			.export_heap_base()
			.import_memory();

		#[cfg(not(feature = "metadata-hash"))]
		builder.build();

		#[cfg(feature = "metadata-hash")]
		builder.enable_metadata_hash("FLIP", 18).build();
	}
}
