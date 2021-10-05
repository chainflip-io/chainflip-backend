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
            CeremonyId, KeygenInfo,
        },
        crypto,
    },
};

use super::{
    client_inner::{Broadcast1, KeyGenMessageWrapped, LegacyKeygenData},
    common::KeygenResultInfo,
    legacy_keygen_state::LegacyKeygenState,
    utils::get_our_idx,
    InnerEvent, KeygenOutcome,
};

#[cfg(test)]
use super::legacy_keygen_state::KeygenStage;

use itertools::Itertools;
use slog::o;
use tokio::sync::mpsc::UnboundedSender;

/// Contains states (`KeygenState`) for different key ids. Responsible for directing
/// incoming messages to the relevant instance of `KeygenState`. Delays processing of
/// Broadcast1 messages before a corresponding keygen request is received.
#[derive(Clone)]
pub struct LegacyKeygenManager {
    /// States for each ceremony_id
    keygen_states: HashMap<CeremonyId, LegacyKeygenState>,
    /// Used to propagate events upstream
    inner_event_sender: UnboundedSender<InnerEvent>,
    /// Validator id of our node
    our_id: AccountId,
    /// Storage for delayed data (only Broadcast1 makes sense here).
    /// We choose not to store it inside KeygenState, as having KeygenState currently
    /// implies that we have received the relevant keygen request
    /// (and know all parties involved), which is not always the case.
    /// The `Instant` is the time at which the first messages was added to the delayed_messages queue
    delayed_messages: HashMap<CeremonyId, (Instant, Vec<(AccountId, Broadcast1)>)>,
    /// Abandon state for a given keygen if we can't make progress for longer than this
    phase_timeout: Duration,
    logger: slog::Logger,
}

impl LegacyKeygenManager {
    pub fn new(
        our_id: AccountId,
        inner_event_sender: UnboundedSender<InnerEvent>,
        phase_timeout: Duration,
        logger: &slog::Logger,
    ) -> Self {
        LegacyKeygenManager {
            keygen_states: Default::default(),
            delayed_messages: Default::default(),
            inner_event_sender,
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
        let KeyGenMessageWrapped { ceremony_id, data } = msg;
        slog::debug!(
            self.logger,
            "[{}] Processing a {} keygen message for ceremony_id: {:?}",
            self.our_id,
            data,
            ceremony_id
        );

        match self.keygen_states.entry(ceremony_id) {
            Entry::Occupied(mut state) => {
                return state.get_mut().process_keygen_message(sender_id, data);
            }
            Entry::Vacant(_) => match data {
                LegacyKeygenData::Broadcast1(bc1) => {
                    slog::trace!(
                        self.logger,
                        "Delaying keygen bc1 for ceremony id: {:?}",
                        ceremony_id
                    );
                    self.add_delayed(ceremony_id, sender_id, bc1);
                }
                LegacyKeygenData::Secret2(_) => {
                    slog::warn!(
                        self.logger,
                        "Unexpected keygen secret2 for ceremony id: {:?}",
                        ceremony_id
                    );
                }
            },
        };

        return None;
    }

    fn add_delayed(&mut self, ceremony_id: CeremonyId, sender_id: AccountId, bc1: Broadcast1) {
        let entry = self
            .delayed_messages
            .entry(ceremony_id)
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
        self.delayed_messages.retain(|ceremony_id, (t, bc1_vec)| {
            if t.elapsed() > timeout {
                slog::warn!(
                    logger_c,
                    "Keygen state expired w/o keygen request for ceremony id: {:?}",
                    ceremony_id
                );

                // We never received a keygen request for this key, so any parties
                // that tried to initiate a new ceremony are deemed malicious
                let bad_validators = bc1_vec.iter().map(|(vid, _)| vid).cloned().collect_vec();
                events_to_send.push(KeygenOutcome::unauthorised(*ceremony_id, bad_validators));
                return false;
            }
            true
        });

        // remove any active states that are taking too long
        self.keygen_states.retain(|ceremony_id, state| {
            if state.last_message_timestamp.elapsed() > timeout {
                slog::warn!(
                    logger_c,
                    "Keygen state expired for ceremony id: {:?}",
                    ceremony_id
                );
                let late_nodes = state.awaited_parties();
                events_to_send.push(KeygenOutcome::timeout(*ceremony_id, late_nodes));
                return false;
            }
            true
        });

        for event in events_to_send {
            if let Err(err) = self
                .inner_event_sender
                .send(InnerEvent::KeygenResult(event))
            {
                slog::error!(logger_c, "Unable to send event, error: {}", err);
            }
        }
    }

    /// Start the keygen ceremony
    pub fn on_keygen_request(&mut self, ki: KeygenInfo) {
        let KeygenInfo {
            ceremony_id,
            signers,
        } = ki;

        match self.keygen_states.entry(ceremony_id) {
            Entry::Occupied(_) => {
                // State should not have been created prior to receiving a keygen request
                slog::warn!(
                    self.logger,
                    "Ignoring a keygen request for a known ceremony id: {:?}",
                    ceremony_id
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

                    let state = LegacyKeygenState::initiate(
                        idx,
                        params,
                        idx_map,
                        ceremony_id,
                        self.inner_event_sender.clone(),
                        &self.logger,
                    );

                    let state = entry.insert(state);

                    // Process delayed messages for `ceremony_id`
                    if let Some((_, messages)) = self.delayed_messages.remove(&ceremony_id) {
                        for (sender_id, msg) in messages {
                            state.process_keygen_message(sender_id, msg.into());
                        }
                    }

                    debug_assert!(self.delayed_messages.get(&ceremony_id).is_none());
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
impl LegacyKeygenManager {
    pub fn get_state_for(&self, ceremony_id: CeremonyId) -> Option<&LegacyKeygenState> {
        self.keygen_states.get(&ceremony_id)
    }

    pub fn get_stage_for(&self, ceremony_id: CeremonyId) -> Option<KeygenStage> {
        self.get_state_for(ceremony_id).map(|s| s.get_stage())
    }

    pub fn expire_all(&mut self) {
        self.phase_timeout = std::time::Duration::from_secs(0);
    }

    pub fn get_delayed_count(&self, ceremony_id: CeremonyId) -> usize {
        // BC1s are stored separately from the state
        let bc_count = self
            .delayed_messages
            .get(&ceremony_id)
            .map(|v| v.1.len())
            .unwrap_or(0);

        let other_count = self
            .keygen_states
            .get(&ceremony_id)
            .map(|s| s.delayed_count())
            .unwrap_or(0);

        bc_count + other_count
    }
}
