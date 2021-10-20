use std::collections::HashMap;
use std::sync::Arc;

use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc;

use super::common::KeygenResultInfo;
use super::keygen::KeygenState;
use super::signing::frost::SigningDataWrapped;
use super::utils::PartyIdxMapping;
use crate::logging::CEREMONY_ID_KEY;

use crate::multisig::{client, InnerEvent, KeygenInfo, KeygenOutcome};

use crate::p2p::AccountId;
use client::{utils::get_index_mapping, KeygenDataWrapped};

use crate::multisig::{MessageHash, SigningOutcome};

use super::signing::SigningState;

/// Responsible for mapping ceremonies to signing states and
/// Generating signer indexes based on the list of parties
#[derive(Clone)]
pub struct CeremonyManager {
    my_account_id: AccountId,
    event_sender: mpsc::UnboundedSender<InnerEvent>,
    signing_states: HashMap<CeremonyId, SigningState>,
    keygen_states: HashMap<CeremonyId, KeygenState>,
    logger: slog::Logger,
}

impl CeremonyManager {
    pub fn new(
        my_account_id: AccountId,
        event_sender: mpsc::UnboundedSender<InnerEvent>,
        logger: &slog::Logger,
    ) -> Self {
        CeremonyManager {
            my_account_id,
            event_sender,
            signing_states: HashMap::new(),
            keygen_states: HashMap::new(),
            logger: logger.clone(),
        }
    }

    // This function is called periodically to check if any
    // ceremony should be aborted, reporting responsible parties
    // and cleaning up any relevant data
    pub fn cleanup(&mut self) {
        let mut events_to_send = vec![];

        let logger = &self.logger;
        self.signing_states.retain(|ceremony_id, state| {
            if let Some(bad_nodes) = state.try_expiring() {
                slog::warn!(logger, "Signing state expired and will be abandoned");
                let outcome = SigningOutcome::timeout(*ceremony_id, bad_nodes);

                events_to_send.push(InnerEvent::SigningResult(outcome));

                false
            } else {
                true
            }
        });

        self.keygen_states.retain(|ceremony_id, state| {
            if let Some(bad_nodes) = state.try_expiring() {
                slog::warn!(logger, "Keygen state expired and will be abandoned");
                let outcome = KeygenOutcome::timeout(*ceremony_id, bad_nodes);

                events_to_send.push(InnerEvent::KeygenResult(outcome));

                false
            } else {
                true
            }
        });

        for event in events_to_send {
            if let Err(err) = self.event_sender.send(event) {
                slog::error!(self.logger, "Unable to send event, error: {}", err);
            }
        }
    }

    fn map_ceremony_parties(
        &self,
        participants: &[AccountId],
        validator_mapping: &PartyIdxMapping,
    ) -> Result<(usize, Vec<usize>), &'static str> {
        if !participants.contains(&self.my_account_id) {
            // slog::warn!(logger, "Request to sign ignored: we are not among signers");
            return Err("we are not among participants");
        }

        // It should be impossible to fail here because of the check above,
        // but I don't like unwrapping (would be better if we
        // could combine this with the check above)
        let our_idx = validator_mapping
            .get_idx(&self.my_account_id)
            .ok_or("could not derive our idx")?;

        // Check that signer ids are known for this key
        let signer_idxs = validator_mapping
            .get_all_idxs(&participants)
            .map_err(|_| "invalid participants")?;

