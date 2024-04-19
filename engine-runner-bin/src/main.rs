use engine_upgrade_utils::{CStrArray, NEW_VERSION, OLD_VERSION};

// Declare the entrypoints into each version of the engine
mod old {
	#[engine_proc_macros::link_engine_library_version("1.3.3")]
	extern "C" {
		pub fn cfe_entrypoint(
			c_args: engine_upgrade_utils::CStrArray,
			start_from: u32,
		) -> engine_upgrade_utils::ExitStatus;
	}
}

mod new {
	#[engine_proc_macros::link_engine_library_version("1.4.0")]
	extern "C" {
		fn cfe_entrypoint(
			c_args: engine_upgrade_utils::CStrArray,
			start_from: u32,
		) -> engine_upgrade_utils::ExitStatus;
	}
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
			let compatible_args = engine_upgrade_utils::args_compatible_with_old(env_args);
			let exit_status_old = old::cfe_entrypoint(
				CStrArray::from_rust_strings(&compatible_args)?,
				engine_upgrade_utils::NO_START_FROM,
			);

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
