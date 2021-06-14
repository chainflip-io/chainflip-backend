use itertools::Itertools;
use log::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    p2p::{P2PMessageCommand, ValidatorId},
    signing::crypto::Parameters,
};

use super::{
    client_inner::{InnerSignal, KeyGenMessage, MultisigMessage},
    shared_secret::SharedSecretState,
    signing_state::KeygenResult,
    InnerEvent,
};

#[derive(Debug, PartialEq, Clone)]
pub enum KeygenStage {
    Uninitialized,
    AwaitingBroadcast1,
    AwaitingSecret2,
    KeyReady,
}

#[derive(Clone)]
pub struct KeygenState {
    pub stage: KeygenStage,
    sss: SharedSecretState,
    event_sender: UnboundedSender<InnerEvent>,
    signer_idx: usize,
    params: Parameters,
    pub delayed_next_stage_data: Vec<(ValidatorId, KeyGenMessage)>,
}

/// A command to the other module to send data to a particular node
struct MessageToSend {
    pub(super) to: ValidatorId,
    pub(super) data: Vec<u8>,
}

impl KeygenState {
    pub(super) fn new(
        idx: usize,
        params: Parameters,
        event_sender: UnboundedSender<InnerEvent>,
    ) -> Self {
        let min_parties = params.share_count;
        KeygenState {
            stage: KeygenStage::Uninitialized,
            sss: SharedSecretState::new(idx, params, min_parties),
            event_sender,
            delayed_next_stage_data: Vec::new(),
            signer_idx: idx,
            params,
        }
    }

    /// Participate in a threshold signature generation procedure
    pub(super) fn initiate_keygen(&mut self) {
        match self.stage {
            KeygenStage::Uninitialized => {
                self.initiate_keygen_inner();
            }
            _ => {
                // TODO: allow subsequent keys to be generated
                warn!("Unexpected keygen request");
            }
        }
    }

    /// Returned value will signal that the key is ready
    pub(super) fn process_keygen_message(
        &mut self,
        sender_id: usize,
        msg: KeyGenMessage,
    ) -> Option<KeygenResult> {
        trace!(
            "[{}] received message from [{}]",
            self.signer_idx,
            sender_id
        );

        // Key to return in case it was created here
        let mut result_key = None;

        match (&self.stage, msg) {
            (KeygenStage::Uninitialized, KeyGenMessage::Broadcast1(bc1)) => {
                self.delayed_next_stage_data.push((sender_id, bc1.into()));
            }
            (KeygenStage::AwaitingBroadcast1, KeyGenMessage::Broadcast1(bc1)) => {
                trace!("[{}] received bc1 from [{}]", self.signer_idx, sender_id);

                if self.sss.process_broadcast1(sender_id, bc1) {
                    self.keygen_phase2();
                    self.process_delayed();
                }
            }
            (KeygenStage::AwaitingBroadcast1, KeyGenMessage::Secret2(sec2)) => {
                trace!(
                    "[{}] delaying Secret2 from [{}]",
                    self.signer_idx,
                    sender_id
                );
                self.delayed_next_stage_data.push((sender_id, sec2.into()));
            }
            (KeygenStage::AwaitingSecret2, KeyGenMessage::Secret2(sec2)) => {
                trace!(
                    "[{}] received secret2 from [{}]",
                    self.signer_idx,
                    sender_id
                );

                if self.sss.process_phase2(sender_id, sec2) {
                    trace!("[{}] Phase 2 (keygen) successful ✅✅", self.signer_idx);
                    if let Ok(key) = self.sss.init_phase3() {
                        info!("[{}] SHARED KEY IS READY 👍", self.signer_idx);

                        self.stage = KeygenStage::KeyReady;
                        result_key = Some(key);

                        let _ = self
                            .event_sender
                            .send(InnerEvent::InnerSignal(InnerSignal::KeyReady));
                    }
                }
            }
            _ => {
                warn!("Unexpected keygen message for stage: {:?}", self.stage);
            }
        }

        return result_key;
    }

    fn initiate_keygen_inner(&mut self) {
        self.stage = KeygenStage::AwaitingBroadcast1;

        trace!("Created key for idx: {}", self.signer_idx);

        let bc1 = self.sss.init_phase1();

        let msg = MultisigMessage::KeyGenMessage(KeyGenMessage::Broadcast1(bc1));

        self.keygen_broadcast(msg);

        self.process_delayed();
    }

    fn keygen_phase2(&mut self) {
        self.stage = KeygenStage::AwaitingSecret2;
        let parties = (1..=self.params.share_count).into_iter().collect_vec();

        // We require all parties to be active during keygen
        match self.sss.init_phase2(&parties) {
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

    fn send(&self, messages: Vec<MessageToSend>) {
        for MessageToSend { to, data } in messages {
            trace!("[{}] sending a message to [{}]", self.signer_idx, to);
            let message = P2PMessageCommand {
                destination: to,
                data,
            };

            let event = InnerEvent::P2PMessageCommand(message);

            if let Err(err) = self.event_sender.send(event) {
                error!("Could not send p2p message command: {}", err);
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
                error!("Could not send p2p message command: {}", err);
            }
        }
    }

    fn process_delayed(&mut self) {
        while let Some((sender_id, msg)) = self.delayed_next_stage_data.pop() {
            trace!("Processing a delayed message from [{}]", sender_id);
            self.process_keygen_message(sender_id, msg);
        }
    }
}
