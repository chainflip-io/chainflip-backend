#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{Chain, ChainAbi, ChainCrypto, SetAggKeyWithAggKey};
use cf_primitives::{AuthorityCount, CeremonyId, EpochIndex, GENESIS_EPOCH};
use cf_runtime_utilities::{EnumVariant, StorageDecodeVariant};
use cf_traits::{
	offence_reporting::OffenceReporter, AsyncResult, Broadcaster, CeremonyIdProvider, Chainflip,
	CurrentEpochIndex, EpochKey, EpochTransitionHandler, KeyProvider, KeyState, SystemStateManager,
	ThresholdSigner, VaultKeyWitnessedHandler, VaultRotator, VaultStatus, VaultTransitionHandler,
};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::{BlockNumberProvider, One, Saturating};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	iter::Iterator,
	prelude::*,
};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
const KEYGEN_CEREMONY_RESPONSE_TIMEOUT_DEFAULT: u32 = 10;

pub type PayloadFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::Payload;
pub type KeygenOutcomeFor<T, I = ()> =
	Result<AggKeyFor<T, I>, BTreeSet<<T as Chainflip>::ValidatorId>>;
pub type AggKeyFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::AggKey;
pub type ChainBlockNumberFor<T, I = ()> = <<T as Config<I>>::Chain as Chain>::ChainBlockNumber;
pub type TransactionIdFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::TransactionId;
pub type ThresholdSignatureFor<T, I = ()> =
	<<T as Config<I>>::Chain as ChainCrypto>::ThresholdSignature;

/// Tracks the current state of the keygen ceremony.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
#[scale_info(skip_type_params(T, I))]
pub struct KeygenResponseStatus<T: Config<I>, I: 'static = ()> {
	/// The total number of candidates participating in the keygen ceremony.
	candidate_count: AuthorityCount,
	/// The candidates that have yet to reply.
	remaining_candidates: BTreeSet<T::ValidatorId>,
	/// A map of new keys with the number of votes for each key.
	success_votes: BTreeMap<AggKeyFor<T, I>, AuthorityCount>,
	/// A map of the number of blame votes that each keygen participant has received.
	blame_votes: BTreeMap<T::ValidatorId, AuthorityCount>,
}

impl<T: Config<I>, I: 'static> KeygenResponseStatus<T, I> {
	pub fn new(candidates: BTreeSet<T::ValidatorId>) -> Self {
		Self {
			candidate_count: candidates.len() as AuthorityCount,
			remaining_candidates: candidates,
			success_votes: Default::default(),
			blame_votes: Default::default(),
		}
	}

	fn super_majority_threshold(&self) -> AuthorityCount {
		utilities::success_threshold_from_share_count(self.candidate_count)
	}

	fn add_success_vote(&mut self, voter: &T::ValidatorId, key: AggKeyFor<T, I>) {
		assert!(self.remaining_candidates.remove(voter));
		*self.success_votes.entry(key).or_default() += 1;

		SuccessVoters::<T, I>::append(key, voter);
	}

	fn add_failure_vote(&mut self, voter: &T::ValidatorId, blamed: BTreeSet<T::ValidatorId>) {
		assert!(self.remaining_candidates.remove(voter));
		for id in blamed {
			*self.blame_votes.entry(id).or_default() += 1
		}

		FailureVoters::<T, I>::append(voter);
	}

	/// How many candidates are we still awaiting a response from?
	fn remaining_candidate_count(&self) -> AuthorityCount {
		self.remaining_candidates.len() as AuthorityCount
	}

	/// Resolves the keygen outcome as follows:
	///
	/// If and only if *all* candidates agree on the same key, return Success.
	///
	/// Otherwise, determine unresponsive, dissenting and blamed nodes and return
	/// `Failure(unresponsive | dissenting | blamed)`
	fn resolve_keygen_outcome(self) -> KeygenOutcomeFor<T, I> {
		// If and only if *all* candidates agree on the same key, return success.
		if let Some((key, votes)) = self.success_votes.iter().next() {
			if *votes == self.candidate_count {
				// This *should* be safe since it's bounded by the number of candidates.
				// We may want to revise.
				// See https://github.com/paritytech/substrate/pull/11490
				let _ignored = SuccessVoters::<T, I>::clear(u32::MAX, None);
				return Ok(*key)
			}
		}

		let super_majority_threshold = self.super_majority_threshold() as usize;

		// We remove who we don't want to punish, and then punish the rest
		if let Some(key) = SuccessVoters::<T, I>::iter_keys().find(|key| {
			SuccessVoters::<T, I>::decode_len(key).unwrap_or_default() >= super_majority_threshold
		}) {
			SuccessVoters::<T, I>::remove(key);
		} else if FailureVoters::<T, I>::decode_len().unwrap_or_default() >=
			super_majority_threshold
		{
			FailureVoters::<T, I>::kill();
		} else {
			let _empty = SuccessVoters::<T, I>::clear(u32::MAX, None);
			FailureVoters::<T, I>::kill();
			log::warn!("Unable to determine a consensus outcome for keygen.");
		}

		Err(SuccessVoters::<T, I>::drain()
			.flat_map(|(_k, dissenters)| dissenters)
			.chain(FailureVoters::<T, I>::take())
			.chain(self.blame_votes.into_iter().filter_map(|(id, vote_count)| {
				if vote_count >= super_majority_threshold as u32 {
					Some(id)
				} else {
					None
				}
			}))
			.chain(self.remaining_candidates)
			.collect())
	}
}

