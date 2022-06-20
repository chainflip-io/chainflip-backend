#[macro_use]
mod utils;
mod common;
mod key_store;
pub mod keygen;
pub mod signing;
mod state_runner;

#[cfg(test)]
mod tests;

pub mod ceremony_manager;

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use crate::{
    common::format_iterator,
    logging::CEREMONY_ID_KEY,
    multisig::{client::common::SigningFailureReason, KeyId},
};

use async_trait::async_trait;
use cf_traits::AuthorityCount;
use futures::Future;
use state_chain_runtime::AccountId;

use serde::{Deserialize, Serialize};

use pallet_cf_vaults::CeremonyId;

use tokio::sync::mpsc::UnboundedSender;
use utilities::threshold_from_share_count;

use keygen::KeygenData;

pub use common::{KeygenResult, KeygenResultInfo};
pub use utils::PartyIdxMapping;

#[cfg(test)]
pub use utils::ensure_unsorted;

use self::{
    ceremony_manager::{CeremonyResultReceiver, CeremonyResultSender},
    key_store::KeyStore,
    signing::frost::SigningData,
};

pub use self::common::{CeremonyFailureReason, KeygenFailureReason};

use super::{
    crypto::{CryptoScheme, ECPoint},
    MessageHash, PersistentKeyDB, Rng,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThresholdParameters {
    /// Total number of key shares (equals the total number of parties in keygen)
    pub share_count: AuthorityCount,
    /// Max number of parties that can *NOT* generate signature
    pub threshold: AuthorityCount,
}

impl ThresholdParameters {
    pub fn from_share_count(share_count: AuthorityCount) -> Self {
        ThresholdParameters {
            share_count,
            threshold: threshold_from_share_count(share_count),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MultisigData<P: ECPoint> {
    #[serde(bound = "")]
    Keygen(KeygenData<P>),
    #[serde(bound = "")]
    Signing(SigningData<P>),
}

derive_try_from_variant!(impl<P: ECPoint> for KeygenData<P>, MultisigData::Keygen, MultisigData<P>);
derive_try_from_variant!(impl<P: ECPoint> for SigningData<P>, MultisigData::Signing, MultisigData<P>);

impl<P: ECPoint> From<SigningData<P>> for MultisigData<P> {
    fn from(data: SigningData<P>) -> Self {
        MultisigData::Signing(data)
    }
}

impl<P: ECPoint> From<KeygenData<P>> for MultisigData<P> {
    fn from(data: KeygenData<P>) -> Self {
        MultisigData::Keygen(data)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultisigMessage<P: ECPoint> {
    ceremony_id: CeremonyId,
    #[serde(bound = "")]
    data: MultisigData<P>,
}

/// The public interface to the multi-signature code
#[async_trait]
pub trait MultisigClientApi<C: CryptoScheme> {
    async fn keygen(
        &self,
        ceremony_id: CeremonyId,
        participants: Vec<AccountId>,
    ) -> Result<
        C::Point,
        (
            BTreeSet<AccountId>,
            CeremonyFailureReason<KeygenFailureReason>,
        ),
    >;
    async fn sign(
        &self,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        signers: Vec<AccountId>,
        data: MessageHash,
    ) -> Result<
        C::Signature,
        (
            BTreeSet<AccountId>,
            CeremonyFailureReason<SigningFailureReason>,
        ),
    >;
}

// This is constructed by hand since mockall
// fails to parse complex generic parameters
// (and none of mockall's features were used
// anyway)
// NOTE: the fact that this mock is needed in tests but
// its methods are never called is a bit of a red flag

#[cfg(test)]
pub mod mocks {

    use super::*;
    use mockall::mock;

    use crate::multisig::crypto::CryptoScheme;

    mock! {
        pub MultisigClientApi<C: CryptoScheme + Send + Sync> {}

        #[async_trait]
        impl<C: CryptoScheme + Send + Sync> MultisigClientApi<C> for MultisigClientApi<C> {
            async fn keygen(
                &self,
                _ceremony_id: CeremonyId,
                _participants: Vec<AccountId>,
            ) -> Result<C::Point, (BTreeSet<AccountId>, CeremonyFailureReason<KeygenFailureReason>)>;
            async fn sign(
                &self,
                _ceremony_id: CeremonyId,
                _key_id: KeyId,
                _signers: Vec<AccountId>,
                _data: MessageHash,
            ) -> Result<<C as CryptoScheme>::Signature, (BTreeSet<AccountId>, CeremonyFailureReason<SigningFailureReason>)>;
        }
    }
}

type KeygenRequestSender<P> = UnboundedSender<(
    CeremonyId,
    Vec<AccountId>,
    Rng,
    CeremonyResultSender<KeygenResultInfo<P>, KeygenFailureReason>,
)>;

type SigningRequestSender<C> = UnboundedSender<(
    CeremonyId,
    Vec<AccountId>,
    MessageHash,
    KeygenResultInfo<<C as CryptoScheme>::Point>,
    Rng,
    CeremonyResultSender<<C as CryptoScheme>::Signature, SigningFailureReason>,
)>;

/// Multisig client acts as the frontend for the multisig functionality, delegating
/// the actual signing to "Ceremony Manager". It is additionally responsible for
/// persistently storing generated keys and providing them to the signing ceremonies.
pub struct MultisigClient<C: CryptoScheme> {
    my_account_id: AccountId,
    keygen_request_sender: KeygenRequestSender<C::Point>,
    signing_request_sender: SigningRequestSender<C>,
    key_store: std::sync::Mutex<KeyStore<C>>,
    logger: slog::Logger,
}

enum RequestStatus<Output, FailureReason> {
    Ready(Output),
    WaitForOneshot(CeremonyResultReceiver<Output, FailureReason>),
}

impl<C> MultisigClient<C>
where
    C: CryptoScheme,
{
    pub fn new(
        my_account_id: AccountId,
        db: Arc<Mutex<PersistentKeyDB<C>>>,
        keygen_request_sender: KeygenRequestSender<C::Point>,
        signing_request_sender: SigningRequestSender<C>,
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
        Output = Result<
            C::Point,
            (
                BTreeSet<AccountId>,
                CeremonyFailureReason<KeygenFailureReason>,
            ),
        >,
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
                RequestStatus::Ready(keygen_result_info) => Ok(keygen_result_info),
                RequestStatus::WaitForOneshot(result_receiver) => result_receiver
                    .await
                    .expect("Keygen result channel dropped before receiving a result"),
            };

            match result {
                Ok(keygen_result_info) => {
                    let key_id = KeyId(keygen_result_info.key.get_public_key_bytes());

                    self.key_store
                        .lock()
                        .unwrap()
                        .set_key(key_id, keygen_result_info.clone());

                    Ok(keygen_result_info.key.get_public_key())
                }
                Err((reported_parties, failure_reason)) => {
                    failure_reason.log(
                        &reported_parties,
                        &self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
                    );

                    Err((reported_parties, failure_reason))
                }
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
    ) -> impl '_
           + Future<
        Output = Result<
            C::Signature,
            (
                BTreeSet<AccountId>,
                CeremonyFailureReason<SigningFailureReason>,
            ),
        >,
    > {
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
            let result = match request {
                Some(RequestStatus::Ready(signature)) => Ok(signature),
                Some(RequestStatus::WaitForOneshot(result_receiver)) => result_receiver
                    .await
                    .expect("Signing result oneshot channel dropped before receiving a result"),
                None => {
                    // No key was found for the given key_id
                    Err((
                        BTreeSet::new(),
                        CeremonyFailureReason::Other(SigningFailureReason::UnknownKey),
                    ))
                }
            };

            result.map_err(|(reported_parties, failure_reason)| {
                failure_reason.log(
                    &reported_parties,
                    &self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
                );

                (reported_parties, failure_reason)
            })
        })
    }

    fn single_party_keygen(&self, rng: Rng) -> KeygenResultInfo<C::Point> {
        slog::info!(self.logger, "Performing solo keygen");

        let (_key_id, key_data) =
            keygen::generate_key_data_until_compatible(&[self.my_account_id.clone()], 30, rng);
        key_data[&self.my_account_id].clone()
    }

    fn single_party_signing(
        &self,
        data: MessageHash,
        keygen_result_info: KeygenResultInfo<C::Point>,
        mut rng: Rng,
    ) -> C::Signature {
        use crate::multisig::crypto::ECScalar;

        slog::info!(self.logger, "Performing solo signing");

        let key = &keygen_result_info.key.key_share;

        let nonce = <C::Point as ECPoint>::Scalar::random(&mut rng);

        let r = C::Point::from_scalar(&nonce);

        let sigma =
            signing::frost::generate_schnorr_response::<C>(&key.x_i, key.y, r, nonce, &data.0);

        C::build_signature(sigma, r)
    }
}

#[async_trait]
impl<C: CryptoScheme> MultisigClientApi<C> for MultisigClient<C> {
    async fn keygen(
        &self,
        ceremony_id: CeremonyId,
        participants: Vec<AccountId>,
    ) -> Result<
        C::Point,
        (
            BTreeSet<AccountId>,
            CeremonyFailureReason<KeygenFailureReason>,
        ),
    > {
        self.initiate_keygen(ceremony_id, participants).await
    }

    async fn sign(
        &self,
        ceremony_id: CeremonyId,
        key_id: KeyId,
        signers: Vec<AccountId>,
        data: MessageHash,
    ) -> Result<
        C::Signature,
        (
            BTreeSet<AccountId>,
            CeremonyFailureReason<SigningFailureReason>,
        ),
    > {
        self.initiate_signing(ceremony_id, key_id, signers, data)
            .await
    }
}
