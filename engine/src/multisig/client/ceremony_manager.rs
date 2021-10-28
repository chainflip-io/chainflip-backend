use std::collections::HashMap;
use std::sync::Arc;

use crate::multisig::client;

use client::{
    keygen_state_runner::KeygenStateRunner,
    signing::frost::{SigningData, SigningDataWrapped},
    state_runner::StateRunner,
    utils::{get_index_mapping, PartyIdxMapping},
    CeremonyAbortReason, EventSender, KeygenDataWrapped, SchnorrSignature,
};
use pallet_cf_vaults::CeremonyId;

use crate::logging::CEREMONY_ID_KEY;

use client::common::{broadcast::BroadcastStage, CeremonyCommon, KeygenResultInfo};

use crate::multisig::{InnerEvent, KeygenInfo, KeygenOutcome, MessageHash, SigningOutcome};

use crate::p2p::AccountId;

type SigningStateRunner = StateRunner<SigningData, SchnorrSignature>;

/// Responsible for mapping ceremonies to the corresponding states and
/// generating signer indexes based on the list of parties
#[derive(Clone)]
pub struct CeremonyManager {
    my_account_id: AccountId,
    event_sender: EventSender,
    signing_states: HashMap<CeremonyId, SigningStateRunner>,
    keygen_states: HashMap<CeremonyId, KeygenStateRunner>,
    logger: slog::Logger,
}

impl CeremonyManager {
    pub fn new(my_account_id: AccountId, event_sender: EventSender, logger: &slog::Logger) -> Self {
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

    /// Process a keygen request
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

        let logger = &self.logger;
        let state = self
            .keygen_states
            .entry(ceremony_id)
            .or_insert_with(|| KeygenStateRunner::new_unauthorised(logger));

        state.on_keygen_request(
            ceremony_id,
            self.event_sender.clone(),
            validator_map,
            our_idx,
            signer_idxs,
        );
    }

    /// Process a request to sign
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

        // Hack to truncate the signers (sorting is also done at a later point
        // but we don't care about duplicating it as this is temporary code)
        signers.sort();
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

        let (own_idx, signer_idxs) =
            match self.map_ceremony_parties(&signers, &key_info.validator_map) {
                Ok(res) => res,
                Err(reason) => {
                    // TODO: alert
                    slog::warn!(logger, "Request to sign ignored: {}", reason);
                    return;
                }
            };

        // We have the key and have received a request to sign
        let logger = &self.logger;
        let state = self
            .signing_states
            .entry(ceremony_id)
            .or_insert_with(|| SigningStateRunner::new_unauthorised(logger));

        let initial_stage = {
            use super::signing::{
                frost_stages::AwaitCommitments1, SigningP2PSender, SigningStateCommonInfo,
            };

            let common = CeremonyCommon {
                ceremony_id,
                p2p_sender: SigningP2PSender::new(
                    key_info.validator_map.clone(),
                    self.event_sender.clone(),
                    ceremony_id,
                ),
                own_idx,
                all_idxs: signer_idxs,
                logger: self.logger.clone(),
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

        state.on_ceremony_request(
            ceremony_id,
            initial_stage,
            key_info.validator_map,
            self.event_sender.clone(),
        );
    }

    /// Process data for a signing ceremony arriving from a peer
    pub fn process_signing_data(&mut self, sender_id: AccountId, wdata: SigningDataWrapped) {
        // Check if we have state for this data and delegate message to that state
        // Delay message otherwise

        let SigningDataWrapped { data, ceremony_id } = wdata;

        slog::trace!(self.logger, "Received signing data {}", &data; CEREMONY_ID_KEY => ceremony_id);

        let logger = &self.logger;
        let state = self
            .signing_states
            .entry(ceremony_id)
            .or_insert_with(|| SigningStateRunner::new_unauthorised(logger));

        if let Some(result) = state.process_message(sender_id, data) {
            self.keygen_states.remove(&ceremony_id);
            match result {
                Ok(schnorr_sig) => {
                    self.event_sender
                        .send(InnerEvent::SigningResult(SigningOutcome {
                            id: ceremony_id,
                            result: Ok(schnorr_sig),
                        }))
                        .unwrap();
                }
                Err(blamed_parties) => {
                    slog::warn!(
                        self.logger,
                        "Signing ceremony failed, blaming parties: {:?} ({:?})",
                        &blamed_parties,
                        blamed_parties,
                    );

                    self.event_sender
                        .send(InnerEvent::SigningResult(SigningOutcome {
                            id: ceremony_id,
                            result: Err((CeremonyAbortReason::Invalid, blamed_parties)),
                        }))
                        .unwrap();
                }
            }
        }
    }

    /// Process data for a keygen ceremony arriving from a peer
    pub fn process_keygen_data(
        &mut self,
        sender_id: AccountId,
        msg: KeygenDataWrapped,
    ) -> Option<KeygenResultInfo> {
        let KeygenDataWrapped { ceremony_id, data } = msg;

        let logger = &self.logger;
        let state = self
            .keygen_states
            .entry(ceremony_id)
            .or_insert_with(|| KeygenStateRunner::new_unauthorised(logger));

        state.process_message(sender_id, data).and_then(|res| {
            self.keygen_states.remove(&ceremony_id);
            slog::debug!(
                self.logger, "Removed a finished keygen ceremony";
                CEREMONY_ID_KEY => ceremony_id
            );

            match res {
                Ok(keygen_result_info) => Some(keygen_result_info),
                Err(blamed_parties) => {
                    slog::warn!(
                        self.logger,
                        "Keygen ceremony failed, blaming parties: {:?} ({:?})",
                        &blamed_parties,
                        blamed_parties,
                    );

                    self.event_sender
                        .send(InnerEvent::KeygenResult(KeygenOutcome {
                            id: ceremony_id,
                            result: Err((CeremonyAbortReason::Invalid, blamed_parties)),
                        }))
                        .unwrap();
                    None
                }
            }
        })
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
