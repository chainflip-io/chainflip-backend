use std::{
    collections::{hash_map::Entry, HashMap},
    time::Duration,
};

use log::*;
use tokio::sync::mpsc;

use crate::signing::{client::SigningInfo, crypto::Parameters, MessageHash, MessageInfo};

use super::{
    client_inner::{InnerEvent, SigningDataWrapped},
    signing_state::{KeygenResult, SigningState},
};

/// Manages multiple signing states for multiple signing processes
#[derive(Clone)]
pub struct SigningStateManager {
    signing_states: HashMap<MessageInfo, SigningState>,
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
            params,
            signer_idx,
            p2p_sender,
            phase_timeout,
        }
    }

    #[cfg(test)]
    pub(super) fn get_state_for(&self, message: &MessageInfo) -> Option<&SigningState> {
        self.signing_states.get(message)
    }

    /// Process signing data, generating new state if necessary
    pub(super) fn process_signing_data(&mut self, sender_id: usize, wdata: SigningDataWrapped) {
        let SigningDataWrapped { data, message } = wdata;

        debug!(
            "receiving signing data for message: {}",
            String::from_utf8_lossy(&message.hash.0)
        );

        let p2p_sender = self.p2p_sender.clone();

        match self.signing_states.entry(message.clone()) {
            Entry::Occupied(mut state) => {
                trace!("Already have state for message");
                // We already have state for the provided message, so
                // process it normally
                state.get_mut().process_signing_message(sender_id, data);
            }
            Entry::Vacant(entry) => {
                trace!("Creating new state for message");
                // We might already have the key, but let's just make it
                // `on_request_to_sign`'s responsibility to set the key
                let key = None;

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

    pub(super) fn on_request_to_sign(
        &mut self,
        data: MessageHash,
        key: KeygenResult,
        sign_info: SigningInfo,
    ) {
        debug!(
            "initiating signing for message: {}",
            String::from_utf8_lossy(&data.0)
        );

        let key_id = sign_info.id;

        let mi = MessageInfo { hash: data, key_id };

        match self.signing_states.entry(mi.clone()) {
            Entry::Occupied(mut entry) => {
                trace!("Already have signing state for message");
                // Already have some data for this message
                let entry = entry.get_mut();

                entry.set_key(key);
                entry.on_request_to_sign(sign_info);
            }
            Entry::Vacant(entry) => {
                // Initiate signing state
                trace!("Creating new signing state for message");
                let key = Some(key);
                let p2p_sender = self.p2p_sender.clone();
                let entry = entry.insert(SigningState::new(
                    self.signer_idx,
                    key,
                    self.params,
                    p2p_sender,
                    mi,
                ));
                entry.on_request_to_sign(sign_info);
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
