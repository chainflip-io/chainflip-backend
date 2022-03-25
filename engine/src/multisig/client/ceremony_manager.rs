use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::common::format_iterator;
use crate::multisig::client::{self, MultisigOutcome};
use crate::multisig::crypto::Rng;
use crate::multisig_p2p::OutgoingMultisigStageMessages;
use state_chain_runtime::AccountId;

use client::{
    signing::frost::SigningData, state_runner::StateRunner, utils::PartyIdxMapping,
    CeremonyAbortReason, MultisigOutcomeSender, SchnorrSignature,
};
use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedSender;

use crate::logging::{
    CEREMONY_ID_KEY, KEYGEN_CEREMONY_FAILED, KEYGEN_REQUEST_IGNORED, REQUEST_TO_SIGN_IGNORED,
    SIGNING_CEREMONY_FAILED,
};

use client::common::{broadcast::BroadcastStage, CeremonyCommon, KeygenResultInfo};

use crate::multisig::{KeygenRequest, KeygenOutcome, MessageHash, SigningOutcome};

use super::ceremony_id_tracker::CeremonyIdTracker;
use super::keygen::{AwaitCommitments1, HashContext, KeygenData, KeygenOptions};

type SigningStateRunner = StateRunner<SigningData, SchnorrSignature>;
type KeygenStateRunner = StateRunner<KeygenData, KeygenResultInfo>;

/// Responsible for mapping ceremonies to the corresponding states and
/// generating signer indexes based on the list of parties
pub struct CeremonyManager {
    my_account_id: AccountId,
    outcome_sender: MultisigOutcomeSender,
    outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    signing_states: HashMap<CeremonyId, SigningStateRunner>,
    keygen_states: HashMap<CeremonyId, KeygenStateRunner>,
    ceremony_id_tracker: CeremonyIdTracker,
    logger: slog::Logger,
}

impl CeremonyManager {
    pub fn new(
        my_account_id: AccountId,
        outcome_sender: MultisigOutcomeSender,
        outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
        logger: &slog::Logger,
    ) -> Self {
        CeremonyManager {
            my_account_id,
            outcome_sender,
            outgoing_p2p_message_sender,
            signing_states: HashMap::new(),
            keygen_states: HashMap::new(),
            logger: logger.clone(),
            ceremony_id_tracker: CeremonyIdTracker::new(logger.clone()),
        }
    }

    // This function is called periodically to check if any
    // ceremony should be aborted, reporting responsible parties
    // and cleaning up any relevant data
    pub fn cleanup(&mut self) {
        // Copy the keys so we can iterate over them while at the same time
        // removing the elements as we go
        let signing_ids: Vec<_> = self.signing_states.keys().copied().collect();

        for ceremony_id in &signing_ids {
            let state = self
                .signing_states
                .get_mut(ceremony_id)
                .expect("state must exist");
            if let Some(result) = state.try_expiring() {
                // NOTE: we only respond (and consume the ceremony id)
                //  if we have received a ceremony request from
                // SC (i.e. the ceremony is "authorised")
                // TODO: report nodes via a different extrinsic instead
                // Only consume the ceremony id if it has been authorized
                if state.is_authorized() {
                    self.process_signing_ceremony_outcome(*ceremony_id, result);
                } else {
                    slog::warn!(self.logger, "Removing expired unauthorised signing ceremony"; CEREMONY_ID_KEY => ceremony_id);

                    self.signing_states.remove(ceremony_id);
                }
            }
        }

        let keygen_ids: Vec<_> = self.keygen_states.keys().copied().collect();

        for ceremony_id in &keygen_ids {
            let state = self
                .keygen_states
                .get_mut(ceremony_id)
                .expect("state must exist");
            if let Some(result) = state.try_expiring() {
                // NOTE: we only respond (and consume the ceremony id)
                // if we have received a ceremony request from
                // SC (i.e. the ceremony is "authorised")
                // TODO: report nodes via a different extrinsic instead
                if state.is_authorized() {
                    self.process_keygen_ceremony_outcome(*ceremony_id, result);
                } else {
                    slog::warn!(self.logger, "Removing expired unauthorised keygen ceremony"; CEREMONY_ID_KEY => ceremony_id);
                    self.keygen_states.remove(ceremony_id);
                }
            }
        }
    }

