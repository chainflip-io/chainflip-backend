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
    constants::PENDING_SIGN_DURATION,
    eth::utils::pubkey_to_eth_addr,
    logging::{CEREMONY_ID_KEY, REQUEST_TO_SIGN_EXPIRED},
    multisig::{client::utils::PartyIdxMapping, crypto::Rng, KeyDB, KeyId},
};

use async_trait::async_trait;
use futures::Future;
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

use self::{
    ceremony_manager::{CeremonyResultReceiver, CeremonyResultSender},
    signing::{frost::SigningData, PendingSigningRequest},
};

pub use keygen::KeygenOptions;

use super::MessageHash;

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

#[cfg(test)]
use mockall::automock;

/// The public interface to the multi-signature code
#[cfg_attr(test, automock)]
#[async_trait]
pub trait MultisigClientApi {
    async fn keygen(
        &self,
        ceremony_id: CeremonyId,
        participants: Vec<AccountId>,
    ) -> Result<secp256k1::PublicKey, (Vec<AccountId>, anyhow::Error)>;
    async fn sign(
        &self,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        signers: Vec<AccountId>,
        data: MessageHash,
    ) -> Result<SchnorrSignature, (Vec<AccountId>, anyhow::Error)>;
}

struct InnerMultisigClientState<KeyDatabase: KeyDB> {
    key_store: KeyStore<KeyDatabase>,
    pending_requests_to_sign: HashMap<KeyId, Vec<PendingSigningRequest>>,
}

/// Multisig client is is responsible for persistently storing generated keys and
/// delaying signing requests (delegating the actual ceremony management to sub components)
pub struct MultisigClient<KeyDatabase>
where
    KeyDatabase: KeyDB,
{
    my_account_id: AccountId,
    keygen_request_sender: UnboundedSender<(
        CeremonyId,
        Vec<AccountId>,
        KeygenOptions,
        Rng,
        CeremonyResultSender<KeygenResultInfo>,
    )>,
    signing_request_sender: UnboundedSender<(
        CeremonyId,
        Vec<AccountId>,
        MessageHash,
        KeygenResultInfo,
        Rng,
        CeremonyResultSender<SchnorrSignature>,
    )>,
    inner_state: std::sync::Mutex<InnerMultisigClientState<KeyDatabase>>,
    keygen_options: KeygenOptions,
    logger: slog::Logger,
}

enum RequestStatus<Output> {
    Ready(Output),
    WaitForOneshot(CeremonyResultReceiver<Output>),
}

