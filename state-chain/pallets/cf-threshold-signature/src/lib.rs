#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
pub mod mock;

#[cfg(test)]
mod tests;

mod benchmarking;
pub mod migrations;
pub mod weights;

mod key_rotator;
mod response_status;

use response_status::ResponseStatus;

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use cf_chains::ChainCrypto;
use cf_primitives::{
	AuthorityCount, CeremonyId, EpochIndex, ThresholdSignatureRequestId as RequestId,
};
use cf_runtime_utilities::{log_or_panic, EnumVariant};
use cf_traits::{
	offence_reporting::OffenceReporter, AsyncResult, CfeMultisigRequest, Chainflip,
	CurrentEpochIndex, EpochInfo, EpochKey, KeyProvider, KeyRotator, SafeMode, Slashing,
	ThresholdSigner, ThresholdSignerNomination,
};
use cfe_events::ThresholdSignatureRequest;
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	ensure,
	sp_runtime::{
		traits::{BlockNumberProvider, Saturating},
		RuntimeDebug,
	},
	traits::{DefensiveOption, EnsureOrigin, Get, StorageVersion, UnfilteredDispatchable},
	weights::Weight,
	RuntimeDebugNoBound,
};

use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
pub use pallet::*;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	marker::PhantomData,
	prelude::*,
};
use weights::WeightInfo;

/// The type used for counting signing attempts.
type AttemptCount = AuthorityCount;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Copy, Clone, PartialEq, Eq, RuntimeDebug)]
#[scale_info(skip_type_params(I))]
pub struct PalletSafeMode<I: 'static> {
	pub slashing_enabled: bool,
	#[doc(hidden)]
	#[codec(skip)]
	_phantom: PhantomData<I>,
}

impl<I: 'static> SafeMode for PalletSafeMode<I> {
	const CODE_RED: Self = PalletSafeMode { slashing_enabled: false, _phantom: PhantomData };
	const CODE_GREEN: Self = PalletSafeMode { slashing_enabled: true, _phantom: PhantomData };
}

pub type SignatureFor<T, I> =
	<<T as Config<I>>::TargetChainCrypto as ChainCrypto>::ThresholdSignature;
type PayloadFor<T, I> = <<T as Config<I>>::TargetChainCrypto as ChainCrypto>::Payload;
pub type KeygenOutcomeFor<T, I = ()> =
	Result<AggKeyFor<T, I>, BTreeSet<<T as Chainflip>::ValidatorId>>;
pub type AggKeyFor<T, I = ()> = <<T as Config<I>>::TargetChainCrypto as ChainCrypto>::AggKey;
pub type KeygenResponseStatus<T, I> =
	ResponseStatus<T, KeygenSuccessVoters<T, I>, KeygenFailureVoters<T, I>, I>;

pub type KeyHandoverResponseStatus<T, I> =
	ResponseStatus<T, KeyHandoverSuccessVoters<T, I>, KeyHandoverFailureVoters<T, I>, I>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	ParticipateSigningFailed,
	FailedKeygen,
	FailedKeyHandover,
}

#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum RequestType<Key, Participants> {
	/// Uses the provided key and selects new participants from the provided epoch.
	/// This signing request will be retried until success.
	SpecificKey(Key, EpochIndex),
	/// Uses the recently generated key and the participants used to generate that key.
	/// This signing request will only be attemped once, as failing this ought to result
	/// in another Keygen ceremony.
	KeygenVerification { key: Key, epoch_index: EpochIndex, participants: Participants },
}

/// The type of a threshold *Ceremony* i.e. after a request has been emitted, it is then a ceremony.
#[derive(Clone, Copy, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum ThresholdCeremonyType {
	Standard,
	KeygenVerification,
}

/// The current status of a key rotation.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, EnumVariant, RuntimeDebugNoBound)]
#[scale_info(skip_type_params(T, I))]
pub enum KeyRotationStatus<T: Config<I>, I: 'static = ()> {
	/// We are waiting for nodes to generate a new aggregate key.
	AwaitingKeygen {
		ceremony_id: CeremonyId,
		keygen_participants: BTreeSet<T::ValidatorId>,
		response_status: KeygenResponseStatus<T, I>,
		new_epoch_index: EpochIndex,
	},
	/// We are waiting for the nodes who generated the new key to complete a signing ceremony to
	/// verify the new key.
	AwaitingKeygenVerification {
		new_public_key: AggKeyFor<T, I>,
	},
	/// Keygen verification is complete for key
	KeygenVerificationComplete {
		new_public_key: AggKeyFor<T, I>,
	},
	AwaitingKeyHandover {
		ceremony_id: CeremonyId,
		response_status: KeyHandoverResponseStatus<T, I>,
		receiving_participants: BTreeSet<T::ValidatorId>,
		next_epoch: EpochIndex,
		new_public_key: AggKeyFor<T, I>,
	},
	AwaitingKeyHandoverVerification {
		new_public_key: AggKeyFor<T, I>,
	},
	KeyHandoverComplete {
		new_public_key: AggKeyFor<T, I>,
	},
	/// We are waiting for the key to be updated on the contract, and witnessed by the network.
	AwaitingActivation {
		request_ids: Vec<RequestId>,
		new_public_key: AggKeyFor<T, I>,
	},
	/// The key has been successfully updated on the external chain, and/or funds rotated to new
	/// key.
	Complete,
	/// The rotation has failed at one of the above stages.
	Failed {
		offenders: BTreeSet<T::ValidatorId>,
	},
	KeyHandoverFailed {
		new_public_key: AggKeyFor<T, I>,
		offenders: BTreeSet<T::ValidatorId>,
	},
}

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(5);

