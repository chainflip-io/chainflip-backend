use std::{collections::HashMap, fmt::Display, time::Duration};

use crate::{
    p2p::{P2PMessage, P2PMessageCommand, ValidatorId},
    signing::{
        client::{KeyId, MultisigInstruction, SigningInfo},
        crypto::{BigInt, KeyGenBroadcastMessage1, LocalSig, Parameters, VerifiableSS, FE, GE},
        MessageHash, MessageInfo,
    },
};

use log::*;
use tokio::sync::mpsc::UnboundedSender;

use super::{
    keygen_manager::KeygenManager, signing_state::KeygenResultInfo,
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

impl Display for SigningData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // We are not interested in the exact contents most of the time
        match self {
            SigningData::Broadcast1(_) => write!(f, "Signing::Broadcast"),
            SigningData::Secret2(_) => write!(f, "Signing::Secret"),
            SigningData::LocalSig(_) => write!(f, "Signing::LocalSig"),
        }
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

impl From<Secret2> for KeygenData {
    fn from(sec2: Secret2) -> Self {
        KeygenData::Secret2(sec2)
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
    pub(super) message: KeygenData,
}

impl KeyGenMessageWrapped {
    pub fn new<M>(key_id: KeyId, m: M) -> Self
    where
        M: Into<KeygenData>,
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
pub enum KeygenData {
    Broadcast1(Broadcast1),
    Secret2(Secret2),
}

impl From<Broadcast1> for KeygenData {
    fn from(bc1: Broadcast1) -> Self {
        KeygenData::Broadcast1(bc1)
    }
}

impl Display for KeygenData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            KeygenData::Broadcast1(_) => write!(f, "Keygen::Broadcast"),
            KeygenData::Secret2(_) => write!(f, "Keygen::Secret"),
        }
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
    /// Requests awaiting a key
    // TODO: make sure this is cleaned up on timeout
    pending_requests_to_sign: HashMap<KeyId, Vec<(MessageHash, SigningInfo)>>,
}

impl MultisigClientInner {
    pub fn new(
        id: ValidatorId,
        params: Parameters,
        tx: UnboundedSender<InnerEvent>,
        phase_timeout: Duration,
    ) -> Self {
        MultisigClientInner {
            keygen: KeygenManager::new(params, id.clone(), tx.clone()),
            params,
            id: id.clone(),
            signing_manager: SigningStateManager::new(params, id, tx, phase_timeout),
            pending_requests_to_sign: Default::default(),
        }
    }

    #[cfg(test)]
    pub fn get_keygen(&self) -> &KeygenManager {
        &self.keygen
    }

    /// Clean up expired states
    pub fn cleanup(&mut self) {
        // TODO: cleanup keygen states as well
        self.signing_manager.cleanup();
    }

    fn add_pending(&mut self, data: MessageHash, sign_info: SigningInfo) {
        debug!("[{}] delaying a request to sign", self.id);

        // TODO: check for duplicates?

        let entry = self
            .pending_requests_to_sign
            .entry(sign_info.id)
            .or_default();

        entry.push((data, sign_info));
    }

    /// Process `instruction` issued internally (i.e. from SC or another local module)
    pub fn process_multisig_instruction(&mut self, instruction: MultisigInstruction) {
        match instruction {
            MultisigInstruction::KeyGen(keygen_info) => {
                // For now disable generating a new key when we already have one

                debug!("[{}] Received keygen instruction", self.id);

                self.keygen.on_keygen_request(keygen_info);
            }
            MultisigInstruction::Sign(hash, sign_info) => {
                debug!("[{}] Received sign instruction", self.id);
                let key_id = sign_info.id;

                let key = self.keygen.get_key_info_by_id(key_id);

                match key {
                    Some(key) => {
                        self.signing_manager
                            .on_request_to_sign(hash, key.clone(), sign_info);
                    }
                    None => {
                        // the key is not ready, delay until it is
                        self.add_pending(hash, sign_info);
                    }
                }
            }
        }
    }

    fn process_pending(&mut self, key_id: KeyId, key_info: KeygenResultInfo) {
        if let Some(reqs) = self.pending_requests_to_sign.remove(&key_id) {
            debug!("Processing pending requests to sign, count: {}", reqs.len());
            for (data, info) in reqs {
                self.signing_manager
                    .on_request_to_sign(data, key_info.clone(), info)
            }
        }
    }

    /// Process message from another validator
    pub fn process_p2p_mq_message(&mut self, msg: P2PMessage) {
        let P2PMessage { sender_id, data } = msg;
        let msg: Result<MultisigMessage, _> = serde_json::from_slice(&data);

        match msg {
            Ok(MultisigMessage::KeyGenMessage(msg)) => {
                // NOTE: we should be able to process Keygen messages
                // even when we are "signing"... (for example, if we want to
                // generate a new key)

                let key_id = msg.key_id;

                if let Some(key) = self.keygen.process_keygen_message(sender_id, msg) {
                    self.process_pending(key_id, key);
                }
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
