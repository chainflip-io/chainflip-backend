use std::{sync::Arc, time::Instant};

use curv::elliptic::curves::traits::ECPoint;
use itertools::Itertools;
use log::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    p2p::{P2PMessageCommand, ValidatorId},
    signing::{
        client::{
            client_inner::{
                client_inner::{KeyGenMessageWrapped, KeygenSuccess},
                shared_secret::StageStatus,
                KeygenOutcome,
            },
            KeyId,
        },
        crypto::Parameters,
    },
};

use super::{
    client_inner::{InnerSignal, KeygenData, MultisigMessage},
    shared_secret::SharedSecretState,
    signing_state::KeygenResultInfo,
    utils::ValidatorMaps,
    InnerEvent,
};

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum KeygenStage {
    AwaitingBroadcast1,
    AwaitingSecret2,
    KeyReady,
    /// The ceremony couldn't proceed, so the associated state should be cleaned up
    Abandoned,
}

#[derive(Clone)]
pub struct KeygenState {
    stage: KeygenStage,
    sss: SharedSecretState,
    event_sender: UnboundedSender<InnerEvent>,
    signer_idx: usize,
    /// Mapping from sender indexes to validator ids and back
    maps_for_validator_id_and_idx: Arc<ValidatorMaps>,
    /// All valid signer indexes (1..=n)
    all_signer_idxs: Vec<usize>,
    delayed_next_stage_data: Vec<(ValidatorId, KeygenData)>,
    key_id: KeyId,
    pub(super) key_info: Option<KeygenResultInfo>,
    /// Last time we were able to make progress
    pub(super) last_message_timestamp: Instant,
}

/// A command to the other module to send data to a particular node
struct MessageToSend {
    pub(super) to_idx: usize,
    pub(super) data: Vec<u8>,
}

impl KeygenState {
    pub(super) fn initiate(
        idx: usize,
        params: Parameters,
        idx_map: ValidatorMaps,
        key_id: KeyId,
        event_sender: UnboundedSender<InnerEvent>,
    ) -> Self {
        let all_signer_idxs = (1..=params.share_count).collect_vec();

        let mut state = KeygenState {
            stage: KeygenStage::AwaitingBroadcast1,
            sss: SharedSecretState::new(idx, params),
            event_sender,
            signer_idx: idx,
            all_signer_idxs,
            delayed_next_stage_data: Vec::new(),
            key_id,
            key_info: None,
            maps_for_validator_id_and_idx: Arc::new(idx_map),
            last_message_timestamp: Instant::now(),
        };

        state.initiate_keygen_inner();

        state
    }

    /// Get index in the (sorted) array of all signers
    fn validator_id_to_signer_idx(&self, id: &ValidatorId) -> Option<usize> {
        self.maps_for_validator_id_and_idx.get_idx(&id)
    }

    fn signer_idx_to_validator_id(&self, idx: usize) -> &ValidatorId {
        // Should be safe to unwrap because the `idx` is carefully
        // chosen by our on module
        let id = self.maps_for_validator_id_and_idx.get_id(idx).unwrap();
        id
    }

    fn update_progress_timestamp(&mut self) {
        self.last_message_timestamp = Instant::now();
    }

    /// Returned value will signal that the key is ready
    pub(super) fn process_keygen_message(
        &mut self,
        sender_id: ValidatorId,
        msg: KeygenData,
    ) -> Option<KeygenResultInfo> {
        trace!("[{}] received {} from [{}]", self.us(), &msg, sender_id);

        let signer_idx = match self.validator_id_to_signer_idx(&sender_id) {
            Some(idx) => idx,
            None => {
                warn!(
                    "[{}] Keygen message is ignored for invalid validator id: {}",
                    self.us(),
                    sender_id
                );
                return None;
            }
        };

        match (&self.stage, msg) {
            (KeygenStage::AwaitingBroadcast1, KeygenData::Broadcast1(bc1)) => {
                match self.sss.process_broadcast1(signer_idx, bc1) {
                    StageStatus::Full => {
                        self.update_progress_timestamp();
                        self.finalise_phase1();
                        self.process_delayed();
                    }
                    StageStatus::MadeProgress => self.update_progress_timestamp(),
                    StageStatus::Ignored => { /* do nothing */ }
                }
            }
            (KeygenStage::AwaitingBroadcast1, KeygenData::Secret2(sec2)) => {
                trace!("[{}] delaying Secret2 from [{}]", self.us(), sender_id);
                self.delayed_next_stage_data.push((sender_id, sec2.into()));
            }
            (KeygenStage::AwaitingSecret2, KeygenData::Secret2(sec2)) => {
                match self.sss.process_phase2(signer_idx, sec2) {
                    StageStatus::Full => {
                        trace!("[{}] Phase 2 (keygen) successful ✅✅", self.us());
                        self.update_progress_timestamp();
                        if let Ok(key) = self.sss.init_phase3() {
                            info!("[{}] SHARED KEY IS READY 👍", self.us());

                            self.stage = KeygenStage::KeyReady;

                            let keygen_success = KeygenSuccess {
                                key_id: self.key_id,
                                key: key.aggregate_pubkey.get_element(),
                            };

                            self.send_event(InnerEvent::KeygenResult(KeygenOutcome::Success(
                                keygen_success,
                            )));

                            let key_info = KeygenResultInfo {
                                key: Arc::new(key),
                                validator_map: Arc::clone(&self.maps_for_validator_id_and_idx),
                            };

                            self.key_info = Some(key_info.clone());

                            // TODO: remove this as KeygenOutcome subsumes it
                            self.send_event(InnerEvent::InnerSignal(InnerSignal::KeyReady));

                            return Some(key_info);
                        } else {
                            error!(
                                "Invalid Phase2 keygen data, abandoning state for key: {:?}",
                                self.key_id
                            );
                            self.stage = KeygenStage::Abandoned;

                            self.send_event(InnerEvent::KeygenResult(KeygenOutcome::invalid(
                                self.key_id,
                                vec![],
                            )));
                        }
                    }
                    StageStatus::MadeProgress => {
                        self.update_progress_timestamp();
                    }
                    StageStatus::Ignored => { /* do nothing */ }
                }
            }
            (KeygenStage::Abandoned, data) => {
                warn!("Dropping {} for abandoned keygen state", data);
            }
            _ => {
                warn!(
                    "[{}] Unexpected message for stage: {:?}",
                    self.us(),
                    self.stage
                );
            }
        }

        return None;
    }

