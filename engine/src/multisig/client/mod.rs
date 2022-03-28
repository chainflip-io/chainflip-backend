#[macro_use]
mod utils;
mod ceremony_id_tracker;
mod common;
mod key_store;
pub mod keygen;
pub mod signing;
mod state_runner;

#[cfg(test)]
mod tests;

pub mod ceremony_manager;

#[cfg(test)]
mod genesis;

use std::{collections::HashMap, sync::Arc, time::Instant};

use crate::{
    common::format_iterator,
    eth::utils::pubkey_to_eth_addr,
    logging::{CEREMONY_ID_KEY, REQUEST_TO_SIGN_EXPIRED},
    multisig::{client::utils::PartyIdxMapping, crypto::Rng, KeyDB, KeyId, MultisigInstruction},
};

use state_chain_runtime::AccountId;

use serde::{Deserialize, Serialize};

use pallet_cf_vaults::CeremonyId;

use key_store::KeyStore;

use tokio::sync::mpsc::UnboundedSender;
use utilities::threshold_from_share_count;

use keygen::KeygenData;

pub use common::KeygenResultInfo;

#[cfg(test)]
pub use utils::ensure_unsorted;

use self::signing::{frost::SigningData, PendingSigningRequest};

pub use keygen::KeygenOptions;

