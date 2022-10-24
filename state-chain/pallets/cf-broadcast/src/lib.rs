#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

use cf_chains::{ApiCall, ChainAbi, ChainCrypto, TransactionBuilder};
use cf_traits::{
	offence_reporting::OffenceReporter, Broadcaster, Chainflip, EpochInfo, SignerNomination,
	ThresholdSigner,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::DispatchResultWithPostInfo, sp_runtime::traits::Saturating, traits::Get, Twox64Concat,
};

use cf_traits::KeyProvider;

use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use scale_info::TypeInfo;
use sp_std::{marker::PhantomData, prelude::*};

/// A unique id for each broadcast.
pub type BroadcastId = u32;

/// The number of broadcast attempts that were made before this one.
pub type AttemptCount = u32;

/// A unique id for each broadcast attempt
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Default, Copy)]
pub struct BroadcastAttemptId {
	pub broadcast_id: BroadcastId,
	pub attempt_count: AttemptCount,
}

impl BroadcastAttemptId {
	/// Increment the attempt count for a particular BroadcastAttemptId
	pub fn next_attempt(&self) -> Self {
		Self { attempt_count: self.attempt_count + 1, ..*self }
	}
}

impl sp_std::fmt::Display for BroadcastAttemptId {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		write!(
			f,
			"BroadcastAttemptId(broadcast_id: {}, attempt_count: {})",
			self.broadcast_id, self.attempt_count
		)
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	FailedToBroadcastTransaction,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::benchmarking_value::BenchmarkValue;
	use cf_traits::{AccountRoleRegistry, KeyProvider};
	use frame_support::{ensure, pallet_prelude::*, traits::EnsureOrigin};
	use frame_system::pallet_prelude::*;

	/// Type alias for the instance's configured SignedTransaction.
	pub type SignedTransactionFor<T, I> =
		<<T as Config<I>>::TargetChain as ChainAbi>::SignedTransaction;

	/// Type alias for the instance's configured UnsignedTransaction.
	pub type UnsignedTransactionFor<T, I> =
		<<T as Config<I>>::TargetChain as ChainAbi>::UnsignedTransaction;

	/// Type alias for the instance's configured TransactionHash.
	pub type TransactionHashFor<T, I> =
		<<T as Config<I>>::TargetChain as ChainCrypto>::TransactionHash;

	/// Type alias for the instance's configured SignerId.
	pub type SignerIdFor<T, I> = <<T as Config<I>>::TargetChain as ChainAbi>::SignerCredential;

	/// Type alias for the payload hash
	pub type ThresholdSignatureFor<T, I> =
		<<T as Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature;

	/// Type alias for the instance's configured Payload.
	pub type PayloadFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::Payload;

	/// Type alias for the Amount type of a particular chain.
	pub type ChainAmountFor<T, I> =
		<<T as Config<I>>::TargetChain as cf_chains::Chain>::ChainAmount;

	/// Type alias for the instance's configured ApiCall.
	pub type ApiCallFor<T, I> = <T as Config<I>>::ApiCall;

