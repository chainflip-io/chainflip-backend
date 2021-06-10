use std::time::Instant;

use itertools::Itertools;
use log::*;
use tokio::sync::mpsc;

use super::client_inner::{InnerEvent, MultisigMessage, SigningDataWrapped};

use crate::{
    p2p::P2PMessageCommand,
    signing::{
        client::{
            client_inner::{utils, InnerSignal},
            SigningInfo,
        },
        crypto::{Keys, LocalSig, Parameters, SharedKeys, Signature, VerifiableSS, GE},
        MessageInfo,
    },
};

use super::{client_inner::SigningData, shared_secret::SharedSecretState};

#[derive(Clone)]
pub(super) struct KeygenResult {
    pub(super) keys: Keys,
    pub(super) shared_keys: SharedKeys,
    pub(super) aggregate_pubkey: GE,
    pub(super) vss: Vec<VerifiableSS<GE>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum SigningStage {
    /// Have not received a request to sign from our node
    Idle,
    AwaitingBroadcast1,
    AwaitingSecret2,
    AwaitingLocalSig3,
}

#[derive(Clone)]
pub(super) struct SigningState {
    signer_idx: usize,
    pub(super) message_info: MessageInfo,
    /// The key might not be available yet, as we might want to
    /// keep some state even before we've finalized keygen
    signing_key: Option<KeygenResult>,
    stage: SigningStage,
    /// Indices(?) of participants who should participate
    signers: Option<Vec<usize>>,
    pub(super) sss: SharedSecretState,
    pub(super) shared_secret: Option<KeygenResult>,
    pub(super) local_sigs: Vec<LocalSig>,
    pub(super) local_sigs_order: Vec<usize>,
    event_sender: mpsc::UnboundedSender<InnerEvent>,
    /// Store data here if case it can't be consumed immediately
    pub(super) delayed_data: Vec<(usize, SigningData)>,
    pub(super) cur_phase_timestamp: Instant,
}

impl SigningState {
    pub(super) fn new(
        idx: usize,
        signing_key: Option<KeygenResult>,
        params: Parameters,
        p2p_sender: mpsc::UnboundedSender<InnerEvent>,
        mi: MessageInfo,
    ) -> Self {
        // Note that params are different for shared secret during the signing state (TODO: investigate why?)
        let params = Parameters {
            threshold: params.threshold,
            share_count: params.threshold + 1,
        };

        let min_parties = params.threshold + 1;

        SigningState {
            signer_idx: idx,
            message_info: mi,
            signing_key,
            stage: SigningStage::Idle,
            signers: None,
            sss: SharedSecretState::new(idx, params, min_parties),
            shared_secret: None,
            local_sigs_order: vec![],
            local_sigs: vec![],
            event_sender: p2p_sender,
            delayed_data: vec![],
            cur_phase_timestamp: Instant::now(),
        }
    }

    #[cfg(test)]
    pub(super) fn get_stage(&self) -> SigningStage {
        self.stage
    }

    #[cfg(test)]
    pub(super) fn delayed_count(&self) -> usize {
        self.delayed_data.len()
    }

    fn record_local_sig(&mut self, signer_id: usize, sig: LocalSig) {
        self.local_sigs_order.push(signer_id);
        self.local_sigs.push(sig);
    }

