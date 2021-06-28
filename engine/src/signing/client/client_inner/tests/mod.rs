mod helpers;
mod keygen_unit_tests;
mod signing_unit_tests;

use lazy_static::lazy_static;
#[allow(unused_imports)]
use log::*;

use super::client_inner::*;

use crate::{
    p2p::{P2PMessage, ValidatorId},
    signing::{
        client::{
            client_inner::{
                keygen_state::KeygenStage,
                signing_state::SigningStage,
                tests::helpers::{
                    generate_valid_keygen_data, keygen_delayed_count, keygen_stage_for,
                    recv_next_signal_message_skipping, sec2_to_p2p_keygen, sec2_to_p2p_signing,
                    sig_to_p2p, signing_delayed_count,
                },
            },
            KeyId, KeygenInfo, MultisigInstruction, SigningInfo, PHASE_TIMEOUT,
        },
        crypto::{Keys, Parameters},
        MessageHash, MessageInfo,
    },
};

// The id to be used by default
const KEY_ID: KeyId = KeyId(0);

lazy_static! {
    static ref VALIDATOR_IDS: Vec<ValidatorId> = vec![
        ValidatorId("1".to_string()),
        ValidatorId("2".to_string()),
        ValidatorId("3".to_string()),
    ];
    static ref SIGNER_IDS: Vec<ValidatorId> =
        vec![VALIDATOR_IDS[0].clone(), VALIDATOR_IDS[1].clone()];
}

lazy_static! {
    static ref MESSAGE: Vec<u8> = "Chainflip".as_bytes().to_vec();
    static ref MESSAGE_HASH: MessageHash = MessageHash(MESSAGE.clone());
    static ref MESSAGE_INFO: MessageInfo = MessageInfo {
        hash: MESSAGE_HASH.clone(),
        key_id: KEY_ID
    };
    static ref SIGN_INFO: SigningInfo = SigningInfo {
        id: KEY_ID,
        signers: SIGNER_IDS.clone()
    };
    static ref KEYGEN_INFO: KeygenInfo = KeygenInfo {
        id: KEY_ID,
        signers: VALIDATOR_IDS.clone()
    };
}

fn create_bc1(signer_idx: usize) -> Broadcast1 {
    let key = Keys::phase1_create(signer_idx);

    let (bc1, blind) = key.phase1_broadcast();

    let y_i = key.y_i;

    Broadcast1 { bc1, blind, y_i }
}

use std::{sync::Once, time::Duration};

use super::client_inner::Broadcast1;

static INIT: Once = Once::new();

/// Initializes the logger and does only once
/// (doing otherwise would result in error)
fn init_logs_once() {
    INIT.call_once(|| {
        env_logger::builder()
            .format_timestamp(None)
            .format_module_path(false)
            .init();
    })
}

fn create_keygen_p2p_message<M>(sender_id: &ValidatorId, message: M) -> P2PMessage
where
    M: Into<KeygenData>,
{
    let wrapped = KeyGenMessageWrapped::new(KEY_ID, message.into());

    let ms_message = MultisigMessage::from(wrapped);

    let data = serde_json::to_vec(&ms_message).unwrap();

    P2PMessage {
        sender_id: sender_id.clone(),
        data,
    }
}

fn get_stage_for_msg(c: &MultisigClientInner, message_info: &MessageInfo) -> Option<SigningStage> {
    c.signing_manager
        .get_state_for(message_info)
        .map(|s| s.get_stage())
}

// INFO: We should be able to continue signing with the old key. When key rotation happens,
// we need to create a new key. A node is likely to remain a validator, so it needs to be
// able to transfer funds from the old key to the new one. SC will send us a command to
// generate a new key for epoch X (and attempt number?). Requests to sign should also
// contain the epoch.

// What needs to be tested (unit tests)
// DONE:
// - Delaying works correctly for Keygen::BC1, Keygen::Secret2, Signing:BC1, Signing::Secret2, Signing::LocalSig
// - BC1 messages are processed after a timely RTS (and can lead to phase 2)
// - RTS is required to proceed to the next phase

// TO DO:
// - Delayed data expires on timeout
// - Signing phases do timeout (only tested for BC1 currently)
// - Parties cannot send two messages for the same phase of signing/keygen
// - When unable to make progress, the state (Signing/Keygen) should be correctly reset
// (i.e. past failures don't impact future signing ceremonies)
// - Should be able to generate new signing keys
// - make sure that we don't process p2p data at index signer_id which is our own
// - test that we penalize the offending nodes
// - test that there is no interaction between different key_ids
// - test that we clean up states that didn't result in a key