const THRESHOLD_SIGNATURE_RESPONSE_TIMEOUT_DEFAULT: u32 = 10;
const KEYGEN_CEREMONY_RESPONSE_TIMEOUT_BLOCKS_DEFAULT: u32 = 90;

struct GetFromU32<C: Get<u32>>(PhantomData<C>);

impl<C: Get<u32>, B: From<u32>> Get<B> for GetFromU32<C> {
	fn get() -> B {
		C::get().into()
	}
}

macro_rules! handle_key_ceremony_report {
	($origin:expr, $ceremony_id:expr, $reported_outcome:expr, $variant:path, $success_event:expr, $failure_event:expr) => {

		let reporter = T::AccountRoleRegistry::ensure_validator($origin)?.into();

		// There is a rotation happening.
		let mut rotation =
			PendingKeyRotation::<T, I>::get().ok_or(Error::<T, I>::NoActiveRotation)?;

		// Keygen is in progress, pull out the details.
		let (pending_ceremony_id, response_status) = ensure_variant!(
			$variant {
				ceremony_id, ref mut response_status, ..
			} => (ceremony_id, response_status),
			rotation,
			Error::<T, I>::InvalidRotationStatus,
		);

		// Make sure the ceremony id matches
		ensure!(pending_ceremony_id == $ceremony_id, Error::<T, I>::InvalidKeygenCeremonyId);
		ensure!(
			response_status.remaining_candidates().contains(&reporter),
			Error::<T, I>::InvalidKeygenRespondent
		);

		Self::deposit_event(match $reported_outcome {
			Ok(key) => {
				response_status.add_success_vote(&reporter, key);
				$success_event(reporter)
			},
			Err(offenders) => {
				// Remove any offenders that are not part of the ceremony and log them
				let (valid_blames, invalid_blames): (BTreeSet<_>, BTreeSet<_>) =
				offenders.into_iter().partition(|id| response_status.candidates().contains(id));
				if !invalid_blames.is_empty() {
					log::warn!(
						"Invalid offenders reported {:?} for ceremony {}.",
						invalid_blames,
						$ceremony_id
					);
				}

				response_status.add_failure_vote(&reporter, valid_blames);
				$failure_event(reporter)
			},
		});

		PendingKeyRotation::<T, I>::put(rotation);
	};
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_primitives::FlipBalance;
	use cf_traits::{
		AccountRoleRegistry, AsyncResult, CfeMultisigRequest, ThresholdSignerNomination,
		VaultActivator,
	};
	use frame_support::{
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
		/// The candidates participating in the signing ceremony (ie. the threshold set).
		pub candidates: BTreeSet<T::ValidatorId>,
		/// The epoch in which the ceremony was started.
		pub epoch: EpochIndex,
		/// The key we want to sign with.
		pub key: <T::TargetChainCrypto as ChainCrypto>::AggKey,
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
		pub request_type:
			RequestType<<T::TargetChainCrypto as ChainCrypto>::AggKey, BTreeSet<T::ValidatorId>>,
	}

	impl<T: Config<I>, I: 'static> RequestInstruction<T, I> {
		pub fn new(
			request_id: RequestId,
			attempt_count: AttemptCount,
			payload: PayloadFor<T, I>,
			request_type: RequestType<
				<T::TargetChainCrypto as ChainCrypto>::AggKey,
				BTreeSet<T::ValidatorId>,
			>,
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
		/// authority set will no longer be in control of the key.
		/// Similarly for vault rotations - we can't abort a rotation at the setAggKey stage: we
		/// have to keep retrying with the current set of authorities.
		pub fn offenders(&self) -> Vec<T::ValidatorId> {
			// A threshold for number of blame 'accusations' that are required for someone to be
			// punished.
			let blame_threshold = (self.candidates.len() as AuthorityCount).saturating_mul(2) / 3;
			// The maximum number of offenders we are willing to report without risking the liveness
			// of the network.
			let liveness_threshold = self.candidates.len() / 2;

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

			if to_report.len() <= liveness_threshold {
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

		/// The top-level offence type must support this pallet's offence type.
		type Offence: From<PalletOffence>;

		/// The top-level origin type of the runtime.
		type RuntimeOrigin: From<Origin<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeOrigin>
			+ Into<Result<Origin<Self, I>, <Self as Config<I>>::RuntimeOrigin>>;

		/// The calls that this pallet can dispatch after generating a signature.
		type ThresholdCallable: Member
			+ Parameter
			+ UnfilteredDispatchable<RuntimeOrigin = <Self as Config<I>>::RuntimeOrigin>
			+ From<Call<Self, I>>;

		/// A marker trait identifying the chain that we are signing for.
		type TargetChainCrypto: ChainCrypto;

		/// trait to activate chains that use this pallet's key
		type VaultActivator: VaultActivator<Self::TargetChainCrypto>;

		/// Signer nomination.
		type ThresholdSignerNomination: ThresholdSignerNomination<SignerId = Self::ValidatorId>;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode<I>>;

		type Slasher: Slashing<AccountId = Self::ValidatorId, BlockNumber = BlockNumberFor<Self>>;

		/// For reporting bad actors.
		type OffenceReporter: OffenceReporter<
			ValidatorId = <Self as Chainflip>::ValidatorId,
			Offence = Self::Offence,
		>;

		/// In case not enough live nodes were available to begin a threshold signing ceremony: The
		/// number of blocks to wait before retrying with a new set.
		#[pallet::constant]
		type CeremonyRetryDelay: Get<BlockNumberFor<Self>>;

		type CfeMultisigRequest: CfeMultisigRequest<Self, Self::TargetChainCrypto>;

		/// Pallet weights
		type Weights: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
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
	pub(super) type ThresholdSignatureResponseTimeout<T: Config<I>, I: 'static = ()> = StorageValue<
		_,
		BlockNumberFor<T>,
		ValueQuery,
		GetFromU32<ConstU32<THRESHOLD_SIGNATURE_RESPONSE_TIMEOUT_DEFAULT>>,
	>;

	/// The epoch whose authorities control the current key.
	#[pallet::storage]
	#[pallet::getter(fn current_key_epoch)]
	pub type CurrentKeyEpoch<T: Config<I>, I: 'static = ()> = StorageValue<_, EpochIndex>;

	/// The map of all keys by epoch.
	#[pallet::storage]
	#[pallet::getter(fn keys)]
	pub type Keys<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, EpochIndex, AggKeyFor<T, I>>;

	/// Key rotation statuses for the current epoch rotation.
	#[pallet::storage]
	#[pallet::getter(fn pending_key_rotations)]
	pub type PendingKeyRotation<T: Config<I>, I: 'static = ()> =
		StorageValue<_, KeyRotationStatus<T, I>>;

	/// The voters who voted for success for a particular agg key rotation
	#[pallet::storage]
	#[pallet::getter(fn keygen_success_voters)]
	pub type KeygenSuccessVoters<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Identity, AggKeyFor<T, I>, Vec<T::ValidatorId>, ValueQuery>;

	/// The voters who voted for failure for a particular agg key rotation
	#[pallet::storage]
	#[pallet::getter(fn keygen_failure_voters)]
	pub type KeygenFailureVoters<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

	/// The voters who voted for success for a particular key handover ceremony
	#[pallet::storage]
	#[pallet::getter(fn key_handover_success_voters)]
	pub type KeyHandoverSuccessVoters<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Identity, AggKeyFor<T, I>, Vec<T::ValidatorId>, ValueQuery>;

	/// The voters who voted for failure for a particular key handover ceremony
	#[pallet::storage]
	#[pallet::getter(fn key_handover_failure_voters)]
	pub type KeyHandoverFailureVoters<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

	/// The block since which we have been waiting for keygen to be resolved.
	#[pallet::storage]
	#[pallet::getter(fn keygen_resolution_pending_since)]
	pub(super) type KeygenResolutionPendingSince<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// The block since which we have been waiting for key handover to be resolved.
	#[pallet::storage]
	#[pallet::getter(fn key_handover_resolution_pending_since)]
	pub(super) type KeyHandoverResolutionPendingSince<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::storage]
	pub(super) type KeygenResponseTimeout<T: Config<I>, I: 'static = ()> = StorageValue<
		_,
		BlockNumberFor<T>,
		ValueQuery,
		GetFromU32<ConstU32<KEYGEN_CEREMONY_RESPONSE_TIMEOUT_BLOCKS_DEFAULT>>,
	>;

	/// The amount of FLIP that is slashed for an agreed reported party expressed in Flipperinos
	/// (2/3 must agree the node was an offender) on keygen failure.
	#[pallet::storage]
	pub(super) type KeygenSlashAmount<T, I = ()> = StorageValue<_, FlipBalance, ValueQuery>;

	/// Counter for generating unique ceremony ids.
	#[pallet::storage]
	#[pallet::getter(fn ceremony_id_counter)]
	pub type CeremonyIdCounter<T: Config<I>, I: 'static = ()> =
		StorageValue<_, CeremonyId, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		pub key: Option<AggKeyFor<T, I>>,
		pub threshold_signature_response_timeout: BlockNumberFor<T>,
		pub keygen_response_timeout: BlockNumberFor<T>,
		pub amount_to_slash: FlipBalance,
		pub _instance: PhantomData<I>,
	}

	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			Self {
				key: None,
				threshold_signature_response_timeout: THRESHOLD_SIGNATURE_RESPONSE_TIMEOUT_DEFAULT
					.into(),
				keygen_response_timeout: KEYGEN_CEREMONY_RESPONSE_TIMEOUT_BLOCKS_DEFAULT.into(),
				amount_to_slash: 0u128,
				_instance: PhantomData,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			ThresholdSignatureResponseTimeout::<T, I>::put(
				self.threshold_signature_response_timeout,
			);
			KeygenResponseTimeout::<T, I>::put(self.keygen_response_timeout);
			if let Some(key) = self.key {
				Pallet::<T, I>::set_key_for_epoch(cf_primitives::GENESIS_EPOCH, key);
			} else {
				log::info!("No genesis key configured for {}.", Pallet::<T, I>::name());
			}
			KeygenSlashAmount::<T, I>::put(self.amount_to_slash);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		ThresholdSignatureRequest {
			request_id: RequestId,
			ceremony_id: CeremonyId,
			epoch: EpochIndex,
			key: <T::TargetChainCrypto as ChainCrypto>::AggKey,
			signatories: BTreeSet<T::ValidatorId>,
			payload: PayloadFor<T, I>,
		},
		ThresholdSignatureFailed {
			request_id: RequestId,
			ceremony_id: CeremonyId,
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
		FailureReportProcessed {
			request_id: RequestId,
			ceremony_id: CeremonyId,
			reporter_id: T::ValidatorId,
		},
		/// Not enough signers were available to reach threshold.
		SignersUnavailable {
			request_id: RequestId,
			attempt_count: AttemptCount,
		},
		/// The threshold signature response timeout has been updated
		ThresholdSignatureResponseTimeoutUpdated {
			new_timeout: BlockNumberFor<T>,
		},

		/// Request a key generation
		KeygenRequest {
			ceremony_id: CeremonyId,
			participants: BTreeSet<T::ValidatorId>,
			/// The epoch index for which the key is being generated.
			epoch_index: EpochIndex,
		},
		/// Request a key handover
		KeyHandoverRequest {
			ceremony_id: CeremonyId,
			from_epoch: EpochIndex,
			key_to_share: AggKeyFor<T, I>,
			sharing_participants: BTreeSet<T::ValidatorId>,
			receiving_participants: BTreeSet<T::ValidatorId>,
			/// The freshly generated key for the next epoch.
			new_key: AggKeyFor<T, I>,
			/// The epoch index for which the key is being handed over.
			to_epoch: EpochIndex,
		},
		/// A keygen participant has reported that keygen was successful \[validator_id\]
		KeygenSuccessReported(T::ValidatorId),
		/// A key handover participant has reported that keygen was successful \[validator_id\]
		KeyHandoverSuccessReported(T::ValidatorId),
		/// A keygen participant has reported that keygen has failed \[validator_id\]
		KeygenFailureReported(T::ValidatorId),
		/// A key handover participant has reported that keygen has failed \[validator_id\]
		KeyHandoverFailureReported(T::ValidatorId),
		/// Keygen was successful \[ceremony_id\]
		KeygenSuccess(CeremonyId),
		/// The key handover was successful
		KeyHandoverSuccess {
			ceremony_id: CeremonyId,
		},
		NoKeyHandover,
		/// The new key was successfully used to sign.
		KeygenVerificationSuccess {
			agg_key: AggKeyFor<T, I>,
		},
		KeyHandoverVerificationSuccess {
			agg_key: AggKeyFor<T, I>,
		},
		/// Verification of the new key has failed.
		KeygenVerificationFailure {
			keygen_ceremony_id: CeremonyId,
		},
		KeyHandoverVerificationFailure {
			handover_ceremony_id: CeremonyId,
		},
		/// Keygen has failed \[ceremony_id\]
		KeygenFailure(CeremonyId),
		/// Keygen response timeout has occurred \[ceremony_id\]
		KeygenResponseTimeout(CeremonyId),
		KeyHandoverResponseTimeout {
			ceremony_id: CeremonyId,
		},
		/// Keygen response timeout was updated \[new_timeout\]
		KeygenResponseTimeoutUpdated {
			new_timeout: BlockNumberFor<T>,
		},
		/// Key handover has failed
		KeyHandoverFailure {
			ceremony_id: CeremonyId,
		},
		/// The vault on chains associated with this key have all rotated
		KeyRotationCompleted,
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided ceremony id is invalid.
		InvalidThresholdSignatureCeremonyId,
		/// An invalid keygen ceremony id
		InvalidKeygenCeremonyId,
		/// The provided threshold signature is invalid.
		InvalidThresholdSignature,
		/// The reporting party is not one of the signatories for this ceremony, or has already
		/// responded.
		InvalidThresholdSignatureRespondent,
		/// An authority sent a response for a ceremony in which they weren't involved, or to which
		/// they have already submitted a response.
		InvalidKeygenRespondent,
		/// The request Id is stale or not yet valid.
		InvalidRequestId,
		/// There is no threshold signature available
		ThresholdSignatureUnavailable,
		/// There is currently no rotation in progress for this key.
		NoActiveRotation,
		/// The requested call is invalid based on the current rotation state.
		InvalidRotationStatus,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			let mut weight = T::DbWeight::get().reads(1);

			// ====== 1. Process pending rotation taskes =======

			if Self::status() == AsyncResult::Pending {
				match PendingKeyRotation::<T, I>::get() {
					Some(KeyRotationStatus::<T, I>::AwaitingKeygen {
						ceremony_id,
						keygen_participants,
						new_epoch_index,
						response_status,
					}) => {
						weight += Self::progress_rotation::<
							KeygenSuccessVoters<T, I>,
							KeygenFailureVoters<T, I>,
							KeygenResolutionPendingSince<T, I>,
						>(
							response_status,
							ceremony_id,
							current_block,
							// no extra checks are necessary for regular keygen
							Ok,
							|new_public_key| {
								Self::deposit_event(Event::KeygenSuccess(ceremony_id));
								Self::trigger_keygen_verification(
									ceremony_id,
									new_public_key,
									keygen_participants,
									new_epoch_index,
								);
							},
							|offenders| {
								Self::terminate_rotation(
									offenders,
									Event::KeygenFailure(ceremony_id),
								);
							},
						);
					},
					Some(KeyRotationStatus::<T, I>::AwaitingKeyHandover {
						ceremony_id,
						response_status,
						receiving_participants,
						next_epoch,
						new_public_key,
					}) => {
						weight += Self::progress_rotation::<
							KeyHandoverSuccessVoters<T, I>,
							KeyHandoverFailureVoters<T, I>,
							KeyHandoverResolutionPendingSince<T, I>,
						>(
							response_status,
							ceremony_id,
							current_block,
							// For key handover we also check that the key is the same as before
							|reported_new_agg_key| {
								let current_key = Self::active_epoch_key()
									.expect("In order to handover the key from the last epoch that key must already exist")
									.key;

								if T::TargetChainCrypto::handover_key_matches(
									&current_key,
									&reported_new_agg_key,
								) {
									Ok(reported_new_agg_key)
								} else {
									log::error!(
										"Handover resulted in an unexpected key: {:?}",
										&reported_new_agg_key
									);
									Err(Default::default())
								}
							},
							|reported_new_public_key| {
								Self::deposit_event(Event::KeyHandoverSuccess { ceremony_id });

								Self::trigger_key_verification(
									reported_new_public_key,
									receiving_participants,
									true,
									next_epoch,
									|req_id| {
										Call::on_handover_verification_result {
											handover_ceremony_id: ceremony_id,
											threshold_request_id: req_id,
											new_public_key: reported_new_public_key,
										}
										.into()
									},
									KeyRotationStatus::<T, I>::AwaitingKeyHandoverVerification {
										new_public_key: reported_new_public_key,
									},
								);
							},
							|offenders| {
								T::OffenceReporter::report_many(
									PalletOffence::FailedKeyHandover,
									offenders.clone(),
								);
								PendingKeyRotation::<T, I>::put(
									KeyRotationStatus::<T, I>::KeyHandoverFailed {
										new_public_key,
										offenders,
									},
								);
								Self::deposit_event(Event::KeyHandoverFailure { ceremony_id });
							},
						);
					},
					_ => {
						// noop
					},
				}
			}

			// ====== 2. Process pending ceremonies =======

			let mut num_retries = 0;
			let mut num_offenders = 0;

			for ceremony_id in CeremonyRetryQueues::<T, I>::take(current_block) {
				if let Some(failed_ceremony_context) = PendingCeremonies::<T, I>::take(ceremony_id)
				{
					let offenders = failed_ceremony_context.offenders();
					num_offenders += offenders.len();
					num_retries += 1;

					let CeremonyContext {
						request_context: RequestContext { request_id, attempt_count, payload },
						threshold_ceremony_type,
						key,
						epoch,
						..
					} = failed_ceremony_context;

					Self::deposit_event(match threshold_ceremony_type {
						ThresholdCeremonyType::Standard => {
							T::OffenceReporter::report_many(
								PalletOffence::ParticipateSigningFailed,
								offenders,
							);

							Self::new_ceremony_attempt(RequestInstruction::new(
								request_id,
								attempt_count.wrapping_add(1),
								payload,
								RequestType::SpecificKey(key, epoch),
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
								offenders,
							}
						},
					})
				}
			}

			for request_id in RequestRetryQueue::<T, I>::take(current_block) {
				if let Some(request_instruction) =
					PendingRequestInstructions::<T, I>::take(request_id)
				{
					Self::new_ceremony_attempt(request_instruction);
				}
			}

			weight +
				T::Weights::on_initialize(T::EpochInfo::current_authority_count(), num_retries) +
				T::Weights::report_offenders(num_offenders as AuthorityCount)
		}
	}

	#[pallet::origin]
	#[derive(PartialEq, Eq, Copy, Clone, RuntimeDebug, Encode, Decode, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T, I))]
	pub struct Origin<T: Config<I>, I: 'static = ()>(pub(super) PhantomData<(T, I)>);

	#[pallet::validate_unsigned]
	impl<T: Config<I>, I: 'static> ValidateUnsigned for Pallet<T, I> {
		type Call = Call<T, I>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::<T, I>::signature_success { ceremony_id, signature } = call {
				let CeremonyContext { key, request_context, .. } =
					PendingCeremonies::<T, I>::get(ceremony_id).ok_or(InvalidTransaction::Stale)?;

				if <T::TargetChainCrypto as ChainCrypto>::verify_threshold_signature(
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
		/// - [InvalidThresholdSignatureCeremonyId](sp_runtime::traits::InvalidThresholdSignatureCeremonyId)
		/// - [BadOrigin](sp_runtime::traits::BadOrigin)
		#[pallet::call_index(0)]
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
				Error::<T, I>::InvalidThresholdSignatureCeremonyId
			})?;

			PendingRequestInstructions::<T, I>::remove(request_id);

			// Report the success once we know the CeremonyId is valid
			Self::deposit_event(Event::<T, I>::ThresholdSignatureSuccess {
				request_id,
				ceremony_id,
			});

			log::debug!(
				"Threshold signature request {} succeeded at ceremony {} after {} attempts.",
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
		/// - [InvalidThresholdSignatureCeremonyId](Error::InvalidThresholdSignatureCeremonyId)
		/// - [InvalidThresholdSignatureRespondent](Error::InvalidThresholdSignatureRespondent)
		#[pallet::call_index(1)]
		#[pallet::weight(T::Weights::report_signature_failed(offenders.len() as u32))]
		pub fn report_signature_failed(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			offenders: BTreeSet<<T as Chainflip>::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			let reporter_id = T::AccountRoleRegistry::ensure_validator(origin)?.into();

			PendingCeremonies::<T, I>::try_mutate(ceremony_id, |maybe_context| {
				maybe_context
					.as_mut()
					.ok_or(Error::<T, I>::InvalidThresholdSignatureCeremonyId)
					.and_then(|context| {
						if !context.remaining_respondents.remove(&reporter_id) {
							return Err(Error::<T, I>::InvalidThresholdSignatureRespondent)
						}

						// Remove any offenders that are not part of the ceremony and log them
						let (valid_blames, invalid_blames): (BTreeSet<_>, BTreeSet<_>) =
							offenders.into_iter().partition(|id| context.candidates.contains(id));

						if !invalid_blames.is_empty() {
							log::warn!(
								"Invalid offenders reported {:?} for ceremony {}.",
								invalid_blames,
								ceremony_id
							);
						}

						for id in valid_blames {
							(*context.blame_counts.entry(id).or_default()) += 1;
						}

						if context.remaining_respondents.is_empty() {
							// No more respondents waiting: we can retry on the next block.
							Self::schedule_ceremony_retry(ceremony_id, 1u32.into());
						}

						Self::deposit_event(Event::<T, I>::FailureReportProcessed {
							request_id: context.request_context.request_id,
							ceremony_id,
							reporter_id,
						});

						Ok(())
					})
			})?;

			Ok(().into())
		}

		#[pallet::call_index(2)]
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

		/// Report the outcome of a keygen ceremony.
		///
		/// See [`KeygenOutcome`] for possible outcomes.
		///
		/// ## Events
		///
		/// - [KeygenSuccessReported](Event::KeygenSuccessReported)
		/// - [KeygenFailureReported](Event::KeygenFailureReported)
		///
		/// ## Errors
		///
		/// - [NoActiveRotation](Error::NoActiveRotation)
		/// - [InvalidRotationStatus](Error::InvalidRotationStatus)
		/// - [InvalidKeygenCeremonyId](Error::InvalidKeygenCeremonyId)
		///
		/// ## Dependencies
		///
		/// - [Threshold Signer Trait](ThresholdSigner)
		#[pallet::call_index(3)]
		#[pallet::weight(T::Weights::report_keygen_outcome())]
		pub fn report_keygen_outcome(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			reported_outcome: KeygenOutcomeFor<T, I>,
		) -> DispatchResultWithPostInfo {
			handle_key_ceremony_report!(
				origin,
				ceremony_id,
				reported_outcome,
				KeyRotationStatus::<T, I>::AwaitingKeygen,
				Event::KeygenSuccessReported,
				Event::KeygenFailureReported
			);

			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(T::Weights::report_keygen_outcome())]
		pub fn report_key_handover_outcome(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			reported_outcome: KeygenOutcomeFor<T, I>,
		) -> DispatchResultWithPostInfo {
			handle_key_ceremony_report!(
				origin,
				ceremony_id,
				reported_outcome,
				KeyRotationStatus::<T, I>::AwaitingKeyHandover,
				Event::KeyHandoverSuccessReported,
				Event::KeyHandoverFailureReported
			);

			Ok(().into())
		}

		/// A callback to be used when the threshold signing ceremony used for keygen verification
		/// completes.
		///
		/// ## Events
		///
		/// - [KeygenVerificationSuccess](Event::KeygenVerificationSuccess)
		/// - [KeygenFailure](Event::KeygenFailure)
		///
		/// ## Errors
		///
		/// - [ThresholdSignatureUnavailable](Error::ThresholdSignatureUnavailable)
		#[pallet::call_index(5)]
		#[pallet::weight(T::Weights::on_keygen_verification_result())]
		pub fn on_keygen_verification_result(
			origin: OriginFor<T>,
			keygen_ceremony_id: CeremonyId,
			threshold_request_id: RequestId,
			new_public_key: AggKeyFor<T, I>,
		) -> DispatchResultWithPostInfo {
			Self::on_key_verification_result(
				origin,
				threshold_request_id,
				KeyRotationStatus::<T, I>::KeygenVerificationComplete { new_public_key },
				Event::KeygenVerificationSuccess { agg_key: new_public_key },
				Event::KeygenVerificationFailure { keygen_ceremony_id },
			)
		}

		#[pallet::call_index(6)]
		#[pallet::weight(T::Weights::on_keygen_verification_result())]
		pub fn on_handover_verification_result(
			origin: OriginFor<T>,
			handover_ceremony_id: CeremonyId,
			threshold_request_id: RequestId,
			new_public_key: AggKeyFor<T, I>,
		) -> DispatchResultWithPostInfo {
			Self::on_key_verification_result(
				origin,
				threshold_request_id,
				KeyRotationStatus::<T, I>::KeyHandoverComplete { new_public_key },
				Event::KeyHandoverVerificationSuccess { agg_key: new_public_key },
				Event::KeyHandoverVerificationFailure { handover_ceremony_id },
			)
		}

		#[pallet::call_index(7)]
		#[pallet::weight(T::Weights::set_keygen_response_timeout())]
		pub fn set_keygen_response_timeout(
			origin: OriginFor<T>,
			new_timeout: BlockNumberFor<T>,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			if new_timeout != KeygenResponseTimeout::<T, I>::get() {
				KeygenResponseTimeout::<T, I>::put(new_timeout);
				Pallet::<T, I>::deposit_event(Event::KeygenResponseTimeoutUpdated { new_timeout });
			}

			Ok(().into())
		}

		#[pallet::call_index(8)]
		#[pallet::weight(T::Weights::set_keygen_response_timeout())]
		pub fn set_keygen_slash_amount(
			origin: OriginFor<T>,
			amount_to_slash: FlipBalance,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			KeygenSlashAmount::<T, I>::put(amount_to_slash);

			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Initiate a new signature request, returning the request id.
	fn inner_request_signature(
		payload: PayloadFor<T, I>,
		request_type: RequestType<
			<T::TargetChainCrypto as ChainCrypto>::AggKey,
			BTreeSet<T::ValidatorId>,
		>,
	) -> RequestId {
		let request_id = ThresholdSignatureRequestIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		});

		Self::new_ceremony_attempt(RequestInstruction {
			request_context: RequestContext { request_id, payload, attempt_count: 0 },
			request_type,
		});

		Signature::<T, I>::insert(request_id, AsyncResult::Pending);

		request_id
	}

	/// Initiates a new ceremony request. Can return None if no ceremony was started.
	fn new_ceremony_attempt(request_instruction: RequestInstruction<T, I>) {
		let request_id = request_instruction.request_context.request_id;
		let attempt_count = request_instruction.request_context.attempt_count;
		let payload = request_instruction.request_context.payload.clone();

		let (maybe_epoch_key_and_participants, ceremony_type) =
			if let RequestType::KeygenVerification { epoch_index, key, ref participants } =
				request_instruction.request_type
			{
				(
					Ok((epoch_index, key, participants.clone())),
					ThresholdCeremonyType::KeygenVerification,
				)
			} else {
				(
					match request_instruction.request_type {
						RequestType::SpecificKey(key, epoch_index) => Ok((key, epoch_index)),
						_ => unreachable!("RequestType::KeygenVerification is handled above"),
					}
					.and_then(|(key, epoch_index)| {
						if let Some(nominees) =
							T::ThresholdSignerNomination::threshold_nomination_with_seed(
								(request_id, attempt_count),
								epoch_index,
							) {
							Ok((epoch_index, key, nominees))
						} else {
							Err(Event::<T, I>::SignersUnavailable { request_id, attempt_count })
						}
					}),
					ThresholdCeremonyType::Standard,
				)
			};

		Self::deposit_event(match maybe_epoch_key_and_participants {
			Ok((epoch, key, participants)) => {
				let ceremony_id = Self::increment_ceremony_id();
				PendingCeremonies::<T, I>::insert(ceremony_id, {
					CeremonyContext {
						request_context: RequestContext {
							request_id,
							attempt_count,
							payload: payload.clone(),
						},
						threshold_ceremony_type: ceremony_type,
						epoch,
						key,
						blame_counts: BTreeMap::new(),
						candidates: participants.clone(),
						remaining_respondents: participants.clone(),
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

				T::CfeMultisigRequest::signature_request(ThresholdSignatureRequest {
					ceremony_id,
					epoch_index: epoch,
					key,
					signatories: participants.clone(),
					payload: payload.clone(),
				});

				// TODO: consider removing this
				Event::<T, I>::ThresholdSignatureRequest {
					request_id,
					ceremony_id,
					epoch,
					key,
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
	}

	fn progress_rotation<SuccessVoters, FailureVoters, PendingSince>(
		response_status: ResponseStatus<T, SuccessVoters, FailureVoters, I>,
		ceremony_id: CeremonyId,
		current_block: BlockNumberFor<T>,
		final_key_check: impl Fn(AggKeyFor<T, I>) -> KeygenOutcomeFor<T, I>,
		on_success_outcome: impl FnOnce(AggKeyFor<T, I>),
		on_failure_outcome: impl FnOnce(BTreeSet<T::ValidatorId>),
	) -> Weight
	where
		T: Config<I>,
		I: 'static,
		SuccessVoters: frame_support::StorageMap<AggKeyFor<T, I>, Vec<T::ValidatorId>>
			+ frame_support::IterableStorageMap<AggKeyFor<T, I>, Vec<T::ValidatorId>>
			+ frame_support::StoragePrefixedMap<Vec<T::ValidatorId>>,
		FailureVoters: frame_support::StorageValue<Vec<T::ValidatorId>>,
		<FailureVoters as frame_support::StorageValue<Vec<T::ValidatorId>>>::Query:
			sp_std::iter::IntoIterator<Item = T::ValidatorId>,
		PendingSince: frame_support::StorageValue<BlockNumberFor<T>, Query = BlockNumberFor<T>>,
	{
		let remaining_candidate_count = response_status.remaining_candidate_count();
		if remaining_candidate_count == 0 {
			log::debug!("All candidates have reported, resolving outcome...");
		} else if current_block.saturating_sub(PendingSince::get()) >=
			KeygenResponseTimeout::<T, I>::get()
		{
			log::debug!("Keygen response timeout has elapsed, attempting to resolve outcome...");
			Self::deposit_event(Event::<T, I>::KeygenResponseTimeout(ceremony_id));
		} else {
			return Weight::from_parts(0, 0)
		};

		let candidate_count = response_status.candidate_count();
		let weight = match response_status.resolve_keygen_outcome(final_key_check) {
			Ok(new_public_key) => {
				debug_assert_eq!(
					remaining_candidate_count, 0,
					"Can't have success unless all candidates responded"
				);
				on_success_outcome(new_public_key);
				T::Weights::on_initialize_success()
			},
			Err(offenders) => {
				let offenders_len = offenders.len();
				let offenders = if (offenders_len as AuthorityCount) <
					cf_utilities::failure_threshold_from_share_count(candidate_count)
				{
					offenders
				} else {
					Default::default()
				};
				on_failure_outcome(offenders);
				T::Weights::on_initialize_failure(offenders_len as u32)
			},
		};
		PendingSince::kill();
		weight
	}

	// Once we've successfully generated the key, we want to do a signing ceremony to verify that
	// the key is useable
	fn trigger_keygen_verification(
		keygen_ceremony_id: CeremonyId,
		new_public_key: AggKeyFor<T, I>,
		participants: BTreeSet<T::ValidatorId>,
		new_epoch_index: EpochIndex,
	) -> RequestId {
		Self::trigger_key_verification(
			new_public_key,
			participants,
			false,
			new_epoch_index,
			|req_id| {
				Call::on_keygen_verification_result {
					keygen_ceremony_id,
					threshold_request_id: req_id,
					new_public_key,
				}
				.into()
			},
			KeyRotationStatus::<T, I>::AwaitingKeygenVerification { new_public_key },
		)
	}

	fn trigger_key_verification(
		new_agg_key: AggKeyFor<T, I>,
		participants: BTreeSet<T::ValidatorId>,
		is_handover: bool,
		next_epoch: EpochIndex,
		signature_callback_fn: impl FnOnce(RequestId) -> <T as Config<I>>::ThresholdCallable,
		status_to_set: KeyRotationStatus<T, I>,
	) -> RequestId {
		let request_id = Self::inner_request_signature(
			T::TargetChainCrypto::agg_key_to_payload(new_agg_key, is_handover),
			RequestType::KeygenVerification {
				key: new_agg_key,
				participants,
				epoch_index: next_epoch,
			},
		);

		if Self::register_callback(request_id, signature_callback_fn(request_id)).is_err() {
			// We should never fail to register a callback for a request that we just created.
			log_or_panic!("Failed to register callback for request {}", request_id);
		}

		PendingKeyRotation::<T, I>::put(status_to_set);

		request_id
	}

	fn terminate_rotation(
		offenders: impl IntoIterator<Item = T::ValidatorId> + Clone,
		event: Event<T, I>,
	) {
		T::OffenceReporter::report_many(PalletOffence::FailedKeygen, offenders.clone());
		if T::SafeMode::get().slashing_enabled {
			offenders.clone().into_iter().for_each(|offender| {
				T::Slasher::slash_balance(&offender, KeygenSlashAmount::<T, I>::get());
			});
		}
		PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::Failed {
			offenders: offenders.into_iter().collect(),
		});
		Self::deposit_event(event);
	}

	fn on_key_verification_result(
		origin: OriginFor<T>,
		threshold_request_id: RequestId,
		status_on_success: KeyRotationStatus<T, I>,
		event_on_success: Event<T, I>,
		event_on_error: Event<T, I>,
	) -> DispatchResultWithPostInfo {
		EnsureThresholdSigned::<T, I>::ensure_origin(Into::<
			<T as pallet::Config<I>>::RuntimeOrigin,
		>::into(origin))?;

		match Self::signature_result(threshold_request_id).ready_or_else(|r| {
			log::error!(
				"Signature not found for threshold request {:?}. Request status: {:?}",
				threshold_request_id,
				r
			);
			Error::<T, I>::ThresholdSignatureUnavailable
		})? {
			Ok(_) => {
				// Now the validator pallet can use this to check for readiness.
				PendingKeyRotation::<T, I>::put(status_on_success);

				Self::deposit_event(event_on_success);

				// We don't do any more here. We wait for the validator pallet to
				// let us know when we can proceed.
			},
			Err(offenders) => Self::terminate_rotation(offenders, event_on_error),
		};
		Ok(().into())
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

	fn activate_new_key(new_agg_key: AggKeyFor<T, I>) {
		PendingKeyRotation::<T, I>::put(KeyRotationStatus::Complete);
		Self::set_key_for_epoch(CurrentEpochIndex::<T>::get().saturating_add(1), new_agg_key);
		Self::deposit_event(Event::KeyRotationCompleted);
	}

	fn set_key_for_epoch(epoch_index: EpochIndex, agg_key: AggKeyFor<T, I>) {
		Keys::<T, I>::insert(epoch_index, agg_key);
		CurrentKeyEpoch::<T, I>::put(epoch_index);
	}

	fn increment_ceremony_id() -> CeremonyId {
		CeremonyIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		})
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
	fn try_successful_origin() -> Result<<T as Config<I>>::RuntimeOrigin, ()> {
		Ok(Origin::<T, I>(Default::default()).into())
	}
}

impl<T, I: 'static> cf_traits::ThresholdSigner<T::TargetChainCrypto> for Pallet<T, I>
where
	T: Config<I>,
{
	type Error = Error<T, I>;
	type Callback = <T as Config<I>>::ThresholdCallable;
	type ValidatorId = T::ValidatorId;

	fn request_signature(payload: PayloadFor<T, I>) -> RequestId {
		let request_type = Self::active_epoch_key().defensive_map_or_else(
			|| RequestType::SpecificKey(Default::default(), Default::default()),
			|EpochKey { key, epoch_index, .. }| RequestType::SpecificKey(key, epoch_index),
		);

		Self::inner_request_signature(payload, request_type)
	}

	fn register_callback(
		request_id: RequestId,
		on_signature_ready: Self::Callback,
	) -> Result<(), Self::Error> {
		ensure!(
			matches!(Signature::<T, I>::get(request_id), AsyncResult::Pending),
			Error::<T, I>::InvalidRequestId
		);
		RequestCallback::<T, I>::insert(request_id, on_signature_ready);
		Ok(())
	}

	fn signature_result(request_id: RequestId) -> cf_traits::AsyncResult<SignatureResultFor<T, I>> {
		Signature::<T, I>::take(request_id)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn insert_signature(
		request_id: RequestId,
		signature: <T::TargetChainCrypto as ChainCrypto>::ThresholdSignature,
	) {
		Signature::<T, I>::insert(request_id, AsyncResult::Ready(Ok(signature)));
	}
}

impl<T: Config<I>, I: 'static> KeyProvider<T::TargetChainCrypto> for Pallet<T, I> {
	fn active_epoch_key() -> Option<EpochKey<<T::TargetChainCrypto as ChainCrypto>::AggKey>> {
		CurrentKeyEpoch::<T, I>::get().map(|current_key_epoch| {
			EpochKey {
				key: Keys::<T, I>::get(current_key_epoch)
					.expect("Key must exist if CurrentKeyEpoch exists since they get set at the same place: set_key_for_epoch()"),
				epoch_index: current_key_epoch,
			}
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_key(key: <T::TargetChainCrypto as ChainCrypto>::AggKey, epoch: EpochIndex) {
		Keys::<T, I>::insert(epoch, key);
	}
}

/// Takes three arguments: a pattern, a variable expression and an error literal.
///
/// If the variable matches the pattern, returns it, otherwise returns an error. The pattern may
/// optionally have an expression attached to process and return inner arguments.
///
/// ## Example
///
/// let x = ensure_variant!(Some(..), optional_value, Error::<T>::ValueIsNone);
///
/// let 2x = ensure_variant!(Some(x) => { 2 * x }, optional_value, Error::<T>::ValueIsNone);
#[macro_export]
macro_rules! ensure_variant {
	( $variant:pat => $varexp:expr, $var:expr, $err:expr $(,)? ) => {
		if let $variant = $var {
			$varexp
		} else {
			frame_support::fail!($err)
		}
	};
	( $variant:pat, $var:expr, $err:expr $(,)? ) => {
		ensure_variant!($variant => { $var }, $var, $err)
	};
}
