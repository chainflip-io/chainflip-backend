mod db_tests;
mod helpers;
mod keygen_unit_tests;
mod signing_unit_tests;

pub use helpers::{KeygenContext, KeygenPhase1Data};

use lazy_static::lazy_static;
#[allow(unused_imports)]
use log::*;

use super::client_inner::*;
use helpers::*;

use super::keygen_state::KeygenStage;
use super::signing_state::SigningStage;

use crate::{
    p2p::AccountId,
    signing::{
        client::{KeyId, KeygenInfo, MultisigInstruction, SigningInfo, PHASE_TIMEOUT},
        MessageHash, MessageInfo,
    },
};

use std::convert::TryInto;
use std::time::Duration;

lazy_static! {
    static ref VALIDATOR_IDS: Vec<AccountId> =
        vec![AccountId([1; 32]), AccountId([2; 32]), AccountId([3; 32]),];
    static ref SIGNER_IDXS: Vec<usize> = vec![0, 1];
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
