#![feature(path_add_extension)]
use std::{
	env,
	error::Error,
	fs::File,
	io::BufWriter,
	path::{Path, PathBuf},
	str::FromStr,
};

use sequoia_openpgp::{
	cert::Cert,
	parse::{stream::*, Parse},
	policy::StandardPolicy,
	KeyHandle,
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

// https://keys.openpgp.org/vks/v1/by-keyid/4E506212E4EF4E0D3E37E568596FBDCACBBCDD37

fn fetch_public_key(key_id: &KeyHandle) -> sequoia_openpgp::Result<Cert> {
	let key_server_url = format!("https://keys.openpgp.org/vks/v1/by-keyid/{}", key_id);

	let response = get(&key_server_url)?;

	if !response.status().is_success() {
		panic!("failed");
		// return Err("Failed to fetch public key".into());
	}

	// The response should be the ASCII-armored public key
	let key_data = response.text().expect("Failed to read response text");

	// Parse the public key
	let cert = Cert::from_str(&key_data)?;

	Ok(cert)
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
		let root_url = if is_mainnet {
			println!("Downloading from pkgs...");
			format!("https://pkgs.chainflip.io/")
		} else {
			println!("Downloading from artifacts...");
			format!("https://artifacts.chainflip.io/")
		};
		let download_dylib = format!("{root_url}{OLD_VERSION}/{dylib_name}");

		std::fs::create_dir_all(dest_folder)?;
		download_file(download_dylib.clone(), dylib_location.clone())?;

		// We want to download the sig file and verify the downloaded dylib only for mainnet.
		if is_mainnet {
			let mut dylib_sig_location = dylib_location.clone();
			dylib_sig_location.add_extension("sig");
			download_file(format!("{download_dylib}.sig"), dylib_sig_location.clone())?;
			pgp_verify_signature(dylib_location, dylib_sig_location)?;
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

struct Helper {}
impl VerificationHelper for Helper {
	fn get_certs(&mut self, _ids: &[KeyHandle]) -> sequoia_openpgp::Result<Vec<Cert>> {
		let cert =
			fetch_public_key(&KeyHandle::from_str("4E506212E4EF4E0D3E37E568596FBDCACBBCDD37")?)?;
		Ok(vec![cert])
	}
	fn check(&mut self, _structure: MessageStructure) -> sequoia_openpgp::Result<()> {
		Ok(())
	}
}

// use: https://github.com/rpgp/rpgp instead - simpler
fn pgp_verify_signature(
	dylib_location: PathBuf,
	dylib_sig_location: PathBuf,
) -> Result<(), Box<dyn Error>> {
	let standard_policy = StandardPolicy::new();
	let mut verifier = DetachedVerifierBuilder::from_file(dylib_sig_location)
		.unwrap()
		.with_policy(&standard_policy, None, Helper {})
		.unwrap();

	Ok(verifier.verify_file(dylib_location)?)
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

	let is_mainnet = match env::var("IS_MAINNET") {
		Ok(val) => val.to_lowercase() == "true",
		Err(_) => false, // Default to false
	};

	// panic!("Is mainnet: {}", is_mainnet);

	download_old_dylib(build_dir, is_mainnet).unwrap();

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
