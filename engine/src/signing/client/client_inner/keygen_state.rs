use std::{sync::Arc, time::Instant};

use itertools::Itertools;
use slog::o;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    logging::{COMPONENT_KEY, SIGNING_SUB_COMPONENT},
    p2p::{P2PMessageCommand, ValidatorId},
    signing::{
        client::{
            client_inner::{
                client_inner::KeyGenMessageWrapped, shared_secret::StageStatus, KeygenOutcome,
            },
            KeyId,
        },
        crypto::{InvalidKey, InvalidSS, Parameters},
    },
};

use super::{
    client_inner::{KeygenData, MultisigMessage},
    common::KeygenResultInfo,
    shared_secret::SharedSecretState,
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
    /// Multisig parameters are only stored here so we can put
    /// them inside `KeygenResultInfo` when we create the key
    params: Parameters,
    /// Last time we were able to make progress
    pub last_message_timestamp: Instant,
    logger: slog::Logger,
}

/// A command to the other module to send data to a particular node
struct MessageToSend {
    pub to_idx: usize,
    pub data: Vec<u8>,
}

impl KeygenState {
    pub fn initiate(
        idx: usize,
        params: Parameters,
        idx_map: ValidatorMaps,
        key_id: KeyId,
        event_sender: UnboundedSender<InnerEvent>,
        logger: &slog::Logger,
    ) -> Self {
        let all_signer_idxs = (1..=params.share_count).collect_vec();

        let mut state = KeygenState {
            stage: KeygenStage::AwaitingBroadcast1,
            sss: SharedSecretState::new(idx, params.clone(), logger),
            event_sender,
            signer_idx: idx,
            all_signer_idxs,
            delayed_next_stage_data: Vec::new(),
            key_id,
            params,
            maps_for_validator_id_and_idx: Arc::new(idx_map),
            last_message_timestamp: Instant::now(),
            logger: logger.new(o!(SIGNING_SUB_COMPONENT => "KeygenState")),
        };

        state.initiate_keygen_inner();

        state
    }

    /// Get ids of validators who haven't sent the data for the current stage
    pub fn awaited_parties(&self) -> Vec<ValidatorId> {
        let awaited_idxs = match self.stage {
            KeygenStage::AwaitingBroadcast1 | KeygenStage::AwaitingSecret2 => {
                self.sss.awaited_parties()
            }
            KeygenStage::KeyReady | KeygenStage::Abandoned => vec![],
        };

        let awaited_ids = awaited_idxs
            .into_iter()
            .map(|idx| self.signer_idx_to_validator_id(idx))
            .cloned()
            .collect_vec();

        awaited_ids
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
    pub fn process_keygen_message(
        &mut self,
        sender_id: ValidatorId,
        msg: KeygenData,
    ) -> Option<KeygenResultInfo> {
        slog::trace!(
            self.logger,
            "[{}] received {} from [{}]",
            self.us(),
            &msg,
            sender_id
        );

        let signer_idx = match self.validator_id_to_signer_idx(&sender_id) {
            Some(idx) => idx,
            None => {
                slog::warn!(
                    self.logger,
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
                    }
                    StageStatus::MadeProgress => self.update_progress_timestamp(),
                    StageStatus::Ignored => { /* do nothing */ }
                }
            }
            (KeygenStage::AwaitingBroadcast1, KeygenData::Secret2(sec2)) => {
                slog::trace!(
                    self.logger,
                    "[{}] delaying Secret2 from [{}]",
                    self.us(),
                    sender_id
                );
                self.delayed_next_stage_data.push((sender_id, sec2.into()));
            }
            (KeygenStage::AwaitingSecret2, KeygenData::Secret2(sec2)) => {
                match self.sss.process_phase2(signer_idx, sec2) {
                    StageStatus::Full => {
                        slog::trace!(
                            self.logger,
                            "[{}] Phase 2 (keygen) successful ✅✅",
                            self.us()
                        );
                        self.update_progress_timestamp();

                        return self.finalize_phase2();
                    }
                    StageStatus::MadeProgress => {
                        self.update_progress_timestamp();
                    }
                    StageStatus::Ignored => { /* do nothing */ }
                }
            }
            (KeygenStage::Abandoned, data) => {
                slog::warn!(self.logger, "Dropping {} for abandoned keygen state", data);
            }
            _ => {
                slog::warn!(
                    self.logger,
                    "[{}] Unexpected message for stage: {:?}",
                    self.us(),
                    self.stage
                );
            }
        }

        return None;
    }

    fn initiate_keygen_inner(&mut self) {
        slog::trace!(
            self.logger,
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

                self.process_delayed();
            }
            Err(InvalidKey(blamed_idxs)) => {
                self.stage = KeygenStage::Abandoned;

                let blamed_ids = self.signer_idxs_to_validator_ids(blamed_idxs);

                slog::error!(
                    self.logger,
                    "phase2 keygen error for key: {:?}, blamed validators: {:?}",
                    self.key_id,
                    // TODO: this should log the base58 ids
                    &blamed_ids
                );

                let event =
                    InnerEvent::KeygenResult(KeygenOutcome::invalid(self.key_id, blamed_ids));

                self.send_event(event);
            }
        }
    }

    fn finalize_phase2(&mut self) -> Option<KeygenResultInfo> {
        match self.sss.finalize_phase2() {
            Ok(key) => {
                slog::info!(self.logger, "[{}] SHARED KEY IS READY 👍", self.us());

                self.stage = KeygenStage::KeyReady;

                let key_info = KeygenResultInfo {
                    key: Arc::new(key),
                    validator_map: Arc::clone(&self.maps_for_validator_id_and_idx),
                    params: self.params,
                };

                return Some(key_info);
            }
            Err(InvalidSS(blamed_idxs)) => {
                slog::error!(
                    self.logger,
                    "Invalid Phase2 keygen data, abandoning state for key: {:?}",
                    self.key_id
                );
                self.stage = KeygenStage::Abandoned;

                let blamed_ids = self.signer_idxs_to_validator_ids(blamed_idxs);

                self.send_event(InnerEvent::KeygenResult(KeygenOutcome::invalid(
                    self.key_id,
                    blamed_ids,
                )));
            }
        }

        None
    }

    fn signer_idxs_to_validator_ids(&self, idxs: Vec<usize>) -> Vec<ValidatorId> {
        idxs.into_iter()
            .map(|idx| self.signer_idx_to_validator_id(idx))
            .cloned()
            .collect()
    }

    fn send_event(&self, event: InnerEvent) {
        if let Err(err) = self.event_sender.send(event) {
            slog::error!(self.logger, "Could not send inner event: {}", err);
        }
    }

    fn send(&self, messages: Vec<MessageToSend>) {
        for MessageToSend { to_idx, data } in messages {
            let destination = self.signer_idx_to_validator_id(to_idx).clone();

            slog::debug!(
                self.logger,
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

            slog::debug!(self.logger, "[{}] Sending to {}", self.us(), destination);

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
            slog::trace!(
                self.logger,
                "[{}] Processing a delayed message from [{}]",
                self.us(),
                sender_id
            );
            self.process_keygen_message(sender_id, msg);
        }
    }

    /// check is the KeygenStage is Abandoned
    pub fn is_abandoned(&self) -> bool {
        match self.stage {
            KeygenStage::Abandoned => true,
            _ => false,
        }
    }

    /// check is the KeygenStage is in the KeyReady stage
    pub fn is_finished(&self) -> bool {
        match self.stage {
            KeygenStage::KeyReady => true,
            _ => false,
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
