fn main() {
	let out_dir = std::env::var("OUT_DIR").unwrap();

	let path = std::path::Path::new(&out_dir)
		.parent()
		.unwrap()
		.parent()
		.unwrap()
		.parent()
		.unwrap()
		.to_str()
		.unwrap(); // target/debug or target/release

	println!("cargo:rustc-link-search=native=oldVersion");
	println!("cargo:rustc-link-search=native={path}");

	println!("cargo:rustc-link-lib=dylib=chainflip_engine_v1_3_0");
	println!("cargo:rustc-link-lib=dylib=chainflip_engine_v1_4_0");
}