	/// Type alias for the threshold signature data.
	pub type ThresholdSignatureInformationFor<T, I> =
		(PayloadFor<T, I>, ThresholdSignatureFor<T, I>, ApiCallFor<T, I>);

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct BroadcastAttempt<T: Config<I>, I: 'static> {
		pub broadcast_attempt_id: BroadcastAttemptId,
		pub unsigned_tx: UnsignedTransactionFor<T, I>,
	}

	/// The first step in the process - a transaction signing attempt.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct TransactionSigningAttempt<T: Config<I>, I: 'static> {
		pub broadcast_attempt: BroadcastAttempt<T, I>,
		pub nominee: T::ValidatorId,
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// The pallet dispatches calls, so it depends on the runtime's aggregated Call type.
		type Call: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::Call>;

		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;

		/// Offences that can be reported in this runtime.
		type Offence: From<PalletOffence>;

		/// A marker trait identifying the chain that we are broadcasting to.
		type TargetChain: ChainAbi;

		/// The api calls supported by this broadcaster.
		type ApiCall: ApiCall<Self::TargetChain> + BenchmarkValue;

		/// Builds the transaction according to the chain's environment settings.
		type TransactionBuilder: TransactionBuilder<Self::TargetChain, Self::ApiCall>;

		/// A threshold signer that can sign calls for this chain, and dispatch callbacks into this
		/// pallet.
		type ThresholdSigner: ThresholdSigner<
			Self::TargetChain,
			Callback = <Self as Config<I>>::Call,
		>;

		/// Signer nomination.
		type SignerNomination: SignerNomination<SignerId = Self::ValidatorId>;

		/// For reporting bad actors.
		type OffenceReporter: OffenceReporter<
			ValidatorId = Self::ValidatorId,
			Offence = Self::Offence,
		>;

		/// Ensure that only threshold signature consensus can trigger a broadcast.
		type EnsureThresholdSigned: EnsureOrigin<Self::Origin>;

		/// The timeout duration for the broadcast, measured in number of blocks.
		#[pallet::constant]
		type BroadcastTimeout: Get<BlockNumberFor<Self>>;

		/// Something that provides the current key for signing.
		type KeyProvider: KeyProvider<Self::TargetChain, KeyId = Self::KeyId>;

		/// The weights for the pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	/// A counter for incrementing the broadcast id.
	#[pallet::storage]
	pub type BroadcastIdCounter<T, I = ()> = StorageValue<_, BroadcastId, ValueQuery>;

	/// Maps a BroadcastId to a list of unresolved broadcast attempt numbers.
	#[pallet::storage]
	pub type BroadcastIdToAttemptNumbers<T, I = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, Vec<AttemptCount>, OptionQuery>;

	/// Contains a list of the authorities that have failed to sign a particular broadcast.
	#[pallet::storage]
	pub type FailedBroadcasters<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, Vec<T::ValidatorId>>;

	/// Live transaction broadcast requests.
	#[pallet::storage]
	pub type AwaitingTransactionBroadcast<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BroadcastAttemptId,
		TransactionSigningAttempt<T, I>,
		OptionQuery,
	>;

	/// Lookup table between Signature -> Broadcast.
	#[pallet::storage]
	pub type SignatureToBroadcastIdLookup<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, ThresholdSignatureFor<T, I>, BroadcastId, OptionQuery>;

	/// The list of failed broadcasts pending retry.
	#[pallet::storage]
	pub type BroadcastRetryQueue<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<BroadcastAttempt<T, I>>, ValueQuery>;

	/// A mapping from block number to a list of signing or broadcast attempts that expire at that
	/// block number.
	#[pallet::storage]
	pub type Timeouts<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, T::BlockNumber, Vec<BroadcastAttemptId>, ValueQuery>;

	/// Stores all needed information to be able to re-request the signature
	#[pallet::storage]
	pub type ThresholdSignatureData<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BroadcastId,
		(ApiCallFor<T, I>, ThresholdSignatureFor<T, I>),
		OptionQuery,
	>;

	/// Tracks how much an account is owed for paying transaction fees.
	#[pallet::storage]
	pub type TransactionFeeDeficit<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, T::AccountId, ChainAmountFor<T, I>>;

	/// A mapping of the transaction hash we expect to witness
	/// to the account id of the authority who will receive a fee
	/// refund if that transaction succeeds.
	#[pallet::storage]
	pub type TransactionHashWhitelist<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Identity, TransactionHashFor<T, I>, T::AccountId>;

	/// The signer id to send refunds to for a given account id.
	#[pallet::storage]
	pub type RefundSignerId<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, T::AccountId, SignerIdFor<T, I>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A request to a specific authority to sign a transaction.
		TransactionBroadcastRequest {
			broadcast_attempt_id: BroadcastAttemptId,
			nominee: T::ValidatorId,
			unsigned_tx: UnsignedTransactionFor<T, I>,
		},
		/// A failed broadcast attempt has been scheduled for retry. \[broadcast_attempt_id\]
		BroadcastRetryScheduled(BroadcastAttemptId),
		/// A broadcast attempt timed out.
		BroadcastAttemptTimeout { broadcast_attempt_id: BroadcastAttemptId },
		/// A broadcast has been aborted after all authorities have attempted to broadcast the
		/// transaction and failed. \[broadcast_id\]
		BroadcastAborted(BroadcastId),
		/// An account id has used a new signer id for a transaction
		/// so we want to refund to that new signer id \[account_id, signer_id\]
		RefundSignerIdUpdated(T::AccountId, SignerIdFor<T, I>),
		/// A broadcast has successfully been completed. \[broadcast_id\]
		BroadcastSuccess(BroadcastId),
		/// A broadcast's threshold signature is invalid, we will attempt to re-sign it.
		/// \[broadcast_id\]
		ThresholdSignatureInvalid(BroadcastId),
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided payload is invalid.
		InvalidPayload,
		/// The provided broadcast id is invalid.
		InvalidBroadcastId,
		/// The provided broadcast attempt id is invalid.
		InvalidBroadcastAttemptId,
		/// The transaction signer is not signer who was nominated.
		InvalidSigner,
		/// A threshold signature was expected but not available.
		ThresholdSignatureUnavailable,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// The `on_initialize` hook for this pallet handles scheduled expiries.
		///
		/// /// ## Events
		///
		/// - [BroadcastAttemptTimeout](Event::BroadcastAttemptTimeout)
		fn on_initialize(block_number: BlockNumberFor<T>) -> frame_support::weights::Weight {
			let expiries = Timeouts::<T, I>::take(block_number);
			for attempt_id in expiries.iter() {
				if let Some(attempt) = Self::take_and_clean_up_broadcast_attempt(*attempt_id) {
					Self::deposit_event(Event::<T, I>::BroadcastAttemptTimeout {
						broadcast_attempt_id: *attempt_id,
					});
					Self::start_next_broadcast_attempt(attempt);
				}
			}

			T::WeightInfo::on_initialize(expiries.len() as u32)
		}

		// We want to retry broadcasts when we have free block space.
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let next_broadcast_weight = T::WeightInfo::start_next_broadcast_attempt();

			let num_retries_that_fit = remaining_weight
				.checked_div(next_broadcast_weight)
				.expect("start_next_broadcast_attempt weight should not be 0")
				as usize;

			let mut retries = BroadcastRetryQueue::<T, I>::take();

			if retries.len() >= num_retries_that_fit {
				BroadcastRetryQueue::<T, I>::put(retries.split_off(num_retries_that_fit));
			}

			let retries_len = retries.len();

			for retry in retries {
				Self::start_next_broadcast_attempt(retry);
			}
			next_broadcast_weight * retries_len as Weight
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Called by the nominated signer when they have completed and signed the transaction, and
		/// it is therefore ready to be transmitted. The signed transaction is stored on-chain so
		/// that any node can potentially transmit it to the target chain. Emits an event that will
		/// trigger the transmission to the target chain.
		///
		/// ## Events
		///
		/// - [TransmissionRequest](Event::TransmissionRequest)
		/// - [BroadcastRetryScheduled](Event::BroadcastRetryScheduled)
		///
		/// ## Errors
		///
		/// - [InvalidBroadcastAttemptId](Error::InvalidBroadcastAttemptId)
		/// - [InvalidSigner](Error::InvalidSigner)
		#[pallet::weight(T::WeightInfo::whitelist_transaction_for_refund())]
		pub fn whitelist_transaction_for_refund(
			origin: OriginFor<T>,
			broadcast_attempt_id: BroadcastAttemptId,
			signed_tx: SignedTransactionFor<T, I>,
			signer_id: SignerIdFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let extrinsic_signer = T::AccountRoleRegistry::ensure_validator(origin)?;

			let signing_attempt = AwaitingTransactionBroadcast::<T, I>::get(broadcast_attempt_id)
				.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			ensure!(
				signing_attempt.nominee == extrinsic_signer.clone().into(),
				Error::<T, I>::InvalidSigner
			);

			if let Ok(tx_hash) = T::TargetChain::verify_signed_transaction(
				&signing_attempt.broadcast_attempt.unsigned_tx,
				&signed_tx,
				&signer_id,
			) {
				// Ensure we've initialised and whitelisted the account id to accumulate a deficit
				if !TransactionFeeDeficit::<T, I>::contains_key(&extrinsic_signer) {
					TransactionFeeDeficit::<T, I>::insert(
						&extrinsic_signer,
						ChainAmountFor::<T, I>::default(),
					);
				}

				// Whitelist the transaction hash. This ensures that we only refund txs that were
				// precommitted to by nominated signers - so we can refund accordingly.
				TransactionHashWhitelist::<T, I>::insert(tx_hash, &extrinsic_signer);

				// store the latest signer id used by an authority
				if RefundSignerId::<T, I>::get(&extrinsic_signer) != Some(signer_id.clone()) {
					RefundSignerId::<T, I>::insert(&extrinsic_signer, &signer_id);
					Self::deposit_event(Event::<T, I>::RefundSignerIdUpdated(
						extrinsic_signer,
						signer_id,
					));
				}
			} else {
				log::warn!(
					"Unable to verify tranaction signature for broadcast attempt id {}",
					broadcast_attempt_id
				);

				Self::take_and_clean_up_broadcast_attempt(broadcast_attempt_id);

				Self::schedule_retry(signing_attempt.broadcast_attempt, extrinsic_signer.into());
			}

			Ok(().into())
		}

		/// Submitted by the nominated node when they cannot sign the transaction.
		/// This triggers a retry of the signing of the transaction
		///
		/// ## Events
		///
		/// - N/A
		///
		/// ## Errors
		///
		/// - [InvalidBroadcastAttemptId](Error::InvalidBroadcastAttemptId)
		/// - [InvalidSigner](Error::InvalidSigner)
		#[pallet::weight(T::WeightInfo::transaction_signing_failure())]
		pub fn transaction_signing_failure(
			origin: OriginFor<T>,
			broadcast_attempt_id: BroadcastAttemptId,
		) -> DispatchResultWithPostInfo {
			let extrinsic_signer = T::AccountRoleRegistry::ensure_validator(origin)?.into();

			let signing_attempt = AwaitingTransactionBroadcast::<T, I>::get(broadcast_attempt_id)
				.ok_or(Error::<T, I>::InvalidBroadcastAttemptId)?;

			// Only the nominated signer can say they failed to sign
			ensure!(signing_attempt.nominee == extrinsic_signer, Error::<T, I>::InvalidSigner);

			Self::take_and_clean_up_broadcast_attempt(broadcast_attempt_id);

			Self::schedule_retry(signing_attempt.broadcast_attempt, extrinsic_signer);

			Ok(().into())
		}

		/// A callback to be used when a threshold signature request completes. Retrieves the
		/// requested signature, uses the configured [TransactionBuilder] to build the transaction
		/// and then initiates the broadcast sequence.
		///
		/// ## Events
		///
		/// - See [Call::start_broadcast].
		///
		/// ## Errors
		///
		/// - [Error::ThresholdSignatureUnavailable]
		#[pallet::weight(T::WeightInfo::on_signature_ready())]
		pub fn on_signature_ready(
			origin: OriginFor<T>,
			threshold_request_id: <T::ThresholdSigner as ThresholdSigner<T::TargetChain>>::RequestId,
			api_call: <T as Config<I>>::ApiCall,
		) -> DispatchResultWithPostInfo {
			let _ = T::EnsureThresholdSigned::ensure_origin(origin)?;

			let signature = T::ThresholdSigner::signature_result(threshold_request_id)
				.ready_or_else(|r| {
					log::error!(
						"Signature not found for threshold request {:?}. Request status: {:?}",
						threshold_request_id,
						r
					);
					Error::<T, I>::ThresholdSignatureUnavailable
				})?
				.expect("signature can not be unavailable");

			Self::start_broadcast(
				&signature,
				T::TransactionBuilder::build_transaction(&api_call.clone().signed(&signature)),
				api_call,
			);
			Ok(().into())
		}

		/// Nodes have witnessed that a signature was accepted on the target chain.
		///
		/// ## Events
		///
		/// - [BroadcastSuccess](Event::BroadcastSuccess)
		///
		/// ## Errors
		///
		/// - [InvalidPayload](Event::InvalidPayload)
		#[pallet::weight(T::WeightInfo::signature_accepted())]
		pub fn signature_accepted(
			origin: OriginFor<T>,
			signature: ThresholdSignatureFor<T, I>,
			tx_fee: ChainAmountFor<T, I>,
			tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;
			let broadcast_id = SignatureToBroadcastIdLookup::<T, I>::take(signature)
				.ok_or(Error::<T, I>::InvalidPayload)?;
			Self::clean_up_broadcast_storage(broadcast_id);
			// Add fee deficits only when we know everything else is ok
			// if this tx hash has been whitelisted, we can add the fee deficit to the authority's
			// account
			if let Some(account_id) = TransactionHashWhitelist::<T, I>::take(&tx_hash) {
				TransactionFeeDeficit::<T, I>::mutate(account_id, |fee_deficit| {
					if let Some(fee_deficit) = fee_deficit.as_mut() {
						*fee_deficit = fee_deficit.saturating_add(tx_fee);
					}
				});
			}
			Self::deposit_event(Event::<T, I>::BroadcastSuccess(broadcast_id));
			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	pub fn clean_up_broadcast_storage(broadcast_id: BroadcastId) {
		for attempt_count in
			BroadcastIdToAttemptNumbers::<T, I>::take(broadcast_id).unwrap_or_default()
		{
			AwaitingTransactionBroadcast::<T, I>::remove(BroadcastAttemptId {
				broadcast_id,
				attempt_count,
			});
		}
		FailedBroadcasters::<T, I>::remove(broadcast_id);

		if let Some((_, signature)) = ThresholdSignatureData::<T, I>::take(broadcast_id) {
			SignatureToBroadcastIdLookup::<T, I>::remove(signature);
		}
	}

	pub fn take_and_clean_up_broadcast_attempt(
		broadcast_attempt_id: BroadcastAttemptId,
	) -> Option<BroadcastAttempt<T, I>> {
		if let Some(signing_attempt) =
			AwaitingTransactionBroadcast::<T, I>::take(broadcast_attempt_id)
		{
			assert_eq!(
				signing_attempt.broadcast_attempt.broadcast_attempt_id,
				broadcast_attempt_id,
				"The broadcast attempt id of the signing attempt should match that of the broadcast attempt id of its key"
			);
			Some(signing_attempt.broadcast_attempt)
		} else {
			None
		}
	}

	/// Request a threshold signature, providing [Call::on_signature_ready] as the callback.
	pub fn threshold_sign_and_broadcast(api_call: <T as Config<I>>::ApiCall) {
		T::ThresholdSigner::request_signature_with_callback(
			api_call.threshold_signature_payload(),
			|id| Call::on_signature_ready { threshold_request_id: id, api_call }.into(),
		);
	}

	/// Begin the process of broadcasting a transaction.
	///
	/// ## Events
	///
	/// - [TransactionBroadcastRequest](Event::TransactionBroadcastRequest)
	fn start_broadcast(
		signature: &ThresholdSignatureFor<T, I>,
		unsigned_tx: UnsignedTransactionFor<T, I>,
		api_call: <T as Config<I>>::ApiCall,
	) -> BroadcastAttemptId {
		let broadcast_id = BroadcastIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		SignatureToBroadcastIdLookup::<T, I>::insert(signature, broadcast_id);
		BroadcastIdToAttemptNumbers::<T, I>::insert(broadcast_id, vec![0]);

		ThresholdSignatureData::<T, I>::insert(broadcast_id, (api_call, signature));

		let broadcast_attempt_id = BroadcastAttemptId { broadcast_id, attempt_count: 0 };
		Self::start_broadcast_attempt(BroadcastAttempt::<T, I> {
			broadcast_attempt_id,
			unsigned_tx,
		});
		broadcast_attempt_id
	}

	fn start_next_broadcast_attempt(broadcast_attempt: BroadcastAttempt<T, I>) {
		let broadcast_id = broadcast_attempt.broadcast_attempt_id.broadcast_id;
		if let Some((api_call, signature)) = ThresholdSignatureData::<T, I>::get(broadcast_id) {
			if <T::TargetChain as ChainCrypto>::verify_threshold_signature(
				&T::KeyProvider::current_key(),
				&api_call.threshold_signature_payload(),
				&signature,
			) {
				let next_broadcast_attempt_id =
					broadcast_attempt.broadcast_attempt_id.next_attempt();

				BroadcastIdToAttemptNumbers::<T, I>::append(
					broadcast_id,
					next_broadcast_attempt_id.attempt_count,
				);

				Self::start_broadcast_attempt(BroadcastAttempt::<T, I> {
					broadcast_attempt_id: next_broadcast_attempt_id,
					..broadcast_attempt
				});
			} else {
				Self::clean_up_broadcast_storage(broadcast_id);
				Self::threshold_sign_and_broadcast(api_call);
				log::info!(
					"Signature is invalid -> rescheduled threshold signature for broadcast id {}.",
					broadcast_id
				);
				Self::deposit_event(Event::<T, I>::ThresholdSignatureInvalid(broadcast_id));
			}
		} else {
			log::error!("No threshold signature data is available.");
		};
	}

	fn start_broadcast_attempt(mut broadcast_attempt: BroadcastAttempt<T, I>) {
		T::TransactionBuilder::refresh_unsigned_transaction(&mut broadcast_attempt.unsigned_tx);

		let seed = (broadcast_attempt.broadcast_attempt_id, broadcast_attempt.unsigned_tx.clone())
			.encode();
		if let Some(nominated_signer) = T::SignerNomination::nomination_with_seed(
			seed,
			&FailedBroadcasters::<T, I>::get(broadcast_attempt.broadcast_attempt_id.broadcast_id)
				.unwrap_or_default(),
		) {
			// write, or overwrite the old entry if it exists (on a retry)
			AwaitingTransactionBroadcast::<T, I>::insert(
				broadcast_attempt.broadcast_attempt_id,
				TransactionSigningAttempt {
					broadcast_attempt: BroadcastAttempt::<T, I> {
						unsigned_tx: broadcast_attempt.unsigned_tx.clone(),
						..broadcast_attempt
					},
					nominee: nominated_signer.clone(),
				},
			);

			Timeouts::<T, I>::append(
				frame_system::Pallet::<T>::block_number() + T::BroadcastTimeout::get(),
				broadcast_attempt.broadcast_attempt_id,
			);

			Self::deposit_event(Event::<T, I>::TransactionBroadcastRequest {
				broadcast_attempt_id: broadcast_attempt.broadcast_attempt_id,
				nominee: nominated_signer,
				unsigned_tx: broadcast_attempt.unsigned_tx,
			});
		} else {
			const FAILED_SIGNER_SELECTION: &str = "Failed to select signer: We should either: a) have a signer eligible for nomination b) already have aborted this broadcast when scheduling the retry";
			log::error!("{FAILED_SIGNER_SELECTION}");
			#[cfg(test)]
			panic!("{FAILED_SIGNER_SELECTION}");
		}
	}

	/// Schedule a failed attempt for retry when the next block is authored.
	/// We will abort the broadcast once all authorities have attempt to sign the transaction
	fn schedule_retry(
		failed_broadcast_attempt: BroadcastAttempt<T, I>,
		failed_signer: T::ValidatorId,
	) {
		FailedBroadcasters::<T, I>::append(
			failed_broadcast_attempt.broadcast_attempt_id.broadcast_id,
			&failed_signer,
		);
		if failed_broadcast_attempt.broadcast_attempt_id.attempt_count <
			// -1 to exclude the first node
			(T::EpochInfo::current_authority_count().saturating_sub(1))
		{
			BroadcastRetryQueue::<T, I>::append(&failed_broadcast_attempt);
			Self::deposit_event(Event::<T, I>::BroadcastRetryScheduled(
				failed_broadcast_attempt.broadcast_attempt_id,
			));
		} else {
			if let Some(failed_signers) = FailedBroadcasters::<T, I>::get(
				failed_broadcast_attempt.broadcast_attempt_id.broadcast_id,
			) {
				T::OffenceReporter::report_many(
					PalletOffence::FailedToBroadcastTransaction,
					&failed_signers,
				);
			}

			Self::clean_up_broadcast_storage(
				failed_broadcast_attempt.broadcast_attempt_id.broadcast_id,
			);

			Self::deposit_event(Event::<T, I>::BroadcastAborted(
				failed_broadcast_attempt.broadcast_attempt_id.broadcast_id,
			));
		}
	}
}

impl<T: Config<I>, I: 'static> Broadcaster<T::TargetChain> for Pallet<T, I> {
	type ApiCall = T::ApiCall;
	fn threshold_sign_and_broadcast(api_call: Self::ApiCall) {
		Self::threshold_sign_and_broadcast(api_call)
	}
}
