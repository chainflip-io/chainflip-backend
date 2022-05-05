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

use std::{collections::BTreeSet, sync::Arc};

use crate::{
    common::format_iterator,
    eth::utils::pubkey_to_eth_addr,
    logging::CEREMONY_ID_KEY,
    multisig::{
        client::{
            common::{SigningFailureReason, SigningRequestIgnoredReason},
            utils::PartyIdxMapping,
        },
        crypto::Rng,
        KeyDB, KeyId,
    },
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
    common::CeremonyFailureReason,
    signing::frost::SigningData,
};

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
            k_times_g_address: pubkey_to_eth_addr(cfe_sig.r),
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
    ) -> Result<secp256k1::PublicKey, (BTreeSet<AccountId>, CeremonyFailureReason)>;
    async fn sign(
        &self,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        signers: Vec<AccountId>,
        data: MessageHash,
    ) -> Result<SchnorrSignature, (BTreeSet<AccountId>, CeremonyFailureReason)>;
}

type KeygenRequestSender = UnboundedSender<(
    CeremonyId,
    Vec<AccountId>,
    Rng,
    CeremonyResultSender<KeygenResultInfo>,
)>;

type SigningRequestSender = UnboundedSender<(
    CeremonyId,
    Vec<AccountId>,
    MessageHash,
    KeygenResultInfo,
    Rng,
    CeremonyResultSender<SchnorrSignature>,
)>;

/// Multisig client acts as the frontend for the multisig functionality, delegating
/// the actual signing to "Ceremony Manager". It is additionally responsible for
/// persistently storing generated keys and providing them to the signing ceremonies.
pub struct MultisigClient<KeyDatabase>
where
    KeyDatabase: KeyDB,
{
    my_account_id: AccountId,
    keygen_request_sender: KeygenRequestSender,
    signing_request_sender: SigningRequestSender,
    key_store: std::sync::Mutex<KeyStore<KeyDatabase>>,
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
        keygen_request_sender: KeygenRequestSender,
        signing_request_sender: SigningRequestSender,
        logger: &slog::Logger,
    ) -> Self {
        MultisigClient {
            my_account_id,
            key_store: std::sync::Mutex::new(KeyStore::new(db)),
            keygen_request_sender,
            signing_request_sender,
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
    ) -> impl '_
           + Future<
        Output = Result<secp256k1::PublicKey, (BTreeSet<AccountId>, CeremonyFailureReason)>,
    > {
        assert!(participants.contains(&self.my_account_id));

        slog::info!(
            self.logger,
            "Received a keygen request";
            "participants" => format!("{}",format_iterator(&participants)),
            CEREMONY_ID_KEY => ceremony_id
        );

        use rand_legacy::FromEntropy;
        let rng = Rng::from_entropy();

        let request_status = if participants.len() == 1 {
            RequestStatus::Ready(self.single_party_keygen(rng))
        } else {
            let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
            self.keygen_request_sender
                .send((ceremony_id, participants, rng, result_sender))
                .ok()
                .unwrap();
            RequestStatus::WaitForOneshot(result_receiver)
        };

        async move {
            let result = match request_status {
                RequestStatus::Ready(keygen_result_info) => Some(Ok(keygen_result_info)),
                RequestStatus::WaitForOneshot(result_receiver) => result_receiver.await.ok(),
            };

            match result {
                Some(Ok(keygen_result_info)) => {
                    let key_id = KeyId(keygen_result_info.key.get_public_key_bytes());

                    self.key_store
                        .lock()
                        .unwrap()
                        .set_key(key_id, keygen_result_info.clone());

                    Ok(keygen_result_info.key.get_public_key().get_element())
                }
                Some(Err(error)) => Err(error),
                None => panic!("Keygen result oneshot channel dropped before receiving a result"),
            }
        }
    }

    // Similarly to initiate_keygen this function is structured to simplify the writing of tests (i.e. should_delay_rts_until_key_is_ready).
    // Once the async function returns it has sent the request to the CeremonyManager/Backend
    // and outputs a second future that will complete only once the CeremonyManager has finished
    // the ceremony. This allows tests to split making the request and waiting for the result.
    pub fn initiate_signing(
        &self,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        signers: Vec<AccountId>,
        data: MessageHash,
    ) -> impl '_ + Future<Output = Result<SchnorrSignature, (BTreeSet<AccountId>, CeremonyFailureReason)>>
    {
        assert!(signers.contains(&self.my_account_id));

        slog::debug!(
            self.logger,
            "Received a request to sign";
            "message_hash" => format!("{}",data),
            "signers" => format!("{}",format_iterator(&signers)),
            CEREMONY_ID_KEY => ceremony_id
        );

        use rand_legacy::FromEntropy;
        let rng = Rng::from_entropy();

        let request = self
            .key_store
            .lock()
            .unwrap()
            .get_key(&key_id)
            .cloned()
            .map(|keygen_result_info| {
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
            });

        Box::pin(async move {
            match request {
                Some(RequestStatus::Ready(signature)) => Ok(signature),
                Some(RequestStatus::WaitForOneshot(result_receiver)) => {
                    if let Ok(result) = result_receiver.await {
                        result
                    } else {
                        panic!("Signing result oneshot channel dropped before receiving a result");
                    }
                }
                None => Err((
                    BTreeSet::new(),
                    CeremonyFailureReason::SigningFailure(SigningFailureReason::RequestIgnored(
                        SigningRequestIgnoredReason::UnknownKey,
                    )),
                )),
            }
        })
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
}

#[async_trait]
impl<KeyDatabase: KeyDB + Send + Sync> MultisigClientApi for MultisigClient<KeyDatabase> {
    async fn keygen(
        &self,
        ceremony_id: CeremonyId,
        participants: Vec<AccountId>,
    ) -> Result<secp256k1::PublicKey, (BTreeSet<AccountId>, CeremonyFailureReason)> {
        self.initiate_keygen(ceremony_id, participants).await
    }

    async fn sign(
        &self,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        signers: Vec<AccountId>,
        data: MessageHash,
    ) -> Result<SchnorrSignature, (BTreeSet<AccountId>, CeremonyFailureReason)> {
        self.initiate_signing(ceremony_id, key_id, signers, data)
            .await
    }
}
