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

use std::{fs, path::Path};

pub fn toml_with_package_version() -> (toml::Value, String) {
	let manifest_path = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("Cargo.toml");
	let manifest_contents = fs::read_to_string(manifest_path).expect("Could not read Cargo.toml");

	let cargo_toml: toml::Value =
		toml::from_str(&manifest_contents).expect("Could not parse Cargo.toml");

	// Get the version from the Cargo.toml
	let package_version = cargo_toml
		.get("package")
		.and_then(|p| p.get("version"))
		.unwrap()
		.as_str()
		.unwrap()
		.to_string();

	(cargo_toml, package_version)
}
