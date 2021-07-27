use std::time::Instant;

use itertools::Itertools;
use slog::o;
use tokio::sync::mpsc;

use super::{
    client_inner::{InnerEvent, MultisigMessage, SigningDataWrapped},
    common::{KeygenResult, KeygenResultInfo},
};

use crate::{
    logging::COMPONENT_KEY,
    p2p::{P2PMessageCommand, ValidatorId},
    signing::{
        client::{
            client_inner::{
                shared_secret::StageStatus,
                utils::{self},
                SigningOutcome,
            },
            SigningInfo,
        },
        crypto::{InvalidKey, InvalidSS, LocalSig, Parameters, Signature},
        MessageInfo,
    },
};

use super::{client_inner::SigningData, shared_secret::SharedSecretState};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SigningStage {
    AwaitingBroadcast1,
    AwaitingSecret2,
    AwaitingLocalSig3,
    Finished,
    Abandoned,
}

#[derive(Clone)]
pub struct SigningState {
    id: ValidatorId,
    signer_idx: usize,
    pub message_info: MessageInfo,
    /// The result of the relevant keygen ceremony
    key_info: KeygenResultInfo,
    stage: SigningStage,
    /// Indices of participants who should participate
    signer_idxs: Vec<usize>,
    signer_ids: Vec<ValidatorId>,
    pub sss: SharedSecretState,
    pub shared_secret: Option<KeygenResult>,
    pub local_sigs: Vec<LocalSig>,
    pub local_sigs_order: Vec<usize>,
    event_sender: mpsc::UnboundedSender<InnerEvent>,
    /// Store data here if case it can't be consumed immediately
    pub delayed_data: Vec<(usize, SigningData)>,
    /// Time at which we transitioned to the current phase
    cur_phase_timestamp: Instant,
    /// The last time at which we made any progress (i.e. received
    /// useful data from peers)
    pub last_progress_timestamp: Instant,
    logger: slog::Logger,
}

impl SigningState {
    pub fn on_request_to_sign(
        id: ValidatorId,
        idx: usize,
        signer_idxs: Vec<usize>,
        key_info: KeygenResultInfo,
        p2p_sender: mpsc::UnboundedSender<InnerEvent>,
        mi: MessageInfo,
        si: SigningInfo,
        logger: &slog::Logger,
    ) -> Self {
        // Note that params for shared secret are different for signing
        // from that for keygen.
        // A secret will be shared between *all* signing parties
        // and would require full participation to "reconstruct" it
        // (i.e. t = n - 1)
        let threshold = key_info.params.threshold;
        let share_count = threshold + 1;

        let params = Parameters {
            threshold,
            share_count,
        };

        let now = Instant::now();

        let mut state = SigningState {
            id,
            signer_idx: idx,
            message_info: mi,
            key_info,
            stage: SigningStage::AwaitingBroadcast1,
            signer_idxs,
            signer_ids: si.signers,
            sss: SharedSecretState::new(idx, params, logger),
            shared_secret: None,
            local_sigs_order: vec![],
            local_sigs: vec![],
            event_sender: p2p_sender,
            delayed_data: vec![],
            cur_phase_timestamp: now,
            last_progress_timestamp: now,
            logger: logger.new(o!(COMPONENT_KEY => "SigningState")),
        };

        state.on_request_to_sign_inner();

        state
    }

    #[cfg(test)]
    pub fn get_stage(&self) -> SigningStage {
        self.stage
    }

