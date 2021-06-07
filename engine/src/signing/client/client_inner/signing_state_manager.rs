use std::{
    collections::{hash_map::Entry, HashMap},
    time::Duration,
};

use log::*;
use tokio::sync::mpsc;

use crate::signing::{bitcoin_schnorr::Parameters, MessageHash};

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

    /// This is a method, because:
    /// - it needs access to the key
    /// - it needs p2p sender
    /// -
    pub(super) fn maybe_process_signing_data(
        &mut self,
        sender_id: usize,
        wdata: SigningDataWrapper,
    ) {
        // enum SigningStage {
        //     Idle,
        //     AwaitingBroadcast1,
        //     Phase2,
        //     SharedSecretReady,
        // }

        // All of the messages we expect to receive are:
        // BC1, Secret2, LocalSig

        // M deemed not known if we are not currently processing it *and* it is not in the list of
        // recently processed messages. (I.e. a message signed a long time ago is treated the same
        // way as a malicious one.)

        // If M is not known, we wait a minute or so (T1) before discarding it (and slashing the sender)
        // If a signing request has been received, we start processing M, popping all of the packets
        // for M from the queue.

        // For Idle, we delay Broadcast1 messages
        // For AwaitingBroadcast1 stage, we process Broadcast1 and delay Secret
        // For Phase2 stage, we process Secret2 and delay LocalSig
        // For SharedSecretReady, we process LocalSig, nothing to delay

        // -----------------------------------------------------------------------------------

        let SigningDataWrapper { data, message } = wdata;

        debug!(
            "receiving signing data for message: {}",
            String::from_utf8_lossy(&message)
        );

        // it is possible that the key is not ready yet...

        let key = self.signing_key.clone();

        let p2p_sender = self.p2p_sender.clone();

        // We've received a p2p message. We might not have a record for this message yet,
        // in which case we should create the record, but delay processing the data until
        // we receive a request to sign it (TODO !!!).

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
