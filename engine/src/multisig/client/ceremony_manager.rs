use anyhow::Result;
use std::collections::{hash_map::Entry, BTreeSet, HashMap};
use std::fmt::Display;
use std::sync::Arc;

use crate::multisig::client;
use crate::multisig::client::common::{KeygenFailureReason, SigningFailureReason};
use crate::multisig::client::keygen::generate_key_data_until_compatible;
use crate::multisig::crypto::ECScalar;
use crate::multisig::crypto::{CryptoScheme, ECPoint, Rng};
use crate::multisig_p2p::OutgoingMultisigStageMessages;
use cf_traits::{AuthorityCount, CeremonyId};
use state_chain_runtime::AccountId;

use client::{
    ceremony_runner::CeremonyRunner, signing::frost::SigningData, utils::PartyIdxMapping,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use crate::logging::{CEREMONY_ID_KEY, CEREMONY_TYPE_KEY};

use client::common::{
    broadcast::BroadcastStage, CeremonyCommon, CeremonyFailureReason, KeygenResultInfo,
};

use crate::multisig::MessageHash;

use super::common::PreProcessStageDataCheck;
use super::keygen::{HashCommitments1, HashContext, KeygenData};
use super::{MultisigData, MultisigMessage};

#[cfg(test)]
use client::common::CeremonyStageName;

pub type CeremonyResultSender<T, R> =
    oneshot::Sender<Result<T, (BTreeSet<AccountId>, CeremonyFailureReason<R>)>>;
pub type CeremonyResultReceiver<T, R> =
    oneshot::Receiver<Result<T, (BTreeSet<AccountId>, CeremonyFailureReason<R>)>>;

/// Responsible for mapping ceremonies to the corresponding states and
/// generating signer indexes based on the list of parties
pub struct CeremonyManager<C: CryptoScheme> {
    my_account_id: AccountId,
    outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    signing_states: CeremonyStates<
        SigningData<<C as CryptoScheme>::Point>,
        <C as CryptoScheme>::Signature,
        SigningFailureReason,
    >,
    keygen_states: CeremonyStates<
        KeygenData<<C as CryptoScheme>::Point>,
        KeygenResultInfo<<C as CryptoScheme>::Point>,
        KeygenFailureReason,
    >,
    allowing_high_pubkey: bool,
    latest_ceremony_id: CeremonyId,
    logger: slog::Logger,
}

impl<C: CryptoScheme> CeremonyManager<C> {
    pub fn new(
        my_account_id: AccountId,
        outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
        latest_ceremony_id: CeremonyId,
        logger: &slog::Logger,
    ) -> Self {
        CeremonyManager {
            my_account_id,
            outgoing_p2p_message_sender,
            signing_states: CeremonyStates::new(),
            keygen_states: CeremonyStates::new(),
            allowing_high_pubkey: false,
            latest_ceremony_id,
            logger: logger.clone(),
        }
    }

    // This function is called periodically to check if any
    // ceremony should be aborted, reporting responsible parties
    // and cleaning up any relevant data
    pub fn check_all_timeouts(&mut self) {
        self.signing_states.try_expiring_all(&self.logger);
        self.keygen_states.try_expiring_all(&self.logger);
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

    /// Process a keygen request
    pub fn on_keygen_request(
        &mut self,
        ceremony_id: CeremonyId,
        participants: Vec<AccountId>,
        rng: Rng,
        result_sender: CeremonyResultSender<KeygenResultInfo<C::Point>, KeygenFailureReason>,
    ) {
        assert!(
            !participants.is_empty(),
            "Keygen request has no participants"
        );

        let logger_with_ceremony_id = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        slog::debug!(logger_with_ceremony_id, "Processing a keygen request");

        if participants.len() == 1 {
            let _result = result_sender.send(Ok(self.single_party_keygen(rng)));
            return;
        }

        let validator_map = Arc::new(PartyIdxMapping::from_unsorted_signers(&participants));

        let (our_idx, signer_idxs) = match self.map_ceremony_parties(&participants, &validator_map)
        {
            Ok(res) => res,
            Err(reason) => {
                slog::debug!(
                    logger_with_ceremony_id,
                    "Keygen request invalid: {}",
                    reason
                );
                let _result = result_sender.send(Err((
                    BTreeSet::new(),
                    CeremonyFailureReason::InvalidParticipants,
                )));
                return;
            }
        };

        let num_of_participants: AuthorityCount =
            signer_idxs.len().try_into().expect("too many participants");

        let state = self
            .keygen_states
            .get_state_or_create_unauthorized(ceremony_id, &self.logger);

        let initial_stage = {
            let common = CeremonyCommon {
                ceremony_id,
                outgoing_p2p_message_sender: self.outgoing_p2p_message_sender.clone(),
                validator_mapping: validator_map.clone(),
                own_idx: our_idx,
                all_idxs: signer_idxs,
                logger: logger_with_ceremony_id,
                rng,
            };

            let processor = HashCommitments1::new(
                common.clone(),
                self.allowing_high_pubkey,
                generate_keygen_context(ceremony_id, participants),
            );

            Box::new(BroadcastStage::new(processor, common))
        };

        if let Some(result) = state.on_ceremony_request(
            initial_stage,
            validator_map,
            result_sender,
            num_of_participants,
        ) {
            self.keygen_states
                .process_ceremony_outcome(ceremony_id, result);
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
        assert!(!signers.is_empty(), "Request to sign has no signers");

        let logger_with_ceremony_id = self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id));

        slog::debug!(logger_with_ceremony_id, "Processing a request to sign");

        if signers.len() == 1 {
            let _result = result_sender.send(Ok(self.single_party_signing(data, key_info, rng)));
            return;
        }

        // Check that the number of signers is enough
        let minimum_signers_needed = key_info.params.threshold + 1;
        let signers_len: AuthorityCount = signers.len().try_into().expect("too many signers");
        if signers_len < minimum_signers_needed {
            slog::debug!(
                logger_with_ceremony_id,
                "Request to sign invalid: not enough signers ({}/{})",
                signers.len(),
                minimum_signers_needed
            );
            let _result = result_sender.send(Err((
                BTreeSet::new(),
                CeremonyFailureReason::Other(SigningFailureReason::NotEnoughSigners),
            )));
            return;
        }

        let (own_idx, signer_idxs) =
            match self.map_ceremony_parties(&signers, &key_info.validator_map) {
                Ok(res) => res,
                Err(reason) => {
                    slog::debug!(
                        logger_with_ceremony_id,
                        "Request to sign invalid: {}",
                        reason
                    );
                    let _result = result_sender.send(Err((
                        BTreeSet::new(),
                        CeremonyFailureReason::InvalidParticipants,
                    )));
                    return;
                }
            };

        // We have the key and have received a request to sign
        let state = self
            .signing_states
            .get_state_or_create_unauthorized(ceremony_id, &self.logger);

        let initial_stage = {
            use super::signing::{frost_stages::AwaitCommitments1, SigningStateCommonInfo};

            let common = CeremonyCommon {
                ceremony_id,
                outgoing_p2p_message_sender: self.outgoing_p2p_message_sender.clone(),
                validator_mapping: key_info.validator_map.clone(),
                own_idx,
                all_idxs: signer_idxs,
                logger: logger_with_ceremony_id,
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

        if let Some(result) = state.on_ceremony_request(
            initial_stage,
            key_info.validator_map,
            result_sender,
            signers_len,
        ) {
            self.signing_states
                .process_ceremony_outcome(ceremony_id, result);
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
            } => self.keygen_states.process_data(
                sender_id,
                ceremony_id,
                data,
                self.latest_ceremony_id,
                &self
                    .logger
                    .new(slog::o!(CEREMONY_ID_KEY => ceremony_id, CEREMONY_TYPE_KEY => "keygen")),
            ),
            MultisigMessage {
                ceremony_id,
                data: MultisigData::Signing(data),
            } => self.signing_states.process_data(
                sender_id,
                ceremony_id,
                data,
                self.latest_ceremony_id,
                &self
                    .logger
                    .new(slog::o!(CEREMONY_ID_KEY => ceremony_id, CEREMONY_TYPE_KEY => "signing")),
            ),
        }
    }

    /// Override the latest ceremony id. Used to limit the spamming of unauthorised ceremonies.
    pub fn update_latest_ceremony_id(&mut self, ceremony_id: CeremonyId) {
        self.latest_ceremony_id = ceremony_id;
    }

    fn single_party_keygen(&self, rng: Rng) -> KeygenResultInfo<C::Point> {
        slog::info!(self.logger, "Performing solo keygen");

        let (_key_id, key_data) =
            generate_key_data_until_compatible(&[self.my_account_id.clone()], 30, rng);
        key_data[&self.my_account_id].clone()
    }

    fn single_party_signing(
        &self,
        data: MessageHash,
        keygen_result_info: KeygenResultInfo<C::Point>,
        mut rng: Rng,
    ) -> C::Signature {
        slog::info!(self.logger, "Performing solo signing");

        let key = &keygen_result_info.key.key_share;

        let nonce = <C::Point as ECPoint>::Scalar::random(&mut rng);

        let r = C::Point::from_scalar(&nonce);

        let sigma = client::signing::frost::generate_schnorr_response::<C>(
            &key.x_i, key.y, r, nonce, &data.0,
        );

        C::build_signature(sigma, r)
    }
}

#[cfg(test)]
impl<C: CryptoScheme> CeremonyManager<C> {
    pub fn expire_all(&mut self) {
        self.signing_states.expire_all();
        self.keygen_states.expire_all();
    }

    pub fn add_keygen_state(
        &mut self,
        ceremony_id: CeremonyId,
        state: CeremonyRunner<
            KeygenData<<C as CryptoScheme>::Point>,
            KeygenResultInfo<<C as CryptoScheme>::Point>,
            KeygenFailureReason,
        >,
    ) {
        self.keygen_states.add_state(ceremony_id, state);
    }

    pub fn get_signing_states_len(&self) -> usize {
        self.signing_states.len()
    }

    pub fn get_keygen_states_len(&self) -> usize {
        self.keygen_states.len()
    }

    pub fn get_keygen_awaited_parties_count_for(
        &self,
        ceremony_id: &CeremonyId,
    ) -> Option<AuthorityCount> {
        self.keygen_states
            .get_awaited_parties_count_for(ceremony_id)
    }

    /// This should not be used in production as it could
    /// result in pubkeys incompatible with the KeyManager
    /// contract, but it is useful in tests that need to be
    /// deterministic and don't interact with the contract
    pub fn allow_high_pubkey(&mut self) {
        self.allowing_high_pubkey = true;
    }

    pub fn get_delayed_keygen_messages_len(&self, ceremony_id: &CeremonyId) -> usize {
        self.keygen_states.get_delayed_messages_len(ceremony_id)
    }

    pub fn get_delayed_signing_messages_len(&self, ceremony_id: &CeremonyId) -> usize {
        self.signing_states.get_delayed_messages_len(ceremony_id)
    }

    pub fn get_keygen_stage_name(&self, ceremony_id: CeremonyId) -> Option<CeremonyStageName> {
        self.keygen_states.get_stage_for(&ceremony_id)
    }

    pub fn get_signing_stage_name(&self, ceremony_id: CeremonyId) -> Option<CeremonyStageName> {
        self.signing_states.get_stage_for(&ceremony_id)
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

struct CeremonyStates<CeremonyData, CeremonyResult, FailureReason> {
    inner: HashMap<u64, CeremonyRunner<CeremonyData, CeremonyResult, FailureReason>>,
}

impl<CeremonyData, CeremonyResult, FailureReason>
    CeremonyStates<CeremonyData, CeremonyResult, FailureReason>
where
    CeremonyData: Display + PreProcessStageDataCheck,
    FailureReason: Display,
{
    fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Process ceremony data arriving from a peer,
    /// returns an error if the data is rejected before being processed
    fn process_data(
        &mut self,
        sender_id: AccountId,
        ceremony_id: CeremonyId,
        data: CeremonyData,
        _latest_ceremony_id: CeremonyId,
        logger: &slog::Logger,
    ) {
        slog::debug!(logger, "Received data {}", &data);

        // Only stage 1 messages can create unauthorised ceremonies
        let state = if data.is_first_stage() {
            match self.inner.entry(ceremony_id) {
                Entry::Vacant(entry) => {
                    // TODO: See issue #1972
                    entry.insert(CeremonyRunner::new_unauthorised(ceremony_id, logger))
                }
                Entry::Occupied(entry) => entry.into_mut(),
            }
        } else {
            match self.inner.get_mut(&ceremony_id) {
                Some(state) => {
                    if state.is_authorized() {
                        // Only first stage messages should be processed (delayed) if we're not authorized
                        state
                    } else {
                        slog::debug!(
                            logger,
                            "Ignoring data: non-initial stage data for unauthorised ceremony"
                        );
                        return;
                    }
                }
                None => {
                    slog::debug!(
                        logger,
                        "Ignoring data: non-initial stage data for non-existent ceremony"
                    );
                    return;
                }
            }
        };

        // Check that the number of elements in the data is what we expect
        if !data.data_size_is_valid(state.get_participant_count()) {
            slog::debug!(logger, "Ignoring data: incorrect number of elements");
            return;
        }

        if let Some(result) = state.process_or_delay_message(sender_id, data) {
            self.process_ceremony_outcome(ceremony_id, result);
        }
    }

    /// Send the ceremony outcome through the result channel
    fn process_ceremony_outcome(
        &mut self,
        ceremony_id: CeremonyId,
        result: Result<CeremonyResult, (BTreeSet<AccountId>, CeremonyFailureReason<FailureReason>)>,
    ) {
        let _result = self
            .inner
            .remove(&ceremony_id)
            .expect("Ceremony should exist")
            .try_into_result_sender()
            .expect("Ceremony should have a result sender")
            .send(result);
    }

    /// Iterate over all of the states and resolve any that are expired
    fn try_expiring_all(&mut self, logger: &slog::Logger) {
        // Copy the keys so we can iterate over them while at the same time
        // removing the elements as we go
        let ceremony_ids: Vec<_> = self.inner.keys().copied().collect();

        for ceremony_id in &ceremony_ids {
            let state = self.inner.get_mut(ceremony_id).expect("state must exist");
            if let Some(result) = state.try_expiring() {
                // NOTE: we only respond if we have received a ceremony request from the SC
                // (i.e. the ceremony is "authorized")
                if state.is_authorized() {
                    self.process_ceremony_outcome(*ceremony_id, result);
                } else {
                    slog::warn!(logger, "Removing expired unauthorised ceremony"; CEREMONY_ID_KEY => ceremony_id);
                    self.inner.remove(ceremony_id);
                }
            }
        }
    }

    /// Returns the state for the given ceremony id if it exists,
    /// otherwise creates a new unauthorized one
    fn get_state_or_create_unauthorized(
        &mut self,
        ceremony_id: CeremonyId,
        logger: &slog::Logger,
    ) -> &mut CeremonyRunner<CeremonyData, CeremonyResult, FailureReason> {
        self.inner
            .entry(ceremony_id)
            .or_insert_with(|| CeremonyRunner::new_unauthorised(ceremony_id, logger))
    }
}

#[cfg(test)]
impl<CeremonyData, CeremonyResult, FailureReason>
    CeremonyStates<CeremonyData, CeremonyResult, FailureReason>
where
    CeremonyData: Display + PreProcessStageDataCheck,
    FailureReason: Display,
{
    fn expire_all(&mut self) {
        for state in self.inner.values_mut() {
            let one_second_ago = std::time::Instant::now() - std::time::Duration::from_secs(1);
            state.set_expiry_time(one_second_ago);
        }
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn get_stage_for(&self, ceremony_id: &CeremonyId) -> Option<CeremonyStageName> {
        self.inner.get(ceremony_id).and_then(|s| s.get_stage_name())
    }

    pub fn get_awaited_parties_count_for(
        &self,
        ceremony_id: &CeremonyId,
    ) -> Option<AuthorityCount> {
        self.inner
            .get(ceremony_id)
            .and_then(|s| s.get_awaited_parties_count())
    }

    pub fn add_state(
        &mut self,
        ceremony_id: CeremonyId,
        state: CeremonyRunner<CeremonyData, CeremonyResult, FailureReason>,
    ) {
        self.inner.insert(ceremony_id, state);
    }

    pub fn get_delayed_messages_len(&self, ceremony_id: &CeremonyId) -> usize {
        self.inner
            .get(ceremony_id)
            .unwrap()
            .get_delayed_messages_len()
    }
}
