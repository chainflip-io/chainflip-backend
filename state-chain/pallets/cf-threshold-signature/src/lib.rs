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

use cf_chains::ChainCrypto;
use cf_primitives::{AuthorityCount, CeremonyId};
use cf_traits::{
	offence_reporting::OffenceReporter, AsyncResult, CeremonyIdProvider, Chainflip, EpochInfo,
	EpochKey, KeyProvider, KeyState, ThresholdSignerNomination,
};

use frame_support::{
	dispatch::UnfilteredDispatchable,
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
	marker::PhantomData,
	prelude::*,
};
use weights::WeightInfo;

/// The type of the Id given to threshold signature requests. Note a single request may
/// result in multiple ceremonies, but only one ceremony should succeed.
pub type RequestId = u32;

/// The type used for counting signing attempts.
type AttemptCount = AuthorityCount;

type SignatureFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature;
type PayloadFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::Payload;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	ParticipateSigningFailed,
}

#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum RequestType<KeyId, Participants> {
	/// Will use the current key and current authority set.
	/// This signing request will be retried until success.
	Standard,
	/// Uses the recently generated key and the participants used to generate that key.
	/// This signing request will only be attemped once, as failing this ought to result
	/// in another Keygen ceremony.
	KeygenVerification { key_id: KeyId, participants: Participants },
}

