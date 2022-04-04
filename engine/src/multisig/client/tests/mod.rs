mod db_tests;
mod frost_unit_tests;
mod helpers;
mod keygen_unit_tests;

pub use helpers::{
    new_nodes, run_keygen_with_err_on_high_pubkey, standard_signing, KeygenCeremonyRunner,
    SigningCeremonyRunner,
};

use lazy_static::lazy_static;

use crate::multisig::KeygenInfo;

use state_chain_runtime::AccountId;

pub const KEYGEN_STAGES: usize = 9;
pub const SIGNING_STAGES: usize = 4;
pub const STAGE_FINISHED_OR_NOT_STARTED: usize = 0;

/// Seeds
// Note: Keygen seeds may need to be updated if keygen changes the way it uses the rng.
// A seed that will produce a contract compatible key
pub const COMPATIBLE_KEYGEN_SEED: [u8; 32] = [8; 32];
// A seed that will produce a contract non-compatible key
pub const NON_COMPATIBLE_KEYGEN_SEED: [u8; 32] = [11; 32];
// Default seed used when signing
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
