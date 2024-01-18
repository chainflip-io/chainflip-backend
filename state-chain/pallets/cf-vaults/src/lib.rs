#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{Chain, ChainCrypto, SetAggKeyWithAggKey};
use cf_primitives::{
	AuthorityCount, CeremonyId, EpochIndex, FlipBalance, ThresholdSignatureRequestId,
};
use cf_runtime_utilities::{EnumVariant, StorageDecodeVariant};
use cf_traits::{
	offence_reporting::OffenceReporter, AccountRoleRegistry, AsyncResult, Broadcaster,
	CfeMultisigRequest, Chainflip, CurrentEpochIndex, EpochKey, GetBlockHeight, KeyProvider,
	SafeMode, SetSafeMode, Slashing, ThresholdSigner, VaultKeyWitnessedHandler, VaultRotator,
	VaultStatus,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::traits::{One, Saturating},
	traits::StorageVersion,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	iter::Iterator,
	marker,
	prelude::*,
};

mod benchmarking;

mod vault_rotator;

mod response_status;

use response_status::ResponseStatus;

pub mod weights;
pub use weights::WeightInfo;
pub mod migrations;
mod mock;
mod tests;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(3);

const KEYGEN_CEREMONY_RESPONSE_TIMEOUT_BLOCKS_DEFAULT: u32 = 90;

pub type PayloadFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::Payload;
pub type KeygenOutcomeFor<T, I = ()> =
	Result<AggKeyFor<T, I>, BTreeSet<<T as Chainflip>::ValidatorId>>;
pub type AggKeyFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey;
pub type ChainBlockNumberFor<T, I = ()> = <<T as Config<I>>::Chain as Chain>::ChainBlockNumber;
pub type TransactionInIdFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId;
pub type TransactionOutIdFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId;
pub type ThresholdSignatureFor<T, I = ()> =
	<<<T as Config<I>>::Chain as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature;

pub type KeygenResponseStatus<T, I> =
	ResponseStatus<T, KeygenSuccessVoters<T, I>, KeygenFailureVoters<T, I>, I>;

pub type KeyHandoverResponseStatus<T, I> =
	ResponseStatus<T, KeyHandoverSuccessVoters<T, I>, KeyHandoverFailureVoters<T, I>, I>;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Copy, Clone, PartialEq, Eq, RuntimeDebug)]
#[scale_info(skip_type_params(I))]
pub struct PalletSafeMode<I: 'static> {
	pub slashing_enabled: bool,
	#[doc(hidden)]
	#[codec(skip)]
	_phantom: marker::PhantomData<I>,
}

impl<I: 'static> SafeMode for PalletSafeMode<I> {
	const CODE_RED: Self =
		PalletSafeMode { slashing_enabled: false, _phantom: marker::PhantomData };
	const CODE_GREEN: Self =
		PalletSafeMode { slashing_enabled: true, _phantom: marker::PhantomData };
}

/// The current status of a vault rotation.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebugNoBound, EnumVariant)]
#[scale_info(skip_type_params(T, I))]
pub enum VaultRotationStatus<T: Config<I>, I: 'static = ()> {
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

impl<T: Config<I>, I: 'static> cf_traits::CeremonyIdProvider for Pallet<T, I> {
	fn increment_ceremony_id() -> CeremonyId {
		CeremonyIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		})
	}
}

/// A single vault.
#[derive(Default, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub struct Vault<T: Chain> {
	/// The vault's public key.
	pub public_key: <<T as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	/// The first active block for this vault
	pub active_from_block: T::ChainBlockNumber,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	FailedKeygen,
	FailedKeyHandover,
}

#[frame_support::pallet]
pub mod pallet {

	use super::*;

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// The event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Ensure that only threshold signature consensus can trigger a key_verification success
		type EnsureThresholdSigned: EnsureOrigin<Self::RuntimeOrigin>;

		/// Offences supported in this runtime.
		type Offence: From<PalletOffence>;

		/// The chain that is managed by this vault must implement the api types.
		type Chain: Chain;

