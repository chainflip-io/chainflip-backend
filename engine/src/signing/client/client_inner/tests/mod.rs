mod db_tests;
mod frost_unit_tests;
mod helpers;
// mod keygen_unit_tests;

// pub use helpers::KeygenPhase1Data;

use lazy_static::lazy_static;
#[allow(unused_imports)]
use log::*;

// use helpers::*;

use crate::{
    p2p::AccountId,
    signing::{
        client::{KeyId, KeygenInfo},
        MessageHash,
    },
};

use std::convert::TryInto;

lazy_static! {
    static ref VALIDATOR_IDS: Vec<AccountId> =
        [1, 2, 3, 4].iter().map(|i| AccountId([*i; 32])).collect();
    static ref SIGNER_IDXS: Vec<usize> = vec![0, 1, 2];
    static ref SIGNER_IDS: Vec<AccountId> = SIGNER_IDXS
        .iter()
        .map(|idx| VALIDATOR_IDS[*idx].clone())
        .collect();
    static ref UNEXPECTED_VALIDATOR_ID: AccountId = AccountId(
        "unexpected|unexpected|unexpected"
            .as_bytes()
            .try_into()
            .unwrap()
    );
}

lazy_static! {
    // MAXIM: This should be removed in favor of SIGN_CEREMONY_ID
    static ref CEREMONY_ID: u64 = 0;
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
        ceremony_id: *CEREMONY_ID,
        signers: VALIDATOR_IDS.clone()
    };
}

// INFO: We should be able to continue signing with the old key. When key rotation happens,
// we need to create a new key. A node is likely to remain a validator, so it needs to be
// able to transfer funds from the old key to the new one. SC will send us a command to
// generate a new key for epoch X (and attempt number?). Requests to sign should also
// contain the epoch.

// TODO (unit tests):
// [Signing]
// - Delayed data expires on timeout
// - Parties cannot send two messages for the same phase
// - make sure that we don't process p2p data at index signer_id which is our own
// - test that we emit events that allow for penalisation of offending nodes to occur
// [Keygen]
// - Parties cannot send two messages for the same phase
// - make sure that we don't process p2p data at index signer_id which is our own
// - test that we emit events that allow for penalisation of offending nodes to occur
