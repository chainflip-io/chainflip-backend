use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::multisig::client::{self, MultisigOutcome};
use crate::p2p::AccountId;

use client::{
    keygen_state_runner::KeygenStateRunner, signing::frost::SigningData, state_runner::StateRunner,
    utils::PartyIdxMapping, CeremonyAbortReason, MultisigOutcomeSender, SchnorrSignature,
};
use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedSender;

use crate::logging::{
    CEREMONY_ID_KEY, KEYGEN_CEREMONY_FAILED, KEYGEN_REQUEST_EXPIRED, KEYGEN_REQUEST_IGNORED,
    REQUEST_TO_SIGN_EXPIRED, REQUEST_TO_SIGN_IGNORED, SIGNING_CEREMONY_FAILED,
};

use client::common::{broadcast::BroadcastStage, CeremonyCommon, KeygenResultInfo};

use crate::multisig::{KeyDB, KeygenInfo, KeygenOutcome, MessageHash, SigningOutcome};

use super::keygen::{HashContext, KeygenData, KeygenOptions};
use super::MultisigMessage;

type SigningStateRunner = StateRunner<SigningData, SchnorrSignature>;

/// Responsible for mapping ceremonies to the corresponding states and
/// generating signer indexes based on the list of parties
#[derive(Clone)]
pub struct CeremonyManager<S>
where
    S: KeyDB,
{
    my_account_id: AccountId,
    outcome_sender: MultisigOutcomeSender,
    outgoing_p2p_message_sender: UnboundedSender<(AccountId, MultisigMessage)>,
    signing_states: HashMap<CeremonyId, SigningStateRunner>,
    keygen_states: HashMap<CeremonyId, KeygenStateRunner>,
    logger: slog::Logger,
    ceremony_id_tracker: CeremonyIdTracker<S>,
}

