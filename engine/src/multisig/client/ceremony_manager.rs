use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::common::format_iterator;
use crate::multisig::client;
use crate::multisig::client::common::{KeygenFailureReason, SigningFailureReason};
use crate::multisig::crypto::{CryptoScheme, Rng};
use crate::multisig_p2p::OutgoingMultisigStageMessages;
use cf_traits::AuthorityCount;
use state_chain_runtime::AccountId;

use client::{signing::frost::SigningData, state_runner::StateRunner, utils::PartyIdxMapping};
use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use crate::logging::{
    CEREMONY_ID_KEY, KEYGEN_CEREMONY_FAILED, KEYGEN_REQUEST_IGNORED, REQUEST_TO_SIGN_IGNORED,
    SIGNING_CEREMONY_FAILED,
};

use client::common::{
    broadcast::BroadcastStage, CeremonyCommon, CeremonyFailureReason, KeygenResultInfo,
};

use crate::multisig::MessageHash;

use super::ceremony_id_tracker::CeremonyIdTracker;
use super::keygen::{HashCommitments1, HashContext, KeygenData};
use super::{MultisigData, MultisigMessage};

pub type CeremonyResultSender<T, R> =
    oneshot::Sender<Result<T, (BTreeSet<AccountId>, CeremonyFailureReason<R>)>>;
pub type CeremonyResultReceiver<T, R> =
    oneshot::Receiver<Result<T, (BTreeSet<AccountId>, CeremonyFailureReason<R>)>>;

type SigningStateRunner<C> = StateRunner<
    SigningData<<C as CryptoScheme>::Point>,
    <C as CryptoScheme>::Signature,
    SigningFailureReason,
>;
type KeygenStateRunner<C> = StateRunner<
    KeygenData<<C as CryptoScheme>::Point>,
    KeygenResultInfo<<C as CryptoScheme>::Point>,
    KeygenFailureReason,
>;

/// Responsible for mapping ceremonies to the corresponding states and
/// generating signer indexes based on the list of parties
pub struct CeremonyManager<C: CryptoScheme> {
    my_account_id: AccountId,
    outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    signing_states: HashMap<CeremonyId, SigningStateRunner<C>>,
    keygen_states: HashMap<CeremonyId, KeygenStateRunner<C>>,
    ceremony_id_tracker: CeremonyIdTracker,
    allowing_high_pubkey: bool,
    logger: slog::Logger,
}

impl<C: CryptoScheme> CeremonyManager<C> {
    pub fn new(
        my_account_id: AccountId,
        outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
        logger: &slog::Logger,
    ) -> Self {
        CeremonyManager {
            my_account_id,
            outgoing_p2p_message_sender,
            signing_states: HashMap::new(),
            keygen_states: HashMap::new(),
            ceremony_id_tracker: CeremonyIdTracker::new(),
            allowing_high_pubkey: false,
            logger: logger.clone(),
        }
    }

    // This function is called periodically to check if any
    // ceremony should be aborted, reporting responsible parties
    // and cleaning up any relevant data
    pub fn check_timeouts(&mut self) {
        // Copy the keys so we can iterate over them while at the same time
        // removing the elements as we go
        let signing_ids: Vec<_> = self.signing_states.keys().copied().collect();

        for ceremony_id in &signing_ids {
            let state = self
                .signing_states
                .get_mut(ceremony_id)
                .expect("state must exist");
            if let Some(result) = state.try_expiring() {
                // NOTE: we only respond (and consume the ceremony id)
                //  if we have received a ceremony request from
                // SC (i.e. the ceremony is "authorized")
                // Only consume the ceremony id if it has been authorized
                if state.is_authorized() {
                    self.process_signing_ceremony_outcome(*ceremony_id, result);
                } else {
                    // TODO: [SC-2898] Re-enable reporting of unauthorised ceremonies #1135
                    slog::warn!(self.logger, "Removing expired unauthorised signing ceremony"; CEREMONY_ID_KEY => ceremony_id);
                    self.signing_states.remove(ceremony_id);
                }
            }
        }

        let keygen_ids: Vec<_> = self.keygen_states.keys().copied().collect();

        for ceremony_id in &keygen_ids {
            let state = self
                .keygen_states
                .get_mut(ceremony_id)
                .expect("state must exist");
            if let Some(result) = state.try_expiring() {
                // NOTE: we only respond (and consume the ceremony id)
                // if we have received a ceremony request from
                // SC (i.e. the ceremony is "authorised")
                if state.is_authorized() {
                    self.process_keygen_ceremony_outcome(*ceremony_id, result);
                } else {
                    // TODO: [SC-2898] Re-enable reporting of unauthorised ceremonies #1135
                    slog::warn!(self.logger, "Removing expired unauthorised keygen ceremony"; CEREMONY_ID_KEY => ceremony_id);
                    self.keygen_states.remove(ceremony_id);
                }
            }
        }
    }

