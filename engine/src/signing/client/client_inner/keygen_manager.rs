use std::{collections::HashMap, sync::Arc};

use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc;

use crate::{p2p::AccountId, signing::KeygenInfo};

use super::{
    client_inner::KeyGenMessageWrapped,
    keygen_state::KeygenState,
    utils::{get_index_mapping, project_signers},
    InnerEvent, KeygenResultInfo,
};

#[derive(Clone)]
pub struct KeygenManager {
    /// States for each ceremony_id
    keygen_states: HashMap<CeremonyId, KeygenState>,
    /// Used to propagate events upstream
    event_sender: mpsc::UnboundedSender<InnerEvent>,
    /// Validator id of our node
    id: AccountId,
    logger: slog::Logger,
}

impl KeygenManager {
    pub fn new(
        id: AccountId,
        event_sender: mpsc::UnboundedSender<InnerEvent>,
        logger: &slog::Logger,
    ) -> Self {
        KeygenManager {
            keygen_states: Default::default(),
            event_sender,
            id,
            logger: logger.clone(),
        }
    }

    pub fn cleanup(&mut self) {
        // todo!();
    }

    pub fn on_keygen_request(&mut self, keygen_info: KeygenInfo) {
        let KeygenInfo {
            ceremony_id,
            signers,
        } = keygen_info;

        // TODO: check the number of participants?

        if !signers.contains(&self.id) {
            // TODO: alert
            slog::warn!(
                self.logger,
                "Keygen request ignored: we are not among participants: [ceremony_id: {}]",
                ceremony_id
            );

            return;
        }

        let validator_map = Arc::new(get_index_mapping(&signers));

        let our_idx = match validator_map.get_idx(&self.id) {
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
        let signer_idxs = match project_signers(&signers, &validator_map) {
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

        let entry = self
            .keygen_states
            .entry(ceremony_id)
            .or_insert(KeygenState::new_unauthorised(self.logger.clone()));

        entry.on_keygen_request(
            ceremony_id,
            self.event_sender.clone(),
            validator_map,
            our_idx,
            signer_idxs,
        );
    }

    pub fn process_keygen_data(
        &mut self,
        sender_id: AccountId,
        msg: KeyGenMessageWrapped,
    ) -> Option<KeygenResultInfo> {
        let KeyGenMessageWrapped { ceremony_id, data } = msg;

        let state = self
            .keygen_states
            .entry(ceremony_id)
            .or_insert(KeygenState::new_unauthorised(self.logger.clone()));

        state.process_message(sender_id, data)
    }
}

#[cfg(test)]
impl KeygenManager {
    pub fn expire_all(&mut self) {
        // TODO
    }

    pub fn get_stage_for(&self, ceremony_id: CeremonyId) -> Option<String> {
        self.keygen_states
            .get(&ceremony_id)
            .and_then(|s| s.get_stage())
    }
}