impl<S> CeremonyManager<S>
where
    S: KeyDB,
{
    pub fn new(
        my_account_id: AccountId,
        outcome_sender: MultisigOutcomeSender,
        outgoing_p2p_message_sender: UnboundedSender<(AccountId, MultisigMessage)>,
        logger: &slog::Logger,
        ceremony_id_db: Arc<Mutex<S>>,
    ) -> Self {
        CeremonyManager {
            my_account_id,
            outcome_sender,
            outgoing_p2p_message_sender,
            signing_states: HashMap::new(),
            keygen_states: HashMap::new(),
            logger: logger.clone(),
            ceremony_id_tracker: CeremonyIdTracker::new(logger.clone(), ceremony_id_db),
        }
    }

    // This function is called periodically to check if any
    // ceremony should be aborted, reporting responsible parties
    // and cleaning up any relevant data
    pub fn cleanup(&mut self) {
        let mut events_to_send = vec![];

        let logger = &self.logger;
        self.signing_states.retain(|ceremony_id, state| {
            if let Some(bad_nodes) = state.try_expiring() {
                slog::warn!(logger, #REQUEST_TO_SIGN_EXPIRED, "Signing state expired and will be abandoned");
                let outcome = SigningOutcome::timeout(*ceremony_id, bad_nodes);

                events_to_send.push(MultisigOutcome::Signing(outcome));

                false
            } else {
                true
            }
        });

        self.keygen_states.retain(|ceremony_id, state| {
            if let Some(bad_nodes) = state.try_expiring() {
                slog::warn!(logger, #KEYGEN_REQUEST_EXPIRED, "Keygen state expired and will be abandoned");
                let outcome = KeygenOutcome::timeout(*ceremony_id, bad_nodes);

                events_to_send.push(MultisigOutcome::Keygen(outcome));

                false
            } else {
                true
            }
        });

        for event in events_to_send {
            if let Err(err) = self.outcome_sender.send(event) {
                slog::error!(self.logger, "Unable to send event, error: {}", err);
            }
        }
    }

    fn map_ceremony_parties(
        &self,
        participants: &[AccountId],
        validator_map: &PartyIdxMapping,
    ) -> Result<(usize, BTreeSet<usize>), &'static str> {
        if !participants.contains(&self.my_account_id) {
            return Err("we are not among participants");
        }

        // It should be impossible to fail here because of the check above,
        // but I don't like unwrapping (would be better if we
        // could combine this with the check above)
        let our_idx = validator_map
            .get_idx(&self.my_account_id)
            .ok_or("could not derive our idx")?;

        // Check that signer ids are known for this key
        let signer_idxs = validator_map
            .get_all_idxs(participants)
            .map_err(|_| "invalid participants")?;

        if signer_idxs.len() != participants.len() {
            return Err("non unique participants");
        }

        Ok((our_idx, signer_idxs))
    }

    /// Process a keygen request
    pub fn on_keygen_request(&mut self, keygen_info: KeygenInfo, keygen_options: KeygenOptions) {
        let KeygenInfo {
            ceremony_id,
            signers,
        } = keygen_info;

        let logger = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        let validator_map = Arc::new(PartyIdxMapping::from_unsorted_signers(&signers));

        let (our_idx, signer_idxs) = match self.map_ceremony_parties(&signers, &validator_map) {
            Ok(res) => res,
            Err(reason) => {
                slog::warn!(logger, #KEYGEN_REQUEST_IGNORED, "Keygen request ignored: {}", reason);
                return;
            }
        };

        let logger = &self.logger;

        if !self.ceremony_id_tracker.is_ceremony_id_used(&ceremony_id) {
            let state = self
                .keygen_states
                .entry(ceremony_id)
                .or_insert_with(|| KeygenStateRunner::new_unauthorised(logger));

            let context = generate_keygen_context(ceremony_id, signers);

            state.on_keygen_request(
                ceremony_id,
                self.outcome_sender.clone(),
                self.outgoing_p2p_message_sender.clone(),
                validator_map,
                our_idx,
                signer_idxs,
                keygen_options,
                context,
            );
        } else {
            slog::warn!(logger, #KEYGEN_REQUEST_IGNORED, "Keygen request ignored: ceremony id {} has already been used", ceremony_id);
        }
    }

    /// Process a request to sign
    pub fn on_request_to_sign(
        &mut self,
        data: MessageHash,
        key_info: KeygenResultInfo,
        signers: Vec<AccountId>,
        ceremony_id: CeremonyId,
    ) {
        let logger = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        slog::debug!(logger, "Processing a request to sign");

        // Check that the number of signers is correct
        let signers_expected = key_info.params.threshold + 1;
        if signers.len() != signers_expected {
            slog::warn!(
                logger,
                #REQUEST_TO_SIGN_IGNORED,
                "Request to sign ignored: incorrect number of signers {}/{}",
                signers.len(), signers_expected
            );
            return;
        }

        let (own_idx, signer_idxs) = match self
            .map_ceremony_parties(&signers, &key_info.validator_map)
        {
            Ok(res) => res,
            Err(reason) => {
                slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "Request to sign ignored: {}", reason);
                return;
            }
        };

        // We have the key and have received a request to sign
        let logger = &self.logger;
        if !self.ceremony_id_tracker.is_ceremony_id_used(&ceremony_id) {
            let state = self
                .signing_states
                .entry(ceremony_id)
                .or_insert_with(|| SigningStateRunner::new_unauthorised(logger));

            let initial_stage = {
                use super::signing::{frost_stages::AwaitCommitments1, SigningStateCommonInfo};

                let common = CeremonyCommon {
                    ceremony_id,
                    outgoing_p2p_message_sender: self.outgoing_p2p_message_sender.clone(),
                    validator_mapping: key_info.validator_map.clone(),
                    own_idx,
                    all_idxs: signer_idxs,
                    logger: self.logger.clone(),
                };

                let processor = AwaitCommitments1::new(
                    common.clone(),
                    SigningStateCommonInfo {
                        data,
                        key: key_info.key.clone(),
                    },
                );

                Box::new(BroadcastStage::new(processor, common))
            };

            if let Err(reason) = state.on_ceremony_request(
                ceremony_id,
                initial_stage,
                key_info.validator_map,
                self.outcome_sender.clone(),
            ) {
                slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "Request to sign ignored: {}", reason);
            }
        } else {
            slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "Request to sign ignored: ceremony id {} has already been used", ceremony_id);
        }
    }

    /// Process data for a signing ceremony arriving from a peer
    pub fn process_signing_data(
        &mut self,
        sender_id: AccountId,
        ceremony_id: CeremonyId,
        data: SigningData,
    ) {
        // Check if we have state for this data and delegate message to that state
        // Delay message otherwise

        slog::trace!(self.logger, "Received signing data {}", &data; CEREMONY_ID_KEY => ceremony_id);

        let logger = &self.logger;
        if !self.ceremony_id_tracker.is_ceremony_id_used(&ceremony_id) {
            let state = self
                .signing_states
                .entry(ceremony_id)
                .or_insert_with(|| SigningStateRunner::new_unauthorised(logger));

            if let Some(result) = state.process_message(sender_id, data) {
                self.remove_signing_ceremony(&ceremony_id);

                match result {
                    Ok(schnorr_sig) => {
                        self.outcome_sender
                            .send(MultisigOutcome::Signing(SigningOutcome {
                                id: ceremony_id,
                                result: Ok(schnorr_sig),
                            }))
                            .unwrap();
                    }
                    Err((blamed_parties, reason)) => {
                        slog::warn!(
                            self.logger,
                            #SIGNING_CEREMONY_FAILED,
                            "Signing ceremony failed: {}",
                            reason; "blamed parties" =>
                            format!("{:?}",blamed_parties)
                        );

                        self.outcome_sender
                            .send(MultisigOutcome::Signing(SigningOutcome {
                                id: ceremony_id,
                                result: Err((CeremonyAbortReason::Invalid, blamed_parties)),
                            }))
                            .unwrap();
                    }
                }
            }
        } else {
            slog::debug!(
                logger,
                "Ignoring signing data from old ceremony {}",
                ceremony_id
            );
        }
    }

    /// Process data for a keygen ceremony arriving from a peer
    pub fn process_keygen_data(
        &mut self,
        sender_id: AccountId,
        ceremony_id: CeremonyId,
        data: KeygenData,
    ) -> Option<KeygenResultInfo> {
        let logger = &self.logger;
        if !self.ceremony_id_tracker.is_ceremony_id_used(&ceremony_id) {
            let state = self
                .keygen_states
                .entry(ceremony_id)
                .or_insert_with(|| KeygenStateRunner::new_unauthorised(logger));

            state.process_message(sender_id, data).and_then(|res| {
                self.remove_keygen_ceremony(&ceremony_id);

                match res {
                    Ok(keygen_result_info) => Some(keygen_result_info),
                    Err((blamed_parties, reason)) => {
                        slog::warn!(
                            self.logger,
                            #KEYGEN_CEREMONY_FAILED,
                            "Keygen ceremony failed: {}",
                            reason; "blamed parties" =>
                            format!("{:?}",blamed_parties)
                        );

                        self.outcome_sender
                            .send(MultisigOutcome::Keygen(KeygenOutcome {
                                id: ceremony_id,
                                result: Err((CeremonyAbortReason::Invalid, blamed_parties)),
                            }))
                            .unwrap();
                        None
                    }
                }
            })
        } else {
            slog::debug!(
                logger,
                "Ignoring keygen data from old ceremony {}",
                ceremony_id
            );
            None
        }
    }

    // Removed a finished keygen ceremony and mark its id as used
    fn remove_keygen_ceremony(&mut self, ceremony_id: &CeremonyId) {
        self.keygen_states.remove(ceremony_id);
        self.ceremony_id_tracker.consume_ceremony_id(ceremony_id);

        slog::debug!(
            self.logger, "Removed a finished keygen ceremony";
            CEREMONY_ID_KEY => ceremony_id
        );
    }

    // Removed a finished signing ceremony and mark its id as used
    fn remove_signing_ceremony(&mut self, ceremony_id: &CeremonyId) {
        self.signing_states.remove(ceremony_id);
        self.ceremony_id_tracker.consume_ceremony_id(ceremony_id);
    }
}

