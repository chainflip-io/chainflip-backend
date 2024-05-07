use engine_upgrade_utils::{build_helpers::toml_with_package_version, NEW_VERSION};

fn main() {
	substrate_build_script_utils::generate_cargo_keys();

	let (_, version) = toml_with_package_version();

	assert_eq!(version, NEW_VERSION);
}
