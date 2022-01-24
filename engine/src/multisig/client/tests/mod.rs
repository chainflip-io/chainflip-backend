mod frost_unit_tests;
mod helpers;
mod keygen_unit_tests;

pub use helpers::{
    new_nodes, run_keygen_with_high_key_failure, standard_signing, KeygenCeremonyRunner,
    SigningCeremonyRunner,
};

use lazy_static::lazy_static;

use crate::multisig::KeygenInfo;

use state_chain_runtime::AccountId;

pub const KEYGEN_STAGES: usize = 7;
pub const SIGNING_STAGES: usize = 4;

lazy_static! {
    static ref ACCOUNT_IDS: Vec<AccountId> = [1, 2, 3, 4]
        .iter()
        .map(|i| AccountId::new([*i; 32]))
        .collect();
}
