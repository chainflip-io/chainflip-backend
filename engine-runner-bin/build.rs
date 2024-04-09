use engine_upgrade_utils::{
	build_helpers::toml_with_package_version, ENGINE_LIB_PREFIX, NEW_VERSION, OLD_VERSION,
};

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

	let (cargo_toml, package_version) = toml_with_package_version();

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

	let mut check_version_suffix = |suffix: &String| {
		assert!(
			flat_deb_assets.any(|item| item.contains(suffix)),
			"Expected to find a deb asset with the version suffix: {}",
			suffix
		);
	};

	check_version_suffix(&new_version_suffix);
	check_version_suffix(&old_version_suffix);
}
