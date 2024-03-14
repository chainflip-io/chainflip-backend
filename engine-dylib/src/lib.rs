use chainflip_engine::settings_and_run_main;
use engine_proc_macros::cfe_entrypoint;
use engine_upgrade_utils::{c_char, ExitStatus};

// The `cfe_entrypoint` macro adds the required C parameters to the function signature
#[cfe_entrypoint]
fn cfe_entrypoint() {
	let rust_string_args = engine_upgrade_utils::rust_string_args(args, n_args);

	settings_and_run_main(rust_string_args, start_from)
}
