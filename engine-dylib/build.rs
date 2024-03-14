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
		.unwrap()
		.replace('.', "_");

	let lib_name = cargo_toml.get("lib").and_then(|l| l.get("name")).expect("Should be a lib");
	let lib_name = lib_name.as_str().unwrap();
	assert_eq!(lib_name, format!("chainflip_engine_v{}", version), "lib name should match version");
}
