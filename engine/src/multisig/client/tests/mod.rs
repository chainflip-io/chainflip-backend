mod db_tests;
mod frost_unit_tests;
mod helpers;
mod keygen_unit_tests;

pub use helpers::KeygenContext;

use lazy_static::lazy_static;
#[allow(unused_imports)]
use log::*;
use pallet_cf_vaults::CeremonyId;

use crate::multisig::{KeygenInfo, MessageHash};

use state_chain_runtime::AccountId;

use std::convert::TryInto;

pub const KEYGEN_CEREMONY_ID: CeremonyId = 0;
pub const SIGN_CEREMONY_ID: CeremonyId = 0;
pub const KEYGEN_STAGES: usize = 7;
pub const SIGNING_STAGES: usize = 4;

lazy_static! {

    static ref ACCOUNT_IDS: Vec<AccountId> =
        [1, 2, 3, 4].iter().map(|i| AccountId::new([*i; 32])).collect();
    static ref SIGNER_IDXS: Vec<usize> = vec![0, 1, 2];
    static ref SIGNER_IDS: Vec<AccountId> = SIGNER_IDXS
        .iter()
        .map(|idx| ACCOUNT_IDS[*idx].clone())
        .collect();
    static ref UNEXPECTED_VALIDATOR_ID: AccountId = AccountId::new(
        "unexpected|unexpected|unexpected"
            .as_bytes()
            .try_into()
            .unwrap()
    );
    static ref MESSAGE: [u8; 32] = "Chainflip:Chainflip:Chainflip:01"
        .as_bytes()
        .try_into()
        .unwrap();
    static ref MESSAGE_HASH: MessageHash = MessageHash(MESSAGE.clone());
    /// Just in case we need to test signing two messages
    static ref MESSAGE2: [u8; 32] = "Chainflip:Chainflip:Chainflip:02"
        .as_bytes()
        .try_into()
        .unwrap();
    static ref KEYGEN_INFO: KeygenInfo = KeygenInfo {
        ceremony_id: KEYGEN_CEREMONY_ID,
        signers: ACCOUNT_IDS.clone()
    };
}