		/// The supported api calls for the chain.
		type SetAggKeyWithAggKey: SetAggKeyWithAggKey<
			<<Self as pallet::Config<I>>::Chain as Chain>::ChainCrypto,
		>;

		/// The pallet dispatches calls, so it depends on the runtime's aggregated Call type.
		type RuntimeCall: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::RuntimeCall>;

		type ThresholdSigner: ThresholdSigner<
			<Self::Chain as Chain>::ChainCrypto,
			Callback = <Self as Config<I>>::RuntimeCall,
			ValidatorId = Self::ValidatorId,
		>;

		/// A broadcaster for the target chain.
		type Broadcaster: Broadcaster<Self::Chain, ApiCall = Self::SetAggKeyWithAggKey>;

		/// For reporting misbehaviour
		type OffenceReporter: OffenceReporter<
			ValidatorId = Self::ValidatorId,
			Offence = Self::Offence,
		>;

		type Slasher: Slashing<AccountId = Self::ValidatorId, BlockNumber = BlockNumberFor<Self>>;

		/// For activating Safe mode: CODE RED for the chain.
		type SafeMode: Get<PalletSafeMode<I>> + SafeMode + SetSafeMode<Self::SafeMode>;

		type ChainTracking: GetBlockHeight<Self::Chain>;

