#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{Chain, ChainAbi, ChainCrypto, SetAggKeyWithAggKey};
use cf_primitives::{
	AuthorityCount, CeremonyId, EpochIndex, ThresholdSignatureRequestId, GENESIS_EPOCH,
};
use cf_runtime_utilities::{EnumVariant, StorageDecodeVariant};
use cf_traits::{
	offence_reporting::OffenceReporter, AsyncResult, Broadcaster, CeremonyIdProvider, Chainflip,
	CurrentEpochIndex, EpochKey, KeyProvider, KeyState, Slashing, SystemStateManager,
	ThresholdSigner, VaultKeyWitnessedHandler, VaultRotator, VaultStatus, VaultTransitionHandler,
};
use frame_support::{pallet_prelude::*, traits::StorageVersion};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::{One, Saturating};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	iter::Iterator,
	prelude::*,
};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod vault_rotator;

mod keygen_response_status;

use keygen_response_status::KeygenResponseStatus;
pub mod weights;
pub use weights::WeightInfo;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

const KEYGEN_CEREMONY_RESPONSE_TIMEOUT_BLOCKS_DEFAULT: u32 = 90;

pub type PayloadFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::Payload;
pub type KeygenOutcomeFor<T, I = ()> =
	Result<AggKeyFor<T, I>, BTreeSet<<T as Chainflip>::ValidatorId>>;
pub type AggKeyFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::AggKey;
pub type ChainBlockNumberFor<T, I = ()> = <<T as Config<I>>::Chain as Chain>::ChainBlockNumber;
pub type TransactionIdFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::TransactionId;
pub type ThresholdSignatureFor<T, I = ()> =
	<<T as Config<I>>::Chain as ChainCrypto>::ThresholdSignature;

/// The current status of a vault rotation.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, EnumVariant)]
#[scale_info(skip_type_params(T, I))]
pub enum VaultRotationStatus<T: Config<I>, I: 'static = ()> {
	/// We are waiting for nodes to generate a new aggregate key.
	AwaitingKeygen {
		keygen_ceremony_id: CeremonyId,
		keygen_participants: BTreeSet<T::ValidatorId>,
		epoch_index: EpochIndex,
		response_status: KeygenResponseStatus<T, I>,
	},
	/// We are waiting for the nodes who generated the new key to complete a signing ceremony to
	/// verify the new key.
	AwaitingKeygenVerification { new_public_key: AggKeyFor<T, I> },
	/// Keygen verification is complete for key
	KeygenVerificationComplete { new_public_key: AggKeyFor<T, I> },
	/// We are waiting for the key to be updated on the contract, and witnessed by the network.
	AwaitingRotation { new_public_key: AggKeyFor<T, I> },
	/// The key has been successfully updated on the contract.
	Complete { tx_id: TransactionIdFor<T, I> },
	/// The rotation has failed at one of the above stages.
	Failed { offenders: BTreeSet<T::ValidatorId> },
}

/// A single vault.
#[derive(Default, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub struct Vault<T: ChainAbi> {
	/// The vault's public key.
	pub public_key: T::AggKey,
	/// The first active block for this vault
	pub active_from_block: T::ChainBlockNumber,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	FailedKeygen,
}

#[derive(Encode, Decode, TypeInfo)]
pub struct VaultEpochAndState {
	pub epoch_index: EpochIndex,
	pub key_state: KeyState,
}

#[frame_support::pallet]
pub mod pallet {

	use cf_traits::{AccountRoleRegistry, ThresholdSigner, VaultTransitionHandler};
	use sp_runtime::Percent;

	use super::*;

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// The event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;

		/// Ensure that only threshold signature consensus can trigger a key_verification success
		type EnsureThresholdSigned: EnsureOrigin<Self::RuntimeOrigin>;

		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;

		/// Offences supported in this runtime.
		type Offence: From<PalletOffence>;

		/// The chain that is managed by this vault must implement the api types.
		type Chain: ChainAbi;

		/// The supported api calls for the chain.
		type SetAggKeyWithAggKey: SetAggKeyWithAggKey<Self::Chain>;

		type VaultTransitionHandler: VaultTransitionHandler<Self::Chain>;

		/// The pallet dispatches calls, so it depends on the runtime's aggregated Call type.
		type RuntimeCall: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::RuntimeCall>;

		type ThresholdSigner: ThresholdSigner<
			Self::Chain,
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