#[cfg(test)]
impl<S> CeremonyManager<S>
where
    S: KeyDB,
{
    pub fn expire_all(&mut self) {
        for (_, state) in &mut self.signing_states {
            state.set_expiry_time(std::time::Instant::now());
        }

        for (_, state) in &mut self.keygen_states {
            state.set_expiry_time(std::time::Instant::now());
        }
    }

    pub fn get_signing_stage_for(&self, ceremony_id: CeremonyId) -> Option<String> {
        self.signing_states
            .get(&ceremony_id)
            .and_then(|s| s.get_stage())
    }

    pub fn get_keygen_stage_for(&self, ceremony_id: CeremonyId) -> Option<String> {
        self.keygen_states
            .get(&ceremony_id)
            .and_then(|s| s.get_stage())
    }

    pub fn set_ceremony_id_tracker(&mut self, ceremony_id_tracker: CeremonyIdTracker<S>)
    where
        S: KeyDB,
    {
        self.ceremony_id_tracker = ceremony_id_tracker;
    }
}

/// Create unique deterministic context used for generating a ZKP to prevent replay attacks
pub fn generate_keygen_context(
    ceremony_id: CeremonyId,
    mut signers: Vec<AccountId>,
) -> HashContext {
    use sha2::{Digest, Sha256};

    // We don't care if sorting is stable as all account ids are meant to be unique
    signers.sort_unstable();

    let mut hasher = Sha256::new();

    hasher.update(ceremony_id.to_be_bytes());

    // NOTE: it should be sufficient to use ceremony_id as context as
    // we never reuse the same id for different ceremonies, but lets
    // put the signers in to make the context hard to predict as well
    for id in signers {
        hasher.update(id.0);
    }

    HashContext(*hasher.finalize().as_ref())
}

