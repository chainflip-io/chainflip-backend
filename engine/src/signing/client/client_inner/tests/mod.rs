mod helpers;
mod keygen_unit_tests;
mod signing_unit_tests;

use lazy_static::lazy_static;
#[allow(unused_imports)]
use log::*;

use super::client_inner::*;
use helpers::*;

use super::keygen_state::KeygenStage;
use super::signing_state::SigningStage;

use crate::{
    p2p::ValidatorId,
    signing::{
        client::{KeyId, KeygenInfo, MultisigInstruction, SigningInfo, PHASE_TIMEOUT},
        crypto::Parameters,
        MessageHash, MessageInfo,
    },
};

use std::convert::TryInto;
use std::{sync::Once, time::Duration};

// The id to be used by default
const KEY_ID: KeyId = KeyId(0);

lazy_static! {
    static ref VALIDATOR_IDS: Vec<ValidatorId> = vec![
        ValidatorId::new(1),
        ValidatorId::new(2),
        ValidatorId::new(3),
    ];
    static ref SIGNER_IDXS: Vec<usize> = vec![0, 1];
    static ref SIGNER_IDS: Vec<ValidatorId> = SIGNER_IDXS
        .iter()
        .map(|idx| VALIDATOR_IDS[*idx].clone())
        .collect();
    static ref UNEXPECTED_VALIDATOR_ID: ValidatorId = ValidatorId(
        "unexpected|unexpected|unexpected"
            .as_bytes()
            .try_into()
            .unwrap()
    );
}

lazy_static! {
    static ref MESSAGE: [u8; 32] = "Chainflip:Chainflip:Chainflip:01"
        .as_bytes()
        .try_into()
        .unwrap();
    static ref MESSAGE_HASH: MessageHash = MessageHash(MESSAGE.clone());
    static ref MESSAGE_INFO: MessageInfo = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: KEY_ID
    };
        /// Just in case we need to test signing two messages
    static ref MESSAGE2: [u8; 32] = "Chainflip:Chainflip:Chainflip:02"
        .as_bytes()
        .try_into()
        .unwrap();
    static ref SIGN_INFO: SigningInfo = SigningInfo {
        id: KEY_ID,
        signers: SIGNER_IDS.clone()
    };
    static ref KEYGEN_INFO: KeygenInfo = KeygenInfo {
        id: KEY_ID,
        signers: VALIDATOR_IDS.clone()
    };
}

static INIT: Once = Once::new();

/// Initializes the logger and does only once
/// (doing otherwise would result in error)
#[allow(dead_code)]
fn init_logs_once() {
    INIT.call_once(|| {
        env_logger::builder()
            .format_timestamp(None)
            .format_module_path(false)
            .init();
    })
}

// INFO: We should be able to continue signing with the old key. When key rotation happens,
// we need to create a new key. A node is likely to remain a validator, so it needs to be
// able to transfer funds from the old key to the new one. SC will send us a command to
// generate a new key for epoch X (and attempt number?). Requests to sign should also
// contain the epoch.

// TO DO (unit tests):
// [Signing]
// - Delayed data expires on timeout
// - Parties cannot send two messages for the same phase
// - make sure that we don't process p2p data at index signer_id which is our own
// - test that we emit events that allow for penalisation of offending nodes to occur
// [Keygen]
// - Parties cannot send two messages for the same phase
// - make sure that we don't process p2p data at index signer_id which is our own
// - test that we emit events that allow for penalisation of offending nodes to occur
