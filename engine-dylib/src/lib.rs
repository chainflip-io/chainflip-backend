use chainflip_engine::settings_and_run_main;
use engine_proc_macros::cfe_entrypoint;
use engine_upgrade_utils::{CStrArray, ExitStatus};

// The `cfe_entrypoint` macro adds the required C parameters to the function signature
#[cfe_entrypoint]
fn cfe_entrypoint() {
	settings_and_run_main(c_args.to_rust_strings(), start_from)
}
