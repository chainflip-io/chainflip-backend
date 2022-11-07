#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{Chain, ChainAbi, ChainCrypto, SetAggKeyWithAggKey};
use cf_primitives::{AuthorityCount, CeremonyId, EpochIndex};
use cf_runtime_utilities::{EnumVariant, StorageDecodeVariant};
use cf_traits::{
	offence_reporting::OffenceReporter, AsyncResult, Broadcaster, CeremonyIdProvider, Chainflip,
	CurrentEpochIndex, EpochTransitionHandler, EthEnvironmentProvider, KeyProvider,
	ReplayProtectionProvider, RetryPolicy, SystemStateManager, ThresholdSigner, VaultRotator,
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

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
pub enum KeygenError<Id> {
	/// Generated key is incompatible with requirements.
	Incompatible,
	/// Keygen failed with the enclosed guilty parties.
	Failure(BTreeSet<Id>),
}

pub type KeygenOutcome<Key, Id> = Result<Key, KeygenError<Id>>;
pub type ReportedKeygenOutcome<Key, Id> = Result<Key, KeygenError<Id>>;

pub type ReportedKeygenOutcomeFor<T, I = ()> =
	ReportedKeygenOutcome<AggKeyFor<T, I>, <T as Chainflip>::ValidatorId>;
pub type PayloadFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::Payload;
pub type KeygenOutcomeFor<T, I = ()> =
	KeygenOutcome<AggKeyFor<T, I>, <T as Chainflip>::ValidatorId>;
pub type AggKeyFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::AggKey;
pub type ChainBlockNumberFor<T, I = ()> = <<T as Config<I>>::Chain as Chain>::ChainBlockNumber;
pub type TransactionHashFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::TransactionHash;
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

	fn add_incompatible_vote(&mut self, voter: &T::ValidatorId) {
		assert!(self.remaining_candidates.remove(voter));
		IncompatibleVoters::<T, I>::append(voter);
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
		} else if IncompatibleVoters::<T, I>::decode_len().unwrap_or_default() >=
			super_majority_threshold
		{
			IncompatibleVoters::<T, I>::kill();
		} else if FailureVoters::<T, I>::decode_len().unwrap_or_default() >=
			super_majority_threshold
		{
			FailureVoters::<T, I>::kill();
		} else {
			let _empty = SuccessVoters::<T, I>::clear(u32::MAX, None);
			FailureVoters::<T, I>::kill();
			IncompatibleVoters::<T, I>::kill();
			log::warn!("Unable to determine a consensus outcome for keygen.");
		}

		Err(KeygenError::Failure(
			SuccessVoters::<T, I>::drain()
				.flat_map(|(_k, dissenters)| dissenters)
				.chain(FailureVoters::<T, I>::take())
				.chain(IncompatibleVoters::<T, I>::take())
				.chain(self.blame_votes.into_iter().filter_map(|(id, vote_count)| {
					if vote_count >= super_majority_threshold as u32 {
						Some(id)
					} else {
						None
					}
				}))
				.chain(self.remaining_candidates)
				.collect(),
		))
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
	/// We are waiting for the key to be updated on the contract, and witnessed by the network.
	AwaitingRotation { new_public_key: AggKeyFor<T, I> },
	/// The key has been successfully updated on the contract.
	Complete { tx_hash: <T::Chain as ChainCrypto>::TransactionHash },
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

#[frame_support::pallet]
pub mod pallet {

	use cf_traits::{AccountRoleRegistry, ApiCallDataProvider, ThresholdSigner};

	use super::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// The event type.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// Ensure that only threshold signature consensus can trigger a key_verification success
		type EnsureThresholdSigned: EnsureOrigin<Self::Origin>;

		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;

		/// Offences supported in this runtime.
		type Offence: From<PalletOffence>;

		/// The chain that is managed by this vault must implement the api types.
		type Chain: ChainAbi;

		/// The supported api calls for the chain.
		type ApiCall: SetAggKeyWithAggKey<Self::Chain>;

		/// The pallet dispatches calls, so it depends on the runtime's aggregated Call type.
		type Call: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::Call>;

		type ThresholdSigner: ThresholdSigner<
			Self::Chain,
			Callback = <Self as Config<I>>::Call,
			ValidatorId = Self::ValidatorId,
		>;

		/// A broadcaster for the target chain.
		type Broadcaster: Broadcaster<Self::Chain, ApiCall = Self::ApiCall>;

		/// For reporting misbehaviour
		type OffenceReporter: OffenceReporter<
			ValidatorId = Self::ValidatorId,
			Offence = Self::Offence,
		>;

		/// Ceremony Id source for keygen ceremonies.
		type CeremonyIdProvider: CeremonyIdProvider<CeremonyId = CeremonyId>;

		/// Something that can provide the key manager address and chain id.
		type EthEnvironmentProvider: EthEnvironmentProvider;

		// Something that can give us the next nonce.
		type ReplayProtectionProvider: ReplayProtectionProvider<Self::Chain>
			+ ApiCallDataProvider<Self::Chain>;

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

			if Self::get_vault_rotation_outcome() != AsyncResult::Pending {
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
						Self::trigger_keygen_verification(
							keygen_ceremony_id,
							new_public_key,
							keygen_participants,
						);
					},
					Err(KeygenError::Incompatible) => {
						PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Failed {
							offenders: Default::default(),
						});
						Self::deposit_event(Event::KeygenIncompatible(keygen_ceremony_id));
					},
					Err(KeygenError::Failure(offenders)) => {
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

	/// The voters who voted that a particular keygen ceremony generated an incompatible key
	#[pallet::storage]
	#[pallet::getter(fn incompatible_voters)]
	pub type IncompatibleVoters<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

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
		/// The Keygen ceremony has been aborted \[ceremony_id\]
		KeygenAborted(CeremonyId),
		/// The vault's key has been rotated externally \[new_public_key\]
		VaultRotatedExternally(<T::Chain as ChainCrypto>::AggKey),
		/// The new public key witnessed externally was not the expected one \[key\]
		UnexpectedPubkeyWitnessed(<T::Chain as ChainCrypto>::AggKey),
		/// A keygen participant has reported that keygen was successful \[validator_id\]
		KeygenSuccessReported(T::ValidatorId),
		/// A keygen participant has reported that an incompatible key was generated
		/// \[validator_id\]
		KeygenIncompatibleReported(T::ValidatorId),
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
		/// Keygen was incompatible \[ceremony_id\]
		KeygenIncompatible(CeremonyId),
		/// Keygen has failed \[ceremony_id\]
		KeygenFailure(CeremonyId),
		/// Keygen response timeout has occurred \[ceremony_id\]
		KeygenResponseTimeout(CeremonyId),
		/// Keygen response timeout was updated \[new_timeout\]
		KeygenResponseTimeoutUpdated { new_timeout: BlockNumberFor<T> },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// An invalid ceremony id
		InvalidCeremonyId,
		/// We have an empty authority set
		EmptyAuthoritySet,
		/// The rotation has not been confirmed
		NotConfirmed,
		/// There is currently no vault rotation in progress for this chain.
		NoActiveRotation,
		/// The requested call is invalid based on the current rotation state.
		InvalidRotationStatus,
		/// The generated key is not a valid public key.
		InvalidPublicKey,
		/// A rotation for the requested ChainId is already underway.
		DuplicateRotationRequest,
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
		/// - [KeygenIncompatibleReported](Event::KeygenIncompatibleReported)
		///
		/// ## Errors
		///
		/// - [NoActiveRotation](Error::NoActiveRotation)
		/// - [InvalidRotationStatus](Error::InvalidRotationStatus)
		/// - [InvalidCeremonyId](Error::InvalidCeremonyId)
		/// - [InvalidPublicKey](Error::InvalidPublicKey)
		///
		/// ## Dependencies
		///
		/// - [Threshold Signer Trait](ThresholdSigner)
		#[pallet::weight(T::WeightInfo::report_keygen_outcome())]
		pub fn report_keygen_outcome(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			reported_outcome: ReportedKeygenOutcomeFor<T, I>,
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
				Err(KeygenError::Incompatible) => {
					keygen_status.add_incompatible_vote(&reporter);
					Self::deposit_event(Event::<T, I>::KeygenIncompatibleReported(reporter));
				},
				Err(KeygenError::Failure(blamed)) => {
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
					T::Broadcaster::threshold_sign_and_broadcast(
						<T::ApiCall as SetAggKeyWithAggKey<_>>::new_unsigned(
							<T::ReplayProtectionProvider>::replay_protection(),
							<T::ReplayProtectionProvider>::chain_extra_data(),
							new_public_key,
						),
					);

					PendingVaultRotation::<T, I>::put(
						VaultRotationStatus::<T, I>::AwaitingRotation { new_public_key },
					);

					Self::deposit_event(Event::KeygenVerificationSuccess {
						agg_key: new_public_key,
					})
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
		/// - [UnexpectedPubkeyWitnessed](Event::UnexpectedPubkeyWitnessed)
		/// - [VaultRotationCompleted](Event::VaultRotationCompleted)
		///
		/// ## Errors
		///
		/// - [NoActiveRotation](Error::NoActiveRotation)
		/// - [InvalidRotationStatus](Error::InvalidRotationStatus)
		/// - [InvalidPublicKey](Error::InvalidPublicKey)
		///
		/// ## Dependencies
		///
		/// - [Epoch Info Trait](EpochInfo)
		#[pallet::weight(T::WeightInfo::vault_key_rotated())]
		pub fn vault_key_rotated(
			origin: OriginFor<T>,
			new_public_key: AggKeyFor<T, I>,
			block_number: ChainBlockNumberFor<T, I>,
			tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;

			let rotation =
				PendingVaultRotation::<T, I>::get().ok_or(Error::<T, I>::NoActiveRotation)?;

			let expected_new_key = ensure_variant!(
				VaultRotationStatus::<T, I>::AwaitingRotation { new_public_key } => new_public_key,
				rotation,
				Error::<T, I>::InvalidRotationStatus
			);

			// If the keys don't match, we don't have much choice but to trust the witnessed one
			// over the one we expected, but we should log the issue nonetheless.
			if new_public_key != expected_new_key {
				log::error!(
					"Unexpected new agg key witnessed. Expected {:?}, got {:?}.",
					expected_new_key,
					new_public_key,
				);
				Self::deposit_event(Event::<T, I>::UnexpectedPubkeyWitnessed(new_public_key));
			}

			PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Complete { tx_hash });

			Self::set_next_vault(new_public_key, block_number);

			Pallet::<T, I>::deposit_event(Event::VaultRotationCompleted);

			Ok(().into())
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
			_tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;

			Self::set_next_vault(new_public_key, block_number);

			T::SystemStateManager::activate_maintenance_mode();

			Pallet::<T, I>::deposit_event(Event::VaultRotatedExternally(new_public_key));

			Ok(().into())
		}

		#[pallet::weight(T::WeightInfo::set_keygen_timeout())]
		pub fn set_keygen_timeout(
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
		pub vault_key: Vec<u8>,
		pub deployment_block: ChainBlockNumberFor<T, I>,
		pub keygen_response_timeout: BlockNumberFor<T>,
	}

	#[cfg(feature = "std")]
	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			use sp_runtime::traits::Zero;
			Self {
				vault_key: Default::default(),
				deployment_block: Zero::zero(),
				keygen_response_timeout: KEYGEN_CEREMONY_RESPONSE_TIMEOUT_DEFAULT.into(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> GenesisBuild<T, I> for GenesisConfig<T, I> {
		fn build(&self) {
			let public_key = AggKeyFor::<T, I>::try_from(self.vault_key.clone())
				// Note: Can't use expect() here without some type shenanigans, but would give
				// clearer error messages.
				.unwrap_or_else(|_| panic!("Can't build genesis without a valid vault key."));

			KeygenResponseTimeout::<T, I>::put(self.keygen_response_timeout);

			Vaults::<T, I>::insert(
				CurrentEpochIndex::<T>::get(),
				Vault { public_key, active_from_block: self.deployment_block },
			);
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn set_next_vault(
		new_public_key: AggKeyFor<T, I>,
		rotated_at_block_number: ChainBlockNumberFor<T, I>,
	) {
		Vaults::<T, I>::insert(
			CurrentEpochIndex::<T>::get().saturating_add(1),
			Vault {
				public_key: new_public_key,
				active_from_block: rotated_at_block_number
					.saturating_add(ChainBlockNumberFor::<T, I>::one()),
			},
		);
	}

	// Once we've successfully generated the key, we want to do a signing ceremony to verify that
	// the key is useable
	fn trigger_keygen_verification(
		keygen_ceremony_id: CeremonyId,
		new_public_key: AggKeyFor<T, I>,
		participants: BTreeSet<T::ValidatorId>,
	) -> (<T::ThresholdSigner as ThresholdSigner<T::Chain>>::RequestId, CeremonyId) {
		let byte_key: Vec<u8> = new_public_key.into();
		let (request_id, signing_ceremony_id) = T::ThresholdSigner::request_signature_with(
			byte_key.into(),
			participants,
			T::Chain::agg_key_to_payload(new_public_key),
			RetryPolicy::Never,
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
		Self::deposit_event(Event::KeygenSuccess(keygen_ceremony_id));
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

// TODO: Implement this on Runtime instead of pallet so that we can rotate multiple vaults.
impl<T: Config<I>, I: 'static> VaultRotator for Pallet<T, I> {
	type ValidatorId = T::ValidatorId;

	/// # Panics
	/// - If an empty BTreeSet of candidates is provided
	/// - If a vault rotation outcome is already Pending (i.e. there's one already in progress)
	fn start_vault_rotation(candidates: BTreeSet<Self::ValidatorId>) {
		assert!(!candidates.is_empty());

		assert_ne!(Self::get_vault_rotation_outcome(), AsyncResult::Pending);

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
	fn get_vault_rotation_outcome() -> AsyncResult<Result<(), BTreeSet<T::ValidatorId>>> {
		match PendingVaultRotation::<T, I>::decode_variant() {
			Some(VaultRotationStatusVariant::AwaitingKeygen) => AsyncResult::Pending,
			Some(VaultRotationStatusVariant::AwaitingKeygenVerification) => AsyncResult::Pending,
			Some(VaultRotationStatusVariant::AwaitingRotation) => AsyncResult::Pending,
			Some(VaultRotationStatusVariant::Complete) => AsyncResult::Ready(Ok(())),
			Some(VaultRotationStatusVariant::Failed) => match PendingVaultRotation::<T, I>::get() {
				Some(VaultRotationStatus::Failed { offenders }) =>
					AsyncResult::Ready(Err(offenders)),
				_ =>
					unreachable!("Unreachable because we are in the branch for the Failed variant."),
			},
			None => AsyncResult::Void,
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_vault_rotation_outcome(outcome: AsyncResult<Result<(), BTreeSet<Self::ValidatorId>>>) {
		match outcome {
			AsyncResult::Pending => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::AwaitingKeygen {
					keygen_ceremony_id: Default::default(),
					keygen_participants: Default::default(),
					response_status: KeygenResponseStatus::new(Default::default()),
				});
			},
			AsyncResult::Ready(Ok(())) => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Complete {
					tx_hash: Default::default(),
				});
			},
			AsyncResult::Ready(Err(offenders)) => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Failed {
					offenders,
				});
			},
			AsyncResult::Void => {
				PendingVaultRotation::<T, I>::kill();
			},
		}
	}
}

impl<T: Config<I>, I: 'static> KeyProvider<T::Chain> for Pallet<T, I> {
	type KeyId = Vec<u8>;

	fn current_key_id() -> Self::KeyId {
		Vaults::<T, I>::get(CurrentEpochIndex::<T>::get())
			.expect("We can't exist without a vault")
			.public_key
			.into()
	}

	fn current_key() -> <T::Chain as ChainCrypto>::AggKey {
		Vaults::<T, I>::get(CurrentEpochIndex::<T>::get())
			.expect("We can't exist without a vault")
			.public_key
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
