use std::{
    collections::{hash_map::Entry, HashMap},
    time::Duration,
};

use log::*;
use tokio::sync::mpsc;

use crate::signing::{crypto::Parameters, MessageHash};

use super::{
    client_inner::{InnerEvent, SigningDataWrapper},
    signing_state::{KeygenResult, SigningState},
};

/// Manages multiple signing states for multiple signing processes
#[derive(Clone)]
pub struct SigningStateManager {
    signing_key: Option<KeygenResult>,
    signing_states: HashMap<MessageHash, SigningState>,
    params: Parameters,
    signer_idx: usize,
    p2p_sender: mpsc::UnboundedSender<InnerEvent>,
    /// Max lifetime of any phase before it expires
    /// and we abandon on the signing ceremony
    phase_timeout: Duration,
}

impl SigningStateManager {
    pub(super) fn new(
        params: Parameters,
        signer_idx: usize,
        p2p_sender: mpsc::UnboundedSender<InnerEvent>,
        phase_timeout: Duration,
    ) -> Self {
        SigningStateManager {
            signing_states: HashMap::new(),
            signing_key: None,
            params,
            signer_idx,
            p2p_sender,
            phase_timeout,
        }
    }

    #[cfg(test)]
    pub(super) fn get_state_for(&self, message: &[u8]) -> Option<&SigningState> {
        self.signing_states.get(message)
    }

    /// Note that the key can be added later to make sure that we
    /// can start recording signing data even before we finished
    /// the last step of keygen locally.
    pub(super) fn set_key(&mut self, signing_key: KeygenResult) {
        self.signing_key = Some(signing_key);
    }

    /// Process signing data, generating new state if necessary
    pub(super) fn process_signing_data(&mut self, sender_id: usize, wdata: SigningDataWrapper) {
        let SigningDataWrapper { data, message } = wdata;

        debug!(
            "receiving signing data for message: {}",
            String::from_utf8_lossy(&message)
        );

        let key = self.signing_key.clone();

        let p2p_sender = self.p2p_sender.clone();

        match self.signing_states.entry(message.clone()) {
            Entry::Occupied(mut state) => {
                // We already have state for the provided message, so
                // process it normally
                state.get_mut().process_signing_message(sender_id, data);
            }
            Entry::Vacant(entry) => {
                // Create state, but in Idle state
                let state = entry.insert(SigningState::new(
                    self.signer_idx,
                    key,
                    self.params,
                    p2p_sender,
                    message,
                ));

                state.process_signing_message(sender_id, data);
            }
        }
    }

    pub(super) fn on_request_to_sign(&mut self, message: Vec<u8>, active_parties: &[usize]) {
        debug!(
            "initiating signing for message: {}",
            String::from_utf8_lossy(&message)
        );

        match self.signing_states.entry(message.clone()) {
            Entry::Occupied(mut entry) => {
                // Already have some data for this message
                entry.get_mut().on_request_to_sign(active_parties);
            }
            Entry::Vacant(entry) => {
                // Initiate signing state
                let key = self.signing_key.clone();
                let p2p_sender = self.p2p_sender.clone();
                let entry = entry.insert(SigningState::new(
                    self.signer_idx,
                    key,
                    self.params,
                    p2p_sender,
                    message,
                ));
                entry.on_request_to_sign(active_parties);
            }
        }
    }

    pub(super) fn cleanup(&mut self) {
        // for every state, check if it expired

        let timeout = self.phase_timeout;

        self.signing_states.retain(|_msg, state| {
            if state.cur_phase_timestamp.elapsed() > timeout {
                warn!("Signing state expired and should be abandoned");
                // TODO: send a signal
                return false;
            }
            true
        });
    }
}