/// The current status of a vault rotation.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, EnumVariant)]
#[scale_info(skip_type_params(T, I))]
pub enum VaultRotationStatus<T: Config<I>, I: 'static = ()> {
	/// We are waiting for nodes to generate a new aggregate key.
	AwaitingKeygen {
		keygen_ceremony_id: CeremonyId,
		keygen_participants: BTreeSet<T::ValidatorId>,
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

impl Default for VaultEpochAndState {
	fn default() -> Self {
		Self { epoch_index: GENESIS_EPOCH, key_state: KeyState::Unavailable }
	}
}

#[frame_support::pallet]
pub mod pallet {

	use cf_traits::{AccountRoleRegistry, ThresholdSigner, VaultTransitionHandler};

	use super::*;

	#[pallet::pallet]
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
		type Chain: ChainAbi<KeyId = Self::KeyId>;

		/// The supported api calls for the chain.
		type SetAggKeyWithAggKey: SetAggKeyWithAggKey<Self::Chain>;

		type VaultTransitionHandler: VaultTransitionHandler<Self::Chain>;

		/// The pallet dispatches calls, so it depends on the runtime's aggregated Call type.
		type RuntimeCall: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::RuntimeCall>;

		type ThresholdSigner: ThresholdSigner<
			Self::Chain,
			Callback = <Self as Config<I>>::RuntimeCall,
			ValidatorId = Self::ValidatorId,
			KeyId = <Self as Chainflip>::KeyId,
		>;

		/// A broadcaster for the target chain.
		type Broadcaster: Broadcaster<Self::Chain, ApiCall = Self::SetAggKeyWithAggKey>;

		/// For reporting misbehaviour
		type OffenceReporter: OffenceReporter<
			ValidatorId = Self::ValidatorId,
			Offence = Self::Offence,
		>;

		/// Ceremony Id source for keygen ceremonies.
		type CeremonyIdProvider: CeremonyIdProvider<CeremonyId = CeremonyId>;

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

				let candidate_count = response_status.candidate_count;
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
		StorageValue<_, VaultEpochAndState, ValueQuery>;

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

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// Request a key generation \[ceremony_id, participants\]
		KeygenRequest(CeremonyId, BTreeSet<T::ValidatorId>),
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
		KeygenVerificationFailure {
			keygen_ceremony_id: CeremonyId,
			failed_signing_ceremony_id: CeremonyId,
		},
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
					keygen_ceremony_id, keygen_participants: _, ref mut response_status,
				} => (keygen_ceremony_id, response_status),
				rotation,
				Error::<T, I>::InvalidRotationStatus,
			);
			// Make sure the ceremony id matches
			ensure!(pending_ceremony_id == ceremony_id, Error::<T, I>::InvalidCeremonyId);
			ensure!(
				keygen_status.remaining_candidates.contains(&reporter),
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
			threshold_request_id: <T::ThresholdSigner as ThresholdSigner<T::Chain>>::RequestId,
			signing_ceremony_id: CeremonyId,
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
					Event::KeygenVerificationFailure {
						keygen_ceremony_id,
						failed_signing_ceremony_id: signing_ceremony_id,
					},
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
				keygen_response_timeout: KEYGEN_CEREMONY_RESPONSE_TIMEOUT_DEFAULT.into(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> GenesisBuild<T, I> for GenesisConfig<T, I> {
		fn build(&self) {
			if let Some(vault_key) = self.vault_key.clone() {
				Pallet::<T, I>::set_vault_for_epoch(
					VaultEpochAndState { epoch_index: GENESIS_EPOCH, key_state: KeyState::Active },
					AggKeyFor::<T, I>::try_from(vault_key)
						// Note: Can't use expect() here without some type shenanigans, but would
						// give clearer error messages.
						.unwrap_or_else(|_| {
							panic!("Can't build genesis without a valid vault key.")
						}),
					self.deployment_block,
				);
			} else {
				CurrentVaultEpochAndState::<T, I>::put(VaultEpochAndState {
					epoch_index: GENESIS_EPOCH,
					key_state: KeyState::Unavailable,
				});
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
				key_state: KeyState::Active,
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
		participants: BTreeSet<T::ValidatorId>,
	) -> (<T::ThresholdSigner as ThresholdSigner<T::Chain>>::RequestId, CeremonyId) {
		let (request_id, signing_ceremony_id) =
			T::ThresholdSigner::request_keygen_verification_signature(
				T::Chain::agg_key_to_payload(new_public_key),
				new_public_key.into(),
				participants,
			);
		T::ThresholdSigner::register_callback(request_id, {
			Call::on_keygen_verification_result {
				keygen_ceremony_id,
				threshold_request_id: request_id,
				signing_ceremony_id,
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

		(request_id, signing_ceremony_id)
	}

	fn terminate_keygen_procedure(offenders: &[T::ValidatorId], event: Event<T, I>) {
		T::OffenceReporter::report_many(PalletOffence::FailedKeygen, offenders);
		PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Failed {
			offenders: offenders.iter().cloned().collect(),
		});
		Self::deposit_event(event);
	}
}

impl<T: Config<I>, I: 'static> VaultRotator for Pallet<T, I> {
	type ValidatorId = T::ValidatorId;

	/// # Panics
	/// - If an empty BTreeSet of candidates is provided
	/// - If a vault rotation outcome is already Pending (i.e. there's one already in progress)
	fn keygen(candidates: BTreeSet<Self::ValidatorId>) {
		assert!(!candidates.is_empty());

		assert_ne!(Self::status(), AsyncResult::Pending);

		let ceremony_id = T::CeremonyIdProvider::next_ceremony_id();

		PendingVaultRotation::<T, I>::put(VaultRotationStatus::AwaitingKeygen {
			keygen_ceremony_id: ceremony_id,
			keygen_participants: candidates.clone(),
			response_status: KeygenResponseStatus::new(candidates.clone()),
		});

		// Start the timer for resolving Keygen - we check this in the on_initialise() hook each
		// block
		KeygenResolutionPendingSince::<T, I>::put(frame_system::Pallet::<T>::current_block_number());

		Pallet::<T, I>::deposit_event(Event::KeygenRequest(ceremony_id, candidates));
	}

	/// Get the status of the current key generation
	fn status() -> AsyncResult<VaultStatus<T::ValidatorId>> {
		match PendingVaultRotation::<T, I>::decode_variant() {
			Some(VaultRotationStatusVariant::AwaitingKeygen) => AsyncResult::Pending,
			Some(VaultRotationStatusVariant::AwaitingKeygenVerification) => AsyncResult::Pending,
			// It's at this point we want the vault to be considered ready to commit to. We don't
			// want to commit until the other vaults are ready
			Some(VaultRotationStatusVariant::KeygenVerificationComplete) =>
				AsyncResult::Ready(VaultStatus::KeygenComplete),
			Some(VaultRotationStatusVariant::AwaitingRotation) => AsyncResult::Pending,
			Some(VaultRotationStatusVariant::Complete) =>
				AsyncResult::Ready(VaultStatus::RotationComplete),
			Some(VaultRotationStatusVariant::Failed) => match PendingVaultRotation::<T, I>::get() {
				Some(VaultRotationStatus::Failed { offenders }) =>
					AsyncResult::Ready(VaultStatus::Failed(offenders)),
				_ =>
					unreachable!("Unreachable because we are in the branch for the Failed variant."),
			},
			None => AsyncResult::Void,
		}
	}

	fn activate() {
		if let Some(VaultRotationStatus::<T, I>::KeygenVerificationComplete { new_public_key }) =
			PendingVaultRotation::<T, I>::get()
		{
			let current_vault_epoch_and_state = CurrentVaultEpochAndState::<T, I>::get();
			match <T::SetAggKeyWithAggKey as SetAggKeyWithAggKey<_>>::new_unsigned(
				Vaults::<T, I>::try_get(current_vault_epoch_and_state.epoch_index)
					.map(|vault| vault.public_key)
					.ok(),
				new_public_key,
			) {
				Ok(rotate_tx) => {
					T::Broadcaster::threshold_sign_and_broadcast(rotate_tx);
					if KeyState::Active == current_vault_epoch_and_state.key_state {
						CurrentVaultEpochAndState::<T, I>::put(VaultEpochAndState {
							epoch_index: current_vault_epoch_and_state.epoch_index,
							key_state: KeyState::Unavailable,
						})
					}
				},
				Err(_) => {
					// The block number value 1, which the vault is being set with is a dummy value
					// and doesn't mean anything. It will be later modified to the real value when
					// we witness the vault rotation manually via governance
					Self::set_vault_for_next_epoch(new_public_key, 1_u32.into());
					Self::deposit_event(Event::<T, I>::AwaitingGovernanceActivation {
						new_public_key,
					})
				},
			}

			PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::AwaitingRotation {
				new_public_key,
			});
		} else {
			#[cfg(not(test))]
			log::error!("activate key called before keygen verification completed");
			#[cfg(test)]
			panic!("activate key called before keygen verification completed");
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<VaultStatus<Self::ValidatorId>>) {
		use cf_chains::benchmarking_value::BenchmarkValue;

		match outcome {
			AsyncResult::Pending => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::AwaitingKeygen {
					keygen_ceremony_id: Default::default(),
					keygen_participants: Default::default(),
					response_status: KeygenResponseStatus::new(Default::default()),
				});
			},
			AsyncResult::Ready(VaultStatus::KeygenComplete) => {
				PendingVaultRotation::<T, I>::put(
					VaultRotationStatus::<T, I>::KeygenVerificationComplete {
						new_public_key: Default::default(),
					},
				);
			},
			AsyncResult::Ready(VaultStatus::Failed(offenders)) => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Failed {
					offenders,
				});
			},
			AsyncResult::Ready(VaultStatus::RotationComplete) => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Complete {
					tx_id: BenchmarkValue::benchmark_value(),
				});
			},
			AsyncResult::Void => {
				PendingVaultRotation::<T, I>::kill();
			},
		}
	}
}

impl<T: Config<I>, I: 'static> KeyProvider<T::Chain> for Pallet<T, I> {
	fn current_epoch_key() -> EpochKey<<T::Chain as ChainCrypto>::AggKey> {
		let current_vault_epoch_and_state = CurrentVaultEpochAndState::<T, I>::get();

		EpochKey {
				key: Vaults::<T, I>::get(current_vault_epoch_and_state.epoch_index).expect("Key must exist if CurrentVaultEpochAndState exists since they get set at the same place: set_next_vault()").public_key,
				epoch_index: current_vault_epoch_and_state.epoch_index,
				key_state: current_vault_epoch_and_state.key_state,
			}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_key(key: <T::Chain as ChainCrypto>::AggKey) {
		Vaults::<T, I>::insert(
			CurrentEpochIndex::<T>::get(),
			Vault { public_key: key, active_from_block: ChainBlockNumberFor::<T, I>::from(0u32) },
		);
	}
}

impl<T: Config<I>, I: 'static> EpochTransitionHandler for Pallet<T, I> {
	type ValidatorId = <T as Chainflip>::ValidatorId;

	fn on_new_epoch(_epoch_authorities: &[Self::ValidatorId]) {
		PendingVaultRotation::<T, I>::kill();
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
