#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
pub mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use cf_chains::{Chain, ChainCrypto};
use cf_traits::{
	offence_reporting::OffenceReporter, AsyncResult, CeremonyIdProvider, Chainflip, EpochInfo,
	KeyProvider, SignerNomination,
};
use frame_support::{
	ensure,
	traits::{EnsureOrigin, Get},
};
use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
pub use pallet::*;
use sp_runtime::{
	traits::{BlockNumberProvider, Saturating},
	RuntimeDebug,
};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	iter::FromIterator,
	marker::PhantomData,
	prelude::*,
};
use weights::WeightInfo;

/// The type of the Id given to signing ceremonies.
pub type CeremonyId = u64;

/// The type of the Id given to threshold signature requests. Note a single request may
/// result in multiple ceremonies, but only one ceremony should succeed.
pub type RequestId = u32;

/// The type used for counting signing attempts.
type AttemptCount = u32;

type SignatureFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature;
type PayloadFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::Payload;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	ParticipateSigningFailed,
}

const THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS_DEFAULT: u32 = 10;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::AsyncResult;
	use frame_support::{
		dispatch::{DispatchResultWithPostInfo, UnfilteredDispatchable},
		pallet_prelude::*,
		unsigned::{TransactionValidity, ValidateUnsigned},
		Twox64Concat,
	};
	use frame_system::{ensure_none, pallet_prelude::*};

	/// Metadata for a pending threshold signature ceremony.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct CeremonyContext<T: Config<I>, I: 'static> {
		/// The respondents that have yet to reply.
		pub remaining_respondents: BTreeSet<T::ValidatorId>,
		/// The number of blame votes (accusations) each authority has received.
		pub blame_counts: BTreeMap<T::ValidatorId, u32>,
		/// The total number of signing participants (ie. the threshold set size).
		pub participant_count: u32,
		/// Phantom data member.
		pub _phantom: PhantomData<I>,
	}

	impl<T: Config<I>, I: 'static> CeremonyContext<T, I> {
		/// Based on the reported blame_counts, decide which nodes should be reported for failure.
		///
		/// We assume that at least 2/3 of participants need to blame a node for it to be reliable.
		///
		/// We also assume any parties that have not responded should be reported.
		///
		/// The absolute maximum number of nodes we can punish here is 1/2 of the participants,
		/// since any more than that would leave us with insufficient nodes to reach the signature
		/// threshold.
		///
		/// **TODO:** See if there is a better / more scientific basis for the abovementioned
		/// assumptions and thresholds. Also consider emergency rotations - we may not want this to
		/// immediately trigger an ER. For instance, imagine a failed tx: if we retry we most likely
		/// want to retry with the current authority set - however if we rotate, then the next
		/// authority set will no longer be in control of the vault.
		/// Similarly for vault rotations - we can't abort a rotation at the setAggKey stage: we
		/// have to keep retrying with the current set of authorities.
		pub fn offenders(&self) -> Vec<T::ValidatorId> {
			// A threshold for number of blame 'accusations' that are required for someone to be
			// punished.
			let blame_threshold = self.participant_count * 2 / 3;
			// The maximum number of offenders we are willing to report without risking the liveness
			// of the network.
			let liveness_threshold = self.participant_count / 2;

			let mut to_report = self
				.blame_counts
				.iter()
				.filter(|(_, count)| **count > blame_threshold)
				.map(|(id, _)| id)
				.cloned()
				.collect::<BTreeSet<_>>();

			for id in self.remaining_respondents.iter() {
				to_report.insert(id.clone());
			}

			let to_report = to_report.into_iter().collect::<Vec<_>>();

			if to_report.len() <= liveness_threshold as usize {
				to_report
			} else {
				Vec::new()
			}
		}
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// The top-level offence type must support this pallet's offence type.
		type Offence: From<PalletOffence>;

		/// The top-level origin type of the runtime.
		type RuntimeOrigin: From<Origin<Self, I>>
			+ IsType<<Self as frame_system::Config>::Origin>
			+ Into<Result<Origin<Self, I>, Self::RuntimeOrigin>>;

		/// The calls that this pallet can dispatch after generating a signature.
		type ThresholdCallable: Member
			+ Parameter
			+ UnfilteredDispatchable<Origin = Self::RuntimeOrigin>;

		/// A marker trait identifying the chain that we are signing for.
		type TargetChain: Chain + ChainCrypto;

		/// Signer nomination.
		type SignerNomination: SignerNomination<SignerId = Self::ValidatorId>;

		/// Something that provides the current key for signing.
		type KeyProvider: KeyProvider<Self::TargetChain, KeyId = Self::KeyId>;

		/// For reporting bad actors.
		type OffenceReporter: OffenceReporter<
			ValidatorId = <Self as Chainflip>::ValidatorId,
			Offence = Self::Offence,
		>;

		/// CeremonyId source.
		type CeremonyIdProvider: CeremonyIdProvider<CeremonyId = CeremonyId>;

		/// In case not enough live nodes were available to begin a threshold signing ceremony: The
		/// number of blocks to wait before retrying with a new set.
		#[pallet::constant]
		type CeremonyRetryDelay: Get<Self::BlockNumber>;

		/// Pallet weights
		type Weights: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	/// A counter to generate fresh request ids.
	#[pallet::storage]
	#[pallet::getter(fn threshold_signature_request_id_counter)]
	pub type ThresholdSignatureRequestIdCounter<T, I = ()> = StorageValue<_, RequestId, ValueQuery>;

	/// Stores the context required for processing live ceremonies.
	#[pallet::storage]
	#[pallet::getter(fn pending_ceremonies)]
	pub type PendingCeremonies<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, CeremonyId, CeremonyContext<T, I>>;

	/// A mapping from ceremony_id to its respective request. There can be several ceremonies for a
	/// single request
	///
	/// Technically a payload is associated with an entire request, however since it's accessed on
	/// every unsigned transaction validation, it makes sense to optimise this by indexing against
	/// ceremony id.
	#[pallet::storage]
	#[pallet::getter(fn open_requests)]
	pub type OpenRequests<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, CeremonyId, (RequestId, AttemptCount, PayloadFor<T, I>)>;

	/// A mapping from request id to to the live ceremony id for that request and what
	/// and how many times we have attempted to sign this request.
	#[pallet::storage]
	#[pallet::getter(fn live_ceremonies)]
	pub type LiveCeremonies<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, RequestId, (CeremonyId, AttemptCount)>;

	/// Callbacks to be dispatched when a request is fulfilled.
	#[pallet::storage]
	#[pallet::getter(fn request_callback)]
	pub type RequestCallback<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, RequestId, <T as Config<I>>::ThresholdCallable>;

	/// Generated signatures.
	#[pallet::storage]
	#[pallet::getter(fn signatures)]
	pub type Signatures<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, RequestId, AsyncResult<SignatureFor<T, I>>, ValueQuery>;

	/// A map containing lists of ceremony ids that should be retried at the block stored in the
	/// key.
	#[pallet::storage]
	#[pallet::getter(fn retry_queues)]
	pub type RetryQueues<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, Vec<CeremonyId>, ValueQuery>;

	/// Maximum duration of a threshold signing ceremony before it is timed out and retried
	#[pallet::storage]
	pub type ThresholdSignatureResponseTimeout<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		pub threshold_signature_response_timeout: BlockNumberFor<T>,
		pub _instance: PhantomData<I>,
	}

	#[cfg(feature = "std")]
	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			Self {
				threshold_signature_response_timeout:
					THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS_DEFAULT.into(),
				_instance: PhantomData,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> GenesisBuild<T, I> for GenesisConfig<T, I> {
		fn build(&self) {
			ThresholdSignatureResponseTimeout::<T, I>::put(
				self.threshold_signature_response_timeout,
			);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// \[ceremony_id, key_id, signatories, payload\]
		ThresholdSignatureRequest(CeremonyId, T::KeyId, Vec<T::ValidatorId>, PayloadFor<T, I>),
		/// \[ceremony_id, key_id, offenders\]
		ThresholdSignatureFailed(CeremonyId, T::KeyId, Vec<T::ValidatorId>),
		/// The threshold signature posted back to the chain was verified.
		ThresholdSignatureSuccess(CeremonyId),
		/// We have had a signature success and we have dispatched the associated callback
		/// \[ceremony_id, result\]
		ThresholdDispatchComplete(CeremonyId, DispatchResult),
		/// \[ceremony_id\]
		RetryRequested(CeremonyId),
		/// The threshold signature has already succeeded or failed, so this retry is no longer
		/// valid \[ceremony_id\]
		StaleRetryDiscarded(CeremonyId),
		/// \[ceremony_id, reporter_id\]
		FailureReportProcessed(CeremonyId, T::ValidatorId),
		/// Not enough signers were available to reach threshold. Ceremony will be retried.
		/// \[ceremony_id\]
		SignersUnavailable(CeremonyId),
		/// The threshold signature response timeout has been updated
		ThresholdSignatureResponseTimeoutUpdated { new_timeout: BlockNumberFor<T> },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided ceremony id is invalid.
		InvalidCeremonyId,
		/// The provided threshold signature is invalid.
		InvalidThresholdSignature,
		/// The reporting party is not one of the signatories for this ceremony, or has already
		/// responded.
		InvalidRespondent,
		/// The request Id is stale or not yet valid.
		InvalidRequestId,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> frame_support::weights::Weight {
			let mut num_retries = 0;

			// Process pending retries.
			for ceremony_id in RetryQueues::<T, I>::take(current_block) {
				if let Some(failed_ceremony_context) = PendingCeremonies::<T, I>::take(ceremony_id)
				{
					num_retries += 1;
					T::OffenceReporter::report_many(
						PalletOffence::ParticipateSigningFailed,
						failed_ceremony_context.offenders().as_slice(),
					);

					// Clean up old ceremony and start a new one.
					if let Some((request_id, attempt, payload)) =
						OpenRequests::<T, I>::take(ceremony_id)
					{
						Self::new_ceremony_attempt(request_id, payload, attempt.wrapping_add(1));

						Self::deposit_event(Event::<T, I>::RetryRequested(ceremony_id));
					} else {
						log::error!("Retry failed: No ceremony such ceremony: {}.", ceremony_id);
					}
				} else {
					Self::deposit_event(Event::<T, I>::StaleRetryDiscarded(ceremony_id))
				}
			}

			// TODO: replace this with benchmark results.
			num_retries as u64 *
				frame_support::weights::RuntimeDbWeight::default().reads_writes(3, 3)
		}
	}

	#[pallet::origin]
	#[derive(PartialEq, Eq, Copy, Clone, RuntimeDebug, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct Origin<T: Config<I>, I: 'static = ()>(pub(super) PhantomData<(T, I)>);

	#[pallet::validate_unsigned]
	impl<T: Config<I>, I: 'static> ValidateUnsigned for Pallet<T, I> {
		type Call = Call<T, I>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::<T, I>::signature_success { ceremony_id, signature } = call {
				let (_, _, payload) =
					OpenRequests::<T, I>::get(ceremony_id).ok_or(InvalidTransaction::Stale)?;

				if <T::TargetChain as ChainCrypto>::verify_threshold_signature(
					&T::KeyProvider::current_key(),
					&payload,
					signature,
				) {
					ValidTransaction::with_tag_prefix(Self::name())
						// We only expect one success per ceremony.
						.and_provides(ceremony_id)
						.build()
				} else {
					InvalidTransaction::BadProof.into()
				}
			} else {
				InvalidTransaction::Call.into()
			}
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// A threshold signature ceremony has succeeded.
		///
		/// This is an **Unsigned** Extrinsic, meaning validation is performed in the
		/// [ValidateUnsigned] implementation for this pallet. This means that this call can only be
		/// triggered if the associated signature is valid, and therfore we don't need to check it
		/// again inside the call.
		///
		/// ## Events
		///
		/// - [ThresholdSignatureSuccess](Event::ThresholdSignatureSuccess)
		/// - [ThresholdDispatchComplete](Event::ThresholdDispatchComplete)
		///
		/// ## Errors
		///
		/// - [InvalidCeremonyId](sp_runtime::traits::InvalidCeremonyId)
		/// - [BadOrigin](sp_runtime::traits::BadOrigin)
		#[pallet::weight(T::Weights::signature_success())]
		pub fn signature_success(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			signature: SignatureFor<T, I>,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			// The request succeeded, remove it.
			let (request_id, attempts, _) =
				OpenRequests::<T, I>::take(ceremony_id).ok_or_else(|| {
					// We check the ceremony_id in the ValidateUnsigned transaction, so if this
					// happens, there is something seriously wrong with our assumptions.
					log::error!("Invalid ceremony_id received {}.", ceremony_id);
					Error::<T, I>::InvalidCeremonyId
				})?;

			PendingCeremonies::<T, I>::remove(ceremony_id);
			LiveCeremonies::<T, I>::remove(request_id);

			// Report the success once we know the CeremonyId is valid
			Self::deposit_event(Event::<T, I>::ThresholdSignatureSuccess(ceremony_id));

			log::debug!(
				"Threshold signature request {} suceeded at ceremony {} after {} attempts.",
				request_id,
				ceremony_id,
				attempts
			);

			Signatures::<T, I>::insert(request_id, AsyncResult::Ready(signature));

			// Dispatch the callback if one has been registered.
			if let Some(call) = RequestCallback::<T, I>::take(request_id) {
				let dispatch_result =
					call.dispatch_bypass_filter(Origin(Default::default()).into());

				Self::deposit_event(Event::<T, I>::ThresholdDispatchComplete(
					ceremony_id,
					dispatch_result.map(|_| ()).map_err(|e| {
						log::error!("Threshold dispatch failed for ceremony {}.", ceremony_id);
						e.error
					}),
				));
			}

			Ok(().into())
		}

		/// Report that a threshold signature ceremony has failed and incriminate the guilty
		/// participants.
		///
		/// The `offenders` argument takes a [BTreeSet]
		///
		/// ## Events
		///
		/// - [FailureReportProcessed](Event::FailureReportProcessed)
		///
		/// ## Errors
		///
		/// - [InvalidCeremonyId](Error::InvalidCeremonyId)
		/// - [InvalidRespondent](Error::InvalidRespondent)
		#[pallet::weight(T::Weights::report_signature_failed(offenders.len() as u32))]
		pub fn report_signature_failed(
			origin: OriginFor<T>,
			id: CeremonyId,
			offenders: BTreeSet<<T as Chainflip>::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			let reporter_id = ensure_signed(origin)?.into();

			PendingCeremonies::<T, I>::try_mutate(id, |maybe_context| {
				maybe_context
					.as_mut()
					.ok_or(Error::<T, I>::InvalidCeremonyId)
					.and_then(|context| {
						if !context.remaining_respondents.remove(&reporter_id) {
							return Err(Error::<T, I>::InvalidRespondent)
						}

						for id in offenders {
							(*context.blame_counts.entry(id).or_default()) += 1;
						}

						if context.remaining_respondents.is_empty() {
							// No more respondents waiting: we can retry
							Self::schedule_retry(id, T::CeremonyRetryDelay::get());
						}

						Ok(())
					})
			})?;

			Self::deposit_event(Event::<T, I>::FailureReportProcessed(id, reporter_id));

			Ok(().into())
		}

		#[pallet::weight(T::Weights::set_threshold_signature_timeout())]
		pub fn set_threshold_signature_timeout(
			origin: OriginFor<T>,
			new_timeout: BlockNumberFor<T>,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			if new_timeout != ThresholdSignatureResponseTimeout::<T, I>::get() {
				ThresholdSignatureResponseTimeout::<T, I>::put(new_timeout);
				Self::deposit_event(Event::<T, I>::ThresholdSignatureResponseTimeoutUpdated {
					new_timeout,
				});
			}

			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Initiate a new signature request, returning the request id.
	fn request_signature(payload: PayloadFor<T, I>) -> (RequestId, CeremonyId) {
		// Get a new request id.
		let request_id = ThresholdSignatureRequestIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		// Start a ceremony.
		let ceremony_id = Self::new_ceremony_attempt(request_id, payload, 0);

		// Schedule an initial retry.
		Self::schedule_retry(ceremony_id, ThresholdSignatureResponseTimeout::<T, I>::get());

		Signatures::<T, I>::insert(request_id, AsyncResult::Pending);

		(request_id, ceremony_id)
	}

	/// Initiates a new ceremony request.
	fn new_ceremony_attempt(
		request_id: RequestId,
		payload: PayloadFor<T, I>,
		attempt: AttemptCount,
	) -> CeremonyId {
		let ceremony_id = T::CeremonyIdProvider::next_ceremony_id();
		OpenRequests::<T, I>::insert(ceremony_id, (request_id, attempt, payload.clone()));
		LiveCeremonies::<T, I>::insert(request_id, (ceremony_id, attempt));

		// Get the current signing key.
		let key_id = T::KeyProvider::current_key_id();

		// Select nominees for threshold signature.
		if let Some(nominees) = T::SignerNomination::threshold_nomination_with_seed(
			(ceremony_id, attempt),
			T::EpochInfo::epoch_index(),
		) {
			// Store the context.
			PendingCeremonies::<T, I>::insert(
				ceremony_id,
				CeremonyContext {
					remaining_respondents: BTreeSet::from_iter(nominees.clone()),
					blame_counts: Default::default(),
					participant_count: nominees.len() as u32,
					_phantom: Default::default(),
				},
			);

			// Emit the request to the CFE.
			Self::deposit_event(Event::<T, I>::ThresholdSignatureRequest(
				ceremony_id,
				key_id,
				nominees,
				payload,
			));
		} else {
			// Store the context, schedule a retry for the next block.
			PendingCeremonies::<T, I>::insert(
				ceremony_id,
				CeremonyContext {
					remaining_respondents: Default::default(),
					blame_counts: Default::default(),
					participant_count: 0,
					_phantom: Default::default(),
				},
			);

			Self::deposit_event(Event::<T, I>::SignersUnavailable(ceremony_id));
			Self::schedule_retry(ceremony_id, T::CeremonyRetryDelay::get());
		}

		ceremony_id
	}

	fn schedule_retry(id: CeremonyId, retry_delay: BlockNumberFor<T>) {
		RetryQueues::<T, I>::append(
			frame_system::Pallet::<T>::current_block_number().saturating_add(retry_delay),
			id,
		);
	}
}

pub struct EnsureThresholdSigned<T: Config<I>, I: 'static = ()>(PhantomData<(T, I)>);

impl<T, I> EnsureOrigin<T::RuntimeOrigin> for EnsureThresholdSigned<T, I>
where
	T: Config<I>,
	I: 'static,
{
	type Success = ();

	fn try_origin(o: T::RuntimeOrigin) -> Result<Self::Success, T::RuntimeOrigin> {
		let res: Result<Origin<T, I>, T::RuntimeOrigin> = o.into();
		res.map(|_| ())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn successful_origin() -> T::RuntimeOrigin {
		Origin::<T, I>(Default::default()).into()
	}
}

impl<T, I: 'static> cf_traits::ThresholdSigner<T::TargetChain> for Pallet<T, I>
where
	T: Config<I>,
{
	type RequestId = RequestId;
	type Error = Error<T, I>;
	type Callback = <T as Config<I>>::ThresholdCallable;

	fn request_signature(payload: PayloadFor<T, I>) -> Self::RequestId {
		Self::request_signature(payload).0
	}

	fn register_callback(
		request_id: Self::RequestId,
		on_signature_ready: Self::Callback,
	) -> Result<(), Self::Error> {
		ensure!(LiveCeremonies::<T, I>::contains_key(request_id), Error::<T, I>::InvalidRequestId);
		RequestCallback::<T, I>::insert(request_id, on_signature_ready);
		Ok(())
	}

	fn signature_result(
		request_id: Self::RequestId,
	) -> cf_traits::AsyncResult<<T::TargetChain as ChainCrypto>::ThresholdSignature> {
		Signatures::<T, I>::take(request_id)
	}
}
