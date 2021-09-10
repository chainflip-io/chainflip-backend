use std::{
    collections::{hash_map::Entry, HashMap},
    time::{Duration, Instant},
};

use crate::{
    logging::COMPONENT_KEY,
    p2p::AccountId,
    signing::{
        client::{
            client_inner::utils::{get_index_mapping, threshold_from_share_count},
            KeyId, KeygenInfo,
        },
        crypto,
    },
};

use super::{
    client_inner::{Broadcast1, KeyGenMessageWrapped, KeygenData},
    common::KeygenResultInfo,
    keygen_state::KeygenState,
    utils::get_our_idx,
    InnerEvent, KeygenOutcome,
};

#[cfg(test)]
use super::keygen_state::KeygenStage;

use itertools::Itertools;
use slog::o;
use tokio::sync::mpsc::UnboundedSender;

/// Contains states (`KeygenState`) for different key ids. Responsible for directing
/// incoming messages to the relevant instance of `KeygenState`. Delays processing of
/// Broadcast1 messages before a corresponding keygen request is received.
#[derive(Clone)]
pub struct KeygenManager {
    /// States for each key id
    keygen_states: HashMap<KeyId, KeygenState>,
    /// Used to propagate events upstream
    event_sender: UnboundedSender<InnerEvent>,
    /// Validator id of our node
    our_id: AccountId,
    /// Storage for delayed data (only Broadcast1 makes sense here).
    /// We choose not to store it inside KeygenState, as having KeygenState currently
    /// implies that we have received the relevant keygen request
    /// (and know all parties involved), which is not always the case.
    delayed_messages: HashMap<KeyId, (Instant, Vec<(AccountId, Broadcast1)>)>,
    /// Abandon state for a given keygen if we can't make progress for longer than this
    phase_timeout: Duration,
    logger: slog::Logger,
}

impl KeygenManager {
    pub fn new(
        our_id: AccountId,
        event_sender: UnboundedSender<InnerEvent>,
        phase_timeout: Duration,
        logger: &slog::Logger,
    ) -> Self {
        KeygenManager {
            keygen_states: Default::default(),
            delayed_messages: Default::default(),
            event_sender,
            our_id,
            phase_timeout,
            logger: logger.new(o!(COMPONENT_KEY => "KeygenManager")),
        }
    }

    pub fn process_keygen_message(
        &mut self,
        sender_id: AccountId,
        msg: KeyGenMessageWrapped,
    ) -> Option<KeygenResultInfo> {
        let KeyGenMessageWrapped { key_id, message } = msg;

        match self.keygen_states.entry(key_id) {
            Entry::Occupied(mut state) => {
                // We have entry, process normally
                return state.get_mut().process_keygen_message(sender_id, message);
            }
            Entry::Vacant(_) => match message {
                KeygenData::Broadcast1(bc1) => {
                    slog::trace!(self.logger, "Delaying keygen bc1 for key id: {:?}", key_id);
                    self.add_delayed(key_id, sender_id, bc1);
                }
                KeygenData::Secret2(_) => {
                    slog::warn!(
                        self.logger,
                        "Unexpected keygen secret2 for key id: {:?}",
                        key_id
                    );
                }
            },
        };

        return None;
    }

    fn add_delayed(&mut self, key_id: KeyId, sender_id: AccountId, bc1: Broadcast1) {
        let entry = self
            .delayed_messages
            .entry(key_id)
            .or_insert((Instant::now(), Vec::new()));
        entry.1.push((sender_id, bc1));
    }

    /// check all states for timeouts and abandonment then remove them
    pub fn cleanup(&mut self) {
        let mut events_to_send = vec![];

        // remove all states that have become abandoned or finished (KeygenOutcome have already been sent)
        self.keygen_states
            .retain(|_, state| !state.is_abandoned() && !state.is_finished());

        let timeout = self.phase_timeout;
        // Remove all pending state that hasn't been updated for
        // longer than `self.phase_timeout`
        let logger_c = self.logger.clone();
        self.delayed_messages.retain(|key_id, (t, bc1_vec)| {
            if t.elapsed() > timeout {
                slog::warn!(
                    logger_c,
                    "Keygen state expired w/o keygen request for id: {:?}",
                    key_id
                );

                // We never received a keygen request for this key, so any parties
                // that tried to initiate a new ceremony are deemed malicious
                let bad_validators = bc1_vec.iter().map(|(vid, _)| vid).cloned().collect_vec();
                events_to_send.push(KeygenOutcome::unauthorised(*key_id, bad_validators));
                return false;
            }
            true
        });

        // remove any active states that are taking too long
        self.keygen_states.retain(|key_id, state| {
            if state.last_message_timestamp.elapsed() > timeout {
                slog::warn!(logger_c, "Keygen state expired for key id: {:?}", key_id);

                let late_nodes = state.awaited_parties();
                events_to_send.push(KeygenOutcome::timeout(*key_id, late_nodes));
                return false;
            }
            true
        });

        for event in events_to_send {
            if let Err(err) = self.event_sender.send(InnerEvent::KeygenResult(event)) {
                slog::error!(logger_c, "Unable to send event, error: {}", err);
            }
        }
    }

    /// Start the keygen ceremony
    pub fn on_keygen_request(&mut self, ki: KeygenInfo) {
        let KeygenInfo {
            id: key_id,
            signers,
        } = ki;

        match self.keygen_states.entry(key_id) {
            Entry::Occupied(_) => {
                // State should not have been created prior to receiving a keygen request
                slog::warn!(
                    self.logger,
                    "Ignoring a keygen request for a known key_id: {:?}",
                    key_id
                );
            }
            Entry::Vacant(entry) => match get_our_idx(&signers, &self.our_id) {
                Some(idx) => {
                    let idx_map = get_index_mapping(&signers);

                    let share_count = signers.len();
                    let threshold = threshold_from_share_count(share_count);

                    let params = crypto::Parameters {
                        threshold,
                        share_count,
                    };

                    let state = KeygenState::initiate(
                        idx,
                        params,
                        idx_map,
                        key_id,
                        self.event_sender.clone(),
                        &self.logger,
                    );

                    let state = entry.insert(state);

                    // Process delayed messages for `key_id`
                    if let Some((_, messages)) = self.delayed_messages.remove(&key_id) {
                        for (sender_id, msg) in messages {
                            state.process_keygen_message(sender_id, msg.into());
                        }
                    }

                    debug_assert!(self.delayed_messages.get(&key_id).is_none());
                }
                None => {
                    slog::error!(
                        self.logger,
                        "Unexpected keygen request w/o us as participants"
                    )
                }
            },
        }
    }
}

#[cfg(test)]
impl KeygenManager {
    pub fn get_state_for(&self, key_id: KeyId) -> Option<&KeygenState> {
        self.keygen_states.get(&key_id)
    }

    pub fn get_stage_for(&self, key_id: KeyId) -> Option<KeygenStage> {
        self.get_state_for(key_id).map(|s| s.get_stage())
    }

    pub fn set_timeout(&mut self, phase_timeout: Duration) {
        self.phase_timeout = phase_timeout;
    }

    pub fn get_delayed_count(&self, key_id: KeyId) -> usize {
        // BC1s are stored separately from the state
        let bc_count = self
            .delayed_messages
            .get(&key_id)
            .map(|v| v.1.len())
            .unwrap_or(0);

        let other_count = self
            .keygen_states
            .get(&key_id)
            .map(|s| s.delayed_count())
            .unwrap_or(0);

        bc_count + other_count
    }
}
