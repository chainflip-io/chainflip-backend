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

use engine_upgrade_utils::{
	build_helpers::toml_with_package_version, ENGINE_LIB_PREFIX, NEW_VERSION,
};

// We want to enforce the fact that the package version, and the version suffix on the dylib
// matches at compile time.
// e.g. if version is `1.4.0` then the dylib lib name should be: `chainflip_engine_v1_4_0`
fn main() {
	let (cargo_toml, version) = toml_with_package_version();

	assert_eq!(version, NEW_VERSION);

	let version_suffix = version.replace('.', "_");

	let lib_name = cargo_toml.get("lib").and_then(|l| l.get("name")).expect("Should be a lib");
	let lib_name = lib_name.as_str().unwrap();
	assert_eq!(
		lib_name,
		format!("{}{}", ENGINE_LIB_PREFIX, version_suffix),
		"lib name version suffix should match package version"
	);
}
