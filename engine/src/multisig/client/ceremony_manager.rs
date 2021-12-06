use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::multisig::client::keygen::AwaitCommitments1;
use crate::multisig::client::{self, MultisigOutcome};
use crate::p2p::AccountId;

use client::{
    signing::frost::SigningData, state_runner::StateRunner,
    utils::PartyIdxMapping, CeremonyAbortReason, MultisigOutcomeSender, SchnorrSignature,
};
use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use crate::logging::{
    CEREMONY_ID_KEY, KEYGEN_CEREMONY_FAILED, KEYGEN_REQUEST_EXPIRED, KEYGEN_REQUEST_IGNORED,
    REQUEST_TO_SIGN_EXPIRED, REQUEST_TO_SIGN_IGNORED, SIGNING_CEREMONY_FAILED,
};

use client::common::{broadcast::BroadcastStage, CeremonyCommon, KeygenResultInfo};

use crate::multisig::{KeygenInfo, KeygenOutcome, MessageHash, MultisigInstruction, SigningOutcome};

use super::common::KeygenResult;
use super::{MultisigData, MultisigMessage};
use super::keygen::{HashContext, KeygenData, KeygenOptions};

type SigningStateRunner = StateRunner<SigningData, SchnorrSignature>;
type KeygenStateRunner = StateRunner<KeygenData, KeygenResultInfo>;

/// Responsible for mapping ceremonies to the corresponding states and
/// generating signer indexes based on the list of parties
pub struct CeremonyManager {
    my_account_id: AccountId,
    outgoing_p2p_message_sender: UnboundedSender<(AccountId, MultisigMessage)>,
    signing_states: HashMap<CeremonyId, SigningStateRunner>,
    keygen_states: HashMap<CeremonyId, KeygenStateRunner>,
    logger: slog::Logger,
}

impl CeremonyManager {
    pub fn new(
        my_account_id: AccountId,
        outgoing_p2p_message_sender: UnboundedSender<(AccountId, MultisigMessage)>,
        logger: &slog::Logger,
    ) -> Self {
        CeremonyManager {
            my_account_id,
            outgoing_p2p_message_sender,
            signing_states: HashMap::new(),
            keygen_states: HashMap::new(),
            logger: logger.clone(),
        }
    }

    /// Process message from another validator
    pub fn process_p2p_message(&mut self, sender_id: AccountId, message: MultisigMessage) {
        match message {
            MultisigMessage {
                ceremony_id,
                data: MultisigData::Keygen(data),
            } => {
                // NOTE: we should be able to process Keygen messages
                // even when we are "signing"... (for example, if we want to
                // generate a new key)
                self.process_keygen_data(sender_id, ceremony_id, data)
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

    /// Process `instruction` issued internally (i.e. from SC or another local module)
    pub fn process_multisig_instruction(&mut self, instruction: MultisigInstruction) {
        match instruction {
            MultisigInstruction::Keygen((keygen_info, keygen_options)) => {
                self
                    .on_keygen_request(
                        keygen_info.ceremony_id,
                        keygen_info.signers,
                        keygen_options,
                        keygen_info.result_sender
                    );
            }
            MultisigInstruction::Sign((sign_info, keygen_result_info)) => {
                self.on_signing_request(
                    sign_info.data,
                    keygen_result_info.clone(),
                    sign_info.signers,
                    sign_info.ceremony_id,
                    sign_info.result_sender
                );
            }
        }
    }

    // This function is called periodically to check if any
    // ceremony should be aborted, reporting responsible parties
    // and cleaning up any relevant data
    pub fn cleanup(&mut self) {
        //let mut events_to_send = vec![];

        /*
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
        */
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
    pub fn on_keygen_request(
        &mut self, 
        ceremony_id: CeremonyId,
        signers: Vec<AccountId>,
        keygen_options: KeygenOptions,
        result_sender: oneshot::Sender<Result<KeygenResultInfo, (CeremonyAbortReason, Vec<AccountId>)>>
    ) {
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

        // TODO: Make sure that we don't process past (already removed) ceremonies
        let state = self
            .keygen_states
            .entry(ceremony_id)
            .or_insert_with(|| KeygenStateRunner::new_unauthorised(logger));

        let context = generate_keygen_context(ceremony_id, signers);
        /*
        state.on_keygen_request(
            ceremony_id,
            result_sender,
            self.outgoing_p2p_message_sender.clone(),
            validator_map,
            our_idx,
            signer_idxs,
            keygen_options,
            context,
        );
        */

        // TODO Duplicating the CeremonyCommon should be avoided
        // TODO The validator_mapping is being passed everywhere
        let common = CeremonyCommon {
            ceremony_id,
            outgoing_p2p_message_sender: self.outgoing_p2p_message_sender.clone(),
            validator_mapping: validator_map.clone(),
            own_idx: our_idx,
            all_idxs: signer_idxs,
            logger: self.logger.clone(),
        };

        let processor = AwaitCommitments1::new(
            common.clone(),
            keygen_options,
            context
        );

        let stage = Box::new(BroadcastStage::new(processor, common));

        if let Err(reason) =
            state
                .on_ceremony_request(ceremony_id, stage, validator_map, result_sender)
        {
            slog::warn!(self.logger, #KEYGEN_REQUEST_IGNORED, "Keygen request ignored: {}", reason);
        }
    }

    /// Process a request to sign
    pub fn on_signing_request(
        &mut self,
        data: MessageHash,
        key_info: KeygenResultInfo,
        signers: Vec<AccountId>,
        ceremony_id: CeremonyId,
        result_sender: oneshot::Sender<Result<SchnorrSignature, (CeremonyAbortReason, Vec<AccountId>)>>
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
            result_sender,
        ) {
            // TODO handle result_sender
            slog::warn!(logger, #REQUEST_TO_SIGN_IGNORED, "Request to sign ignored: {}", reason);
        }
    }

    // TODO: Remove duplication (process_signing_data, process_keygen_data)

    /// Process data for a signing ceremony arriving from a peer
    pub fn process_signing_data(
        &mut self,
        sender_id: AccountId,
        ceremony_id: CeremonyId,
        data: SigningData,
    ) {
        let logger = &self.logger;
        let state = self
            .signing_states
            .entry(ceremony_id)
            .or_insert_with(|| SigningStateRunner::new_unauthorised(logger));

        if state.process_message(sender_id, data) {
            self.signing_states.remove(&ceremony_id);
        }
    }

    /// Process data for a keygen ceremony arriving from a peer
    pub fn process_keygen_data(
        &mut self,
        sender_id: AccountId,
        ceremony_id: CeremonyId,
        data: KeygenData,
    ) {
        let logger = &self.logger;
        let state = self
            .keygen_states
            .entry(ceremony_id)
            .or_insert_with(|| KeygenStateRunner::new_unauthorised(logger));

        if state.process_message(sender_id, data) {
            self.keygen_states.remove(&ceremony_id);
        }
    }
}

#[cfg(test)]
impl CeremonyManager {
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
