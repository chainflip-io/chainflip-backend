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
