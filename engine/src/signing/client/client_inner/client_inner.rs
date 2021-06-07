use std::time::Duration;

use crate::{
    p2p::{P2PMessage, P2PMessageCommand, ValidatorId},
    signing::{
        bitcoin_schnorr::{KeyGenBroadcastMessage1, LocalSig, Parameters},
        client::MultisigInstruction,
        MessageHash,
    },
};

use curv::{
    cryptographic_primitives::secret_sharing::feldman_vss::VerifiableSS,
    elliptic::curves::secp256_k1::{FE, GE},
    BigInt,
};
use itertools::Itertools;
use log::*;
use tokio::sync::mpsc::UnboundedSender;

use super::{shared_secret::SharedSecretState, signing_state_manager::SigningStateManager};

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

#[derive(Clone)]
pub struct KeygenState {
    pub stage: KeygenStage,
    sss: SharedSecretState,

    pub delayed_next_stage_data: Vec<(ValidatorId, KeyGenMessage)>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum KeygenStage {
    Uninitialized,
    AwaitingBroadcast1,
    AwaitingSecret2,
    KeyReady,
}

/// Whether we are currently in the process of generating new keys
/// or a new signature
#[derive(Debug)]
enum MultisigStage {
    Keygen(KeygenStage),
    KeyReady,
}

impl KeygenState {
    fn new(idx: usize, params: Parameters) -> Self {
        let min_parties = params.share_count;
        KeygenState {
            stage: KeygenStage::Uninitialized,
            sss: SharedSecretState::new(idx, params, min_parties),
            delayed_next_stage_data: Vec::new(),
        }
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

/// A command to the other module to send data to a particilar node
struct MessageToSend {
    pub(super) to: ValidatorId,
    pub(super) data: Vec<u8>,
}

#[derive(Clone)]
pub struct MultisigClientInner {
    // TODO: might have two(?) keygen states during vault rotation
    pub keygen_state: KeygenState,
    params: Parameters,
    signer_idx: usize,
    pub signing_manager: SigningStateManager,
    event_sender: UnboundedSender<InnerEvent>,
}

impl MultisigClientInner {
    pub fn new(
        idx: usize,
        params: Parameters,
        tx: UnboundedSender<InnerEvent>,
        phase_timeout: Duration,
    ) -> Self {
        MultisigClientInner {
            keygen_state: KeygenState::new(idx, params),
            params,
            signer_idx: idx,
            event_sender: tx.clone(),
            signing_manager: SigningStateManager::new(params, idx, tx, phase_timeout),
        }
    }

    fn keygen_phase2(&mut self) {
        self.keygen_state.stage = KeygenStage::AwaitingSecret2;
        let parties = (1..=self.params.share_count).into_iter().collect_vec();

        // We require all parties to be active during keygen
        match self.keygen_state.sss.init_phase2(&parties) {
            Ok(msgs) => {
                let msgs = msgs
                    .into_iter()
                    .map(|(idx, secret2)| {
                        let secret2 =
                            MultisigMessage::KeyGenMessage(KeyGenMessage::Secret2(secret2));
                        let data = serde_json::to_vec(&secret2).unwrap();
                        MessageToSend { to: idx, data }
                    })
                    .collect_vec();

                self.send(msgs);
            }
            Err(_) => {
                error!("phase2 keygen error")
            }
        }
    }

    /// Returned value will signal that the key is ready
    fn process_keygen_message(&mut self, sender_id: usize, msg: KeyGenMessage) {
        trace!(
            "[{}] received message from [{}]",
            self.signer_idx,
            sender_id
        );

        match (&self.keygen_state.stage, msg) {
            (KeygenStage::Uninitialized, KeyGenMessage::Broadcast1(bc1)) => {
                self.keygen_state
                    .delayed_next_stage_data
                    .push((sender_id, bc1.into()));
            }
            (KeygenStage::AwaitingBroadcast1, KeyGenMessage::Broadcast1(bc1)) => {
                debug!("[{}] received bc1 from [{}]", self.signer_idx, sender_id);

                if self.keygen_state.sss.process_broadcast1(sender_id, bc1) {
                    self.keygen_phase2();
                    self.process_delayed();
                }
            }
            (KeygenStage::AwaitingBroadcast1, KeyGenMessage::Secret2(sec2)) => {
                debug!(
                    "[{}] delaying Secret2 from [{}]",
                    self.signer_idx, sender_id
                );
                self.keygen_state
                    .delayed_next_stage_data
                    .push((sender_id, sec2.into()));
            }
            (KeygenStage::AwaitingSecret2, KeyGenMessage::Secret2(sec2)) => {
                debug!(
                    "[{}] received secret2 from [{}]",
                    self.signer_idx, sender_id
                );

                if self.keygen_state.sss.process_phase2(sender_id, sec2) {
                    info!("[{}] Phase 2 (keygen) successful ✅✅", self.signer_idx);
                    if let Ok(key) = self.keygen_state.sss.init_phase3() {
                        info!("[{}] SHARED KEY IS READY 👍", self.signer_idx);

                        self.keygen_state.stage = KeygenStage::KeyReady;
                        self.signing_manager.set_key(key);

                        let _ = self
                            .event_sender
                            .send(InnerEvent::InnerSignal(InnerSignal::KeyReady));
                    }
                }
            }
            _ => {
                warn!(
                    "Unexpected keygen message for stage: {:?}",
                    self.keygen_state.stage
                );
            }
        }
    }

    fn send(&self, messages: Vec<MessageToSend>) {
        for MessageToSend { to, data } in messages {
            // debug!("[{}] sending a message to [{}]", self.signer_idx, to);
            let message = P2PMessageCommand {
                destination: to,
                data,
            };

            let event = InnerEvent::P2PMessageCommand(message);

            if let Err(err) = self.event_sender.send(event) {
                error!("Could not send p2p: {}", err);
            }
        }
    }

    fn keygen_broadcast(&self, msg: MultisigMessage) {
        // TODO: see if there is a way to publish a bunch of messages
        for i in 1..=self.params.share_count {
            if i == self.signer_idx {
                continue;
            }

            let message = P2PMessageCommand {
                destination: i,
                data: serde_json::to_vec(&msg).unwrap(),
            };

            let event = InnerEvent::P2PMessageCommand(message);

            if let Err(err) = self.event_sender.send(event) {
                error!("Could not send p2p: {}", err);
            }
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

                match self.keygen_state.stage {
                    KeygenStage::Uninitialized => {
                        self.initiate_keygen();
                    }
                    _ => {
                        warn!("Unexpected keygen request. (TODO: allow subsequent keys to be generated)");
                    }
                }
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

    fn process_delayed(&mut self) {
        while let Some((sender_id, msg)) = self.keygen_state.delayed_next_stage_data.pop() {
            debug!("Processing a delayed message from [{}]", sender_id);
            self.process_keygen_message(sender_id, msg);
        }
    }

    /// Participate in a threshold signature generation procedure
    fn initiate_keygen(&mut self) {
        self.keygen_state.stage = KeygenStage::AwaitingBroadcast1;

        debug!("Created key for idx: {}", self.signer_idx);

        let bc1 = self.keygen_state.sss.init_phase1();

        let msg = MultisigMessage::KeyGenMessage(KeyGenMessage::Broadcast1(bc1));

        self.keygen_broadcast(msg);

        self.process_delayed();
    }

    pub fn process_p2p_mq_message(&mut self, msg: P2PMessage) {
        let P2PMessage { sender_id, data } = msg;
        let msg: Result<MultisigMessage, _> = serde_json::from_slice(&data);

        match msg {
            Ok(MultisigMessage::KeyGenMessage(msg)) => {
                // NOTE: we should be able to process Keygen messages
                // even when we are "signing"... (for example, if we want to
                // generate a new key)
                self.process_keygen_message(sender_id, msg);
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