use super::{KeygenRequest, MessageHash, SigningRequest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchnorrSignature {
    /// Scalar component
    pub s: [u8; 32],
    /// Point component (commitment)
    pub r: secp256k1::PublicKey,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThresholdParameters {
    /// Total number of key shares (equals the total number of parties in keygen)
    pub share_count: usize,
    /// Max number of parties that can *NOT* generate signature
    pub threshold: usize,
}

impl ThresholdParameters {
    pub fn from_share_count(share_count: usize) -> Self {
        ThresholdParameters {
            share_count,
            threshold: threshold_from_share_count(share_count as u32) as usize,
        }
    }
}

impl From<SchnorrSignature> for cf_chains::eth::SchnorrVerificationComponents {
    fn from(cfe_sig: SchnorrSignature) -> Self {
        Self {
            s: cfe_sig.s,
            k_times_g_addr: pubkey_to_eth_addr(cfe_sig.r),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MultisigData {
    Keygen(KeygenData),
    Signing(SigningData),
}

derive_try_from_variant!(KeygenData, MultisigData::Keygen, MultisigData);
derive_try_from_variant!(SigningData, MultisigData::Signing, MultisigData);

impl From<SigningData> for MultisigData {
    fn from(data: SigningData) -> Self {
        MultisigData::Signing(data)
    }
}

impl From<KeygenData> for MultisigData {
    fn from(data: KeygenData) -> Self {
        MultisigData::Keygen(data)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultisigMessage {
    ceremony_id: CeremonyId,
    data: MultisigData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CeremonyAbortReason {
    // Isn't used, but will once we re-enable unauthorised reporting this will be used again
    Unauthorised,
    Timeout,
    Invalid,
}

/// (Abort reason, reported ceremony ids)
pub type CeremonyError = (CeremonyAbortReason, Vec<AccountId>);
pub type CeremonyOutcomeResult<Output> = Result<Output, CeremonyError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CeremonyOutcome<Id, Output> {
    pub id: Id,
    pub result: CeremonyOutcomeResult<Output>,
}
impl<Id, Output> CeremonyOutcome<Id, Output> {
    pub fn success(id: Id, output: Output) -> Self {
        Self {
            id,
            result: Ok(output),
        }
    }
    pub fn unauthorised(id: Id, bad_validators: Vec<AccountId>) -> Self {
        Self {
            id,
            result: Err((CeremonyAbortReason::Unauthorised, bad_validators)),
        }
    }
    pub fn timeout(id: Id, bad_validators: Vec<AccountId>) -> Self {
        Self {
            id,
            result: Err((CeremonyAbortReason::Timeout, bad_validators)),
        }
    }
    pub fn invalid(id: Id, bad_validators: Vec<AccountId>) -> Self {
        Self {
            id,
            result: Err((CeremonyAbortReason::Invalid, bad_validators)),
        }
    }
}

/// The final result of a keygen ceremony
pub type KeygenOutcome = CeremonyOutcome<CeremonyId, KeygenResultInfo>;
/// The final result of a Signing ceremony
pub type SigningOutcome = CeremonyOutcome<CeremonyId, SchnorrSignature>;

pub type MultisigOutcomeSender = tokio::sync::mpsc::UnboundedSender<MultisigOutcome>;

#[derive(Debug, Serialize, Deserialize)]
pub enum MultisigOutcome {
    Signing(SigningOutcome),
    Keygen(KeygenOutcome),
}

derive_try_from_variant!(SigningOutcome, MultisigOutcome::Signing, MultisigOutcome);
derive_try_from_variant!(KeygenOutcome, MultisigOutcome::Keygen, MultisigOutcome);

/// Multisig client is is responsible for persistently storing generated keys and
/// delaying signing requests (delegating the actual ceremony management to sub components)
pub struct MultisigClient<S>
where
    S: KeyDB,
{
    my_account_id: AccountId,
    key_store: KeyStore<S>,
    multisig_outcome_sender: MultisigOutcomeSender,
    // Used to forward requests to ceremony_manager
    keygen_request_sender: UnboundedSender<(Rng, KeygenRequest, KeygenOptions)>,
    signing_request_sender: UnboundedSender<(
        Rng,
        MessageHash,
        KeygenResultInfo,
        Vec<AccountId>,
        CeremonyId,
    )>,
    /// Requests awaiting a key
    pending_requests_to_sign: HashMap<KeyId, Vec<PendingSigningRequest>>,
    keygen_options: KeygenOptions,
    logger: slog::Logger,
}

impl<S> MultisigClient<S>
where
    S: KeyDB,
{
    pub fn new(
        my_account_id: AccountId,
        db: S,
        multisig_outcome_sender: MultisigOutcomeSender,
        keygen_request_sender: UnboundedSender<(Rng, KeygenRequest, KeygenOptions)>,
        signing_request_sender: UnboundedSender<(
            Rng,
            MessageHash,
            KeygenResultInfo,
            Vec<AccountId>,
            CeremonyId,
        )>,
        keygen_options: KeygenOptions,
        logger: &slog::Logger,
    ) -> Self {
        MultisigClient {
            my_account_id,
            key_store: KeyStore::new(db),
            multisig_outcome_sender,
            keygen_request_sender,
            signing_request_sender,
            pending_requests_to_sign: Default::default(),
            keygen_options,
            logger: logger.clone(),
        }
    }

    /// Clean up expired states
    pub fn cleanup(&mut self) {
        // cleanup stale signing_request in pending_requests_to_sign
        let logger = &self.logger;

        let mut expired_ceremony_ids = vec![];

        self.pending_requests_to_sign
            .retain(|key_id, pending_signing_requests| {
                pending_signing_requests.retain(|pending| {
                    if pending.should_expire_at < Instant::now() {
                        let ceremony_id = pending.signing_request.ceremony_id;

                        slog::warn!(
                            logger,
                            #REQUEST_TO_SIGN_EXPIRED,
                            "Request to sign expired waiting for key id: {:?}",
                            key_id;
                            CEREMONY_ID_KEY => ceremony_id,
                        );

                        expired_ceremony_ids.push(ceremony_id);
                        return false;
                    }
                    true
                });
                !pending_signing_requests.is_empty()
            });

        for id in expired_ceremony_ids {
            if let Err(err) = self
                .multisig_outcome_sender
                .send(MultisigOutcome::Keygen(KeygenOutcome::timeout(id, vec![])))
            {
                slog::error!(
                    self.logger,
                    "Could not send KeygenOutcome::timeout: {}",
                    err
                );
            }
        }
    }

    fn single_party_keygen(&mut self, keygen_request: KeygenRequest) {
        slog::info!(self.logger, "Performing solo keygen");

        if !keygen_request.signers.contains(&self.my_account_id) {
            slog::warn!(
                self.logger,
                "Keygen request ignored: we are not among participants"
            );
            return;
        }

        use crate::multisig::crypto::{KeyShare, Point, Scalar};
        use common::KeygenResult;

        let params = ThresholdParameters::from_share_count(1);

        // By default this will have a 50/50 chance of generating
        // a contract incompatible signature to match the behavior
        // of multi-party ceremonies. Toggle this off to always
        // generate a contract compatible signature.
        const ALLOWING_HIGH_PUBKEY: bool = true;

        let (secret_key, public_key) = loop {
            let secret_key = {
                use rand_legacy::FromEntropy;
                let mut rng = Rng::from_entropy();
                Scalar::random(&mut rng)
            };

            let public_key = Point::from_scalar(&secret_key);

            if keygen::is_contract_compatible(&public_key.get_element()) || ALLOWING_HIGH_PUBKEY {
                break (secret_key, public_key);
            }
        };

        let key_result_info = KeygenResultInfo {
            key: Arc::new(KeygenResult {
                key_share: KeyShare {
                    y: public_key,
                    x_i: secret_key,
                },
                // This is not going to be used in solo ceremonies
                party_public_keys: vec![public_key],
            }),
            validator_map: Arc::new(PartyIdxMapping::from_unsorted_signers(
                &keygen_request.signers,
            )),
            params,
        };

        self.on_key_generated(key_result_info);
    }

    fn single_party_signing(
        &mut self,
        signing_request: SigningRequest,
        keygen_result_info: KeygenResultInfo,
    ) {
        use crate::multisig::crypto::{Point, Scalar};

        slog::info!(self.logger, "Performing solo signing");

        if !signing_request.signers.contains(&self.my_account_id) {
            slog::warn!(
                self.logger,
                "Signing request ignored: we are not among participants"
            );
            return;
        }

        let key = &keygen_result_info.key.key_share;

        let nonce = {
            use rand_legacy::FromEntropy;
            let mut rng = Rng::from_entropy();
            Scalar::random(&mut rng)
        };

        let r = Point::from_scalar(&nonce);

        let sigma = signing::frost::generate_contract_schnorr_sig(
            key.x_i.clone(),
            key.y,
            r,
            nonce,
            &signing_request.data.0,
        );

        let sig = SchnorrSignature {
            s: *sigma.as_bytes(),
            r: r.get_element(),
        };

        self.multisig_outcome_sender
            .send(MultisigOutcome::Signing(SigningOutcome {
                id: signing_request.ceremony_id,
                result: Ok(sig),
            }))
            .unwrap();
    }

    /// Process `instruction` issued internally (i.e. from SC or another local module)
    pub fn process_multisig_instruction(
        &mut self,
        instruction: MultisigInstruction,
        rng: &mut Rng,
    ) {
        match instruction {
            MultisigInstruction::Keygen(keygen_request) => {
                use rand_legacy::{Rng as _, SeedableRng};

                slog::info!(
                    self.logger,
                    "Received a keygen request, participants: {}",
                    format_iterator(&keygen_request.signers);
                    CEREMONY_ID_KEY => keygen_request.ceremony_id
                );
                let rng = Rng::from_seed(rng.gen());

                if keygen_request.signers.len() == 1 {
                    self.single_party_keygen(keygen_request);
                } else {
                    self.keygen_request_sender
                        .send((rng, keygen_request, self.keygen_options));
                }
            }
            MultisigInstruction::Sign(signing_request) => {
                let key_id = &signing_request.key_id;

                slog::debug!(
                    self.logger,
                    "Received a request to sign, message_hash: {}, signers: {}",
                    signing_request.data, format_iterator(&signing_request.signers);
                    CEREMONY_ID_KEY => signing_request.ceremony_id
                );

                let key = self.key_store.get_key(key_id).cloned();
                match key {
                    Some(keygen_result_info) => {
                        use rand_legacy::{Rng as _, SeedableRng};
                        let rng = Rng::from_seed(rng.gen());
                        if signing_request.signers.len() == 1 {
                            self.single_party_signing(signing_request, keygen_result_info);
                        } else {
                            self.signing_request_sender
                                .send((
                                    rng,
                                    signing_request.data,
                                    keygen_result_info,
                                    signing_request.signers,
                                    signing_request.ceremony_id,
                                ))
                                .unwrap();
                        }
                    }
                    None => {
                        // The key is not ready, delay until either it is ready or timeout

                        slog::debug!(
                            self.logger,
                            "Delaying a request to sign for unknown key: {:?}",
                            signing_request.key_id;
                            CEREMONY_ID_KEY => signing_request.ceremony_id
                        );

                        self.pending_requests_to_sign
                            .entry(signing_request.key_id.clone())
                            .or_default()
                            .push(PendingSigningRequest::new(signing_request));
                    }
                }
            }
        }
    }

    /// Process requests to sign that required the key in `key_info`
    fn process_pending_requests_to_sign(&mut self, key_info: KeygenResultInfo) {
        if let Some(reqs) = self
            .pending_requests_to_sign
            .remove(&KeyId(key_info.key.get_public_key_bytes()))
        {
            for pending in reqs {
                let signing_request = pending.signing_request;
                slog::debug!(
                    self.logger,
                    "Processing a pending request to sign";
                    CEREMONY_ID_KEY => signing_request.ceremony_id
                );

                use rand_legacy::FromEntropy;

                let rng = Rng::from_entropy();

                self.signing_request_sender.send((
                    rng,
                    signing_request.data,
                    key_info.clone(),
                    signing_request.signers,
                    signing_request.ceremony_id,
                ));
            }
        }
    }

    pub fn on_key_generated(&mut self, key_info: KeygenResultInfo) {
        self.key_store
            .set_key(KeyId(key_info.key.get_public_key_bytes()), key_info.clone());
        self.process_pending_requests_to_sign(key_info);
    }
}

#[cfg(test)]
impl<S> MultisigClient<S>
where
    S: KeyDB,
{
    pub fn get_key(&self, key_id: &KeyId) -> Option<&KeygenResultInfo> {
        self.key_store.get_key(key_id)
    }

    pub fn get_db(&self) -> &S {
        self.key_store.get_db()
    }
}
