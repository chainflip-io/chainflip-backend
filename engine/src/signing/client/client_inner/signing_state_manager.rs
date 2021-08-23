use std::{
    collections::{hash_map::Entry, HashMap},
    time::Duration,
};

use itertools::Itertools;
use slog::o;
use tokio::sync::mpsc;

use crate::{
    logging::COMPONENT_KEY,
    p2p::ValidatorId,
    signing::{
        client::{client_inner::client_inner::SigningData, SigningInfo, SigningOutcome},
        MessageHash, MessageInfo,
    },
};

use super::{
    client_inner::{Broadcast1, InnerEvent, SigningDataWrapped},
    common::KeygenResultInfo,
    signing_state::SigningState,
};

/// Manages multiple signing states for multiple signing processes
#[derive(Clone)]
pub struct SigningStateManager {
    signing_states: HashMap<MessageInfo, SigningState>,
    id: ValidatorId,
    p2p_sender: mpsc::UnboundedSender<InnerEvent>,
    /// Max lifetime of any phase before it expires
    /// and we abandon on the signing ceremony
    phase_timeout: Duration,
    /// Storage for messages for which we are not able to create a SigningState yet.
    /// Processing these is triggered by a request to sign
    delayed_messages: HashMap<MessageInfo, (std::time::Instant, Vec<(ValidatorId, Broadcast1)>)>,
    logger: slog::Logger,
}

impl SigningStateManager {
    pub fn new(
        id: ValidatorId,
        p2p_sender: mpsc::UnboundedSender<InnerEvent>,
        phase_timeout: Duration,
        logger: &slog::Logger,
    ) -> Self {
        SigningStateManager {
            signing_states: HashMap::new(),
            id,
            p2p_sender,
            phase_timeout,
            delayed_messages: HashMap::new(),
            logger: logger.new(o!(COMPONENT_KEY => "SigningStateManager")),
        }
    }

    #[cfg(test)]
    pub fn get_state_for(&self, message_info: &MessageInfo) -> Option<&SigningState> {
        self.signing_states.get(message_info)
    }

    #[cfg(test)]
    pub fn get_delayed_count(&self, message_info: &MessageInfo) -> usize {
        // BC1s are stored separately from the state
        let bc_count = self
            .delayed_messages
            .get(message_info)
            .map(|v| v.1.len())
            .unwrap_or(0);

        let other_count = self
            .signing_states
            .get(message_info)
            .map(|s| s.delayed_count())
            .unwrap_or(0);

        bc_count + other_count
    }

    #[cfg(test)]
    pub fn set_timeout(&mut self, phase_timeout: Duration) {
        self.phase_timeout = phase_timeout;
    }

    fn add_delayed(&mut self, mi: MessageInfo, bc1_entry: (ValidatorId, Broadcast1)) {
        slog::trace!(self.logger, "Signing manager adds delayed bc1");
        let entry = self
            .delayed_messages
            .entry(mi)
            .or_insert((std::time::Instant::now(), Vec::new()));
        entry.1.push(bc1_entry);
    }

    /// Process signing data, generating new state if necessary
    pub fn process_signing_data(&mut self, sender_id: ValidatorId, wdata: SigningDataWrapped) {
        let SigningDataWrapped { data, message } = wdata;

        slog::debug!(
            self.logger,
            "receiving signing data for message: {}",
            String::from_utf8_lossy(&message.hash.0)
        );

        match self.signing_states.get_mut(&message) {
            Some(state) => {
                state.process_signing_message(sender_id, data);
            }
            None => {
                match data {
                    SigningData::Broadcast1(bc1) => self.add_delayed(message, (sender_id, bc1)),
                    other => slog::warn!(
                        self.logger,
                        "Unexpected {} for message: {:?}",
                        other,
                        message.hash
                    ),
                };
            }
        }
    }

    fn process_delayed(&mut self, mi: &MessageInfo) {
        if let Some((_t, messages)) = self.delayed_messages.remove(mi) {
            for (sender, bc1) in messages {
                slog::debug!(self.logger, "Processing delayed signing bc1");

                let wdata = SigningDataWrapped {
                    data: bc1.into(),
                    message: mi.clone(),
                };
                self.process_signing_data(sender, wdata);
            }
        }
    }

    pub fn on_request_to_sign(
        &mut self,
        data: MessageHash,
        key_info: KeygenResultInfo,
        sign_info: SigningInfo,
    ) {
        slog::debug!(
            self.logger,
            "initiating signing for message: {}",
            String::from_utf8_lossy(&data.0)
        );

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

        match self.signing_states.entry(mi.clone()) {
            Entry::Occupied(_) => {
                slog::warn!(
                    self.logger,
                    "Ignoring a request to sign the same message again"
                );
            }
            Entry::Vacant(entry) => {
                // We have the key and have received a request to sign
                slog::trace!(
                    self.logger,
                    "Creating new signing state for message: {:?}",
                    mi.hash
                );
                let p2p_sender = self.p2p_sender.clone();

                let state = SigningState::on_request_to_sign(
                    self.id.clone(),
                    our_idx,
                    signer_idxs,
                    key_info,
                    p2p_sender,
                    mi.clone(),
                    sign_info,
                    &self.logger,
                );

                entry.insert(state);

                self.process_delayed(&mi);
            }
        }
    }

    /// check all states for timeouts and abandonment then remove them
    pub fn cleanup(&mut self) {
        let mut events_to_send = vec![];

        // remove all active states that have become abandoned or finished (SigningOutcome have already been sent)
        self.signing_states
            .retain(|_, state| !state.is_abandoned() && !state.is_finished());

        let timeout = self.phase_timeout;
        // for every pending state, check if it expired
        let logger = self.logger.clone();
        self.delayed_messages.retain(|message_info, (t, bc1_vec)| {
            if t.elapsed() > timeout {
                slog::warn!(logger, "BC1 for signing expired");

                // We never received a Signing request for this message, so any parties
                // that tried to initiate a new ceremony are deemed malicious
                let bad_validators = bc1_vec.iter().map(|(vid, _)| vid).cloned().collect_vec();
                events_to_send.push(SigningOutcome::unauthorised(
                    message_info.clone(),
                    bad_validators,
                ));
                return false;
            }
            true
        });

        // for every active state, check if it expired
        self.signing_states.retain(|message_info, state| {
            if state.last_progress_timestamp.elapsed() > timeout {
                slog::warn!(logger, "Signing state expired and should be abandoned");

                let late_nodes = state.awaited_parties();
                events_to_send.push(SigningOutcome::timeout(message_info.clone(), late_nodes));
                return false;
            }
            true
        });

        for event in events_to_send {
            if let Err(err) = self.p2p_sender.send(InnerEvent::SigningResult(event)) {
                slog::error!(logger, "Unable to send event, error: {}", err);
            }
        }
    }
}

/// Map all signer ids to their corresponding signer idx
fn project_signers(signer_ids: &[ValidatorId], info: &KeygenResultInfo) -> Result<Vec<usize>, ()> {
    // There is probably a more efficient way of doing this
    // for for now this shoud be good enough

    let mut results = Vec::with_capacity(signer_ids.len());
    for id in signer_ids {
        let idx = info.get_idx(id).ok_or(())?;
        results.push(idx);
    }

    Ok(results)
}
