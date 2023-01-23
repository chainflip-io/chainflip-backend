#[macro_use]
mod utils;
mod ceremony_runner;
mod common;
pub mod key_store;
pub mod keygen;
pub mod signing;

#[cfg(test)]
mod helpers;

#[cfg(test)]
mod multisig_client_tests;

pub mod ceremony_manager;

use std::collections::BTreeSet;

use crate::{common::format_iterator, logging::CEREMONY_ID_KEY, multisig::KeyId};

use cf_primitives::{AuthorityCount, CeremonyId};
use futures::{future::BoxFuture, FutureExt};
use state_chain_runtime::AccountId;

use serde::{Deserialize, Serialize};

use tokio::sync::mpsc::UnboundedSender;
use utilities::threshold_from_share_count;

use keygen::KeygenData;

pub use common::{
	CeremonyFailureReason, KeygenFailureReason, KeygenResult, KeygenResultInfo, KeygenStageName,
	SigningFailureReason,
};
pub use utils::PartyIdxMapping;

#[cfg(test)]
pub use utils::ensure_unsorted;

#[cfg(test)]
pub use helpers::get_key_data_for_test;

#[cfg(test)]
pub use signing::{gen_signing_data_stage1, gen_signing_data_stage4};

#[cfg(test)]
pub use keygen::{gen_keygen_data_hash_comm1, gen_keygen_data_verify_hash_comm2};

#[cfg(test)]
use mockall::automock;

use self::{
	ceremony_manager::{CeremonyResultSender, KeygenCeremony, SigningCeremony},
	key_store::KeyStore,
	signing::SigningData,
};

