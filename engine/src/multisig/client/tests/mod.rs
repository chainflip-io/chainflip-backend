mod ceremony_manager_tests;
mod frost_unit_tests;
mod helpers;
mod keygen_data_tests;
mod keygen_unit_tests;
mod multisig_client_tests;

use cf_primitives::CeremonyId;
pub use helpers::{
	cause_ceremony_timeout, gen_invalid_keygen_stage_2_state, gen_invalid_local_sig,
	gen_invalid_signing_comm1, get_key_data_for_test, new_nodes,
	run_keygen_with_err_on_high_pubkey, standard_signing, KeygenCeremonyRunner,
	SigningCeremonyRunner,
};

pub use keygen_data_tests::gen_keygen_data_verify_hash_comm2;

use lazy_static::lazy_static;

use state_chain_runtime::AccountId;

/// Default seeds
pub const DEFAULT_KEYGEN_SEED: [u8; 32] = [8; 32];
pub const DEFAULT_SIGNING_SEED: [u8; 32] = [4; 32];

// Default ceremony ids used in many unit tests.
/// The initial latest ceremony id starts at 0,
/// so the first ceremony request must have a ceremony id of 1.
/// Also the SC will never send a ceremony request at id 0.
pub const INITIAL_LATEST_CEREMONY_ID: CeremonyId = 0;
// Ceremony ids must be consecutive.
pub const DEFAULT_KEYGEN_CEREMONY_ID: CeremonyId = INITIAL_LATEST_CEREMONY_ID + 1;
pub const DEFAULT_SIGNING_CEREMONY_ID: CeremonyId = DEFAULT_KEYGEN_CEREMONY_ID + 1;

lazy_static! {
	pub static ref ACCOUNT_IDS: Vec<AccountId> =
		[1, 2, 3, 4].iter().map(|i| AccountId::new([*i; 32])).collect();
}
