use std::{fs, path::Path};

// We want to enforce the fact that the package version, and the version suffix on the dylib
// matches at compile time.
// e.g. if version is `1.4.0` then the dylib lib name should be: `chainflip_engine_v1_4_0`
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
		.unwrap()
		.replace('.', "_");

	let lib_name = cargo_toml.get("lib").and_then(|l| l.get("name")).expect("Should be a lib");
	let lib_name = lib_name.as_str().unwrap();
	assert_eq!(
		lib_name,
		format!("chainflip_engine_v{}", version),
		"lib name version suffix should match package version"
	);
}