		type Slasher: Slashing<AccountId = Self::ValidatorId, BlockNumber = Self::BlockNumber>;

		/// Ceremony Id source for keygen ceremonies.
		type CeremonyIdProvider: CeremonyIdProvider;

		// A trait which allows us to put the chain into maintenance mode.
		type SystemStateManager: SystemStateManager;

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

			// Check if we need to finalize keygen
			if let Some(VaultRotationStatus::<T, I>::AwaitingKeygen {
				keygen_ceremony_id,
				keygen_participants,
				epoch_index,
				response_status,
			}) = PendingVaultRotation::<T, I>::get()
			{
				let remaining_candidate_count = response_status.remaining_candidate_count();
				if remaining_candidate_count == 0 {
					log::debug!("All keygen candidates have reported, resolving outcome...");
				} else if current_block.saturating_sub(KeygenResolutionPendingSince::<T, I>::get()) >=
					KeygenResponseTimeout::<T, I>::get()
				{
					log::debug!(
						"Keygen response timeout has elapsed, attempting to resolve outcome..."
					);
					Self::deposit_event(Event::<T, I>::KeygenResponseTimeout(keygen_ceremony_id));
				} else {
					return weight
				};

				let candidate_count = response_status.candidate_count();
				match response_status.resolve_keygen_outcome() {
					Ok(new_public_key) => {
						debug_assert_eq!(
							remaining_candidate_count, 0,
							"Can't have success unless all candidates responded"
						);
						weight += T::WeightInfo::on_initialize_success();
						Self::deposit_event(Event::KeygenSuccess(keygen_ceremony_id));
						Self::trigger_keygen_verification(
							keygen_ceremony_id,
							new_public_key,
							epoch_index,
							keygen_participants,
						);
					},
					Err(offenders) => {
						weight += T::WeightInfo::on_initialize_failure(offenders.len() as u32);
						Self::terminate_keygen_procedure(
							&if (offenders.len() as AuthorityCount) <
								utilities::failure_threshold_from_share_count(candidate_count)
							{
								offenders.into_iter().collect::<Vec<_>>()
							} else {
								Vec::default()
							},
							Event::KeygenFailure(keygen_ceremony_id),
						);
					},
				}
				KeygenResolutionPendingSince::<T, I>::kill();
			}