    fn map_ceremony_parties(
        &self,
        participants: &[AccountId],
        validator_map: &PartyIdxMapping,
    ) -> Result<(AuthorityCount, BTreeSet<AuthorityCount>), &'static str> {
        assert!(
            participants.contains(&self.my_account_id),
            "we are not among participants"
        );

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

    fn process_signing_ceremony_outcome(
        &mut self,
        ceremony_id: CeremonyId,
        result: Result<
            C::Signature,
            (
                BTreeSet<AccountId>,
                CeremonyFailureReason<SigningFailureReason>,
            ),
        >,
    ) {
        let result_sender = self
            .signing_states
            .remove(&ceremony_id)
            .unwrap()
            .try_into_result_sender()
            .unwrap();
        self.ceremony_id_tracker.consume_signing_id(&ceremony_id);
        if let Err((blamed_parties, reason)) = &result {
            slog::warn!(
                self.logger,
                #SIGNING_CEREMONY_FAILED,
                "{}",
                reason; "reported parties" =>
                format_iterator(blamed_parties).to_string(),
                CEREMONY_ID_KEY => ceremony_id,
            );
        }
        let _result = result_sender.send(result);
    }

    fn process_keygen_ceremony_outcome(
        &mut self,
        ceremony_id: CeremonyId,
        result: Result<
            KeygenResultInfo<C::Point>,
            (
                BTreeSet<AccountId>,
                CeremonyFailureReason<KeygenFailureReason>,
            ),
        >,
    ) {
        let result_sender = self
            .keygen_states
            .remove(&ceremony_id)
            .unwrap()
            .try_into_result_sender()
            .unwrap();
        self.ceremony_id_tracker.consume_keygen_id(&ceremony_id);
        if let Err((blamed_parties, reason)) = &result {
            slog::warn!(
                self.logger,
                #KEYGEN_CEREMONY_FAILED,
                "{}",
                reason; "reported parties" =>
                format_iterator(blamed_parties).to_string(),
                CEREMONY_ID_KEY => ceremony_id,
            );
        }
        let _result = result_sender.send(result);
    }

