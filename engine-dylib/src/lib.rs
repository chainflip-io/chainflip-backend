use chainflip_engine::settings_and_run_main;

use engine_upgrade_utils::{CStrArray, ExitStatus};

#[no_mangle]
extern "C" fn cfe_entrypoint(c_args: CStrArray, start_from: u32) -> ExitStatus {
	settings_and_run_main(c_args.to_rust_strings(), start_from)
}
