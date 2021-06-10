use std::collections::{hash_map::Entry, HashMap};

use crate::{
    p2p::ValidatorId,
    signing::{
        client::{KeyId, KeygenInfo},
        crypto::Parameters,
    },
};

use super::{
    client_inner::{Broadcast1, KeyGenMessage, KeyGenMessageWrapped},
    keygen_state::KeygenState,
    signing_state::KeygenResult,
    InnerEvent,
};

use log::*;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone)]
pub struct KeygenManager {
    keygen_states: HashMap<KeyId, KeygenState>,
    event_sender: UnboundedSender<InnerEvent>,
    params: Parameters,
    our_id: ValidatorId,
    /// Storage for delayed data (only Broadcast1 makes sense here).
    /// We choose not to store it inside KeygenState, as having KeygenState currently
    /// implies that we have received the relevant keygen request
    /// (and know all parties involved), which is not always the case.
    delayed_messages: HashMap<KeyId, Vec<(ValidatorId, Broadcast1)>>,
}

impl KeygenManager {
    pub fn new(
        params: Parameters,
        our_id: ValidatorId,
        event_sender: UnboundedSender<InnerEvent>,
    ) -> Self {
        KeygenManager {
            keygen_states: Default::default(),
            delayed_messages: Default::default(),
            event_sender,
            params,
            our_id,
        }
    }

    #[cfg(test)]
    pub fn get_state_for(&self, key_id: KeyId) -> Option<&KeygenState> {
        self.keygen_states.get(&key_id)
    }

    #[cfg(test)]
    pub fn get_stage_for(&self, key_id: KeyId) -> Option<KeygenStage> {
        self.get_state_for(key_id).map(|s| s.get_stage())
    }

    #[cfg(test)]
    pub fn get_delayed_count(&self, key_id: KeyId) -> usize {
        // BC1s are stored separately from the state
        let bc_count = self
            .delayed_messages
            .get(&key_id)
            .map(|v| v.len())
            .unwrap_or(0);

        let other_count = self
            .keygen_states
            .get(&key_id)
            .map(|s| s.delayed_count())
            .unwrap_or(0);

        bc_count + other_count
    }

    // Get the key that was generated as the result of
    // a keygen ceremony between the winners of auction `id`
    pub(super) fn get_key_by_id(&self, id: KeyId) -> Option<&KeygenResult> {
        let entry = self.keygen_states.get(&id)?;

        entry.key.as_ref()
    }

    pub(super) fn process_keygen_message(&mut self, sender_id: usize, msg: KeyGenMessageWrapped) {
        let KeyGenMessageWrapped { key_id, message } = msg;

        match self.keygen_states.entry(key_id) {
            Entry::Occupied(mut state) => {
                // We have entry, process normally
                state.get_mut().process_keygen_message(sender_id, message);
            }
            Entry::Vacant(_) => match message {
                KeyGenMessage::Broadcast1(bc1) => {
                    trace!("Delaying keygen bc1 for key id: {:?}", key_id);
                    self.add_delayed(key_id, sender_id, bc1);
                }
                KeyGenMessage::Secret2(_) => {
                    warn!("Unexpected keygen secret2 for key id: {:?}", key_id);
                }
            },
        };
    }

    fn add_delayed(&mut self, key_id: KeyId, sender_id: ValidatorId, bc1: Broadcast1) {
        let entry = self.delayed_messages.entry(key_id).or_default();
        entry.push((sender_id, bc1));
    }

    pub fn on_keygen_request(&mut self, ki: KeygenInfo) {
        let KeygenInfo {
            id: key_id,
            signers,
        } = ki;

        match self.keygen_states.entry(key_id) {
            Entry::Occupied(_) => {
                // State should not have been created prior to receiving a keygen request
                warn!("Ignoring a keygen request for a known key_id: {:?}", key_id);
            }
            Entry::Vacant(entry) => match get_our_idx(&signers, self.our_id) {
                Some(idx) => {
                    debug!(
                        "Creating new keygen state for key id: {:?}, our idx: {:?}",
                        key_id, self.our_id
                    );
                    let state =
                        KeygenState::initiate(idx, self.params, key_id, self.event_sender.clone());

                    let state = entry.insert(state);

                    // Process delayed messages:
                    let messages = self.delayed_messages.remove(&key_id).unwrap_or_default();

                    for (sender_id, msg) in messages {
                        state.process_keygen_message(sender_id, msg.into());
                    }

                    assert!(self.delayed_messages.get(&key_id).is_none());
                }
                None => {
                    error!("Unexpected keygen request w/o us as participants")
                }
            },
        }
    }
}

fn get_our_idx(signers: &Vec<ValidatorId>, id: ValidatorId) -> Option<usize> {
    // sort validators and find our index

    let mut signers = signers.clone();

    signers.sort();

    signers.iter().find(|s| *s == &id).copied()
}
