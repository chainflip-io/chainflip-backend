use engine_upgrade_utils::{CStrArray, NEW_VERSION, OLD_VERSION};

// Declare the entrypoints into each version of the engine
mod old {
	#[engine_proc_macros::link_engine_library_version("1.6.0")]
	extern "C" {
		pub fn cfe_entrypoint(
			c_args: engine_upgrade_utils::CStrArray,
			start_from: u32,
		) -> engine_upgrade_utils::ExitStatus;
	}
}

mod new {
	#[engine_proc_macros::link_engine_library_version("1.7.0")]
	extern "C" {
		fn cfe_entrypoint(
			c_args: engine_upgrade_utils::CStrArray,
			start_from: u32,
		) -> engine_upgrade_utils::ExitStatus;
	}
}

fn filter_args_and_run_old(args: Vec<String>) -> anyhow::Result<engine_upgrade_utils::ExitStatus> {
	let compatible_args = engine_upgrade_utils::args_compatible_with_old(args);
	let c_str_array = CStrArray::from_rust_strings(&compatible_args)?;
	Ok(old::cfe_entrypoint(c_str_array, engine_upgrade_utils::NO_START_FROM))
}

// Define the runner function.
// 1. Run the new version first - this is so the new version can provide settings that are backwards
//    compatible with the old settings.
// 2. If the new version is not yet compatible, run the old version. If it's no longer compatible,
//    then this runner is too old and needs to be updated.
// 3. If the old version is no longer compatible, run the new version, as we've just done an
//    upgrade, making the new version copmatible now.
// 4. If this new version completes, then we're done. The engine should be upgraded before this is
//    the case.
fn main() -> anyhow::Result<()> {
	let env_args = std::env::args().collect::<Vec<String>>();

	let c_str_array = CStrArray::from_rust_strings(&env_args)?;

	// Attempt to run the new version first
	let exit_status_new_first =
		new::cfe_entrypoint(c_str_array.clone(), engine_upgrade_utils::NO_START_FROM);
	println!("The new version has exited with exit status: {:?}", exit_status_new_first);

	match exit_status_new_first.status_code {
		engine_upgrade_utils::NO_LONGER_COMPATIBLE => {
			println!("You need to update your CFE. The current version of the CFE you are running is not compatible with the latest runtime update.");
		},
		engine_upgrade_utils::NOT_YET_COMPATIBLE => {
			// The new version is not compatible yet, so run the old version
			println!("The latest version {NEW_VERSION} is not yet compatible. Running the old version {OLD_VERSION}...");

			let exit_status_old = filter_args_and_run_old(env_args)?;

			println!("Old version has exited with exit status: {:?}", exit_status_old);

			// Check if we need to switch back to the new version
			if exit_status_old.status_code == engine_upgrade_utils::NO_LONGER_COMPATIBLE {
				println!("Switching to the new version {NEW_VERSION} after the old version {OLD_VERSION} is no longer compatible.");
				// Attempt to run the new version again
				let exit_status_new = new::cfe_entrypoint(c_str_array, exit_status_old.at_block);
				println!("New version has exited with exit status: {:?}", exit_status_new);
			} else {
				println!(
					"An error has occurred running the old version with exit status: {:?}",
					exit_status_old
				);
			}
		},
		_ => {
			println!(
				"An error has occurred running the new version on first run with exit status: {:?}",
				exit_status_new_first
			);
		},
	}
	Ok(())
}

#[cfg(test)]
mod tests {

	use engine_upgrade_utils::ERROR_READING_SETTINGS;

	use super::*;