    /// This is where we compute local shares of the signatures and distribute them
    pub(super) fn init_local_sig(&mut self) -> LocalSig {
        let own_idx = self.signer_idx;
        let key = &self.signing_key.as_ref().expect("must have key");

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

    fn on_local_sigs_collected(&self) {
        debug!("Collected all local sigs ✅✅✅");

        let key = self.signing_key.as_ref().expect("must have key");

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
                    ss.aggregate_pubkey,
                );
                let verify_sig = signature.verify(&self.message_info.hash.0, &key.aggregate_pubkey);
                assert!(verify_sig.is_ok());

                if verify_sig.is_ok() {
                    info!("Generated signature is correct! 🎉");
                    let _ = self.event_sender.send(InnerEvent::InnerSignal(
                        InnerSignal::MessageSigned(self.message_info.clone()),
                    ));
                }
            }
            Err(_) => {
                // TODO: emit a signal and remove local state
                warn!("Invalid local signatures, aborting. ❌");
            }
        }
    }

    fn process_delayed(&mut self) {
        while let Some((sender_id, msg)) = self.delayed_data.pop() {
            trace!("Processing a delayed message from [{}]", sender_id);
            self.process_signing_message(sender_id, msg);
        }
    }

    fn update_stage(&mut self, stage: SigningStage) {
        // Make sure that the transition is valid
        match (self.stage, stage) {
            (SigningStage::Idle, SigningStage::AwaitingBroadcast1) => {}
            (SigningStage::AwaitingBroadcast1, SigningStage::AwaitingSecret2) => {}
            (SigningStage::AwaitingSecret2, SigningStage::AwaitingLocalSig3) => {}
            _ => {
                error!("Invalid transition from {:?} to {:?}", self.stage, stage);
                panic!();
            }
        }

        self.stage = stage;
        let elapsed = self.cur_phase_timestamp.elapsed();
        self.cur_phase_timestamp = Instant::now();
        debug!(
            "Entering phase {:?}. Previous phase took: {:?}",
            stage, elapsed
        );
    }

    pub fn set_key(&mut self, key: KeygenResult) {
        self.signing_key = Some(key);
    }

    pub fn on_request_to_sign(&mut self, info: SigningInfo) {
        self.update_stage(SigningStage::AwaitingBroadcast1);

        let SigningInfo { id: _, signers } = info;

        self.signers = Some(signers);

        let bc1 = self.sss.init_phase1();
        let bc1 = SigningData::Broadcast1(bc1);
        let bc1 = SigningDataWrapped::new(bc1, self.message_info.clone());
        let msg = MultisigMessage::from(bc1);

        trace!("[{}] Signing: created BC1", self.signer_idx);

        self.broadcast(msg);

        self.process_delayed();
    }

    fn add_delayed(&mut self, sender_id: usize, data: SigningData) {
        trace!("Added a delayed message");
        self.delayed_data.push((sender_id, data));
    }

    pub fn process_signing_message(&mut self, sender_id: usize, msg: SigningData) {
        if let SigningStage::Idle = self.stage {
            // do nothing yet
        } else {
            // MAXIM: need to make sure (add tests) that for any combination state/message we don't crash!
            // (it's happened a few times during development)

            // Ignore if the the sender is not in active_parties
            let active_parties = self.signers.as_ref().expect("should know active parties");

            if !active_parties.contains(&sender_id) {
                warn!(
                    "Ignoring a message from sender not in active_parties: {}",
                    sender_id
                );
                return;
            }
        }

        match (self.stage, msg) {
            (SigningStage::Idle, SigningData::Broadcast1(bc1)) => {
                self.add_delayed(sender_id, SigningData::Broadcast1(bc1));
            }
            (SigningStage::AwaitingBroadcast1, SigningData::Broadcast1(bc1)) => {
                trace!("[{}] received bc1 from [{}]", self.signer_idx, sender_id);
                if self.sss.process_broadcast1(sender_id, bc1) {
                    self.signing_phase2();
                    self.process_delayed(); // Process delayed Secret2
                }
            }
            (SigningStage::AwaitingBroadcast1, SigningData::Secret2(sec2)) => {
                self.add_delayed(sender_id, SigningData::Secret2(sec2));
            }
            (SigningStage::AwaitingSecret2, SigningData::Secret2(sec2)) => {
                trace!(
                    "[{}] received secret2 from [{}]",
                    self.signer_idx,
                    sender_id
                );

                if self.sss.process_phase2(sender_id, sec2) {
                    info!("[{}] Phase 2 (signing) successful ✅✅", self.signer_idx);
                    if let Ok(key) = self.sss.init_phase3() {
                        info!("[{}] SHARED SECRET IS READY 👍", self.signer_idx);

                        self.shared_secret = Some(key);

                        self.init_local_sigs_phase();
                        self.process_delayed(); // Process delayed LocalSig
                    }
                }
            }
            (SigningStage::AwaitingSecret2, SigningData::LocalSig(sig)) => {
                self.add_delayed(sender_id, SigningData::LocalSig(sig));
            }
            (SigningStage::AwaitingLocalSig3, SigningData::LocalSig(sig)) => {
                trace!("[{}] Received Local Sig", self.signer_idx);

                self.on_local_sig_received(sender_id, sig);
            }
            _ => {
                warn!("Dropping unexpected message for stage {:?}", self.stage);
            }
        }
    }

    fn signing_phase2(&mut self) {
        self.update_stage(SigningStage::AwaitingSecret2);
        trace!("Parties Indexes: {:?}", self.sss.phase1_order);
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
                        P2PMessageCommand {
                            destination: idx,
                            data,
                        }
                    })
                    .collect_vec();

                self.send(msgs)
            }
            Err(_) => {
                error!("phase2 keygen error")
            }
        }
    }

    fn send(&self, msgs: Vec<P2PMessageCommand>) {
        for msg in msgs {
            let event = InnerEvent::P2PMessageCommand(msg);
            if let Err(err) = self.event_sender.send(event) {
                error!("Could not send p2p message: {}", err);
            }
        }
    }

    fn broadcast(&self, data: MultisigMessage) {
        // TODO: see if there is a way to publish a bunch of messages
        // at once and whether that makes any difference performance-wise}

        let active_parties = self.signers.as_ref().expect("should know active parties");

        for idx in active_parties {
            if *idx == self.signer_idx {
                continue;
            }

            trace!("[{}] sending bc1 to [{}]", self.signer_idx, idx);

            let msg = P2PMessageCommand {
                destination: *idx,
                data: serde_json::to_vec(&data).unwrap(),
            };

            let event = InnerEvent::P2PMessageCommand(msg);

            if let Err(err) = self.event_sender.send(event) {
                error!("Could not send p2p message: {}", err);
            }
        }
    }

    fn init_local_sigs_phase(&mut self) {
        self.update_stage(SigningStage::AwaitingLocalSig3);

        let local_sig = self.init_local_sig();

        let local_sig = SigningData::LocalSig(local_sig);
        let local_sig = SigningDataWrapped {
            data: local_sig,
            message: self.message_info.clone(),
        };

        let msg = MultisigMessage::SigningMessage(local_sig);

        self.broadcast(msg);

        debug!("[{}] generated local sig!", self.signer_idx);
    }
}
