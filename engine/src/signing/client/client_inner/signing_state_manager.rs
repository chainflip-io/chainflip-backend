use std::{
    collections::{hash_map::Entry, HashMap},
    time::Duration,
};

use log::*;
use tokio::sync::mpsc;

use crate::{
    p2p::ValidatorId,
    signing::{
        client::{client_inner::client_inner::SigningData, SigningInfo},
        crypto::Parameters,
        MessageHash, MessageInfo,
    },
};

use super::{
    client_inner::{Broadcast1, InnerEvent, SigningDataWrapped},
    signing_state::{KeygenResultInfo, SigningState},
};

/// Manages multiple signing states for multiple signing processes
#[derive(Clone)]
pub struct SigningStateManager {
    signing_states: HashMap<MessageInfo, SigningState>,
    params: Parameters,
    id: ValidatorId,
    p2p_sender: mpsc::UnboundedSender<InnerEvent>,
    /// Max lifetime of any phase before it expires
    /// and we abandon on the signing ceremony
    phase_timeout: Duration,
    /// Storage for messages for which we are not able to create a SigningState yet.
    /// Processing these is triggered by a request to sign
    delayed_messages: HashMap<MessageInfo, (std::time::Instant, Vec<(ValidatorId, Broadcast1)>)>,
}

impl SigningStateManager {
    pub(super) fn new(
        params: Parameters,
        id: ValidatorId,
        p2p_sender: mpsc::UnboundedSender<InnerEvent>,
        phase_timeout: Duration,
    ) -> Self {
        SigningStateManager {
            signing_states: HashMap::new(),
            params,
            id,
            p2p_sender,
            phase_timeout,
            delayed_messages: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn get_state_for(&self, message_info: &MessageInfo) -> Option<&SigningState> {
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

    fn add_delayed(&mut self, mi: MessageInfo, bc1_entry: (ValidatorId, Broadcast1)) {
        trace!("Signing manager adds delayed bc1");
        let entry = self
            .delayed_messages
            .entry(mi)
            .or_insert((std::time::Instant::now(), Vec::new()));
        entry.1.push(bc1_entry);
    }

    /// Process signing data, generating new state if necessary
    pub(super) fn process_signing_data(
        &mut self,
        sender_id: ValidatorId,
        wdata: SigningDataWrapped,
    ) {
        let SigningDataWrapped { data, message } = wdata;

        debug!(
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
                    other => warn!("Unexpected {} for message: {:?}", other, message.hash),
                };
            }
        }
    }

    fn process_delayed(&mut self, mi: &MessageInfo) {
        if let Some((_t, messages)) = self.delayed_messages.remove(mi) {
            for (sender, bc1) in messages {
                debug!("Processing delayed signging bc1");

                let wdata = SigningDataWrapped {
                    data: bc1.into(),
                    message: mi.clone(),
                };
                self.process_signing_data(sender, wdata);
            }
        }
    }

    pub(super) fn on_request_to_sign(
        &mut self,
        data: MessageHash,
        key_info: KeygenResultInfo,
        sign_info: SigningInfo,
    ) {
        debug!(
            "initiating signing for message: {}",
            String::from_utf8_lossy(&data.0)
        );

        if !sign_info.signers.contains(&self.id) {
            warn!("Request to sign ignored: we are not among signers.");
            return;
        }

        let our_idx = match key_info.get_idx(&self.id) {
            Some(idx) => idx,
            None => {
                // This should be impossible because of the check above,
                // but I don't like unwrapping (would be better if we
                // could combine this with the check above)
                warn!("Request to sign ignored: could not derive our idx");
                return;
            }
        };

        // Check that signer ids are known for this key
        let signer_idxs = match project_signers(&sign_info.signers, &key_info) {
            Ok(signer_idxs) => signer_idxs,
            Err(_) => {
                warn!("Request to sign ignored: invalid signers.");
                return;
            }
        };

        let key_id = sign_info.id;

        let mi = MessageInfo { hash: data, key_id };

        match self.signing_states.entry(mi.clone()) {
            Entry::Occupied(_) => {
                warn!("Ingoring a request to sign the same message again");
            }
            Entry::Vacant(entry) => {
                // We have the key and have received a request to sign
                trace!("Creating new signing state for message");
                let p2p_sender = self.p2p_sender.clone();

                let state = SigningState::on_request_to_sign(
                    self.id.clone(),
                    our_idx,
                    signer_idxs,
                    key_info,
                    self.params,
                    p2p_sender,
                    mi.clone(),
                    sign_info,
                );

                entry.insert(state);

                self.process_delayed(&mi);
            }
        }
    }

    pub(super) fn cleanup(&mut self) {
        // for every state, check if it expired

        info!("cleanup");

        let timeout = self.phase_timeout;

        self.delayed_messages.retain(|_key, (t, _)| {
            if t.elapsed() > timeout {
                warn!("BC1 for signing expired");
                // TODO: send a signal
                return false;
            }
            true
        });

        self.signing_states.retain(|_msg, state| {
            if state.cur_phase_timestamp.elapsed() > timeout {
                // TODO: successful ceremonies should clean up themselves!
                warn!("Signing state expired and should be abandoned");
                // TODO: send a signal
                return false;
            }
            true
        });
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