    #[cfg(test)]
    pub fn delayed_count(&self) -> usize {
        self.delayed_data.len()
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

    /// Get ids of validators who haven't sent the data for the current stage
    pub fn awaited_parties(&self) -> Vec<ValidatorId> {
        let awaited_idxs = match self.stage {
            SigningStage::AwaitingBroadcast1 | SigningStage::AwaitingSecret2 => {
                self.sss.awaited_parties()
            }
            SigningStage::AwaitingLocalSig3 => {
                let received_idxs = &self.local_sigs_order;
                let mut idxs: Vec<usize> = (1..=self.sss.params.share_count).collect();
                idxs.retain(|idx| !received_idxs.contains(idx));
                idxs
            }
            SigningStage::Abandoned => vec![],
            SigningStage::Finished => vec![],
        };

        let awaited_ids = awaited_idxs
            .into_iter()
            .map(|idx| self.signer_idx_to_validator_id(idx))
            .collect_vec();

        awaited_ids
    }

    fn signer_idx_to_validator_id(&self, idx: usize) -> ValidatorId {
        let id = self.key_info.get_id(idx);
        id
    }

    fn send_event(&self, event: InnerEvent) {
        if let Err(err) = self.event_sender.send(event) {
            slog::error!(self.logger, "Could not send inner event: {}", err);
        }
    }

    fn record_local_sig(&mut self, signer_id: usize, sig: LocalSig) {
        self.local_sigs_order.push(signer_id);
        self.local_sigs.push(sig);
    }

    /// This is where we compute local shares of the signatures and distribute them
    pub fn init_local_sig(&mut self) -> LocalSig {
        let own_idx = self.signer_idx;
        let key = &self.key_info.key;

        let ss = self
            .shared_secret
            .as_ref()
            .expect("must have shared secret");

        let local_sig =
            LocalSig::compute(&self.message_info.hash.0, &ss.shared_keys, &key.shared_keys);

        self.record_local_sig(own_idx, local_sig.clone());

        local_sig
    }

    fn on_local_sig_received(&mut self, signer_id: usize, local_sig: LocalSig) {
        self.record_local_sig(signer_id, local_sig);

        let full = self.local_sigs.len() == self.sss.params.share_count;

        if full {
            utils::reorg_vector(&mut self.local_sigs, &self.local_sigs_order);
            self.on_local_sigs_collected();
        }
    }

    fn on_local_sigs_collected(&mut self) {
        slog::debug!(self.logger, "Collected all local sigs ✅✅✅");

        let key = &self.key_info.key;

        let ss = self
            .shared_secret
            .as_ref()
            .expect("must have shared secret");

        // The expected indices are expected to start with 0
        let mut parties_index_vec = self
            .local_sigs_order
            .clone()
            .into_iter()
            .map(|i| i.checked_sub(1).unwrap())
            .collect_vec();

        // NOTE: The order of indices is important, it needs to match
        // the one in the previous phase.
        parties_index_vec.sort_unstable();

        let verify_local_sig =
            LocalSig::verify_local_sigs(&self.local_sigs, &parties_index_vec, &key.vss, &ss.vss);

        match verify_local_sig {
            Ok(vss_sum_local_sigs) => {
                // each party / dealer can generate the signature
                let signature = Signature::generate(
                    &vss_sum_local_sigs,
                    &self.local_sigs,
                    &parties_index_vec,
                    ss.get_public_key(),
                );
                let verify_sig = signature.verify(&self.message_info.hash.0, &key.get_public_key());

                if verify_sig.is_ok() {
                    slog::info!(self.logger, "Generated signature is correct! 🎉");
                    self.send_event(InnerEvent::SigningResult(SigningOutcome::success(
                        self.message_info.clone(),
                        signature,
                    )));
                } else {
                    self.update_stage(SigningStage::Abandoned);

                    slog::error!(self.logger, 
                        "Unexpected signature verification failure. This should never happen. {:?}",
                        self.message_info
                    );

                    let event = InnerEvent::SigningResult(SigningOutcome::invalid(
                        self.message_info.clone(),
                        vec![],
                    ));

                    self.send_event(event);
                }
                self.update_stage(SigningStage::Finished);
            }
            Err(InvalidSS(blamed_idxs)) => {
                let blamed_ids = self.signer_idxs_to_validator_ids(blamed_idxs);
                self.update_stage(SigningStage::Abandoned);

                slog::error!(self.logger, 
                    "Local Sigs verify error for message: {:?}, blamed validators: {:?}",
                    self.message_info, &blamed_ids
                );

                let event = InnerEvent::SigningResult(SigningOutcome::invalid(
                    self.message_info.clone(),
                    blamed_ids,
                ));

                self.send_event(event);
            }
        }
    }

    fn update_progress_timestamp(&mut self) {
        self.last_progress_timestamp = Instant::now();
    }

    fn process_delayed(&mut self) {
        while let Some((sender_id, msg)) = self.delayed_data.pop() {
            slog::trace!(self.logger, "Processing a delayed message from [{}]", sender_id);
            self.process_signing_message_inner(sender_id, msg);
        }
    }

    fn update_stage(&mut self, stage: SigningStage) {
        // Make sure that the transition is valid
        match (self.stage, stage) {
            (SigningStage::AwaitingBroadcast1, SigningStage::AwaitingSecret2) => {}
            (SigningStage::AwaitingSecret2, SigningStage::AwaitingLocalSig3) => {}
            (SigningStage::AwaitingBroadcast1, SigningStage::Abandoned) => {}
            (SigningStage::AwaitingSecret2, SigningStage::Abandoned) => {}
            (SigningStage::AwaitingLocalSig3, SigningStage::Abandoned) => {}
            (SigningStage::AwaitingLocalSig3, SigningStage::Finished) => {}
            _ => {
                slog::error!(self.logger, "Invalid transition from {:?} to {:?}", self.stage, stage);
                panic!();
            }
        }

        self.stage = stage;
        let elapsed = self.cur_phase_timestamp.elapsed();
        self.cur_phase_timestamp = Instant::now();
        slog::debug!(self.logger, 
            "Entering phase {:?}. Previous phase took: {:?}",
            stage, elapsed
        );
    }

    fn on_request_to_sign_inner(&mut self) {
        let bc1 = self.sss.init_phase1();
        let bc1 = SigningData::Broadcast1(bc1);

        slog::trace!(self.logger, "[{}] Generated {}", self.us(), &bc1);

        let bc1 = SigningDataWrapped::new(bc1, self.message_info.clone());
        let msg = MultisigMessage::from(bc1);

        self.broadcast(msg);

        self.process_delayed();
    }

    fn add_delayed(&mut self, sender_id: usize, data: SigningData) {
        slog::trace!(self.logger, "Added a delayed message");
        self.delayed_data.push((sender_id, data));
    }

    pub fn process_signing_message(&mut self, sender_id: ValidatorId, msg: SigningData) {
        let sender_idx = self.key_info.get_idx(&sender_id);

        if let Some(idx) = sender_idx {
            self.process_signing_message_inner(idx, msg);
        } else {
        }
    }

    pub fn process_signing_message_inner(&mut self, sender_id: usize, msg: SigningData) {
        let active_parties = &self.signer_idxs;
        // Ignore if the the sender is not in active_parties
        if !active_parties.contains(&sender_id) {
            slog::warn!(self.logger, 
                "Ignoring a message from sender not in active_parties: {}",
                sender_id
            );
            return;
        }

        slog::trace!(self.logger, "[{}] received {} from [{}]", self.us(), &msg, sender_id);

        match (self.stage, msg) {
            (SigningStage::AwaitingBroadcast1, SigningData::Broadcast1(bc1)) => {
                match self.sss.process_broadcast1(sender_id, bc1) {
                    StageStatus::Full => {
                        self.update_progress_timestamp();
                        self.signing_phase2();
                        self.process_delayed(); // Process delayed Secret2
                    }
                    StageStatus::MadeProgress => self.update_progress_timestamp(),
                    StageStatus::Ignored => { /* do nothing */ }
                }
            }
            (SigningStage::AwaitingBroadcast1, SigningData::Secret2(sec2)) => {
                self.add_delayed(sender_id, SigningData::Secret2(sec2));
            }
            (SigningStage::AwaitingSecret2, SigningData::Secret2(sec2)) => {
                match self.sss.process_phase2(sender_id, sec2) {
                    StageStatus::Full => {
                        slog::info!(self.logger, "[{}] Phase 2 (signing) successful ✅✅", self.us());
                        self.update_progress_timestamp();
                        self.finalize_phase2();
                    }
                    StageStatus::MadeProgress => self.update_progress_timestamp(),
                    StageStatus::Ignored => { /* do nothing */ }
                }
            }
            (SigningStage::AwaitingSecret2, SigningData::LocalSig(sig)) => {
                self.add_delayed(sender_id, SigningData::LocalSig(sig));
            }
            (SigningStage::AwaitingLocalSig3, SigningData::LocalSig(sig)) => {
                self.on_local_sig_received(sender_id, sig);
            }
            (SigningStage::Abandoned, data) => {
                slog::warn!(self.logger, 
                    "Dropping {} for abandoned Signing state, Message: {:?}",
                    data, self.message_info
                );
            }
            (_, data) => {
                slog::warn!(self.logger, 
                    "Dropping unexpected message for stage {:?}, Dropped: {:?}",
                    self.stage, data
                );
            }
        }
    }

    fn signing_phase2(&mut self) {
        self.update_stage(SigningStage::AwaitingSecret2);
        slog::trace!(self.logger, "Parties Indexes: {:?}", self.sss.phase1_order);
        let mut parties = self.sss.phase1_order.clone();

        // TODO: investigate whether sorting is necessary
        parties.sort_unstable();

        match self.sss.init_phase2(&parties) {
            Ok(msgs) => {
                let msgs = msgs
                    .into_iter()
                    .map(|(idx, secret2)| {
                        let secret2 = SigningDataWrapped::new(secret2, self.message_info.clone());
                        let secret2 = MultisigMessage::from(secret2);
                        let data = serde_json::to_vec(&secret2).unwrap();

                        let id = self.key_info.get_id(idx);

                        P2PMessageCommand {
                            destination: id,
                            data,
                        }
                    })
                    .collect_vec();

                self.send(msgs)
            }
            Err(InvalidKey(blamed_idxs)) => {
                let blamed_ids = self.signer_idxs_to_validator_ids(blamed_idxs);

                slog::error!(self.logger, 
                    "phase2 signing error for message: {:?}, blamed validators: {:?}",
                    self.message_info, &blamed_ids
                );

                let event = InnerEvent::SigningResult(SigningOutcome::invalid(
                    self.message_info.clone(),
                    blamed_ids,
                ));

                self.send_event(event);

                self.update_stage(SigningStage::Abandoned);
            }
        }
    }

    fn signer_idxs_to_validator_ids(&self, idxs: Vec<usize>) -> Vec<ValidatorId> {
        idxs.into_iter()
            .map(|idx| self.signer_idx_to_validator_id(idx))
            .collect()
    }

    fn send(&self, msgs: Vec<P2PMessageCommand>) {
        for msg in msgs {
            let event = InnerEvent::P2PMessageCommand(msg);
            if let Err(err) = self.event_sender.send(event) {
                slog::error!(self.logger, "Could not send p2p message: {}", err);
            }
        }
    }

    fn broadcast(&self, data: MultisigMessage) {
        // TODO: see if there is a way to publish a bunch of messages
        // at once and whether that makes any difference performance-wise}

        for id in &self.signer_ids {
            if *id == self.id {
                continue;
            }

            slog::trace!(self.logger, "[{}] sending bc1 to [{}]", self.us(), id);

            let msg = P2PMessageCommand {
                destination: id.clone(),
                data: serde_json::to_vec(&data).unwrap(),
            };

            let event = InnerEvent::P2PMessageCommand(msg);

            if let Err(err) = self.event_sender.send(event) {
                slog::error!(self.logger, "Could not send p2p message: {}", err);
            }
        }
    }

    fn finalize_phase2(&mut self) {
        match self.sss.finalize_phase2() {
            Ok(key) => {
                slog::info!(self.logger, "[{}] SHARED KEY IS READY 👍", self.us());

                self.shared_secret = Some(key);

                self.init_local_sigs_phase();
                self.process_delayed(); // Process delayed LocalSig
            }
            Err(InvalidSS(blamed_idxs)) => {
                slog::error!(self.logger, 
                    "Invalid Phase2 keygen data, abandoning state for message_info: {:?}, Blaming: {:?}",
                    self.message_info,blamed_idxs
                );
                self.update_stage(SigningStage::Abandoned);

                let blamed_ids = self.signer_idxs_to_validator_ids(blamed_idxs);

                self.send_event(InnerEvent::SigningResult(SigningOutcome::invalid(
                    self.message_info.clone(),
                    blamed_ids,
                )));
            }
        }
    }

    fn init_local_sigs_phase(&mut self) {
        self.update_stage(SigningStage::AwaitingLocalSig3);

        let local_sig = self.init_local_sig();

        let local_sig = SigningData::LocalSig(local_sig);

        slog::debug!(self.logger, "[{}] generated {}!", self.us(), &local_sig);

        let local_sig = SigningDataWrapped {
            data: local_sig,
            message: self.message_info.clone(),
        };

        let msg = MultisigMessage::SigningMessage(local_sig);

        self.broadcast(msg);
    }

    /// check is the SigningStage is Abandoned
    pub fn is_abandoned(&self) -> bool {
        match self.stage {
            SigningStage::Abandoned => true,
            _ => false,
        }
    }

    /// check is the SigningStage is Finished
    pub fn is_finished(&self) -> bool {
        match self.stage {
            SigningStage::Finished => true,
            _ => false,
        }
    }
}