		type CfeMultisigRequest: CfeMultisigRequest<Self, <Self::Chain as Chain>::ChainCrypto>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> frame_support::weights::Weight {
			let mut weight = T::DbWeight::get().reads(1);

			// We don't need self, we can get our own data.
			if Self::status() != AsyncResult::Pending {
				return weight
			}

			match PendingVaultRotation::<T, I>::get() {
				Some(VaultRotationStatus::<T, I>::AwaitingKeygen {
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
							Self::terminate_rotation(offenders, Event::KeygenFailure(ceremony_id));
						},
					);
				},
				Some(VaultRotationStatus::<T, I>::AwaitingKeyHandover {
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
								.expect("key must exist during handover")
								.key;

							if <<T::Chain as Chain>::ChainCrypto as ChainCrypto>::handover_key_matches(
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
								VaultRotationStatus::<T, I>::AwaitingKeyHandoverVerification {
									new_public_key: reported_new_public_key,
								},
							);
						},
						|offenders| {
							T::OffenceReporter::report_many(
								PalletOffence::FailedKeyHandover,
								offenders.clone(),
							);
							PendingVaultRotation::<T, I>::put(
								VaultRotationStatus::<T, I>::KeyHandoverFailed {
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

			weight
		}
	}

	/// A map of vaults by epoch.
	#[pallet::storage]
	#[pallet::getter(fn vaults)]
	pub type Vaults<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, EpochIndex, Vault<T::Chain>>;

	/// The epoch whose authorities control the current vault key.
	#[pallet::storage]
	#[pallet::getter(fn current_keyholders_epoch)]
	pub type CurrentVaultEpoch<T: Config<I>, I: 'static = ()> = StorageValue<_, EpochIndex>;

	/// Vault rotation statuses for the current epoch rotation.
	#[pallet::storage]
	#[pallet::getter(fn pending_vault_rotations)]
	pub type PendingVaultRotation<T: Config<I>, I: 'static = ()> =
		StorageValue<_, VaultRotationStatus<T, I>>;

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
	pub(super) type KeygenResponseTimeout<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// The amount of FLIP that is slashed for an agreed reported party expressed in Flipperinos
	/// (2/3 must agree the node was an offender) on keygen failure.
	#[pallet::storage]
	pub(super) type KeygenSlashAmount<T, I = ()> = StorageValue<_, FlipBalance, ValueQuery>;

	/// Counter for generating unique ceremony ids.
	#[pallet::storage]
	#[pallet::getter(fn ceremony_id_counter)]
	pub type CeremonyIdCounter<T: Config<I>, I: 'static = ()> =
		StorageValue<_, CeremonyId, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
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
			key_to_share: <<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
			sharing_participants: BTreeSet<T::ValidatorId>,
			receiving_participants: BTreeSet<T::ValidatorId>,
			/// The freshly generated key for the next epoch.
			new_key: <<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
			/// The epoch index for which the key is being handed over.
			to_epoch: EpochIndex,
		},
		/// The vault for the request has rotated
		VaultRotationCompleted,
		/// The vault's key has been rotated externally \[new_public_key\]
		VaultRotatedExternally(<<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey),
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
			agg_key: <<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
		},
		KeyHandoverVerificationSuccess {
			agg_key: <<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
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
		/// The new key has been generated, we must activate the new key on the external
		/// chain via governance.
		AwaitingGovernanceActivation {
			new_public_key: <<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey,
		},
		/// Key handover has failed
		KeyHandoverFailure {
			ceremony_id: CeremonyId,
		},
		/// The vault rotation has been aborted early.
		VaultRotationAborted,
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// An invalid ceremony id
		InvalidCeremonyId,
		/// There is currently no vault rotation in progress for this chain.
		NoActiveRotation,
		/// The requested call is invalid based on the current rotation state.
		InvalidRotationStatus,
		/// An authority sent a response for a ceremony in which they weren't involved, or to which
		/// they have already submitted a response.
		InvalidRespondent,
		/// There is no threshold signature available
		ThresholdSignatureUnavailable,
	}

	macro_rules! handle_key_ceremony_report {
		($origin:expr, $ceremony_id:expr, $reported_outcome:expr, $variant:path, $success_event:expr, $failure_event:expr) => {

			let reporter = T::AccountRoleRegistry::ensure_validator($origin)?.into();

			// There is a rotation happening.
			let mut rotation =
				PendingVaultRotation::<T, I>::get().ok_or(Error::<T, I>::NoActiveRotation)?;

			// Keygen is in progress, pull out the details.
			let (pending_ceremony_id, response_status) = ensure_variant!(
				$variant {
					ceremony_id, ref mut response_status, ..
				} => (ceremony_id, response_status),
				rotation,
				Error::<T, I>::InvalidRotationStatus,
			);

			// Make sure the ceremony id matches
			ensure!(pending_ceremony_id == $ceremony_id, Error::<T, I>::InvalidCeremonyId);
			ensure!(
				response_status.remaining_candidates().contains(&reporter),
				Error::<T, I>::InvalidRespondent
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

			PendingVaultRotation::<T, I>::put(rotation);
		};
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
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
		/// - [InvalidCeremonyId](Error::InvalidCeremonyId)
		///
		/// ## Dependencies
		///
		/// - [Threshold Signer Trait](ThresholdSigner)
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::report_keygen_outcome())]
		pub fn report_keygen_outcome(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			reported_outcome: KeygenOutcomeFor<T, I>,
		) -> DispatchResultWithPostInfo {
			handle_key_ceremony_report!(
				origin,
				ceremony_id,
				reported_outcome,
				VaultRotationStatus::<T, I>::AwaitingKeygen,
				Event::KeygenSuccessReported,
				Event::KeygenFailureReported
			);

			Ok(().into())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::report_keygen_outcome())]
		pub fn report_key_handover_outcome(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			reported_outcome: KeygenOutcomeFor<T, I>,
		) -> DispatchResultWithPostInfo {
			handle_key_ceremony_report!(
				origin,
				ceremony_id,
				reported_outcome,
				VaultRotationStatus::<T, I>::AwaitingKeyHandover,
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
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::on_keygen_verification_result())]
		pub fn on_keygen_verification_result(
			origin: OriginFor<T>,
			keygen_ceremony_id: CeremonyId,
			threshold_request_id: ThresholdSignatureRequestId,
			new_public_key: AggKeyFor<T, I>,
		) -> DispatchResultWithPostInfo {
			Self::on_key_verification_result(
				origin,
				threshold_request_id,
				VaultRotationStatus::<T, I>::KeygenVerificationComplete { new_public_key },
				Event::KeygenVerificationSuccess { agg_key: new_public_key },
				Event::KeygenVerificationFailure { keygen_ceremony_id },
			)
		}

		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::on_keygen_verification_result())]
		pub fn on_handover_verification_result(
			origin: OriginFor<T>,
			handover_ceremony_id: CeremonyId,
			threshold_request_id: ThresholdSignatureRequestId,
			new_public_key: AggKeyFor<T, I>,
		) -> DispatchResultWithPostInfo {
			Self::on_key_verification_result(
				origin,
				threshold_request_id,
				VaultRotationStatus::<T, I>::KeyHandoverComplete { new_public_key },
				Event::KeyHandoverVerificationSuccess { agg_key: new_public_key },
				Event::KeyHandoverVerificationFailure { handover_ceremony_id },
			)
		}

		/// Deprecated! This extrinsic does nothing
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::zero())]
		pub fn vault_key_rotated(
			origin: OriginFor<T>,
			_block_number: ChainBlockNumberFor<T, I>,

			// This field is primarily required to ensure the witness calls are unique per
			// transaction (on the external chain)
			_tx_id: TransactionInIdFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;
			Ok(().into())
		}

		/// The vault's key has been updated externally, outside of the rotation
		/// cycle. This is an unexpected event as far as our chain is concerned, and
		/// the only thing we can do is to halt and wait for further governance
		/// intervention.
		///
		/// This function activates CODE RED for the runtime's safe mode, which halts
		/// many functions on the statechain.
		///
		/// ## Events
		///
		/// - [VaultRotatedExternally](Event::VaultRotatedExternally)
		///
		/// ## Errors
		///
		/// - None
		///
		/// ## Dependencies
		///
		/// - [Epoch Info Trait](EpochInfo)
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::vault_key_rotated_externally())]
		pub fn vault_key_rotated_externally(
			origin: OriginFor<T>,
			new_public_key: AggKeyFor<T, I>,
			block_number: ChainBlockNumberFor<T, I>,
			_tx_id: TransactionInIdFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;

			Self::activate_new_key(new_public_key, block_number);

			T::SafeMode::set_code_red();

			Pallet::<T, I>::deposit_event(Event::VaultRotatedExternally(new_public_key));

			Ok(().into())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::set_keygen_response_timeout())]
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

		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::set_keygen_response_timeout())]
		pub fn set_keygen_slash_amount(
			origin: OriginFor<T>,
			amount_to_slash: FlipBalance,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			KeygenSlashAmount::<T, I>::put(amount_to_slash);

			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		pub vault_key: Option<AggKeyFor<T, I>>,
		pub deployment_block: ChainBlockNumberFor<T, I>,
		pub keygen_response_timeout: BlockNumberFor<T>,
		pub amount_to_slash: FlipBalance,
	}

	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			use frame_support::sp_runtime::traits::Zero;
			Self {
				vault_key: None,
				deployment_block: Zero::zero(),
				keygen_response_timeout: KEYGEN_CEREMONY_RESPONSE_TIMEOUT_BLOCKS_DEFAULT.into(),
				amount_to_slash: 0u128,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			if let Some(vault_key) = self.vault_key {
				Pallet::<T, I>::set_vault_key_for_epoch(
					cf_primitives::GENESIS_EPOCH,
					Vault { public_key: vault_key, active_from_block: self.deployment_block },
				);
			} else {
				log::info!("No genesis vault key configured for {}.", Pallet::<T, I>::name());
			}
			KeygenSlashAmount::<T, I>::put(self.amount_to_slash);
			KeygenResponseTimeout::<T, I>::put(self.keygen_response_timeout);
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
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
				T::WeightInfo::on_initialize_success()
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
				T::WeightInfo::on_initialize_failure(offenders_len as u32)
			},
		};
		PendingSince::kill();
		weight
	}

	fn set_vault_key_for_epoch(epoch_index: EpochIndex, vault: Vault<T::Chain>) {
		Vaults::<T, I>::insert(epoch_index, vault);
		CurrentVaultEpoch::<T, I>::put(epoch_index);
	}

	// Once we've successfully generated the key, we want to do a signing ceremony to verify that
	// the key is useable
	fn trigger_keygen_verification(
		keygen_ceremony_id: CeremonyId,
		new_public_key: AggKeyFor<T, I>,
		participants: BTreeSet<T::ValidatorId>,
		new_epoch_index: EpochIndex,
	) -> ThresholdSignatureRequestId {
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
			VaultRotationStatus::<T, I>::AwaitingKeygenVerification { new_public_key },
		)
	}

	fn trigger_key_verification(
		new_agg_key: AggKeyFor<T, I>,
		participants: BTreeSet<T::ValidatorId>,
		is_handover: bool,
		next_epoch: EpochIndex,
		signature_callback_fn: impl FnOnce(ThresholdSignatureRequestId) -> <T as Config<I>>::RuntimeCall,
		status_to_set: VaultRotationStatus<T, I>,
	) -> ThresholdSignatureRequestId {
		let request_id = T::ThresholdSigner::request_verification_signature(
			<T::Chain as Chain>::ChainCrypto::agg_key_to_payload(new_agg_key, is_handover),
			participants,
			new_agg_key,
			next_epoch,
			signature_callback_fn,
		);

		PendingVaultRotation::<T, I>::put(status_to_set);

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
		PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Failed {
			offenders: offenders.into_iter().collect(),
		});
		Self::deposit_event(event);
	}

	fn on_key_verification_result(
		origin: OriginFor<T>,
		threshold_request_id: ThresholdSignatureRequestId,
		status_on_success: VaultRotationStatus<T, I>,
		event_on_success: Event<T, I>,
		event_on_error: Event<T, I>,
	) -> DispatchResultWithPostInfo {
		T::EnsureThresholdSigned::ensure_origin(origin)?;

		match T::ThresholdSigner::signature_result(threshold_request_id).ready_or_else(|r| {
			log::error!(
				"Signature not found for threshold request {:?}. Request status: {:?}",
				threshold_request_id,
				r
			);
			Error::<T, I>::ThresholdSignatureUnavailable
		})? {
			Ok(_) => {
				// Now the validator pallet can use this to check for readiness.
				PendingVaultRotation::<T, I>::put(status_on_success);

				Self::deposit_event(event_on_success);

				// We don't do any more here. We wait for the validator pallet to
				// let us know when we can proceed.
			},
			Err(offenders) => Self::terminate_rotation(offenders, event_on_error),
		};
		Ok(().into())
	}

	fn activate_new_key(new_agg_key: AggKeyFor<T, I>, block_number: ChainBlockNumberFor<T, I>) {
		PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Complete);
		Self::set_vault_key_for_epoch(
			CurrentEpochIndex::<T>::get().saturating_add(1),
			Vault {
				public_key: new_agg_key,
				active_from_block: block_number.saturating_add(One::one()),
			},
		);
		Self::deposit_event(Event::VaultRotationCompleted);
	}
}