        Ok((our_idx, signer_idxs))
    }

    pub fn on_keygen_request(&mut self, keygen_info: KeygenInfo) {
        let KeygenInfo {
            ceremony_id,
            signers,
        } = keygen_info;

        let logger = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        let validator_map = Arc::new(get_index_mapping(&signers));

        let (our_idx, signer_idxs) = match self.map_ceremony_parties(&signers, &validator_map) {
            Ok(res) => res,
            Err(reason) => {
                // TODO: alert
                slog::warn!(logger, "Keygen request ignored: {}", reason);
                return;
            }
        };

        let logger = self.logger.clone();

        let entry = self
            .keygen_states
            .entry(ceremony_id)
            .or_insert_with(|| KeygenState::new_unauthorised(logger));

        entry.on_keygen_request(
            ceremony_id,
            self.event_sender.clone(),
            validator_map,
            our_idx,
            signer_idxs,
        );
    }

    // some functionality could be extracted and shared between keygen/signing
    pub fn on_request_to_sign(
        &mut self,
        data: MessageHash,
        key_info: KeygenResultInfo,
        mut signers: Vec<AccountId>,
        ceremony_id: CeremonyId,
    ) {
        let logger = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        slog::debug!(logger, "Processing a request to sign");

        let signers_expected = key_info.params.threshold + 1;

        // Hack to truncate the signers
        if signers.len() > signers_expected {
            slog::warn!(
                logger,
                "Request to sign contains more signers than necessary, truncating the list",
            );
            signers.truncate(signers_expected);
        } else if signers.len() < signers_expected {
            slog::warn!(
                logger,
                "Request to sign ignored: incorrect number of signers"
            );
            return;
        }

        // NOTE: truncation above might remove us (but it should never be applied anyway)

        let (our_idx, signer_idxs) =
            match self.map_ceremony_parties(&signers, &key_info.validator_map) {
                Ok(res) => res,
                Err(reason) => {
                    // TODO: alert
                    slog::warn!(logger, "Request to sign ignored: {}", reason);
                    return;
                }
            };

        // We have the key and have received a request to sign
        let logger = self.logger.clone();
        let state = self
            .signing_states
            .entry(ceremony_id)
            .or_insert_with(|| SigningState::new_unauthorised(logger));

        state.on_request_to_sign(
            ceremony_id,
            our_idx,
            signer_idxs,
            key_info,
            data,
            self.event_sender.clone(),
        );
    }

    pub fn process_signing_data(&mut self, sender_id: AccountId, wdata: SigningDataWrapped) {
        // Check if we have state for this data and delegate message to that state
        // Delay message otherwise

        let SigningDataWrapped { data, ceremony_id } = wdata;

        slog::trace!(self.logger, "Received signing data {}", &data; CEREMONY_ID_KEY => ceremony_id);

        let logger = self.logger.clone();
        let state = self
            .signing_states
            .entry(ceremony_id)
            .or_insert_with(|| SigningState::new_unauthorised(logger));

        state.process_message(sender_id, data);
    }

    pub fn process_keygen_data(
        &mut self,
        sender_id: AccountId,
        msg: KeygenDataWrapped,
    ) -> Option<KeygenResultInfo> {
        let KeygenDataWrapped { ceremony_id, data } = msg;

        // TODO: how can I avoid cloning the logger?
        let logger = self.logger.clone();

        let state = self
            .keygen_states
            .entry(ceremony_id)
            .or_insert_with(|| KeygenState::new_unauthorised(logger));

        let res = state.process_message(sender_id, data);

        // TODO: this is not a complete solution, we need to clean up the state
        // when it is failed too
        if res.is_some() {
            self.keygen_states.remove(&ceremony_id);
            slog::debug!(
                self.logger, "Removed a successfully finished keygen ceremony";
                CEREMONY_ID_KEY => ceremony_id
            );
        }

        res
    }
}

#[cfg(test)]
impl CeremonyManager {
    pub fn expire_all(&mut self) {
        for (_, state) in &mut self.signing_states {
            state.set_expiry_time(std::time::Instant::now());
        }

        for (_, state) in &mut self.keygen_states {
            state.set_expiry_time(std::time::Instant::now());
        }
    }

    pub fn get_signing_stage_for(&self, ceremony_id: CeremonyId) -> Option<String> {
        self.signing_states
            .get(&ceremony_id)
            .and_then(|s| s.get_stage())
    }

    pub fn get_keygen_stage_for(&self, ceremony_id: CeremonyId) -> Option<String> {
        self.keygen_states
            .get(&ceremony_id)
            .and_then(|s| s.get_stage())
    }
}
