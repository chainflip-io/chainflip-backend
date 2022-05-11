mod frost_unit_tests;
mod helpers;
mod keygen_unit_tests;
mod multisig_client_tests;

pub use helpers::{
    new_nodes, run_keygen_with_err_on_high_pubkey, standard_signing, KeygenCeremonyRunner,
    SigningCeremonyRunner,
};

use lazy_static::lazy_static;

use state_chain_runtime::AccountId;

pub const KEYGEN_STAGES: usize = 9;
pub const SIGNING_STAGES: usize = 4;
pub const STAGE_FINISHED_OR_NOT_STARTED: usize = 0;

/// Default seeds
pub const DEFAULT_KEYGEN_SEED: [u8; 32] = [8; 32];
pub const DEFAULT_SIGNING_SEED: [u8; 32] = [4; 32];

/// Default ceremony ids
pub const DEFAULT_KEYGEN_CEREMONY_ID: u64 = 1;
pub const DEFAULT_SIGNING_CEREMONY_ID: u64 = 2;

lazy_static! {
    static ref ACCOUNT_IDS: Vec<AccountId> = [1, 2, 3, 4]
        .iter()
        .map(|i| AccountId::new([*i; 32]))
        .collect();
}