    /// Process a keygen request
    pub fn on_keygen_request(
        &mut self,
        ceremony_id: CeremonyId,
        participants: Vec<AccountId>,
        rng: Rng,
        result_sender: CeremonyResultSender<KeygenResultInfo<C::Point>, KeygenFailureReason>,
    ) {
        // TODO: Consider similarity in structure to on_request_to_sign(). Maybe possible to factor some commonality

        let logger = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        let validator_map = Arc::new(PartyIdxMapping::from_unsorted_signers(&participants));

        let (our_idx, signer_idxs) = match self.map_ceremony_parties(&participants, &validator_map)
        {
            Ok(res) => res,
            Err(reason) => {
                slog::warn!(logger, #KEYGEN_REQUEST_IGNORED, "Keygen request ignored: {}", reason);
                let _result = result_sender.send(Err((
                    BTreeSet::new(),
                    CeremonyFailureReason::InvalidParticipants,
                )));
                return;
            }
        };

        if self
            .ceremony_id_tracker
            .is_keygen_ceremony_id_used(&ceremony_id)
        {
            slog::warn!(logger, #KEYGEN_REQUEST_IGNORED, "Keygen request ignored: ceremony id {} has already been used", ceremony_id);
            let _result = result_sender.send(Err((
                BTreeSet::new(),
                CeremonyFailureReason::CeremonyIdAlreadyUsed,
            )));
            return;
        }

        let logger_no_ceremony_id = &self.logger;
        let state = self.keygen_states.entry(ceremony_id).or_insert_with(|| {
            KeygenStateRunner::<C>::new_unauthorised(ceremony_id, logger_no_ceremony_id)
        });

        let initial_stage = {
            let context = generate_keygen_context(ceremony_id, participants);

            let common = CeremonyCommon {
                ceremony_id,
                outgoing_p2p_message_sender: self.outgoing_p2p_message_sender.clone(),
                validator_mapping: validator_map.clone(),
                own_idx: our_idx,
                all_idxs: signer_idxs,
                logger: logger.clone(),
                rng,
            };

            let processor =
                HashCommitments1::new(common.clone(), self.allowing_high_pubkey, context);

            Box::new(BroadcastStage::new(processor, common))
        };

        if let Some(result) = state.on_ceremony_request(initial_stage, validator_map, result_sender)
        {
            self.process_keygen_ceremony_outcome(ceremony_id, result);
        };
    }

    /// Process a request to sign
    pub fn on_request_to_sign(
        &mut self,
        ceremony_id: CeremonyId,
        signers: Vec<AccountId>,
        data: MessageHash,
        key_info: KeygenResultInfo<C::Point>,
        rng: Rng,
        result_sender: CeremonyResultSender<C::Signature, SigningFailureReason>,
    ) {
        let logger = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        slog::debug!(logger, "Processing a request to sign");

        // Check that the number of signers is enough
        let minimum_signers_needed = key_info.params.threshold + 1;
        let signers_len: AuthorityCount = signers.len().try_into().expect("too many signers");
        if signers_len < minimum_signers_needed {
            slog::warn!(
                logger,
                #REQUEST_TO_SIGN_IGNORED,
                "Request to sign ignored: not enough signers {}/{}",
                signers.len(), minimum_signers_needed
            );
            let _result = result_sender.send(Err((
                BTreeSet::new(),
                CeremonyFailureReason::Other(SigningFailureReason::NotEnoughSigners),
            )));
            return;
        }

        let (own_idx, signer_idxs) = match self
            .map_ceremony_parties(&signers, &key_info.validator_map)
        {
            Ok(res) => res,
            Err(reason) => {
                slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "Request to sign ignored: {}", reason);
                let _result = result_sender.send(Err((
                    BTreeSet::new(),
                    CeremonyFailureReason::InvalidParticipants,
                )));
                return;
            }
        };

        if self
            .ceremony_id_tracker
            .is_signing_ceremony_id_used(&ceremony_id)
        {
            slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "Request to sign ignored: ceremony id {} has already been used", ceremony_id);
            let _result = result_sender.send(Err((
                BTreeSet::new(),
                CeremonyFailureReason::CeremonyIdAlreadyUsed,
            )));
            return;
        }

        // We have the key and have received a request to sign
        let logger_no_ceremony_id = &self.logger;

        let state = self.signing_states.entry(ceremony_id).or_insert_with(|| {
            SigningStateRunner::<C>::new_unauthorised(ceremony_id, logger_no_ceremony_id)
        });

        let initial_stage = {
            use super::signing::{frost_stages::AwaitCommitments1, SigningStateCommonInfo};

            let common = CeremonyCommon {
                ceremony_id,
                outgoing_p2p_message_sender: self.outgoing_p2p_message_sender.clone(),
                validator_mapping: key_info.validator_map.clone(),
                own_idx,
                all_idxs: signer_idxs,
                logger: logger.clone(),
                rng,
            };

            let processor = AwaitCommitments1::<C>::new(
                common.clone(),
                SigningStateCommonInfo {
                    data,
                    key: key_info.key.clone(),
                },
            );

            Box::new(BroadcastStage::new(processor, common))
        };

        if let Some(result) =
            state.on_ceremony_request(initial_stage, key_info.validator_map, result_sender)
        {
            self.process_signing_ceremony_outcome(ceremony_id, result);
        };
    }

    /// Process message from another validator
    pub fn process_p2p_message(
        &mut self,
        sender_id: AccountId,
        message: MultisigMessage<C::Point>,
    ) {
        match message {
            MultisigMessage {
                ceremony_id,
                data: MultisigData::Keygen(data),
            } => {
                // NOTE: we should be able to process Keygen messages
                // even when we are "signing"... (for example, if we want to
                // generate a new key)
                self.process_keygen_data(sender_id, ceremony_id, data);
            }
            MultisigMessage {
                ceremony_id,
                data: MultisigData::Signing(data),
            } => {
                // NOTE: we should be able to process Signing messages
                // even when we are generating a new key (for example,
                // we should be able to receive phase1 messages before we've
                // finalized the signing key locally)
                self.process_signing_data(sender_id, ceremony_id, data);
            }
        }
    }

