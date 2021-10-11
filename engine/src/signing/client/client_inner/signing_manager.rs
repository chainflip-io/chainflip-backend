use std::collections::HashMap;

use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc;

use super::common::KeygenResultInfo;
use super::frost::SigningDataWrapped;
use super::InnerEvent;
use crate::p2p::AccountId;

use crate::signing::{MessageHash, SigningOutcome};

use super::signing_state::SigningState;

/// Responsible for mapping ceremonies to signing states and
/// Generating signer indexes based on the list of paries
#[derive(Clone)]
pub struct SigningManager {
    // rename acctid
    id: AccountId,
    event_sender: mpsc::UnboundedSender<InnerEvent>,
    signing_states: HashMap<CeremonyId, SigningState>,
    logger: slog::Logger,
}

impl SigningManager {
    pub fn new(
        id: AccountId,
        event_sender: mpsc::UnboundedSender<InnerEvent>,
        logger: &slog::Logger,
    ) -> Self {
        SigningManager {
            id,
            event_sender,
            signing_states: HashMap::new(),
            logger: logger.clone(),
        }
    }

    // This function is called periodically to check if any
    // ceremony should be aborted, reporting responsible parites
    // and cleaning up any relevant data
    pub fn cleanup(&mut self) {
        let mut events_to_send = vec![];

        let logger = self.logger.clone();
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

        for event in events_to_send {
            if let Err(err) = self.event_sender.send(event) {
                slog::error!(self.logger, "Unable to send event, error: {}", err);
            }
        }
    }

    pub fn start_signing_data(
        &mut self,
        data: MessageHash,
        key_info: KeygenResultInfo,
        mut signers: Vec<AccountId>,
        ceremony_id: CeremonyId,
    ) {
        slog::debug!(
            self.logger,
            "Processing a request to sign [ceremony_id: {}]",
            ceremony_id
        );

        // Hack to truncate the signers - ticket?
        if signers.len() > (key_info.params.threshold + 1) {
            slog::warn!(
                self.logger,
                "Request to sign contains more signers than necessary, truncating the list [ceremony_id: {}]",
                ceremony_id
            );
            signers.truncate(key_info.params.threshold + 1);
        }

        if signers.len() != key_info.params.threshold + 1 {
            slog::warn!(
                self.logger,
                "Request to sign ignored: incorrect number of signers [ceremony_id: {}]",
                ceremony_id
            );
            return;
        }

        // try combine
        // y warn
        if !signers.contains(&self.id) {
            // TODO: alert
            slog::warn!(
                self.logger,
                "Request to sign ignored: we are not among signers [ceremony_id: {}]",
                ceremony_id
            );
            return;
        }

        let our_idx = match key_info.get_idx(&self.id) {
            Some(idx) => idx,
            None => {
                // This should be impossible because of the check above,
                // but I don't like unwrapping (would be better if we
                // could combine this with the check above)
                slog::warn!(
                    self.logger,
                    "Request to sign ignored: could not derive our idx [ceremony_id: {}]",
                    ceremony_id
                );
                return;
            }
        };

        // Check that signer ids are known for this key
        let signer_idxs = match project_signers(&signers, &key_info) {
            Ok(signer_idxs) => signer_idxs,
            Err(_) => {
                // TODO: alert
                slog::warn!(
                    self.logger,
                    "Request to sign ignored: invalid signers [ceremony_id: {}]",
                    ceremony_id
                );
                return;
            }
        };

        // We have the key and have received a request to sign
        let entry = self
            .signing_states
            .entry(ceremony_id)
            .or_insert(SigningState::new_unauthorised(self.logger.clone()));

        entry.on_request_to_sign_with_state(
            ceremony_id,
            our_idx,
            signer_idxs,
            key_info,
            data,
            self.event_sender.clone(),
            &self.logger,
        );
    }

    pub fn process_signing_data(&mut self, sender_id: AccountId, wdata: SigningDataWrapped) {
        // Check if we have state for this data and delegate message to that state
        // Delay message otherwise

        let SigningDataWrapped { data, ceremony_id } = wdata;

        slog::trace!(
            self.logger,
            "Received signing data {}: [ceremony_id: {}]",
            &data,
            ceremony_id
        );

        let state = self
            .signing_states
            .entry(ceremony_id)
            .or_insert(SigningState::new_unauthorised(self.logger.clone()));

        state.process_message(sender_id, data);
    }
}

#[cfg(test)]
impl SigningManager {
    pub fn expire_all(&mut self) {
        for (_, state) in &mut self.signing_states {
            state.set_expiry_time(std::time::Instant::now());
        }
    }

    pub fn get_stage_for(&self, ceremony_id: CeremonyId) -> Option<String> {
        self.signing_states
            .get(&ceremony_id)
            .and_then(|s| s.get_stage())
    }
}

/// Map all signer ids to their corresponding signer idx
fn project_signers(signer_ids: &[AccountId], info: &KeygenResultInfo) -> Result<Vec<usize>, ()> {
    // There is probably a more efficient way of doing this
    // for for now this should be good enough

    let mut results = Vec::with_capacity(signer_ids.len());
    for id in signer_ids {
        let idx = info.get_idx(id).ok_or(())?;
        results.push(idx);
    }

    Ok(results)
}
