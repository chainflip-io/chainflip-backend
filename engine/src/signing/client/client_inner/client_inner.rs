use std::time::Duration;

use crate::{
    p2p::{P2PMessage, P2PMessageCommand},
    signing::{
        client::MultisigInstruction,
        crypto::{BigInt, KeyGenBroadcastMessage1, LocalSig, Parameters, VerifiableSS, FE, GE},
        MessageHash,
    },
};

use log::*;
use tokio::sync::mpsc::UnboundedSender;

use super::{
    keygen_state::{KeygenStage, KeygenState},
    signing_state_manager::SigningStateManager,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(super) enum SigningData {
    Broadcast1(Broadcast1),
    Secret2(Secret2),
    LocalSig(LocalSig),
}

impl From<Broadcast1> for SigningData {
    fn from(bc1: Broadcast1) -> Self {
        SigningData::Broadcast1(bc1)
    }
}

/// Protocol data plus the message to sign
#[derive(Serialize, Deserialize, Debug)]
pub struct SigningDataWrapper {
    pub(super) data: SigningData,
    pub(super) message: MessageHash,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum MultisigMessage {
    KeyGenMessage(KeyGenMessage),
    SigningMessage(SigningDataWrapper),
}

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Broadcast1 {
    pub bc1: KeyGenBroadcastMessage1,
    pub blind: BigInt,
    pub y_i: GE,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Secret2 {
    pub(super) vss: VerifiableSS<GE>,
    pub(super) secret_share: FE,
}

impl From<Secret2> for KeyGenMessage {
    fn from(sec2: Secret2) -> Self {
        KeyGenMessage::Secret2(sec2)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum KeyGenMessage {
    Broadcast1(Broadcast1),
    Secret2(Secret2),
}

impl From<Broadcast1> for KeyGenMessage {
    fn from(bc1: Broadcast1) -> Self {
        KeyGenMessage::Broadcast1(bc1)
    }
}

/// public interfaces will return this to indicate
/// that something potentially interesting has happened
#[derive(Debug, PartialEq)]
pub enum InnerSignal {
    KeyReady,
    MessageSigned(MessageHash),
}

#[derive(Debug, PartialEq)]
pub enum InnerEvent {
    P2PMessageCommand(P2PMessageCommand),
    InnerSignal(InnerSignal),
}

#[derive(Clone)]
pub struct MultisigClientInner {
    // TODO: might have two(?) keygen states during vault rotation
    pub keygen_state: KeygenState,
    params: Parameters,
    signer_idx: usize,
    pub signing_manager: SigningStateManager,
}

impl MultisigClientInner {
    pub fn new(
        idx: usize,
        params: Parameters,
        tx: UnboundedSender<InnerEvent>,
        phase_timeout: Duration,
    ) -> Self {
        MultisigClientInner {
            keygen_state: KeygenState::new(idx, params, tx.clone()),
            params,
            signer_idx: idx,
            signing_manager: SigningStateManager::new(params, idx, tx, phase_timeout),
        }
    }

    pub fn cleanup(&mut self) {
        // TODO: cleanup keygen states as well
        self.signing_manager.cleanup();
    }

    pub fn process_multisig_instruction(&mut self, instruction: MultisigInstruction) {
        match instruction {
            MultisigInstruction::KeyGen => {
                // For now disable generating a new key when we already have one

                debug!("[{}] Received keygen instruction", self.signer_idx);

                self.keygen_state.initiate_keygen();
            }
            MultisigInstruction::Sign(data, parties) => {
                debug!("[{}] Received sign instruction", self.signer_idx);

                // TODO: We should be able to start receiving signing data even
                // before we have the key locally!
                match self.keygen_state.stage {
                    KeygenStage::KeyReady => {
                        self.signing_manager.on_request_to_sign(data, &parties);
                    }
                    _ => {
                        // TODO: add a queue of messages to sign
                        warn!("Failed attempt to sign: key not ready");
                    }
                }
            }
        }
    }

    pub fn process_p2p_mq_message(&mut self, msg: P2PMessage) {
        let P2PMessage { sender_id, data } = msg;
        let msg: Result<MultisigMessage, _> = serde_json::from_slice(&data);

        match msg {
            Ok(MultisigMessage::KeyGenMessage(msg)) => {
                // NOTE: we should be able to process Keygen messages
                // even when we are "signing"... (for example, if we want to
                // generate a new key)
                if let Some(key) = self.keygen_state.process_keygen_message(sender_id, msg) {
                    self.signing_manager.set_key(key);
                }
            }
            Ok(MultisigMessage::SigningMessage(msg)) => {
                // NOTE: we should be able to process Signing messages
                // even when we are generating a new key (for example,
                // we should be able to receive phase1 messages before we've
                // finalized the signing key locally)
                self.signing_manager
                    .maybe_process_signing_data(sender_id, msg);
            }
            Err(_) => {
                warn!("Invalid multisig message, discarding");
            }
        }
    }
}
