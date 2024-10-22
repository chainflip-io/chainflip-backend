#![feature(path_add_extension)]
use std::{
	env,
	error::Error,
	fs::File,
	io::BufWriter,
	path::{Path, PathBuf},
	process::{Command, Stdio},
};

use engine_upgrade_utils::{
	build_helpers::toml_with_package_version, ENGINE_LIB_PREFIX, NEW_VERSION, OLD_VERSION,
};
use reqwest::blocking::get;

fn download_file(download_url: String, dest: PathBuf) -> Result<(), Box<dyn Error>> {
	let mut response: reqwest::blocking::Response = get(&download_url)?;

	if response.status().is_success() {
		let mut dest: BufWriter<File> = BufWriter::new(File::create(dest)?);
		response.copy_to(&mut dest)?;
		Ok(())
	} else {
		Err(Box::from(format!("Failed to download from {download_url}: {}", response.status())))
	}
}

fn download_old_dylib(dest_folder: &Path, is_mainnet: bool) -> Result<(), Box<dyn Error>> {
	let target: String = env::var("TARGET").unwrap();

	let prebuilt_supported =
		target.contains("aarch64-apple-darwin") || target.contains("x86_64-unknown-linux-gnu");

	let shared_lib_ext = if target.contains("apple") { "dylib" } else { "so" };

	let underscored_version = OLD_VERSION.replace('.', "_");
	let dylib_name = format!("libchainflip_engine_v{underscored_version}.{shared_lib_ext}");

	let dylib_location = dest_folder.join(&dylib_name);

	// If prebuilt is supported we download every time. This is to ensure that if we have retagged,
	// or added another commit on top then we get the latest build artifacts for a particular
	// version.
	if prebuilt_supported {
		let download_dylib = format!(
			"https://{}.chainflip.io/{OLD_VERSION}/{dylib_name}",
			if is_mainnet {
				println!("Downloading from pkgs...");
				"pkgs"
			} else {
				println!("Downloading from artifacts...");
				"artifacts"
			}
		);

		std::fs::create_dir_all(dest_folder)?;
		download_file(download_dylib.clone(), dylib_location.clone())?;

		// We want to download the sig file and verify the downloaded dylib only for mainnet.
		if is_mainnet {
			let mut dylib_sig_location = dylib_location.clone();
			dylib_sig_location.add_extension("sig");

			download_file(format!("{download_dylib}.sig"), dylib_sig_location.clone())?;
			gpg_verify_signature(dylib_location, dylib_sig_location)?;
		}

		Ok(())
	} else if dylib_location.exists() {
		// They've already been built and moved to the correct folder, so we can continue the
		// build.
		Ok(())
	} else {
		Err(Box::from(format!(
				"Unsupported target {target} for downloading prebuilt shared libraries. You need to build from source and insert the shared libs into the target/debug or target/release folder.",
			)))
	}
}

// This is using the local gpg keystore.
fn gpg_verify_signature(
	dylib_location: PathBuf,
	dylib_sig_location: PathBuf,
) -> Result<(), Box<dyn Error>> {
	if Command::new("gpg")
		.arg("--verify")
		.arg(dylib_sig_location)
		.arg(dylib_location)
		.stdout(Stdio::inherit())
		.stderr(Stdio::inherit())
		.status()?
		.success()
	{
		Ok(())
	} else {
		Err(Box::from(format!("Failed to verify gpg signature")))
	}
}

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

	download_old_dylib(
		build_dir,
		match env::var("IS_MAINNET") {
			Ok(val) => val.to_lowercase() == "true",
			Err(_) => false, // Default to false
		},
	)
	.unwrap();

	let build_dir_str = build_dir.to_str().unwrap();

	println!("cargo:rustc-link-search=native={build_dir_str}");

	let old_version_suffix = OLD_VERSION.replace('.', "_");
	let new_version_suffix = NEW_VERSION.replace('.', "_");

	println!("cargo:rustc-link-lib=dylib={}{}", ENGINE_LIB_PREFIX, old_version_suffix);
	println!("cargo:rustc-link-lib=dylib={}{}", ENGINE_LIB_PREFIX, new_version_suffix);

	if env::var("TARGET").unwrap().contains("apple") {
		// === For local testing on Mac ===
		// The new dylib is in the same directory as the binary.
		println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
	} else {
		// === For local testing on Linux ===
		// The new dylib is in the same directory as the binary.
		println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");

		// === For releasing ===
		// This path is where we store the libraries in the docker image, and as part of the apt
		// installation.
		println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/chainflip-engine");
		// For docker
		println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/local/lib");
	}

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
