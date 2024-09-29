use assert_cmd::Command;
use engine_upgrade_utils::NEW_VERSION;

fn assert_command_arg_for_latest_version(arg: &str) {
	Command::cargo_bin("engine-runner")
		.unwrap()
		.arg(arg)
		.assert()
		.success()
		.stdout(predicates::str::contains(format!("chainflip-engine {NEW_VERSION}")));
}

#[test]
fn version_should_return_for_latest_version() {
	assert_command_arg_for_latest_version("--version");
	assert_command_arg_for_latest_version("-V");
}

#[test]
fn help_should_return_for_latest_version() {
	assert_command_arg_for_latest_version("--help");
	assert_command_arg_for_latest_version("-h");
}