/// The type of a threshold *Ceremony* i.e. after a request has been emitted, it is then a ceremony.
#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum ThresholdCeremonyType {
	Standard,
	KeygenVerification,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{AccountRoleRegistry, AsyncResult, ThresholdSignerNomination};
	use frame_support::{
		dispatch::DispatchResultWithPostInfo,
		pallet_prelude::{InvalidTransaction, *},
		unsigned::{TransactionValidity, ValidateUnsigned},
		Twox64Concat,
	};
	use frame_system::ensure_none;
	/// Context for tracking the progress of a threshold signature ceremony.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct CeremonyContext<T: Config<I>, I: 'static> {
		pub request_context: RequestContext<T, I>,
		/// The respondents that have yet to reply.
		pub remaining_respondents: BTreeSet<T::ValidatorId>,
		/// The number of blame votes (accusations) each authority has received.
		pub blame_counts: BTreeMap<T::ValidatorId, AuthorityCount>,
		/// The total number of signing participants (ie. the threshold set size).
		pub participant_count: AuthorityCount,
		/// The key id being used for verification of this ceremony.
		pub key_id: T::KeyId,
		/// Determines how/if we deal with ceremony failure.
		pub threshold_ceremony_type: ThresholdCeremonyType,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct RequestContext<T: Config<I>, I: 'static> {
		pub request_id: RequestId,
		/// The number of ceremonies attempted so far, excluding the current one.
		/// Currently we do not limit the number of retry attempts for ceremony type Standard.
		/// Most transactions are critical, so we should retry until success.
		pub attempt_count: AttemptCount,
		/// The payload to be signed over.
		pub payload: PayloadFor<T, I>,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct RequestInstruction<T: Config<I>, I: 'static> {
		pub request_context: RequestContext<T, I>,
		pub request_type: RequestType<<T as Chainflip>::KeyId, BTreeSet<T::ValidatorId>>,
	}

	impl<T: Config<I>, I: 'static> RequestInstruction<T, I> {
		pub fn new(
			request_id: RequestId,
			attempt_count: AttemptCount,
			payload: PayloadFor<T, I>,
			request_type: RequestType<<T as Chainflip>::KeyId, BTreeSet<T::ValidatorId>>,
		) -> Self {
			Self {
				request_context: RequestContext { request_id, attempt_count, payload },
				request_type,
			}
		}
	}

	pub type SignatureResultFor<T, I> =
		Result<SignatureFor<T, I>, Vec<<T as Chainflip>::ValidatorId>>;

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
			let blame_threshold = self.participant_count.saturating_mul(2) / 3;
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
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<<Self as frame_system::Config>::RuntimeOrigin>;

		/// The top-level offence type must support this pallet's offence type.
		type Offence: From<PalletOffence>;

		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;

		/// The top-level origin type of the runtime.
		type RuntimeOrigin: From<Origin<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeOrigin>
			+ Into<Result<Origin<Self, I>, <Self as Config<I>>::RuntimeOrigin>>;

		/// The calls that this pallet can dispatch after generating a signature.
		type ThresholdCallable: Member
			+ Parameter
			+ UnfilteredDispatchable<RuntimeOrigin = <Self as Config<I>>::RuntimeOrigin>;

		/// A marker trait identifying the chain that we are signing for.
		type TargetChain: ChainCrypto<KeyId = <Self as Chainflip>::KeyId>;

		/// Signer nomination.
		type ThresholdSignerNomination: ThresholdSignerNomination<SignerId = Self::ValidatorId>;

		/// Something that provides the current key for signing.
		type KeyProvider: KeyProvider<Self::TargetChain>;

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

	// These are requests we need to kick off a ceremony for
	#[pallet::storage]
	#[pallet::getter(fn pending_requests)]
	pub type PendingRequestInstructions<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, RequestId, RequestInstruction<T, I>>;

	/// Callbacks to be dispatched when a request is fulfilled.
	#[pallet::storage]
	#[pallet::getter(fn request_callback)]
	pub type RequestCallback<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, RequestId, <T as Config<I>>::ThresholdCallable>;

	/// State of the threshold signature requested.
	#[pallet::storage]
	#[pallet::getter(fn signature)]
	pub type Signature<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, RequestId, AsyncResult<SignatureResultFor<T, I>>, ValueQuery>;

	/// A map containing lists of ceremony ids that should be retried at the block stored in the
	/// key.
	#[pallet::storage]
	#[pallet::getter(fn ceremony_retry_queues)]
	pub type CeremonyRetryQueues<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, Vec<CeremonyId>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn request_retry_queues)]
	pub type RequestRetryQueue<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, Vec<RequestId>, ValueQuery>;

	/// Maximum duration of a threshold signing ceremony before it is timed out and retried
	#[pallet::storage]
	#[pallet::getter(fn threshold_signature_response_timeout)]
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
			Self { threshold_signature_response_timeout: 10_u32.into(), _instance: PhantomData }
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
		ThresholdSignatureRequest {
			request_id: RequestId,
			ceremony_id: CeremonyId,
			key_id: T::KeyId,
			signatories: BTreeSet<T::ValidatorId>,
			payload: PayloadFor<T, I>,
		},
		ThresholdSignatureFailed {
			request_id: RequestId,
			ceremony_id: CeremonyId,
			key_id: T::KeyId,
			offenders: Vec<T::ValidatorId>,
		},
		/// The threshold signature posted back to the chain was verified.
		ThresholdSignatureSuccess {
			request_id: RequestId,
			ceremony_id: CeremonyId,
		},
		/// We have had a signature success and we have dispatched the associated callback
		ThresholdDispatchComplete {
			request_id: RequestId,
			ceremony_id: CeremonyId,
			result: DispatchResult,
		},
		RetryRequested {
			request_id: RequestId,
			ceremony_id: CeremonyId,
		},
		/// The threshold signature has already succeeded or failed, so this retry is discarded
		StaleRetryDiscarded {
			ceremony_id: CeremonyId,
		},
		FailureReportProcessed {
			request_id: RequestId,
			ceremony_id: CeremonyId,
			reporter_id: T::ValidatorId,
		},
		/// Not enough signers were available to reach threshold. Ceremony will be retried.
		SignersUnavailable {
			request_id: RequestId,
			ceremony_id: CeremonyId,
		},
		/// We cannot sign because the key is unavailable.
		CurrentKeyUnavailable {
			request_id: RequestId,
		},
		/// The threshold signature response timeout has been updated
		ThresholdSignatureResponseTimeoutUpdated {
			new_timeout: BlockNumberFor<T>,
		},
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
			let mut num_offenders = 0;

			// Process pending retries.
			for ceremony_id in CeremonyRetryQueues::<T, I>::take(current_block) {
				if let Some(failed_ceremony_context) = PendingCeremonies::<T, I>::take(ceremony_id)
				{
					let offenders = failed_ceremony_context.offenders();
					num_offenders += offenders.len();
					num_retries += 1;
					let CeremonyContext {
						request_context: RequestContext { request_id, attempt_count, payload },
						key_id,
						threshold_ceremony_type,
						..
					} = failed_ceremony_context;

					Self::deposit_event(match threshold_ceremony_type {
						ThresholdCeremonyType::Standard => {
							T::OffenceReporter::report_many(
								PalletOffence::ParticipateSigningFailed,
								&offenders[..],
							);

							Self::new_ceremony_attempt(RequestInstruction::new(
								request_id,
								attempt_count.wrapping_add(1),
								payload,
								RequestType::Standard,
							));
							Event::<T, I>::RetryRequested { request_id, ceremony_id }
						},
						ThresholdCeremonyType::KeygenVerification => {
							Signature::<T, I>::insert(
								request_id,
								AsyncResult::Ready(Err(offenders.clone())),
							);
							Self::maybe_dispatch_callback(request_id, ceremony_id);
							Event::<T, I>::ThresholdSignatureFailed {
								request_id,
								ceremony_id,
								key_id,
								offenders,
							}
						},
					})
				} else {
					Self::deposit_event(Event::<T, I>::StaleRetryDiscarded { ceremony_id })
				}
			}

			for request_id in RequestRetryQueue::<T, I>::take(current_block) {
				if let Some(request_instruction) =
					PendingRequestInstructions::<T, I>::take(request_id)
				{
					Self::new_ceremony_attempt(request_instruction);
				}
			}

			T::Weights::on_initialize(T::EpochInfo::current_authority_count(), num_retries) +
				T::Weights::report_offenders(num_offenders as AuthorityCount)
		}
	}

	#[pallet::origin]
	#[derive(PartialEq, Eq, Copy, Clone, RuntimeDebug, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct Origin<T: Config<I>, I: 'static = ()>(pub(super) PhantomData<(T, I)>);

	#[pallet::validate_unsigned]
	impl<T: Config<I>, I: 'static> ValidateUnsigned for Pallet<T, I>
	where
		<T as Chainflip>::KeyId: TryInto<<<T as Config<I>>::TargetChain as ChainCrypto>::AggKey>,
	{
		type Call = Call<T, I>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::<T, I>::signature_success { ceremony_id, signature } = call {
				let CeremonyContext { key_id, request_context, .. } =
					PendingCeremonies::<T, I>::get(ceremony_id).ok_or(InvalidTransaction::Stale)?;

				let key = key_id.try_into().map_err(|_| InvalidTransaction::BadProof)?;
				if <T::TargetChain as ChainCrypto>::verify_threshold_signature(
					&key,
					&request_context.payload,
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

			let CeremonyContext {
				request_context: RequestContext { request_id, attempt_count, .. },
				..
			} = PendingCeremonies::<T, I>::take(ceremony_id).ok_or_else(|| {
				// We check the ceremony_id in the ValidateUnsigned transaction, so if this
				// happens, there is something seriously wrong with our assumptions.
				log::error!("Invalid ceremony_id received {}.", ceremony_id);
				Error::<T, I>::InvalidCeremonyId
			})?;

			PendingRequestInstructions::<T, I>::remove(request_id);

			// Report the success once we know the CeremonyId is valid
			Self::deposit_event(Event::<T, I>::ThresholdSignatureSuccess {
				request_id,
				ceremony_id,
			});

			log::debug!(
				"Threshold signature request {} suceeded at ceremony {} after {} attempts.",
				request_id,
				ceremony_id,
				attempt_count
			);

			Signature::<T, I>::insert(request_id, AsyncResult::Ready(Ok(signature)));
			Self::maybe_dispatch_callback(request_id, ceremony_id);

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
			let reporter_id = T::AccountRoleRegistry::ensure_validator(origin)?.into();

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
							// No more respondents waiting: we can retry on the next block.
							Self::schedule_ceremony_retry(id, 1u32.into());
						}

						Self::deposit_event(Event::<T, I>::FailureReportProcessed {
							request_id: context.request_context.request_id,
							ceremony_id: id,
							reporter_id,
						});

						Ok(())
					})
			})?;

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
	fn inner_request_signature(
		payload: PayloadFor<T, I>,
		request_type: RequestType<<T as Chainflip>::KeyId, BTreeSet<T::ValidatorId>>,
	) -> (RequestId, CeremonyId) {
		// Get a new request id.
		let request_id = ThresholdSignatureRequestIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		// Start a ceremony.
		let ceremony_id = Self::new_ceremony_attempt(RequestInstruction {
			request_context: RequestContext { request_id, payload, attempt_count: 0 },
			request_type,
		});

		Signature::<T, I>::insert(request_id, AsyncResult::Pending);

		(request_id, ceremony_id)
	}

	/// Initiates a new ceremony request.
	fn new_ceremony_attempt(request_instruction: RequestInstruction<T, I>) -> CeremonyId {
		let ceremony_id = T::CeremonyIdProvider::next_ceremony_id();

		let request_id = request_instruction.request_context.request_id;
		let attempt_count = request_instruction.request_context.attempt_count;
		let payload = request_instruction.request_context.payload.clone();

		let (maybe_key_id_participants, ceremony_type) =
			if let RequestType::KeygenVerification { ref key_id, ref participants } =
				request_instruction.request_type
			{
				(
					Ok((key_id.clone(), participants.clone())),
					ThresholdCeremonyType::KeygenVerification,
				)
			} else {
				let EpochKey { key, epoch_index, key_state } = T::KeyProvider::current_epoch_key();
				(
					match key_state {
						KeyState::Active =>
							if let Some(nominees) =
								T::ThresholdSignerNomination::threshold_nomination_with_seed(
									(ceremony_id, attempt_count),
									epoch_index,
								) {
								Ok((key.into(), nominees))
							} else {
								Err(Event::<T, I>::SignersUnavailable { request_id, ceremony_id })
							},
						KeyState::Unavailable =>
							Err(Event::<T, I>::CurrentKeyUnavailable { request_id }),
					},
					ThresholdCeremonyType::Standard,
				)
			};

		Self::deposit_event(match maybe_key_id_participants {
			Ok((key_id, participants)) => {
				PendingCeremonies::<T, I>::insert(ceremony_id, {
					let remaining_respondents: BTreeSet<_> =
						participants.clone().into_iter().collect();
					CeremonyContext {
						request_context: RequestContext {
							request_id,
							attempt_count,
							payload: payload.clone(),
						},
						threshold_ceremony_type: ceremony_type,
						key_id: key_id.clone(),
						blame_counts: BTreeMap::new(),
						participant_count: remaining_respondents.len() as AuthorityCount,
						remaining_respondents,
					}
				});
				Self::schedule_ceremony_retry(
					ceremony_id,
					ThresholdSignatureResponseTimeout::<T, I>::get(),
				);
				log::trace!(
					target: "threshold-signing",
					"Threshold set selected for request {}, requesting signature ceremony {}.",
					request_id,
					attempt_count
				);
				Event::<T, I>::ThresholdSignatureRequest {
					request_id,
					ceremony_id,
					key_id,
					signatories: participants,
					payload,
				}
			},
			Err(event) => {
				PendingRequestInstructions::<T, I>::insert(request_id, request_instruction);
				RequestRetryQueue::<T, I>::append(
					frame_system::Pallet::<T>::current_block_number()
						.saturating_add(T::CeremonyRetryDelay::get()),
					request_id,
				);

				log::trace!(
					target: "threshold-signing",
					"Scheduling retry: {:?}", event
				);

				event
			},
		});

		ceremony_id
	}

	// We've kicked off a ceremony, now we start a timeout, where it'll retry after that point.
	fn schedule_ceremony_retry(id: CeremonyId, retry_delay: BlockNumberFor<T>) {
		CeremonyRetryQueues::<T, I>::append(
			frame_system::Pallet::<T>::current_block_number().saturating_add(retry_delay),
			id,
		);
	}

	/// Dispatches the callback if one has been registered.
	fn maybe_dispatch_callback(request_id: RequestId, ceremony_id: CeremonyId) {
		if let Some(call) = RequestCallback::<T, I>::take(request_id) {
			Self::deposit_event(Event::<T, I>::ThresholdDispatchComplete {
				request_id,
				ceremony_id,
				result: call
					.dispatch_bypass_filter(Origin(Default::default()).into())
					.map(|_| ())
					.map_err(|e| {
						log::error!("Threshold dispatch failed for ceremony {}.", ceremony_id);
						e.error
					}),
			});
		}
	}
}

