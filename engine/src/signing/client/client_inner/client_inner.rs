use std::time::Duration;

use crate::{
    p2p::{P2PMessage, P2PMessageCommand, ValidatorId},
    signing::{
        client::{KeyId, MultisigInstruction},
        crypto::{BigInt, KeyGenBroadcastMessage1, LocalSig, Parameters, VerifiableSS, FE, GE},
        MessageInfo,
    },
};

use log::*;
use tokio::sync::mpsc::UnboundedSender;

use super::{keygen_manager::KeygenManager, signing_state_manager::SigningStateManager};

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
pub struct SigningDataWrapped {
    pub(super) data: SigningData,
    pub(super) message: MessageInfo,
}

impl SigningDataWrapped {
    pub(super) fn new<S>(data: S, message: MessageInfo) -> Self
    where
        S: Into<SigningData>,
    {
        SigningDataWrapped {
            data: data.into(),
            message,
        }
    }
}

impl From<SigningDataWrapped> for MultisigMessage {
    fn from(wrapped: SigningDataWrapped) -> Self {
        MultisigMessage::SigningMessage(wrapped)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum MultisigMessage {
    KeyGenMessage(KeyGenMessageWrapped),
    SigningMessage(SigningDataWrapped),
}

impl From<LocalSig> for SigningData {
    fn from(sig: LocalSig) -> Self {
        SigningData::LocalSig(sig)
    }
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

impl From<Secret2> for SigningData {
    fn from(sec2: Secret2) -> Self {
        SigningData::Secret2(sec2)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyGenMessageWrapped {
    pub(super) key_id: KeyId,
    pub(super) message: KeyGenMessage,
}

impl KeyGenMessageWrapped {
    pub fn new<M>(key_id: KeyId, m: M) -> Self
    where
        M: Into<KeyGenMessage>,
    {
        KeyGenMessageWrapped {
            key_id,
            message: m.into(),
        }
    }
}

impl From<KeyGenMessageWrapped> for MultisigMessage {
    fn from(wrapped: KeyGenMessageWrapped) -> Self {
        MultisigMessage::KeyGenMessage(wrapped)
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
    MessageSigned(MessageInfo),
}

#[derive(Debug, PartialEq)]
pub enum InnerEvent {
    P2PMessageCommand(P2PMessageCommand),
    InnerSignal(InnerSignal),
}

#[derive(Clone)]
pub struct MultisigClientInner {
    keygen: KeygenManager,
    params: Parameters,
    id: ValidatorId,
    pub signing_manager: SigningStateManager,
}

impl MultisigClientInner {
    pub fn new(
        id: ValidatorId,
        params: Parameters,
        tx: UnboundedSender<InnerEvent>,
        phase_timeout: Duration,
    ) -> Self {
        MultisigClientInner {
            keygen: KeygenManager::new(params, id, tx.clone()),
            params,
            id,
            // MAXIM: id is wrong here (below)...
            signing_manager: SigningStateManager::new(params, id, tx, phase_timeout),
        }
    }

    #[cfg(test)]
    pub fn get_keygen(&self) -> &KeygenManager {
        &self.keygen
    }

    pub fn cleanup(&mut self) {
        // TODO: cleanup keygen states as well
        self.signing_manager.cleanup();
    }

    pub fn process_multisig_instruction(&mut self, instruction: MultisigInstruction) {
        match instruction {
            MultisigInstruction::KeyGen(epoch) => {
                // For now disable generating a new key when we already have one

                debug!("[{}] Received keygen instruction", self.id);

                self.keygen.on_keygen_request(epoch);
            }
            MultisigInstruction::Sign(hash, sign_info) => {
                debug!("[{}] Received sign instruction", self.id);
                let key_id = sign_info.id;

                let key = self.keygen.get_key_by_id(key_id);

                match key {
                    Some(key) => {
                        self.signing_manager
                            .on_request_to_sign(hash, key.clone(), sign_info);
                    }
                    None => {
                        // We don't have the key yet, but already received
                        // a signing requiest using it. The solution is to
                        // delay the request a little bit, replay it once the key
                        // is ready.

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
                self.keygen.process_keygen_message(sender_id, msg);
            }
            Ok(MultisigMessage::SigningMessage(msg)) => {
                // NOTE: we should be able to process Signing messages
                // even when we are generating a new key (for example,
                // we should be able to receive phase1 messages before we've
                // finalized the signing key locally)
                self.signing_manager.process_signing_data(sender_id, msg);
            }
            Err(_) => {
                warn!("Cannot parse multisig message, discarding");
            }
        }
    }
}
