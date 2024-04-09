use std::{fs, path::Path};

use engine_upgrade_utils::{NEW_VERSION, OLD_VERSION, ENGINE_LIB_PREFIX};

fn main() {
	// === Ensure the runner runs the linker checks at compile time ===

	let out_dir = std::env::var("OUT_DIR").unwrap();

	let build_dir = std::path::Path::new(&out_dir)
		.parent()
		.unwrap()
		.parent()
		.unwrap()
		.parent()
		.unwrap(); // target/debug or target/release

	// ./old-engine-dylib from project root.
	let old_version = build_dir.parent().unwrap().parent().unwrap().join("old-engine-dylib");

	let old_version_str = old_version.to_str().unwrap();

	let build_dir_str = build_dir.to_str().unwrap(); // target/debug or target/release

	println!("cargo:rustc-link-search=native={old_version_str}");
	println!("cargo:rustc-link-search=native={build_dir_str}");

	let old_version_suffix = OLD_VERSION.replace('.', "_");
	let new_version_suffix = NEW_VERSION.replace('.', "_");

	println!("cargo:rustc-link-lib=dylib={}{}", ENGINE_LIB_PREFIX, old_version_suffix);
	println!("cargo:rustc-link-lib=dylib={}{}", ENGINE_LIB_PREFIX, new_version_suffix);

	// ===  Sanity check that the the assets have an item with the matching version. ===

	let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
	let manifest_contents = fs::read_to_string(manifest_path).expect("Could not read Cargo.toml");

	let cargo_toml: toml::Value =
		toml::from_str(&manifest_contents).expect("Could not parse Cargo.toml");

	// Get the version from the Cargo.toml
	let package_version = cargo_toml
		.get("package")
		.and_then(|p| p.get("version"))
		.unwrap()
		.as_str()
		.unwrap();

	assert_eq!(package_version, NEW_VERSION);

	let deb_assets: Vec<Vec<String>> = cargo_toml
		.get("package")
		.unwrap()
		.get("metadata")
		.unwrap()
		.get("deb")
		.unwrap()
		.get("assets")
		.unwrap()
		.clone()
		.try_into()
		.unwrap();

	let mut flat_deb_assets = deb_assets.iter().flatten();
	assert!(flat_deb_assets.any(|item| item.contains(&new_version_suffix)));
	assert!(flat_deb_assets.any(|item| item.contains(&old_version_suffix)));
}