/// Used to track every ceremony id that has been used in the past,
/// so we can make sure they are not reused.
#[derive(Clone)]
pub struct CeremonyIdTracker<S>
where
    S: KeyDB,
{
    // (lowest_used_id, Highest_used_id)d
    used_id_window: Option<(CeremonyId, CeremonyId)>,
    // All unused id's within the `used_id_window`
    unused_ids: HashSet<CeremonyId>,
    db: Arc<Mutex<S>>,
    logger: slog::Logger,
}

impl<S> CeremonyIdTracker<S>
where
    S: KeyDB,
{
    /// Create a new `CeremonyIdTracker` and load the persistent information
    pub fn new(logger: slog::Logger, ceremony_id_db: Arc<Mutex<S>>) -> Self {
        let tracker = CeremonyIdTracker {
            used_id_window: None,
            unused_ids: HashSet::new(),
            db: ceremony_id_db,
            logger,
        };
        {
            // Load values from the db
            let db = tracker.db.lock().unwrap();
            db.load_unused_ceremony_ids();
            db.load_used_ceremony_id_window();
        }
        tracker
    }

    /// Mark this ceremony id as used
    pub fn consume_ceremony_id(&mut self, ceremony_id: &CeremonyId) {
        match self.used_id_window {
            Some((lowest, highest)) => {
                if *ceremony_id > highest {
                    // new highest ceremony id, so push it up and record any gaps
                    for c in (highest + 1)..*ceremony_id {
                        self.insert_unused_ceremony_id(c);
                    }
                    self.update_used_ceremony_id_window((lowest, *ceremony_id));
                } else if *ceremony_id < lowest {
                    // new lowest ceremony id, so push it down and record any gaps
                    for c in (*ceremony_id + 1)..lowest {
                        self.insert_unused_ceremony_id(c);
                    }
                    self.update_used_ceremony_id_window((*ceremony_id, highest));
                } else {
                    // Its within the used id window, remove the ceremony id from the list of unused id's
                    self.remove_unused_ceremony_id(ceremony_id);
                }
            }
            None => {
                self.update_used_ceremony_id_window((*ceremony_id, *ceremony_id));
            }
        }
    }

    /// Check if the ceremony id has already been used (false = never seen before, safe to continue)
    pub fn is_ceremony_id_used(&self, ceremony_id: &CeremonyId) -> bool {
        match self.used_id_window {
            Some((lowest, highest)) => {
                if ceremony_id > &highest || ceremony_id < &lowest {
                    false
                } else {
                    !self.unused_ids.contains(ceremony_id)
                }
            }
            None => false,
        }
    }

    fn update_used_ceremony_id_window(&mut self, window: (CeremonyId, CeremonyId)) {
        self.used_id_window = Some(window);
        self.db
            .lock()
            .unwrap()
            .update_used_ceremony_id_window(window);
    }

    fn remove_unused_ceremony_id(&mut self, ceremony_id: &CeremonyId) {
        if !self.unused_ids.remove(ceremony_id) {
            slog::warn!(
                self.logger,
                "Ceremony id tracking error: already consumed id {}",
                ceremony_id
            );
        }
        self.db
            .lock()
            .unwrap()
            .remove_unused_ceremony_id(ceremony_id);
    }

    fn insert_unused_ceremony_id(&mut self, ceremony_id: CeremonyId) {
        self.unused_ids.insert(ceremony_id);
        self.db.lock().unwrap().save_unused_ceremony_id(ceremony_id);
    }
}

#[test]
fn test_ceremony_id_tracker() {
    use crate::multisig::db::KeyDBMock;

    let logger = crate::logging::test_utils::new_test_logger();

    let mut tracker = CeremonyIdTracker::new(logger, Arc::new(Mutex::new(KeyDBMock::new())));

    // Test the starting condition (starting from non-zero)
    assert!(!tracker.is_ceremony_id_used(&0));
    tracker.consume_ceremony_id(&10);
    assert!(tracker.is_ceremony_id_used(&10));
    assert!(!tracker.is_ceremony_id_used(&0));

    // Large set with a gap
    for i in 11..=99 {
        if i != 42 {
            tracker.consume_ceremony_id(&i);
        }
    }

    // Setting the lowest used id
    tracker.consume_ceremony_id(&5);

    assert!(!tracker.is_ceremony_id_used(&4));
    assert!(tracker.is_ceremony_id_used(&5));
    assert!(tracker.is_ceremony_id_used(&50));
    assert!(tracker.is_ceremony_id_used(&99));
    assert!(!tracker.is_ceremony_id_used(&42));
    assert!(!tracker.is_ceremony_id_used(&100));
}