	// This tests:
	// 1. We can run by providing all necessary settings via command line.
	// 2. That args are filtered out correctly before passing them into the old version.
	// 3. That the old version can run with the filtered args. implying that the settings in the new
	//    version are backwards compatible.
	#[test]
	fn incompatible_args_should_be_filtered_out() {
		// create temporary file
		let tempdir = tempfile::TempDir::new().unwrap();
		let config_root = tempdir.path();

		// create settings file
		let settings_file = config_root.join("config/Settings.toml");
		std::fs::create_dir_all(settings_file.parent().unwrap()).unwrap();
		// Create an empty Settings.toml file
		// This is necessarily because if we set a custom config root, the settings file must exist.
		std::fs::write(settings_file, "").unwrap();

		let config_root_str = config_root.to_str().unwrap();

		let some_file = "some_file";
		let some_file_path = tempdir.path().join(some_file);
		// create some_file
		std::fs::write(some_file_path, "").unwrap();

		let rust_args = vec![
			"my-file-name".to_string(),
			format!("--config-root={config_root_str}"),
			// SC
			"--state_chain.ws_endpoint=ws://localhost:3112".to_string(),
			format!("--state_chain.signing_key_file={some_file}"),
			// Eth
			format!("--eth.private_key_file={some_file}"),
			"--eth.rpc.http_endpoint=http://localhost:8545".to_string(),
			"--eth.backup_rpc.http_endpoint=http://localhost:8546".to_string(),
			"--eth.rpc.ws_endpoint=ws://localhost:8545".to_string(),
			"--eth.backup_rpc.ws_endpoint=ws://localhost:8546".to_string(),
			// Arb
			format!("--arb.private_key_file={some_file}"),
			"--arb.rpc.http_endpoint=http://localhost:8545".to_string(),
			"--arb.backup_rpc.http_endpoint=http://localhost:8546".to_string(),
			"--arb.rpc.ws_endpoint=ws://localhost:8545".to_string(),
			"--arb.backup_rpc.ws_endpoint=ws://localhost:8546".to_string(),
			// Dot
			"--dot.rpc.ws_endpoint=ws://localhost:8545".to_string(),
			"--dot.backup_rpc.ws_endpoint=ws://localhost:8546".to_string(),
			"--dot.rpc.http_endpoint=http://localhost:8545".to_string(),
			"--dot.backup_rpc.http_endpoint=http://localhost:8546".to_string(),
			// Btc
			"--btc.rpc.http_endpoint=http://localhost:8545".to_string(),
			"--btc.rpc.basic_auth_user=user".to_string(),
			"--btc.rpc.basic_auth_password=password".to_string(),
			"--btc.backup_rpc.http_endpoint=http://localhost:8546".to_string(),
			"--btc.backup_rpc.basic_auth_user=user".to_string(),
			"--btc.backup_rpc.basic_auth_password=password".to_string(),
			// Sol
			"--sol.rpc.ws_endpoint=ws://localhost:8899".to_string(),
			"--sol.rpc.http_endpoint=http://localhost:8899".to_string(),
			"--sol.backup_rpc.ws_endpoint=ws://localhost:8899".to_string(),
			"--sol.backup_rpc.http_endpoint=http://localhost:8899".to_string(),
			// p2p
			format!("--p2p.node_key_file={some_file}"),
			"--p2p.ip_address=0.1.2.3".to_string(),
			"--p2p.port=1234".to_string(),
			"--p2p.allow_local_ip=true".to_string(),
			// Health
			"--health_check.hostname=localhost".to_string(),
			"--health_check.port=1234".to_string(),
			// prometheus
			"--prometheus.hostname=localhost".to_string(),
			"--prometheus.port=1234".to_string(),
			// signing db
			"--signing.db_file=/some/some_database.db".to_string(),
			// Logging
			"--logging.span_lifecycle".to_string(),
			"--logging.command_server_port=1234".to_string(),
		];

		// There should be no error reading settings error - it will likely error due to no
		// connection, but settings should be ok since all args are provided in command line args.
		assert_ne!(
			new::cfe_entrypoint(
				CStrArray::from_rust_strings(&rust_args).unwrap(),
				engine_upgrade_utils::NO_START_FROM,
			)
			.status_code,
			ERROR_READING_SETTINGS
		);

		// Here the args should be filtered, so the old version should also not error with a
		// settings error.
		assert_ne!(filter_args_and_run_old(rust_args).unwrap().status_code, ERROR_READING_SETTINGS);
	}
}