impl<S> MultisigClient<S>
where
    S: KeyDB,
{
    pub fn new(
        my_account_id: AccountId,
        db: S,
        keygen_request_sender: UnboundedSender<(
            CeremonyId,
            Vec<AccountId>,
            KeygenOptions,
            Rng,
            CeremonyResultSender<KeygenResultInfo>,
        )>,
        signing_request_sender: UnboundedSender<(
            CeremonyId,
            Vec<AccountId>,
            MessageHash,
            KeygenResultInfo,
            Rng,
            CeremonyResultSender<SchnorrSignature>,
        )>,
        keygen_options: KeygenOptions,
        logger: &slog::Logger,
    ) -> Self {
        MultisigClient {
            my_account_id,
            inner_state: std::sync::Mutex::new(InnerMultisigClientState {
                key_store: KeyStore::new(db),
                pending_requests_to_sign: Default::default(),
            }),
            keygen_request_sender,
            signing_request_sender,
            keygen_options,
            logger: logger.clone(),
        }
    }

    // This function is structured to simplify the writing of tests (i.e. should_delay_rts_until_key_is_ready).
    // When the function is called it will send the request to the CeremonyManager/Backend immediately
    // The function returns a future that will complete only once the CeremonyManager has finished
    // the ceremony. This allows tests to split making the request and waiting for the result.
    pub fn initiate_keygen(
        &self,
        ceremony_id: CeremonyId,
        participants: Vec<AccountId>,
    ) -> impl '_ + Future<Output = Result<secp256k1::PublicKey, (Vec<AccountId>, anyhow::Error)>>
    {
        assert!(participants.contains(&self.my_account_id));

        slog::info!(
            self.logger,
            "Received a keygen request, participants: {}",
            format_iterator(&participants);
            CEREMONY_ID_KEY => ceremony_id
        );

        use rand_legacy::FromEntropy;
        let rng = Rng::from_entropy();

        let request = if participants.len() == 1 {
            RequestStatus::Ready(self.single_party_keygen(rng))
        } else {
            let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
            self.keygen_request_sender
                .send((
                    ceremony_id,
                    participants,
                    self.keygen_options,
                    rng,
                    result_sender,
                ))
                .ok()
                .unwrap();
            RequestStatus::WaitForOneshot(result_receiver)
        };

        async move {
            let result = match request {
                RequestStatus::Ready(keygen_result_info) => Some(Ok(keygen_result_info)),
                RequestStatus::WaitForOneshot(result_receiver) => result_receiver.await.ok(),
            };

            match result {
                Some(Ok(keygen_result_info)) => {
                    let key_id = KeyId(keygen_result_info.key.get_public_key_bytes());

                    let mut inner_state = self.inner_state.lock().unwrap();

                    inner_state
                        .key_store
                        .set_key(key_id.clone(), keygen_result_info.clone());

                    // Process requests to sign that required the key in `key_info`
                    if let Some(requests) = inner_state.pending_requests_to_sign.remove(&key_id) {
                        for pending_request in requests {
                            slog::debug!(
                                self.logger,
                                "Processing a pending request to sign";
                                CEREMONY_ID_KEY => pending_request.ceremony_id
                            );
                            if pending_request.signers.len() == 1 {
                                let _result = pending_request.result_sender.send(Ok(self
                                    .single_party_signing(
                                        pending_request.data,
                                        keygen_result_info.clone(),
                                        pending_request.rng,
                                    )));
                            } else {
                                self.signing_request_sender
                                    .send((
                                        pending_request.ceremony_id,
                                        pending_request.signers,
                                        pending_request.data,
                                        keygen_result_info.clone(),
                                        pending_request.rng,
                                        pending_request.result_sender,
                                    ))
                                    .unwrap();
                            }
                        }
                    }

                    Ok(keygen_result_info.key.get_public_key().get_element())
                }
                Some(Err(error)) => Err(error),
                None => Err((vec![], anyhow::Error::msg("Keygen request ignored"))),
            }
        }
    }

    // Similarly to initiate_keygen this function is structured to simplify the writing of tests (i.e. should_delay_rts_until_key_is_ready).
    // Once the async function returns it has sent the request to the CeremonyManager/Backend
    // and outputs a second future that will complete only once the CeremonyManager has finished
    // the ceremony. This allows tests to split making the request and waiting for the result.
    pub async fn initiate_signing(
        &self,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        signers: Vec<AccountId>,
        data: MessageHash,
    ) -> impl '_ + Future<Output = Result<SchnorrSignature, (Vec<AccountId>, anyhow::Error)>> {
        assert!(signers.contains(&self.my_account_id));

        slog::debug!(
            self.logger,
            "Received a request to sign, message_hash: {}, signers: {}",
            data, format_iterator(&signers);
            CEREMONY_ID_KEY => ceremony_id
        );

        use rand_legacy::FromEntropy;
        let rng = Rng::from_entropy();

        let mut inner_state = self.inner_state.lock().unwrap();

        let request = match inner_state.key_store.get_key(&key_id).cloned() {
            Some(keygen_result_info) => {
                if signers.len() == 1 {
                    RequestStatus::Ready(self.single_party_signing(data, keygen_result_info, rng))
                } else {
                    let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                    self.signing_request_sender
                        .send((
                            ceremony_id,
                            signers,
                            data,
                            keygen_result_info,
                            rng,
                            result_sender,
                        ))
                        .unwrap();
                    RequestStatus::WaitForOneshot(result_receiver)
                }
            }
            None => {
                // The key is not ready, delay until either it is ready or timeout

                slog::debug!(
                    self.logger,
                    "Delaying a request to sign for unknown key: {:?}",
                    key_id;
                    CEREMONY_ID_KEY => ceremony_id
                );

                let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
                inner_state
                    .pending_requests_to_sign
                    .entry(key_id)
                    .or_default()
                    .push(PendingSigningRequest {
                        ceremony_id,
                        signers,
                        data,
                        rng,
                        should_expire_at: Instant::now() + PENDING_SIGN_DURATION,
                        result_sender,
                    });

                RequestStatus::WaitForOneshot(result_receiver)
            }
        };

        Box::pin(async move {
            match request {
                RequestStatus::Ready(signature) => Ok(signature),
                RequestStatus::WaitForOneshot(result_receiver) => {
                    if let Ok(result) = result_receiver.await {
                        result
                    } else {
                        Err((vec![], anyhow::Error::msg("Signing request ignored")))
                    }
                }
            }
        })
    }

    /// Clean up stale pending signing requests
    #[allow(clippy::unnecessary_filter_map)] // Clippy is wrong
    pub async fn check_timeouts(&self) {
        let logger = &self.logger;

        let mut inner_state = self.inner_state.lock().unwrap();

        inner_state.pending_requests_to_sign
            .retain(|key_id, pending_signing_requests| {
                // TODO: Replace with drain_filter() once stablized
                *pending_signing_requests = pending_signing_requests.drain(..).filter_map(|pending_signing_request| {
                    if pending_signing_request.should_expire_at < Instant::now() {
                        // TODO: Remove this logging and add these details to the Result error (Possibly by using an error enum (instead of an anyhow::Error) that has a variant for each type of error.
                        slog::warn!(
                            logger,
                            #REQUEST_TO_SIGN_EXPIRED,
                            "Request to sign expired waiting for key id: {:?}",
                            key_id;
                            CEREMONY_ID_KEY => pending_signing_request.ceremony_id,
                        );

                        let _result = pending_signing_request.result_sender.send(Err((vec![], anyhow::Error::msg("Signing ceremony timed out before the associated key was generated."))));

                        None
                    } else {
                        Some(pending_signing_request)
                    }
                }).collect();
                !pending_signing_requests.is_empty()
            });
    }

    fn single_party_keygen(&self, mut rng: Rng) -> KeygenResultInfo {
        slog::info!(self.logger, "Performing solo keygen");

        use crate::multisig::crypto::{KeyShare, Point, Scalar};
        use common::KeygenResult;

        let params = ThresholdParameters::from_share_count(1);

        // By default this will have a 50/50 chance of generating
        // a contract incompatible signature to match the behavior
        // of multi-party ceremonies. Toggle this off to always
        // generate a contract compatible signature.
        const ALLOWING_HIGH_PUBKEY: bool = true;

        let (secret_key, public_key) = loop {
            let secret_key = Scalar::random(&mut rng);

            let public_key = Point::from_scalar(&secret_key);

            if keygen::is_contract_compatible(&public_key.get_element()) || ALLOWING_HIGH_PUBKEY {
                break (secret_key, public_key);
            }
        };

        KeygenResultInfo {
            key: Arc::new(KeygenResult {
                key_share: KeyShare {
                    y: public_key,
                    x_i: secret_key,
                },
                // This is not going to be used in solo ceremonies
                party_public_keys: vec![public_key],
            }),
            validator_map: Arc::new(PartyIdxMapping::from_unsorted_signers(&[self
                .my_account_id
                .clone()])),
            params,
        }
    }

    fn single_party_signing(
        &self,
        data: MessageHash,
        keygen_result_info: KeygenResultInfo,
        mut rng: Rng,
    ) -> SchnorrSignature {
        use crate::multisig::crypto::{Point, Scalar};

        slog::info!(self.logger, "Performing solo signing");

        let key = &keygen_result_info.key.key_share;

        let nonce = Scalar::random(&mut rng);

        let r = Point::from_scalar(&nonce);

        let sigma = signing::frost::generate_contract_schnorr_sig(
            key.x_i.clone(),
            key.y,
            r,
            nonce,
            &data.0,
        );

        SchnorrSignature {
            s: *sigma.as_bytes(),
            r: r.get_element(),
        }
    }

    #[cfg(test)]
    pub fn expire_all(&self) {
        for pending in self
            .inner_state
            .try_lock()
            .unwrap()
            .pending_requests_to_sign
            .values_mut()
            .flatten()
        {
            pending.should_expire_at = std::time::Instant::now();
        }
    }
}

#[async_trait]
impl<KeyDatabase: KeyDB + Send + Sync> MultisigClientApi for MultisigClient<KeyDatabase> {
    async fn keygen(
        &self,
        ceremony_id: CeremonyId,
        participants: Vec<AccountId>,
    ) -> Result<secp256k1::PublicKey, (Vec<AccountId>, anyhow::Error)> {
        self.initiate_keygen(ceremony_id, participants).await
    }

    async fn sign(
        &self,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        signers: Vec<AccountId>,
        data: MessageHash,
    ) -> Result<SchnorrSignature, (Vec<AccountId>, anyhow::Error)> {
        self.initiate_signing(ceremony_id, key_id, signers, data)
            .await
            .await
    }
}