pub struct EnsureThresholdSigned<T: Config<I>, I: 'static = ()>(PhantomData<(T, I)>);

impl<T, I> EnsureOrigin<<T as Config<I>>::RuntimeOrigin> for EnsureThresholdSigned<T, I>
where
	T: Config<I>,
	I: 'static,
{
	type Success = ();

	fn try_origin(
		o: <T as Config<I>>::RuntimeOrigin,
	) -> Result<Self::Success, <T as Config<I>>::RuntimeOrigin> {
		let res: Result<Origin<T, I>, <T as Config<I>>::RuntimeOrigin> = o.into();
		res.map(|_| ())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn successful_origin() -> <T as Config<I>>::RuntimeOrigin {
		Origin::<T, I>(Default::default()).into()
	}
}

impl<T, I: 'static> cf_traits::ThresholdSigner<T::TargetChain> for Pallet<T, I>
where
	T: Config<I>,
	<T as Chainflip>::KeyId: TryInto<<<T as Config<I>>::TargetChain as ChainCrypto>::AggKey>,
{
	type RequestId = RequestId;
	type Error = Error<T, I>;
	type Callback = <T as Config<I>>::ThresholdCallable;
	type KeyId = T::KeyId;
	type ValidatorId = T::ValidatorId;

	fn request_signature(payload: PayloadFor<T, I>) -> (Self::RequestId, CeremonyId) {
		Self::inner_request_signature(payload, RequestType::Standard)
	}

	fn request_keygen_verification_signature(
		payload: <T::TargetChain as ChainCrypto>::Payload,
		key_id: Self::KeyId,
		participants: BTreeSet<Self::ValidatorId>,
	) -> (Self::RequestId, CeremonyId) {
		Self::inner_request_signature(
			payload,
			RequestType::KeygenVerification { key_id, participants },
		)
	}

	fn register_callback(
		request_id: Self::RequestId,
		on_signature_ready: Self::Callback,
	) -> Result<(), Self::Error> {
		ensure!(
			matches!(Signature::<T, I>::get(request_id), AsyncResult::Pending),
			Error::<T, I>::InvalidRequestId
		);
		RequestCallback::<T, I>::insert(request_id, on_signature_ready);
		Ok(())
	}

	fn signature_result(
		request_id: Self::RequestId,
	) -> cf_traits::AsyncResult<SignatureResultFor<T, I>> {
		Signature::<T, I>::take(request_id)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn insert_signature(
		request_id: Self::RequestId,
		signature: <T::TargetChain as ChainCrypto>::ThresholdSignature,
	) {
		Signature::<T, I>::insert(request_id, AsyncResult::Ready(Ok(signature)));
	}
}
