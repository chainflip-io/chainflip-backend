use engine_upgrade_utils::NEW_VERSION;
use std::{fs, path::Path};

fn main() {
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
