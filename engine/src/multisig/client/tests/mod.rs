mod ceremony_manager_tests;
mod frost_unit_tests;
mod helpers;
mod keygen_data_tests;
mod keygen_unit_tests;
mod multisig_client_tests;

pub use helpers::{
    gen_invalid_local_sig, gen_invalid_signing_comm1, new_nodes,
    run_keygen_with_err_on_high_pubkey, standard_signing, KeygenCeremonyRunner,
    SigningCeremonyRunner,
};

use lazy_static::lazy_static;

use state_chain_runtime::AccountId;

pub const KEYGEN_STAGES: usize = 9;
pub const SIGNING_STAGES: usize = 4;

/// Default seeds
pub const DEFAULT_KEYGEN_SEED: [u8; 32] = [8; 32];
pub const DEFAULT_SIGNING_SEED: [u8; 32] = [4; 32];

/// Default ceremony ids
/// We start at id 1 because the latests ceremony id starts at 0 for tests (making 0 invalid),
/// Also the SC will never send a ceremony request at id 0.
pub const INITIAL_LATEST_CEREMONY_ID: u64 = 0;
pub const DEFAULT_KEYGEN_CEREMONY_ID: u64 = 1;
pub const DEFAULT_SIGNING_CEREMONY_ID: u64 = 2;

lazy_static! {
    static ref ACCOUNT_IDS: Vec<AccountId> = [1, 2, 3, 4]
        .iter()
        .map(|i| AccountId::new([*i; 32]))
        .collect();
}
