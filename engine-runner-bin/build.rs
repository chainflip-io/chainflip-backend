fn main() {
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

	println!("cargo:rustc-link-lib=dylib=chainflip_engine_v1_3_0");
	println!("cargo:rustc-link-lib=dylib=chainflip_engine_v1_4_0");
}