    fn map_ceremony_parties(
        &self,
        participants: &[AccountId],
        validator_map: &PartyIdxMapping,
    ) -> Result<(usize, BTreeSet<usize>), &'static str> {
        if !participants.contains(&self.my_account_id) {
            return Err("we are not among participants");
        }

        // It should be impossible to fail here because of the check above,
        // but I don't like unwrapping (would be better if we
        // could combine this with the check above)
        let our_idx = validator_map
            .get_idx(&self.my_account_id)
            .ok_or("could not derive our idx")?;

        // Check that signer ids are known for this key
        let signer_idxs = validator_map
            .get_all_idxs(participants)
            .map_err(|_| "invalid participants")?;

        if signer_idxs.len() != participants.len() {
            return Err("non unique participants");
        }

        Ok((our_idx, signer_idxs))
    }

    fn process_signing_ceremony_outcome(
        &mut self,
        ceremony_id: CeremonyId,
        result: Result<SchnorrSignature, (Vec<AccountId>, anyhow::Error)>,
    ) {
        self.signing_states.remove(&ceremony_id);
        self.ceremony_id_tracker.consume_signing_id(&ceremony_id);
        slog::debug!(
            self.logger, "Removed a finished signing ceremony";
            CEREMONY_ID_KEY => ceremony_id
        );

        match result {
            Ok(schnorr_sig) => {
                self.outcome_sender
                    .send(MultisigOutcome::Signing(SigningOutcome {
                        id: ceremony_id,
                        result: Ok(schnorr_sig),
                    }))
                    .unwrap();
            }
            Err((blamed_parties, reason)) => {
                slog::warn!(
                    self.logger,
                    #SIGNING_CEREMONY_FAILED,
                    "Signing ceremony failed: {}",
                    reason; "reported parties" =>
                    format_iterator(&blamed_parties).to_string(),
                    CEREMONY_ID_KEY => ceremony_id,
                );

                self.outcome_sender
                    .send(MultisigOutcome::Signing(SigningOutcome {
                        id: ceremony_id,
                        result: Err((CeremonyAbortReason::Invalid, blamed_parties)),
                    }))
                    .unwrap();
            }
        }
    }

    fn process_keygen_ceremony_outcome(
        &mut self,
        ceremony_id: CeremonyId,
        result: Result<KeygenResultInfo, (Vec<AccountId>, anyhow::Error)>,
    ) -> Option<KeygenResultInfo> {
        self.keygen_states.remove(&ceremony_id);
        self.ceremony_id_tracker.consume_keygen_id(&ceremony_id);
        slog::debug!(
            self.logger, "Removed a finished keygen ceremony";
            CEREMONY_ID_KEY => ceremony_id
        );

        match result {
            Ok(keygen_result_info) => Some(keygen_result_info),
            Err((blamed_parties, reason)) => {
                slog::warn!(
                    self.logger,
                    #KEYGEN_CEREMONY_FAILED,
                    "Keygen ceremony failed: {}",
                    reason; "reported parties" =>
                    format_iterator(&blamed_parties).to_string(),
                    CEREMONY_ID_KEY => ceremony_id,
                );

                self.outcome_sender
                    .send(MultisigOutcome::Keygen(KeygenOutcome {
                        id: ceremony_id,
                        result: Err((CeremonyAbortReason::Invalid, blamed_parties)),
                    }))
                    .unwrap();
                None
            }
        }
    }

    /// Process a keygen request
    pub fn on_keygen_request(
        &mut self,
        rng: Rng,
        keygen_request: KeygenRequest,
        keygen_options: KeygenOptions,
    ) {
        // TODO: Consider similarity in structure to on_request_to_sign(). Maybe possible to factor some commonality

        let KeygenRequest {
            ceremony_id,
            signers,
        } = keygen_request;

        let logger = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        let validator_map = Arc::new(PartyIdxMapping::from_unsorted_signers(&signers));

        let (our_idx, signer_idxs) = match self.map_ceremony_parties(&signers, &validator_map) {
            Ok(res) => res,
            Err(reason) => {
                slog::warn!(logger, #KEYGEN_REQUEST_IGNORED, "Keygen request ignored: {}", reason);
                return;
            }
        };

        if self
            .ceremony_id_tracker
            .is_keygen_ceremony_id_used(&ceremony_id)
        {
            slog::warn!(logger, #KEYGEN_REQUEST_IGNORED, "Keygen request ignored: ceremony id {} has already been used", ceremony_id);
            return;
        }

        let state = self
            .keygen_states
            .entry(ceremony_id)
            .or_insert_with(|| KeygenStateRunner::new_unauthorised(ceremony_id, &logger));

        let initial_stage = {
            let context = generate_keygen_context(ceremony_id, signers);

            let common = CeremonyCommon {
                ceremony_id,
                outgoing_p2p_message_sender: self.outgoing_p2p_message_sender.clone(),
                validator_mapping: validator_map.clone(),
                own_idx: our_idx,
                all_idxs: signer_idxs,
                logger: logger.clone(),
                rng,
            };

            let processor = AwaitCommitments1::new(common.clone(), keygen_options, context);

            Box::new(BroadcastStage::new(processor, common))
        };

        match state.on_ceremony_request(initial_stage, validator_map, self.outcome_sender.clone()) {
            Ok(Some(result)) => {
                self.process_keygen_ceremony_outcome(ceremony_id, result);
            }
            Err(reason) => {
                slog::warn!(self.logger, #KEYGEN_REQUEST_IGNORED, "Keygen request ignored: {}", reason);
            }
            _ => { /* nothing to do */ }
        };
    }

    /// Process a request to sign
    pub fn on_request_to_sign(
        &mut self,
        rng: Rng,
        data: MessageHash,
        key_info: KeygenResultInfo,
        signers: Vec<AccountId>,
        ceremony_id: CeremonyId,
    ) {
        let logger = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        slog::debug!(logger, "Processing a request to sign");

        // Check that the number of signers is enough
        let minimum_signers_needed = key_info.params.threshold + 1;
        if signers.len() < minimum_signers_needed {
            slog::warn!(
                logger,
                #REQUEST_TO_SIGN_IGNORED,
                "Request to sign ignored: not enough signers {}/{}",
                signers.len(), minimum_signers_needed
            );
            return;
        }

        let (own_idx, signer_idxs) = match self
            .map_ceremony_parties(&signers, &key_info.validator_map)
        {
            Ok(res) => res,
            Err(reason) => {
                slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "Request to sign ignored: {}", reason);
                return;
            }
        };

        if self
            .ceremony_id_tracker
            .is_signing_ceremony_id_used(&ceremony_id)
        {
            slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "Request to sign ignored: ceremony id {} has already been used", ceremony_id);
            return;
        }

        // We have the key and have received a request to sign
        let logger = &self.logger;

        let state = self
            .signing_states
            .entry(ceremony_id)
            .or_insert_with(|| SigningStateRunner::new_unauthorised(ceremony_id, logger));

        let initial_stage = {
            use super::signing::{frost_stages::AwaitCommitments1, SigningStateCommonInfo};

            let common = CeremonyCommon {
                ceremony_id,
                outgoing_p2p_message_sender: self.outgoing_p2p_message_sender.clone(),
                validator_mapping: key_info.validator_map.clone(),
                own_idx,
                all_idxs: signer_idxs,
                logger: self.logger.clone(),
                rng,
            };

            let processor = AwaitCommitments1::new(
                common.clone(),
                SigningStateCommonInfo {
                    data,
                    key: key_info.key.clone(),
                },
            );

            Box::new(BroadcastStage::new(processor, common))
        };

        match state.on_ceremony_request(
            initial_stage,
            key_info.validator_map,
            self.outcome_sender.clone(),
        ) {
            Ok(Some(result)) => {
                self.process_signing_ceremony_outcome(ceremony_id, result);
            }
            Err(reason) => {
                slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "Request to sign ignored: {}", reason);
            }
            _ => { /* nothing to do */ }
        };
    }

    /// Process data for a signing ceremony arriving from a peer
    pub fn process_signing_data(
        &mut self,
        sender_id: AccountId,
        ceremony_id: CeremonyId,
        data: SigningData,
    ) {
        // Check if we have state for this data and delegate message to that state
        // Delay message otherwise

        if self
            .ceremony_id_tracker
            .is_signing_ceremony_id_used(&ceremony_id)
        {
            slog::debug!(
                self.logger,
                "Ignoring signing data from old ceremony id {}",
                ceremony_id
            );
            return;
        }

        slog::debug!(self.logger, "Received signing data {}", &data; CEREMONY_ID_KEY => ceremony_id);

        let logger = &self.logger;
        let state = self
            .signing_states
            .entry(ceremony_id)
            .or_insert_with(|| SigningStateRunner::new_unauthorised(ceremony_id, logger));

        if let Some(result) = state.process_message(sender_id, data) {
            self.process_signing_ceremony_outcome(ceremony_id, result);
        }
    }

    /// Process data for a keygen ceremony arriving from a peer
    pub fn process_keygen_data(
        &mut self,
        sender_id: AccountId,
        ceremony_id: CeremonyId,
        data: KeygenData,
    ) -> Option<KeygenResultInfo> {
        if self
            .ceremony_id_tracker
            .is_keygen_ceremony_id_used(&ceremony_id)
        {
            slog::debug!(
                self.logger,
                "Ignoring keygen data from old ceremony id {}",
                ceremony_id
            );
            return None;
        }

        let logger = &self.logger;
        let state = self
            .keygen_states
            .entry(ceremony_id)
            .or_insert_with(|| KeygenStateRunner::new_unauthorised(ceremony_id, logger));

        state
            .process_message(sender_id, data)
            .and_then(|res| self.process_keygen_ceremony_outcome(ceremony_id, res))
    }
}