    fn initiate_keygen_inner(&mut self) {
        trace!(
            "[{}] Initiating keygen for key {:?}",
            self.us(),
            self.key_id
        );

        let bc1 = self.sss.init_phase1();

        let wrapped = KeyGenMessageWrapped::new(self.key_id, bc1);

        let msg = MultisigMessage::from(wrapped);

        self.keygen_broadcast(msg);

        self.process_delayed();
    }

    fn finalise_phase1(&mut self) {
        // We require all parties to be active during keygen
        match self.sss.init_phase2(&self.all_signer_idxs) {
            Ok(msgs) => {
                self.stage = KeygenStage::AwaitingSecret2;
                let msgs = msgs
                    .into_iter()
                    .map(|(idx, secret2)| {
                        let wrapped = KeyGenMessageWrapped::new(self.key_id, secret2);
                        let secret2 = MultisigMessage::from(wrapped);
                        let data = serde_json::to_vec(&secret2).unwrap();
                        MessageToSend { to_idx: idx, data }
                    })
                    .collect_vec();

                self.send(msgs);
            }
            Err(_) => {
                error!("phase2 keygen error for key: {:?}", self.key_id);

                self.stage = KeygenStage::Abandoned;

                // TODO: need to provide responsible nodes
                let event = InnerEvent::KeygenResult(KeygenOutcome::invalid(self.key_id, vec![]));

                self.send_event(event);
            }
        }
    }

    fn send_event(&self, event: InnerEvent) {
        if let Err(err) = self.event_sender.send(event) {
            error!("Could not send inner event: {}", err);
        }
    }

    fn send(&self, messages: Vec<MessageToSend>) {
        for MessageToSend { to_idx, data } in messages {
            let destination = self.signer_idx_to_validator_id(to_idx).clone();

            debug!(
                "[{}] sending direct message to [{}]",
                self.us(),
                destination
            );

            let message = P2PMessageCommand { destination, data };

            let event = InnerEvent::P2PMessageCommand(message);

            self.send_event(event);
        }
    }

    fn keygen_broadcast(&self, msg: MultisigMessage) {
        // TODO: see if there is a way to publish a bunch of messages
        for idx in &self.all_signer_idxs {
            if *idx == self.signer_idx {
                continue;
            }

            let destination = self.signer_idx_to_validator_id(*idx).clone();

            debug!("[{}] Sending to {}", self.us(), destination);

            let message = P2PMessageCommand {
                destination,
                data: serde_json::to_vec(&msg).unwrap(),
            };

            let event = InnerEvent::P2PMessageCommand(message);

            self.send_event(event);
        }
    }

    fn process_delayed(&mut self) {
        while let Some((sender_id, msg)) = self.delayed_next_stage_data.pop() {
            trace!(
                "[{}] Processing a delayed message from [{}]",
                self.us(),
                sender_id
            );
            self.process_keygen_message(sender_id, msg);
        }
    }

    #[cfg(test)]
    pub fn delayed_count(&self) -> usize {
        self.delayed_next_stage_data.len()
    }

    #[cfg(test)]
    pub fn get_stage(&self) -> KeygenStage {
        self.stage
    }

    /// We want to be able to control how our id is printed in tests
    #[cfg(test)]
    fn us(&self) -> String {
        self.signer_idx.to_string()
    }

    /// We don't want to print our id in production. Generating an empty
    /// string should not result in memory allocation, and therefore should be fast
    #[cfg(not(test))]
    fn us(&self) -> String {
        String::default()
    }
}
