use std::{fs, path::Path};

use engine_upgrade_utils::NEW_VERSION;

fn main() {
	substrate_build_script_utils::generate_cargo_keys();

	// Ensure the version in the Cargo.toml matches the version set in the engine_upgrade_utils.
	// This is important for the engine, because it uses this version to check for compatibility
	// with the SC. It reads the version from its toml.
	let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
	let manifest_contents = fs::read_to_string(manifest_path).expect("Could not read Cargo.toml");

	let cargo_toml: toml::Value =
		toml::from_str(&manifest_contents).expect("Could not parse Cargo.toml");

	// Get the version from the Cargo.toml
	let version = cargo_toml
		.get("package")
		.and_then(|p| p.get("version"))
		.unwrap()
		.as_str()
		.unwrap();

	assert_eq!(version, NEW_VERSION);
}
