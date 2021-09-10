use std::collections::HashMap;
#[cfg(test)]
use std::time::Duration;

use tokio::sync::mpsc;

use super::common::KeygenResultInfo;
use super::frost::SigningDataWrapped;
use super::InnerEvent;
use crate::p2p::AccountId;

use crate::signing::{MessageHash, MessageInfo, SigningInfo, SigningOutcome};

use super::signing_state::SigningState;

/// Responsible for mapping ceremonies to signing states and
/// Generating signer indexes based on the list of paries
#[derive(Clone)]
pub struct SigningManager {
    id: AccountId,
    event_sender: mpsc::UnboundedSender<InnerEvent>,
    signing_states: HashMap<MessageInfo, SigningState>,
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

        // Have to clone so it can be used inside the closure
        let logger = self.logger.clone();
        self.signing_states.retain(|message_info, state| {
            if let Some(bad_nodes) = state.try_expiring() {
                slog::warn!(logger, "Signing state expired and will be abandoned");
                let outcome = SigningOutcome::timeout(message_info.clone(), bad_nodes);

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

    pub fn on_request_to_sign(
        &mut self,
        data: MessageHash,
        key_info: KeygenResultInfo,
        sign_info: SigningInfo,
    ) {
        slog::debug!(self.logger, "Received request to sign");

        if !sign_info.signers.contains(&self.id) {
            slog::warn!(
                self.logger,
                "Request to sign ignored: we are not among signers."
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
                    "Request to sign ignored: could not derive our idx"
                );
                return;
            }
        };

        // Check that signer ids are known for this key
        let signer_idxs = match project_signers(&sign_info.signers, &key_info) {
            Ok(signer_idxs) => signer_idxs,
            Err(_) => {
                slog::warn!(self.logger, "Request to sign ignored: invalid signers.");
                return;
            }
        };

        let key_id = sign_info.id;

        let mi = MessageInfo { hash: data, key_id };

        let entry = self
            .signing_states
            .entry(mi.clone())
            .or_insert(SigningState::new_unauthorised());

        // We have the key and have received a request to sign
        slog::trace!(
            self.logger,
            "Creating new signing state for message: {:?}",
            mi.hash
        );

        entry.on_request_to_sign(
            our_idx,
            signer_idxs,
            key_info,
            mi.clone(),
            self.event_sender.clone(),
            &self.logger,
        );
    }

    pub fn process_signing_data(&mut self, sender_id: AccountId, wdata: SigningDataWrapped) {
        // Check if we have state for this data and delegate message to that state
        // Delay message otherwise

        let SigningDataWrapped { data, message: mi } = wdata;

        slog::info!(self.logger, "process_signing_data: {}", &data);

        let state = self
            .signing_states
            .entry(mi)
            .or_insert(SigningState::new_unauthorised());

        state.process_message(sender_id, data);
    }
}

#[cfg(test)]
impl SigningManager {
    pub fn set_timeout(&mut self, phase_timeout: Duration) {
        slog::info!(self.logger, "TODO: set timeout");
    }

    pub fn get_stage_for(&self, mi: &MessageInfo) -> Option<String> {
        self.signing_states.get(mi).and_then(|s| s.get_stage())
    }
}

/// Map all signer ids to their corresponding signer idx
fn project_signers(signer_ids: &[AccountId], info: &KeygenResultInfo) -> Result<Vec<usize>, ()> {
    // There is probably a more efficient way of doing this
    // for for now this shoud be good enough

    let mut results = Vec::with_capacity(signer_ids.len());
    for id in signer_ids {
        let idx = info.get_idx(id).ok_or(())?;
        results.push(idx);
    }

    Ok(results)
}