impl<T: Config<I>, I: 'static> KeyProvider<<T::Chain as Chain>::ChainCrypto> for Pallet<T, I> {
	fn active_epoch_key(
	) -> Option<EpochKey<<<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey>> {
		CurrentVaultEpoch::<T, I>::get().map(|current_vault_epoch| {
			EpochKey {
				key: Vaults::<T, I>::get(current_vault_epoch)
					.expect("Key must exist if CurrentVaultEpoch exists since they get set at the same place: set_vault_key_for_epoch()").public_key,
				epoch_index: current_vault_epoch,
			}
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_key(key: <<T::Chain as Chain>::ChainCrypto as ChainCrypto>::AggKey, epoch: EpochIndex) {
		Vaults::<T, I>::insert(
			epoch,
			Vault { public_key: key, active_from_block: ChainBlockNumberFor::<T, I>::from(0u32) },
		);
	}
}

impl<T: Config<I>, I: 'static> VaultKeyWitnessedHandler<T::Chain> for Pallet<T, I> {
	fn on_new_key_activated(block_number: ChainBlockNumberFor<T, I>) -> DispatchResultWithPostInfo {
		let rotation =
			PendingVaultRotation::<T, I>::get().ok_or(Error::<T, I>::NoActiveRotation)?;

		let new_public_key = ensure_variant!(
			VaultRotationStatus::<T, I>::AwaitingActivation { new_public_key } => new_public_key,
			rotation,
			Error::<T, I>::InvalidRotationStatus
		);
		Self::activate_new_key(new_public_key, block_number);

		Ok(().into())
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