use super::{
	crypto::{CryptoScheme, ECPoint},
	Rng,
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
		ThresholdParameters { share_count, threshold: threshold_from_share_count(share_count) }
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
/// The initiate functions of this trait when called send a ceremony request and return a future
/// that can be await'ed on for the result of that ceremony. Splitting requesting and waiting for a
/// ceremony to complete allows the requests to all be sent synchronously which is required as we
/// expect the requests to be ordered by ceremony_id
#[cfg_attr(test, automock)]
pub trait MultisigClientApi<C: CryptoScheme> {
	fn initiate_keygen(
		&self,
		ceremony_id: CeremonyId,
		participants: BTreeSet<AccountId>,
	) -> BoxFuture<'_, Result<C::AggKey, (BTreeSet<AccountId>, KeygenFailureReason)>>;

	fn initiate_signing(
		&self,
		ceremony_id: CeremonyId,
		key_id: KeyId,
		signers: BTreeSet<AccountId>,
		payload: C::SigningPayload,
	) -> BoxFuture<'_, Result<C::Signature, (BTreeSet<AccountId>, SigningFailureReason)>>;

	fn update_latest_ceremony_id(&self, ceremony_id: CeremonyId);
}

/// The ceremony details are optional to alow the updating of the ceremony id tracking
/// when we are not participating in the ceremony.
pub struct CeremonyRequest<C: CryptoScheme> {
	pub ceremony_id: CeremonyId,
	pub details: Option<CeremonyRequestDetails<C>>,
}
pub enum CeremonyRequestDetails<C>
where
	C: CryptoScheme,
{
	Keygen(KeygenRequestDetails<C>),
	Sign(SigningRequestDetails<C>),
}

pub struct KeygenRequestDetails<C: CryptoScheme> {
	pub participants: BTreeSet<AccountId>,
	pub rng: Rng,
	pub result_sender: CeremonyResultSender<KeygenCeremony<C>>,
}

pub struct SigningRequestDetails<C>
where
	C: CryptoScheme,
{
	pub participants: BTreeSet<AccountId>,
	pub payload: C::SigningPayload,
	pub keygen_result_info: KeygenResultInfo<C>,
	pub rng: Rng,
	pub result_sender: CeremonyResultSender<SigningCeremony<C>>,
}

/// Multisig client acts as the frontend for the multisig functionality, delegating
/// the actual signing to "Ceremony Manager". It is additionally responsible for
/// persistently storing generated keys and providing them to the signing ceremonies.
pub struct MultisigClient<C: CryptoScheme> {
	my_account_id: AccountId,
	ceremony_request_sender: UnboundedSender<CeremonyRequest<C>>,
	key_store: std::sync::Mutex<KeyStore<C>>,
	logger: slog::Logger,
}

impl<C> MultisigClient<C>
where
	C: CryptoScheme,
{
	pub fn new(
		my_account_id: AccountId,
		key_store: KeyStore<C>,
		ceremony_request_sender: UnboundedSender<CeremonyRequest<C>>,
		logger: &slog::Logger,
	) -> Self {
		MultisigClient {
			my_account_id,
			key_store: std::sync::Mutex::new(key_store),
			ceremony_request_sender,
			logger: logger.clone(),
		}
	}
}

impl<C: CryptoScheme> MultisigClientApi<C> for MultisigClient<C> {
	fn initiate_keygen(
		&self,
		ceremony_id: CeremonyId,
		participants: BTreeSet<AccountId>,
	) -> BoxFuture<'_, Result<C::AggKey, (BTreeSet<AccountId>, KeygenFailureReason)>> {
		assert!(participants.contains(&self.my_account_id));

		slog::info!(
			self.logger,
			"Received a keygen request";
			"participants" => format_iterator(&participants).to_string(),
			CEREMONY_ID_KEY => ceremony_id
		);

		use rand_legacy::FromEntropy;
		let rng = Rng::from_entropy();

		let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
		self.ceremony_request_sender
			.send(CeremonyRequest {
				ceremony_id,
				details: Some(CeremonyRequestDetails::Keygen(KeygenRequestDetails {
					participants,
					rng,
					result_sender,
				})),
			})
			.ok()
			.expect("Should send keygen request");

		async move {
			// Wait for the request to return a result, then log and return the result
			let result = result_receiver
				.await
				.expect("Keygen result channel dropped before receiving a result");

			match result {
				Ok(keygen_result_info) => {
					let key_id = KeyId(keygen_result_info.key.get_public_key_bytes());

					self.key_store.lock().unwrap().set_key(key_id, keygen_result_info.clone());

					Ok(C::agg_key(&keygen_result_info.key.get_public_key()))
				},
				Err((reported_parties, failure_reason)) => {
					failure_reason.log(
						&reported_parties,
						&self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
					);

					Err((reported_parties, failure_reason))
				},
			}
		}
		.boxed()
	}

	fn initiate_signing(
		&self,
		ceremony_id: CeremonyId,
		key_id: KeyId,
		signers: BTreeSet<AccountId>,
		payload: C::SigningPayload,
	) -> BoxFuture<'_, Result<C::Signature, (BTreeSet<AccountId>, SigningFailureReason)>> {
		assert!(signers.contains(&self.my_account_id));

		slog::debug!(
			self.logger,
			"Received a request to sign";
			"payload" => payload.to_string(),
			"signers" => format_iterator(&signers).to_string(),
			CEREMONY_ID_KEY => ceremony_id
		);

		use rand_legacy::FromEntropy;
		let rng = Rng::from_entropy();

		// Find the correct key and send the request to sign with that key
		let request =
			self.key_store
				.lock()
				.unwrap()
				.get_key(&key_id)
				.cloned()
				.map(|keygen_result_info| {
					let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
					self.ceremony_request_sender
						.send(CeremonyRequest {
							ceremony_id,
							details: Some(CeremonyRequestDetails::Sign(SigningRequestDetails {
								participants: signers,
								payload,
								keygen_result_info,
								rng,
								result_sender,
							})),
						})
						.ok()
						.expect("Should send signing request");
					result_receiver
				});

		async move {
			// Wait for the request to return a result, then log and return the result
			if let Some(result_receiver) = request {
				let result = result_receiver
					.await
					.expect("Signing result oneshot channel dropped before receiving a result");

				result.map_err(|(reported_parties, failure_reason)| {
					failure_reason.log(
						&reported_parties,
						&self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
					);

					(reported_parties, failure_reason)
				})
			} else {
				// No key was found for the given key_id
				let reported_parties = BTreeSet::new();
				let failure_reason = SigningFailureReason::UnknownKey;
				failure_reason.log(
					&reported_parties,
					&self.logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
				);
				Err((reported_parties, failure_reason))
			}
		}
		.boxed()
	}

	fn update_latest_ceremony_id(&self, ceremony_id: CeremonyId) {
		self.ceremony_request_sender
			.send(CeremonyRequest { ceremony_id, details: None })
			.ok()
			.expect("Should send ceremony request");
	}
}
