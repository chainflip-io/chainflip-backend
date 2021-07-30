use crate::signing::{
    client::client_inner::utils,
    crypto::{
        BigInt, InvalidKey, InvalidSS, KeyGenBroadcastMessage1, Keys, Parameters, VerifiableSS, FE,
        GE,
    },
};

use super::{
    client_inner::{Broadcast1, Secret2},
    common::KeygenResult,
};

use log::*;

#[derive(Clone)]
enum SharedSecretStage {
    AwaitingBroadcast1,
    AwaitingSecret2,
    Done,
}

#[derive(Clone)]
pub struct SharedSecretState {
    stage: SharedSecretStage,
    key: Keys,
    // Phase 1
    bc1_vec: Vec<KeyGenBroadcastMessage1>,
    blind_vec: Vec<BigInt>,
    y_vec: Vec<GE>,
    pub phase1_order: Vec<usize>,
    // Phase 2
    vss_vec: Vec<VerifiableSS<GE>>,
    ss_vec: Vec<FE>,
    // Order in which the first broadcasts came in
    phase2_order: Vec<usize>,
    pub params: Parameters,
    signer_idx: usize,
}

/// Indicates whether we've collected all data
/// necessary to proceed to the next stage
pub enum StageStatus {
    Full,
    MadeProgress,
    Ignored,
}

impl SharedSecretState {
    pub fn init_phase1(&mut self) -> Broadcast1 {
        let (bc1, blind) = self.key.phase1_broadcast();

        // remember our own value
        self.bc1_vec.push(bc1.clone());
        self.blind_vec.push(blind.clone());
        self.y_vec.push(self.key.y_i);

        self.phase1_order.push(self.signer_idx);

        let y_i = self.key.y_i;

        // TODO: (Q) can we distribute bc1 and blind at the same time?
        Broadcast1 { bc1, blind, y_i }
    }

    pub fn process_broadcast1(&mut self, sender_id: usize, bc1: Broadcast1) -> StageStatus {
        if self.phase1_order.contains(&sender_id) {
            error!(
                "[{}] Received bc1 from the same sender idx: {}",
                self.signer_idx, sender_id
            );
            return StageStatus::Ignored;
        }

        self.phase1_order.push(sender_id);

        self.bc1_vec.push(bc1.bc1);
        self.blind_vec.push(bc1.blind);
        self.y_vec.push(bc1.y_i);

        let full = self.bc1_vec.len() == self.params.share_count;

        if full {
            // Reorganise all of our state, so they are ordered based on party idx
            utils::reorg_vector(&mut self.bc1_vec, &self.phase1_order);
            utils::reorg_vector(&mut self.blind_vec, &self.phase1_order);
            utils::reorg_vector(&mut self.y_vec, &self.phase1_order);
            return StageStatus::Full;
        }

        StageStatus::MadeProgress
    }

    pub fn init_phase2(&mut self, parties: &[usize]) -> Result<Vec<(usize, Secret2)>, InvalidKey> {
        trace!("[{}] entering phase 2", self.signer_idx);

        self.stage = SharedSecretStage::AwaitingSecret2;

        let bc1_vec = &self.bc1_vec;
        let blind_vec = &self.blind_vec;
        let y_vec = &self.y_vec;

        let params = &self.params;

        let res = self
            .key
            .phase1_verify_com_phase2_distribute(params, blind_vec, y_vec, bc1_vec, &parties);

        res.map(|(vss_scheme, secret_shares, _idx)| {
            debug!("[{}] phase 1 successful ✅", self.signer_idx);

            assert_eq!(secret_shares.len(), parties.len());

            let mut messages = vec![];

            // Share secret shares with the right parties
            for (idx, ss) in parties.iter().zip(secret_shares) {
                if *idx == self.signer_idx {
                    // Save our own value
                    self.vss_vec.push(vss_scheme.clone());
                    self.ss_vec.push(ss.clone());
                    self.phase2_order.push(self.signer_idx);
                } else {
                    let secret2 = Secret2 {
                        vss: vss_scheme.clone(),
                        secret_share: ss.clone(),
                    };

                    messages.push((*idx, secret2));
                }
            }

            messages
        })
    }

    pub fn process_phase2(&mut self, sender_idx: usize, sec2: Secret2) -> StageStatus {
        if self.phase2_order.contains(&sender_idx) {
            error!(
                "[{}] Received sec2 from the same sender idx: {}",
                self.signer_idx, sender_idx
            );
            return StageStatus::Ignored;
        }

        let Secret2 { vss, secret_share } = sec2;

        self.phase2_order.push(sender_idx);

        self.vss_vec.push(vss);
        self.ss_vec.push(secret_share);

        let full = self.vss_vec.len() == self.params.share_count;

        if full {
            utils::reorg_vector(&mut self.vss_vec, &self.phase2_order);
            utils::reorg_vector(&mut self.ss_vec, &self.phase2_order);
            return StageStatus::Full;
        }

        StageStatus::MadeProgress
    }

    pub fn finalize_phase2(&mut self) -> Result<KeygenResult, InvalidSS> {
        info!("[{}] entering phase 3", self.signer_idx);

        self.stage = SharedSecretStage::Done;

        let params = &self.params;
        let index = &self.signer_idx;

        let y_vec = &self.y_vec;
        let ss_vec = &self.ss_vec;
        let vss_vec = &self.vss_vec;

        let res = self
            .key
            .phase2_verify_vss_construct_keypair(params, y_vec, ss_vec, vss_vec, index);

        res.map(|shared_keys| {
            info!("[{}] phase 3 is OK", self.signer_idx);

            KeygenResult {
                keys: self.key.clone(),
                shared_keys,
                vss: self.vss_vec.clone(),
            }
        })
    }

    /// Get indexes of validators who haven't sent the data for the current stage
    pub fn awaited_parties(&self) -> Vec<usize> {
        let received_idxs = match self.stage {
            SharedSecretStage::AwaitingBroadcast1 => &self.phase1_order,
            SharedSecretStage::AwaitingSecret2 => &self.phase2_order,
            SharedSecretStage::Done => return vec![],
        };

        let mut idxs: Vec<usize> = (1..=self.params.share_count).collect();

        idxs.retain(|idx| !received_idxs.contains(idx));

        idxs
    }

    pub fn new(idx: usize, params: Parameters) -> Self {
        let key = Keys::phase1_create(idx);

        SharedSecretState {
            stage: SharedSecretStage::AwaitingBroadcast1,
            key,
            bc1_vec: vec![],
            blind_vec: vec![],
            y_vec: vec![],
            vss_vec: vec![],
            ss_vec: vec![],
            phase1_order: vec![],
            phase2_order: vec![],
            params,
            signer_idx: idx,
        }
    }
}