#[cfg(test)]
impl CeremonyManager {
    pub fn expire_all(&mut self) {
        for state in self.signing_states.values_mut() {
            state.set_expiry_time(std::time::Instant::now());
        }

        for state in self.keygen_states.values_mut() {
            state.set_expiry_time(std::time::Instant::now());
        }
    }

    pub fn get_signing_stage_for(&self, ceremony_id: CeremonyId) -> Option<String> {
        self.signing_states
            .get(&ceremony_id)
            .and_then(|s| s.get_stage())
    }

    pub fn get_signing_states_len(&self) -> usize {
        self.signing_states.len()
    }

    pub fn get_keygen_stage_for(&self, ceremony_id: CeremonyId) -> Option<String> {
        self.keygen_states
            .get(&ceremony_id)
            .and_then(|s| s.get_stage())
    }

    pub fn get_keygen_states_len(&self) -> usize {
        self.keygen_states.len()
    }
}

/// Create unique deterministic context used for generating a ZKP to prevent replay attacks
pub fn generate_keygen_context(
    ceremony_id: CeremonyId,
    mut signers: Vec<AccountId>,
) -> HashContext {
    use sha2::{Digest, Sha256};

    // We don't care if sorting is stable as all account ids are meant to be unique
    signers.sort_unstable();

    let mut hasher = Sha256::new();

    hasher.update(ceremony_id.to_be_bytes());

    // NOTE: it should be sufficient to use ceremony_id as context as
    // we never reuse the same id for different ceremonies, but lets
    // put the signers in to make the context hard to predict as well
    for id in signers {
        hasher.update(id);
    }

    HashContext(*hasher.finalize().as_ref())
}