			weight
		}

		fn on_runtime_upgrade() -> Weight {
			// For new pallet instances, genesis items need to be set.
			if !KeygenResponseTimeout::<T, I>::exists() {
				KeygenResponseTimeout::<T, I>::set(
					KEYGEN_CEREMONY_RESPONSE_TIMEOUT_BLOCKS_DEFAULT.into(),
				);
			}
			Weight::zero()
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
	pub type CurrentVaultEpochAndState<T: Config<I>, I: 'static = ()> =
		StorageValue<_, VaultEpochAndState>;

	/// Vault rotation statuses for the current epoch rotation.
	#[pallet::storage]
	#[pallet::getter(fn pending_vault_rotations)]
	pub type PendingVaultRotation<T: Config<I>, I: 'static = ()> =
		StorageValue<_, VaultRotationStatus<T, I>>;

	/// The voters who voted for success for a particular agg key rotation
	#[pallet::storage]
	#[pallet::getter(fn success_voters)]
	pub type SuccessVoters<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Identity, AggKeyFor<T, I>, Vec<T::ValidatorId>, ValueQuery>;

	/// The voters who voted for failure for a particular agg key rotation
	#[pallet::storage]
	#[pallet::getter(fn failure_voters)]
	pub type FailureVoters<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

	/// The block since which we have been waiting for keygen to be resolved.
	#[pallet::storage]
	#[pallet::getter(fn keygen_resolution_pending_since)]
	pub(super) type KeygenResolutionPendingSince<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::storage]
	pub(super) type KeygenResponseTimeout<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// The % amoount of the bond that is slashed for an agreed reported party
	/// (2/3 must agree the node was an offender) on keygen failure.
	#[pallet::storage]
	pub(super) type KeygenSlashRate<T, I = ()> = StorageValue<_, Percent, ValueQuery>;

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
		/// The vault for the request has rotated
		VaultRotationCompleted,
		/// The vault's key has been rotated externally \[new_public_key\]
		VaultRotatedExternally(<T::Chain as ChainCrypto>::AggKey),
		/// A keygen participant has reported that keygen was successful \[validator_id\]
		KeygenSuccessReported(T::ValidatorId),
		/// A keygen participant has reported that keygen has failed \[validator_id\]
		KeygenFailureReported(T::ValidatorId),
		/// Keygen was successful \[ceremony_id\]
		KeygenSuccess(CeremonyId),
		/// The new key was successfully used to sign.
		KeygenVerificationSuccess { agg_key: <T::Chain as ChainCrypto>::AggKey },
		/// Verification of the new key has failed.
		KeygenVerificationFailure { keygen_ceremony_id: CeremonyId },
		/// Keygen has failed \[ceremony_id\]
		KeygenFailure(CeremonyId),
		/// Keygen response timeout has occurred \[ceremony_id\]
		KeygenResponseTimeout(CeremonyId),
		/// Keygen response timeout was updated \[new_timeout\]
		KeygenResponseTimeoutUpdated { new_timeout: BlockNumberFor<T> },
		/// The new key has been generated, we must activate the new key on the external
		/// chain via governance.
		AwaitingGovernanceActivation { new_public_key: <T::Chain as ChainCrypto>::AggKey },
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
		#[pallet::weight(T::WeightInfo::report_keygen_outcome())]
		pub fn report_keygen_outcome(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			reported_outcome: KeygenOutcomeFor<T, I>,
		) -> DispatchResultWithPostInfo {
			let reporter = T::AccountRoleRegistry::ensure_validator(origin)?.into();

			// -- Validity checks.

			// There is a rotation happening.
			let mut rotation =
				PendingVaultRotation::<T, I>::get().ok_or(Error::<T, I>::NoActiveRotation)?;

			// Keygen is in progress, pull out the details.
			let (pending_ceremony_id, keygen_status) = ensure_variant!(
				VaultRotationStatus::<T, I>::AwaitingKeygen {
					keygen_ceremony_id, ref mut response_status, ..
				} => (keygen_ceremony_id, response_status),
				rotation,
				Error::<T, I>::InvalidRotationStatus,
			);
			// Make sure the ceremony id matches
			ensure!(pending_ceremony_id == ceremony_id, Error::<T, I>::InvalidCeremonyId);
			ensure!(
				keygen_status.remaining_candidates().contains(&reporter),
				Error::<T, I>::InvalidRespondent
			);

			// -- Tally the votes.

			match reported_outcome {
				Ok(key) => {
					keygen_status.add_success_vote(&reporter, key);
					Self::deposit_event(Event::<T, I>::KeygenSuccessReported(reporter));
				},
				Err(blamed) => {
					keygen_status.add_failure_vote(&reporter, blamed);
					Self::deposit_event(Event::<T, I>::KeygenFailureReported(reporter));
				},
			}

			PendingVaultRotation::<T, I>::put(rotation);

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
		#[pallet::weight(T::WeightInfo::on_keygen_verification_result())]
		pub fn on_keygen_verification_result(
			origin: OriginFor<T>,
			keygen_ceremony_id: CeremonyId,
			threshold_request_id: ThresholdSignatureRequestId,
			new_public_key: AggKeyFor<T, I>,
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
					PendingVaultRotation::<T, I>::put(
						VaultRotationStatus::<T, I>::KeygenVerificationComplete { new_public_key },
					);

					Self::deposit_event(Event::KeygenVerificationSuccess {
						agg_key: new_public_key,
					});

					// We don't do any more here. We wait for the validator pallet to
					// let us know when we can start the external rotation.
				},
				Err(offenders) => Self::terminate_keygen_procedure(
					&offenders[..],
					Event::KeygenVerificationFailure { keygen_ceremony_id },
				),
			};
			Ok(().into())
		}

		/// A vault rotation event has been witnessed, we update the vault with the new key.
		///
		/// ## Events
		///
		/// - [VaultRotationCompleted](Event::VaultRotationCompleted)
		///
		/// ## Errors
		///
		/// - [NoActiveRotation](Error::NoActiveRotation)
		/// - [InvalidRotationStatus](Error::InvalidRotationStatus)
		///
		/// ## Dependencies
		///
		/// - [Epoch Info Trait](EpochInfo)
		#[pallet::weight(T::WeightInfo::vault_key_rotated())]
		pub fn vault_key_rotated(
			origin: OriginFor<T>,
			new_public_key: AggKeyFor<T, I>,
			block_number: ChainBlockNumberFor<T, I>,

			// This field is primarily required to ensure the witness calls are unique per
			// transaction (on the external chain)
			tx_id: TransactionIdFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;

			Self::on_new_key_activated(new_public_key, block_number, tx_id)
		}

		/// The vault's key has been updated externally, outside of the rotation
		/// cycle. This is an unexpected event as far as our chain is concerned, and
		/// the only thing we can do is to halt and wait for further governance
		/// intervention.
		///
		/// ## Events
		///
		/// - [VaultRotatedExternally](Event::VaultRotatedExternally)
		/// - [SystemStateHasBeenChanged](Event::SystemStateHasBeenChanged)
		///
		/// ## Errors
		///
		/// - None
		///
		/// ## Dependencies
		///
		/// - [Epoch Info Trait](EpochInfo)
		#[pallet::weight(T::WeightInfo::vault_key_rotated_externally())]
		pub fn vault_key_rotated_externally(
			origin: OriginFor<T>,
			new_public_key: AggKeyFor<T, I>,
			block_number: ChainBlockNumberFor<T, I>,
			_tx_id: TransactionIdFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;

			Self::set_next_vault(new_public_key, block_number);

			T::SystemStateManager::activate_maintenance_mode();

			Pallet::<T, I>::deposit_event(Event::VaultRotatedExternally(new_public_key));

			Ok(().into())
		}

		#[pallet::weight(T::WeightInfo::set_keygen_response_timeout())]
		pub fn set_keygen_response_timeout(
			origin: OriginFor<T>,
			new_timeout: T::BlockNumber,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			if new_timeout != KeygenResponseTimeout::<T, I>::get() {
				KeygenResponseTimeout::<T, I>::put(new_timeout);
				Pallet::<T, I>::deposit_event(Event::KeygenResponseTimeoutUpdated { new_timeout });
			}

			Ok(().into())
		}

		#[pallet::weight(T::WeightInfo::set_keygen_response_timeout())]
		pub fn set_keygen_slash_rate(
			origin: OriginFor<T>,
			percent_of_stake: Percent,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			KeygenSlashRate::<T, I>::put(percent_of_stake);

			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		/// The provided Vec must be convertible to the chain's AggKey.
		///
		/// GenesisConfig members require `Serialize` and `Deserialize` which isn't
		/// implemented for the AggKey type, hence we use Vec<u8> and covert during genesis.
		pub vault_key: Option<Vec<u8>>,
		pub deployment_block: ChainBlockNumberFor<T, I>,
		pub keygen_response_timeout: BlockNumberFor<T>,
	}

	#[cfg(feature = "std")]
	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			use sp_runtime::traits::Zero;
			Self {
				vault_key: None,
				deployment_block: Zero::zero(),
				keygen_response_timeout: KEYGEN_CEREMONY_RESPONSE_TIMEOUT_BLOCKS_DEFAULT.into(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> GenesisBuild<T, I> for GenesisConfig<T, I> {
		fn build(&self) {
			if let Some(vault_key) = self.vault_key.clone() {
				Pallet::<T, I>::set_vault_for_epoch(
					VaultEpochAndState {
						epoch_index: GENESIS_EPOCH,
						key_state: KeyState::Unlocked,
					},
					AggKeyFor::<T, I>::try_from(vault_key)
						// Note: Can't use expect() here without some type shenanigans, but would
						// give clearer error messages.
						.unwrap_or_else(|_| {
							panic!("Can't build genesis without a valid vault key.")
						}),
					self.deployment_block,
				);
			} else {
				log::info!("No genesis vault key configured for {}.", Pallet::<T, I>::name());
			}

			KeygenResponseTimeout::<T, I>::put(self.keygen_response_timeout);
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn set_next_vault(
		new_public_key: AggKeyFor<T, I>,
		rotated_at_block_number: ChainBlockNumberFor<T, I>,
	) {
		Self::set_vault_for_next_epoch(new_public_key, rotated_at_block_number);
		T::VaultTransitionHandler::on_new_vault();
	}

	fn set_vault_for_next_epoch(
		new_public_key: AggKeyFor<T, I>,
		rotated_at_block_number: ChainBlockNumberFor<T, I>,
	) {
		Self::set_vault_for_epoch(
			VaultEpochAndState {
				epoch_index: CurrentEpochIndex::<T>::get().saturating_add(1),
				key_state: KeyState::Unlocked,
			},
			new_public_key,
			rotated_at_block_number.saturating_add(ChainBlockNumberFor::<T, I>::one()),
		);
	}

	fn set_vault_for_epoch(
		current_vault_and_state: VaultEpochAndState,
		new_public_key: AggKeyFor<T, I>,
		active_from_block: ChainBlockNumberFor<T, I>,
	) {
		Vaults::<T, I>::insert(
			current_vault_and_state.epoch_index,
			Vault { public_key: new_public_key, active_from_block },
		);
		CurrentVaultEpochAndState::<T, I>::put(current_vault_and_state);
	}

	// Once we've successfully generated the key, we want to do a signing ceremony to verify that
	// the key is useable
	fn trigger_keygen_verification(
		keygen_ceremony_id: CeremonyId,
		new_public_key: AggKeyFor<T, I>,
		epoch_index: EpochIndex,
		participants: BTreeSet<T::ValidatorId>,
	) -> ThresholdSignatureRequestId {
		let request_id = T::ThresholdSigner::request_keygen_verification_signature(
			T::Chain::agg_key_to_payload(new_public_key),
			T::Chain::agg_key_to_key_id(new_public_key, epoch_index),
			participants,
		);
		T::ThresholdSigner::register_callback(request_id, {
			Call::on_keygen_verification_result {
				keygen_ceremony_id,
				threshold_request_id: request_id,
				new_public_key,
			}
			.into()
		})
		.unwrap_or_else(|e| {
			log::error!(
				"Unable to register threshold signature callback. This should not be possible. Error: '{:?}'",
				e.into()
			);
		});

		PendingVaultRotation::<T, I>::put(
			VaultRotationStatus::<T, I>::AwaitingKeygenVerification { new_public_key },
		);

		request_id
	}

	fn terminate_keygen_procedure(offenders: &[T::ValidatorId], event: Event<T, I>) {
		T::OffenceReporter::report_many(PalletOffence::FailedKeygen, offenders);
		for offender in offenders {
			T::Slasher::slash_stake(offender, KeygenSlashRate::<T, I>::get());
		}
		PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Failed {
			offenders: offenders.iter().cloned().collect(),
		});
		Self::deposit_event(event);
	}
}

impl<T: Config<I>, I: 'static> KeyProvider<T::Chain> for Pallet<T, I> {
	fn current_epoch_key() -> Option<EpochKey<<T::Chain as ChainCrypto>::AggKey>> {
		CurrentVaultEpochAndState::<T, I>::get().map(|current_vault_epoch_and_state| {
			EpochKey {
				key: Vaults::<T, I>::get(current_vault_epoch_and_state.epoch_index)
					.expect("Key must exist if CurrentVaultEpochAndState exists since they get set at the same place: set_next_vault()").public_key,
				epoch_index: current_vault_epoch_and_state.epoch_index,
				key_state: current_vault_epoch_and_state.key_state,
			}
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_key(key: <T::Chain as ChainCrypto>::AggKey) {
		Vaults::<T, I>::insert(
			CurrentEpochIndex::<T>::get(),
			Vault { public_key: key, active_from_block: ChainBlockNumberFor::<T, I>::from(0u32) },
		);
	}
}

impl<T: Config<I>, I: 'static> VaultKeyWitnessedHandler<T::Chain> for Pallet<T, I> {
	fn on_new_key_activated(
		new_public_key: AggKeyFor<T, I>,
		block_number: ChainBlockNumberFor<T, I>,
		tx_id: TransactionIdFor<T, I>,
	) -> DispatchResultWithPostInfo {
		let rotation =
			PendingVaultRotation::<T, I>::get().ok_or(Error::<T, I>::NoActiveRotation)?;

		let expected_new_key = ensure_variant!(
			VaultRotationStatus::<T, I>::AwaitingRotation { new_public_key } => new_public_key,
			rotation,
			Error::<T, I>::InvalidRotationStatus
		);

		// The expected new key should match the new key witnessed
		debug_assert_eq!(new_public_key, expected_new_key);

		PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Complete { tx_id });

		// Unlock the key that was used to authorise the activation.
		// TODO: use broadcast callbacks for this.
		CurrentVaultEpochAndState::<T, I>::try_mutate(|state: &mut Option<VaultEpochAndState>| {
			state
				.as_mut()
				.map(|VaultEpochAndState { key_state, .. }| key_state.unlock())
				.ok_or(())
		})
		.expect("CurrentVaultEpochAndState must exist for the locked key, otherwise we couldn't have signed.");

		Self::set_next_vault(new_public_key, block_number);

		Pallet::<T, I>::deposit_event(Event::VaultRotationCompleted);

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