    /// Process data for a signing ceremony arriving from a peer
    pub fn process_signing_data(
        &mut self,
        sender_id: AccountId,
        ceremony_id: CeremonyId,
        data: SigningData<C::Point>,
    ) {
        use std::collections::hash_map::Entry;
        // Check if we have state for this data and delegate message to that state
        // Delay message otherwise

        if self
            .ceremony_id_tracker
            .is_signing_ceremony_id_used(&ceremony_id)
        {
            slog::debug!(
                self.logger,
                "Ignoring signing data from old ceremony id {}",
                ceremony_id
            );
            return;
        }

        slog::debug!(self.logger, "Received signing data {}", &data; CEREMONY_ID_KEY => ceremony_id);

        // Only stage 1 messages can create unauthorised ceremonies
        let state = if matches!(data, SigningData::CommStage1(_)) {
            self.signing_states.entry(ceremony_id).or_insert_with(|| {
                SigningStateRunner::<C>::new_unauthorised(ceremony_id, &self.logger)
            })
        } else {
            match self.signing_states.entry(ceremony_id) {
                Entry::Occupied(entry) => {
                    let state = entry.into_mut();
                    if state.is_authorized() {
                        // Only first stage messages should be processed (delayed) if we're not authorized
                        state
                    } else {
                        slog::debug!(
                            self.logger,
                            "Ignoring non-initial stage signing data for unauthorised ceremony {}",
                            ceremony_id
                        );
                        return;
                    }
                }
                Entry::Vacant(_) => {
                    slog::debug!(
                        self.logger,
                        "Ignoring non-initial stage signing data for non-existent ceremony {}",
                        ceremony_id
                    );
                    return;
                }
            }
        };

        if let Some(result) = state.process_or_delay_message(sender_id, data) {
            self.process_signing_ceremony_outcome(ceremony_id, result);
        }
    }

    /// Process data for a keygen ceremony arriving from a peer
    pub fn process_keygen_data(
        &mut self,
        sender_id: AccountId,
        ceremony_id: CeremonyId,
        data: KeygenData<C::Point>,
    ) {
        use std::collections::hash_map::Entry;

        if self
            .ceremony_id_tracker
            .is_keygen_ceremony_id_used(&ceremony_id)
        {
            slog::debug!(
                self.logger,
                "Ignoring keygen data from old ceremony id {}",
                ceremony_id
            );
            return;
        }

        slog::debug!(self.logger, "Received keygen data {}", &data; CEREMONY_ID_KEY => ceremony_id);

        // Only stage 1 messages can create unauthorised ceremonies
        let state = if matches!(data, KeygenData::HashComm1(_)) {
            self.keygen_states.entry(ceremony_id).or_insert_with(|| {
                KeygenStateRunner::<C>::new_unauthorised(ceremony_id, &self.logger)
            })
        } else {
            match self.keygen_states.entry(ceremony_id) {
                Entry::Occupied(entry) => {
                    let state = entry.into_mut();
                    if state.is_authorized() {
                        // Only first stage messages should be processed (delayed) if we're not authorized
                        state
                    } else {
                        slog::debug!(
                            self.logger,
                            "Ignoring non-initial stage keygen data for unauthorised ceremony {}",
                            ceremony_id
                        );
                        return;
                    }
                }
                Entry::Vacant(_) => {
                    slog::debug!(
                        self.logger,
                        "Ignoring non-initial stage keygen data for non-existent ceremony {}",
                        ceremony_id
                    );
                    return;
                }
            }
        };

        if let Some(result) = state.process_or_delay_message(sender_id, data) {
            self.process_keygen_ceremony_outcome(ceremony_id, result);
        }
    }
}

#[cfg(test)]
impl<C: CryptoScheme> CeremonyManager<C> {
    pub fn expire_all(&mut self) {
        for state in self.signing_states.values_mut() {
            state.set_expiry_time(std::time::Instant::now());
        }

        for state in self.keygen_states.values_mut() {
            state.set_expiry_time(std::time::Instant::now());
        }
    }

    pub fn get_signing_stage_for(&self, ceremony_id: CeremonyId) -> Option<String> {
        self.signing_states
            .get(&ceremony_id)
            .and_then(|s| s.get_stage())
    }

    pub fn get_signing_states_len(&self) -> usize {
        self.signing_states.len()
    }

    pub fn get_keygen_stage_for(&self, ceremony_id: CeremonyId) -> Option<String> {
        self.keygen_states
            .get(&ceremony_id)
            .and_then(|s| s.get_stage())
    }

    pub fn get_keygen_states_len(&self) -> usize {
        self.keygen_states.len()
    }

    /// This should not be used in production as it could
    /// result in pubkeys incompatible with the KeyManager
    /// contract, but it is useful in tests that need to be
    /// deterministic and don't interact with the contract
    pub fn allow_high_pubkey(&mut self) {
        self.allowing_high_pubkey = true;
    }

    pub fn get_delayed_keygen_messages_len(&self, ceremony_id: &CeremonyId) -> usize {
        self.keygen_states
            .get(ceremony_id)
            .unwrap()
            .get_delayed_messages_len()
    }

    pub fn get_delayed_signing_messages_len(&self, ceremony_id: &CeremonyId) -> usize {
        self.signing_states
            .get(ceremony_id)
            .unwrap()
            .get_delayed_messages_len()
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
        hasher.update(id);
    }

    HashContext(*hasher.finalize().as_ref())
}
