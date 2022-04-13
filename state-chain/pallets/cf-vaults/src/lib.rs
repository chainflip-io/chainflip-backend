#![cfg_attr(not(feature = "std"), no_std)]
#![feature(assert_matches)]
#![feature(array_map)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{ChainAbi, ChainCrypto, SetAggKeyWithAggKey};
use cf_runtime_utilities::{EnumVariant, StorageDecodeVariant};
use cf_traits::{
	offence_reporting::OffenceReporter, AsyncResult, Broadcaster, CeremonyIdProvider, Chainflip,
	CurrentEpochIndex, EpochIndex, EpochInfo, EpochTransitionHandler, KeyProvider, NonceProvider,
	SuccessOrFailure, VaultRotator,
};
use frame_support::{
	dispatch::{DispatchError, DispatchResult},
	pallet_prelude::*,
};
use frame_system::{ensure_signed, pallet_prelude::*};
pub use pallet::*;
use sp_runtime::traits::{BlockNumberProvider, One, Saturating};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	iter::{FromIterator, Iterator},
	prelude::*,
};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod migrations;

pub mod weights;
pub use weights::WeightInfo;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum KeygenOutcome<Key, Id> {
	/// Keygen succeeded with the enclosed public threshold key.
	Success(Key),
	/// Keygen failed with the enclosed guilty parties.
	Failure(BTreeSet<Id>),
}

impl<Key, Id: Ord> Default for KeygenOutcome<Key, Id> {
	fn default() -> Self {
		Self::Failure(BTreeSet::new())
	}
}

pub type CeremonyId = u64;
pub type KeygenOutcomeFor<T, I = ()> =
	KeygenOutcome<AggKeyFor<T, I>, <T as Chainflip>::ValidatorId>;
pub type AggKeyFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::AggKey;
pub type TransactionHashFor<T, I = ()> = <<T as Config<I>>::Chain as ChainCrypto>::TransactionHash;
pub type ThresholdSignatureFor<T, I = ()> =
	<<T as Config<I>>::Chain as ChainCrypto>::ThresholdSignature;

/// Tracks the current state of the keygen ceremony.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct KeygenResponseStatus<T: Config<I>, I: 'static = ()> {
	/// The total number of candidates participating in the keygen ceremony.
	candidate_count: u32,
	/// The candidates that have yet to reply.
	remaining_candidates: BTreeSet<T::ValidatorId>,
	/// A map of new keys with the number of votes for each key.
	success_votes: BTreeMap<AggKeyFor<T, I>, u32>,
	/// A map of the number of blame votes that each validator has received.
	blame_votes: BTreeMap<T::ValidatorId, u32>,
}

impl<T: Config<I>, I: 'static> KeygenResponseStatus<T, I> {
	pub fn new(candidates: BTreeSet<T::ValidatorId>) -> Self {
		Self {
			candidate_count: candidates.len() as u32,
			remaining_candidates: candidates,
			success_votes: Default::default(),
			blame_votes: Default::default(),
		}
	}

	/// The success threshold is the smallest number of respondents able to reach consensus.
	///
	/// Note this is not the same as the threshold defined in the signing literature.
	fn success_threshold(&self) -> u32 {
		utilities::success_threshold_from_share_count(self.candidate_count)
	}

	/// The blame threshold is the number of blame votes that result in punishment.
	fn blame_threshold(&self) -> u32 {
		self.success_threshold()
	}

	/// Accumulate a success vote into the keygen status.
	///
	/// Does not mutate on the error case.
	fn add_success_vote(&mut self, voter: &T::ValidatorId, key: AggKeyFor<T, I>) -> DispatchResult {
		ensure!(self.remaining_candidates.remove(voter), Error::<T, I>::InvalidRespondent);

		*self.success_votes.entry(key).or_default() += 1;

		SuccessVoters::<T, I>::append(key, voter);

		Ok(())
	}

	/// Accumulate a failure vote into the keygen status.
	///
	/// Does not mutate on the error case.
	fn add_failure_vote(
		&mut self,
		voter: &T::ValidatorId,
		blamed: BTreeSet<T::ValidatorId>,
	) -> DispatchResult {
		ensure!(self.remaining_candidates.remove(voter), Error::<T, I>::InvalidRespondent);

		for id in blamed {
			*self.blame_votes.entry(id).or_default() += 1
		}

		FailureVoters::<T, I>::append(voter);

		Ok(())
	}

	/// How many candidates are we still awaiting a response from?
	fn remaining_candidate_count(&self) -> u32 {
		self.remaining_candidates.len() as u32
	}

	/// How many responses have we received so far?
	fn response_count(&self) -> u32 {
		self.candidate_count.saturating_sub(self.remaining_candidate_count())
	}

	/// Returns `Some(key)` *iff any* key has at least `self.success_threshold()` number of votes,
	/// otherwise returns `None`.
	fn success_consensus(&self) -> Option<AggKeyFor<T, I>> {
		for key in SuccessVoters::<T, I>::iter_keys() {
			if SuccessVoters::<T, I>::decode_len(key).unwrap_or_default() >=
				self.success_threshold() as usize
			{
				return Some(key)
			}
		}
		None
	}

	/// Returns `Some(blamed_nodes)` *iff* at least `self.success_threshold()` number of nodes voted
	/// for failure, where `blamed_nodes` are the nodes with at least `self.success_threshold()`
	/// votes.
	///
	/// If less than `self.success_threshold()` voted for failure, returns `None`.
	fn failure_consensus(&self) -> Option<BTreeSet<T::ValidatorId>> {
		if FailureVoters::<T, I>::decode_len().unwrap_or_default() <
			self.success_threshold() as usize
		{
			return None
		}

		Some(
			self.blame_votes
				.iter()
				.filter_map(
					|(id, vote_count)| {
						if *vote_count >= self.blame_threshold() {
							Some(id)
						} else {
							None
						}
					},
				)
				.cloned()
				.collect(),
		)
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
				SuccessVoters::<T, I>::remove_all(None);
				return KeygenOutcome::Success(*key)
			}
		}

		let mut to_punish = self.remaining_candidates.clone();
		match self.consensus_outcome() {
			Some(KeygenOutcome::Success(consensus_key)) => {
				// all nodes that reported failure *and* all nodes that reported another success.
				SuccessVoters::<T, I>::remove(consensus_key);
				for (_bad_key, key_dissenters) in SuccessVoters::<T, I>::drain() {
					for dissenter in key_dissenters {
						to_punish.insert(dissenter);
					}
				}
				for failure_voter in FailureVoters::<T, I>::take() {
					to_punish.insert(failure_voter);
				}
			},
			Some(KeygenOutcome::Failure(mut blamed)) => {
				to_punish.append(&mut blamed);
				FailureVoters::<T, I>::kill();
				for (_bad_key, key_dissenters) in SuccessVoters::<T, I>::drain() {
					for dissenter in key_dissenters {
						to_punish.insert(dissenter);
					}
				}
			},
			None => {
				SuccessVoters::<T, I>::remove_all(None);
				FailureVoters::<T, I>::kill();
				log::warn!("Unable to determine a consensus outcome for keygen.")
			},
		};

		KeygenOutcome::Failure(to_punish)
	}

	/// Determines the keygen outcome based on threshold consensus.
	fn consensus_outcome(&self) -> Option<KeygenOutcomeFor<T, I>> {
		if self.response_count() < self.success_threshold() {
			return None
		}

		self.success_consensus()
			// If it's a success, return success.
			.map(KeygenOutcome::Success)
			// Otherwise check if we have consensus on failure.
			.or_else(|| self.failure_consensus().map(KeygenOutcome::Failure))
	}
}

/// The current status of a vault rotation.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, EnumVariant)]
pub enum VaultRotationStatus<T: Config<I>, I: 'static = ()> {
	AwaitingKeygen { keygen_ceremony_id: CeremonyId, response_status: KeygenResponseStatus<T, I> },
	AwaitingRotation { new_public_key: AggKeyFor<T, I> },
	Complete { tx_hash: <T::Chain as ChainCrypto>::TransactionHash },
	Failed,
}

impl<T: Config<I>, I: 'static> VaultRotationStatus<T, I> {
	fn new(id: CeremonyId, candidates: BTreeSet<T::ValidatorId>) -> Self {
		Self::AwaitingKeygen {
			keygen_ceremony_id: id,
			response_status: KeygenResponseStatus::new(candidates),
		}
	}
}

/// The bounds within which a public key for a vault should be used for witnessing.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, Default)]
pub struct BlockHeightWindow {
	pub from: u64,
	pub to: Option<u64>,
}

/// A single vault.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct Vault<T: ChainCrypto> {
	/// The vault's public key.
	pub public_key: T::AggKey,
	/// The active window for this vault
	pub active_window: BlockHeightWindow,
}

pub mod releases {
	use frame_support::traits::StorageVersion;

	// Genesis version
	pub const V0: StorageVersion = StorageVersion::new(0);
	// Version 1 - Makes the pallet instantiable.
	pub const V1: StorageVersion = StorageVersion::new(1);
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum PalletOffence {
	/// Failing a keygen ceremony carries its own consequences.
	ParticipateKeygenFailed,
	/// In addition, failing keygen is considered a regular signing offence.
	SigningOffence,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::storage_version(releases::V1)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// The event type.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// Offences supported in this runtime.
		type Offence: From<PalletOffence>;

		/// The chain that managed by this vault must implement the api types.
		type Chain: ChainAbi;

		/// The supported api calls for the chain.
		type ApiCall: SetAggKeyWithAggKey<Self::Chain>;

		/// A broadcaster for the target chain.
		type Broadcaster: Broadcaster<Self::Chain, ApiCall = Self::ApiCall>;

		/// For reporting misbehaving validators.
		type OffenceReporter: OffenceReporter<
			ValidatorId = Self::ValidatorId,
			Offence = Self::Offence,
		>;

		/// Ceremony Id source for keygen ceremonies.
		type CeremonyIdProvider: CeremonyIdProvider<CeremonyId = CeremonyId>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;

		/// The maximum number of blocks to wait after the first keygen response comes in.
		#[pallet::constant]
		type KeygenResponseGracePeriod: Get<BlockNumberFor<Self>>;
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
				response_status,
			}) = PendingVaultRotation::<T, I>::get()
			{
				let resolve = if response_status.remaining_candidate_count() == 0 {
					log::debug!("All keygen candidates have reported, resolving outcome...");
					true
				} else if Self::has_grace_period_elapsed(current_block) {
					log::debug!(
						"Keygen response grace period has elapsed, reporting keygen failure."
					);
					Self::deposit_event(Event::<T, I>::KeygenGracePeriodElapsed(
						keygen_ceremony_id,
					));
					true
				} else {
					false
				};

				if resolve {
					match response_status.resolve_keygen_outcome() {
						KeygenOutcome::Success(new_public_key) => {
							weight += T::WeightInfo::on_initialize_success();
							Self::on_keygen_success(keygen_ceremony_id, new_public_key);
						},
						KeygenOutcome::Failure(offenders) => {
							weight += T::WeightInfo::on_initialize_failure(offenders.len() as u32);
							Self::on_keygen_failure(
								keygen_ceremony_id,
								&offenders.into_iter().collect::<Vec<_>>(),
							);
						},
					}
					KeygenResolutionPendingSince::<T, I>::kill();
				}
			}

			weight
		}

		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			migrations::migrate_storage::<T, I>()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<(), &'static str> {
			migrations::pre_migration_checks::<T, I>()
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade() -> Result<(), &'static str> {
			migrations::post_migration_checks::<T, I>()
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

	/// Vault rotation statuses for the current epoch rotation.
	#[pallet::storage]
	#[pallet::getter(fn success_voters)]
	pub type SuccessVoters<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Identity, AggKeyFor<T, I>, Vec<T::ValidatorId>, ValueQuery>;

	/// Vault rotation statuses for the current epoch rotation.
	#[pallet::storage]
	#[pallet::getter(fn failure_voters)]
	pub type FailureVoters<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

	/// Threshold key nonces for this chain.
	#[pallet::storage]
	#[pallet::getter(fn chain_nonce)]
	pub(super) type ChainNonce<T, I = ()> =
		StorageValue<_, <<T as Config<I>>::Chain as ChainAbi>::Nonce, ValueQuery>;

	/// The block since which we have been waiting for keygen to be resolved.
	#[pallet::storage]
	#[pallet::getter(fn keygen_resolution_pending_since)]
	pub(super) type KeygenResolutionPendingSince<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// Request a key generation \[ceremony_id, participants\]
		KeygenRequest(CeremonyId, Vec<T::ValidatorId>),
		/// The vault for the request has rotated
		VaultRotationCompleted,
		/// The Keygen ceremony has been aborted \[ceremony_id\]
		KeygenAborted(CeremonyId),
		/// The vault has been rotated
		VaultsRotated,
		/// The new public key witnessed externally was not the expected one \[key\]
		UnexpectedPubkeyWitnessed(<T::Chain as ChainCrypto>::AggKey),
		/// A validator has reported that keygen was successful \[validator_id\]
		KeygenSuccessReported(T::ValidatorId),
		/// A validator has reported that keygen has failed \[validator_id\]
		KeygenFailureReported(T::ValidatorId),
		/// Keygen was successful \[ceremony_id\]
		KeygenSuccess(CeremonyId),
		/// Keygen has failed \[ceremony_id\]
		KeygenFailure(CeremonyId),
		/// Keygen grace period has elapsed \[ceremony_id\]
		KeygenGracePeriodElapsed(CeremonyId),
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// An invalid ceremony id
		InvalidCeremonyId,
		/// We have an empty validator set
		EmptyValidatorSet,
		/// The rotation has not been confirmed
		NotConfirmed,
		/// There is currently no vault rotation in progress for this chain.
		NoActiveRotation,
		/// The specified chain is not supported.
		UnsupportedChain,
		/// The requested call is invalid based on the current rotation state.
		InvalidRotationStatus,
		/// The generated key is not a valid public key.
		InvalidPublicKey,
		/// A rotation for the requested ChainId is already underway.
		DuplicateRotationRequest,
		/// A validator sent a response for a ceremony in which they weren't involved, or to which
		/// they have already submitted a response.
		InvalidRespondent,
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
		/// - [InvalidPublicKey](Error::InvalidPublicKey)
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
			let reporter = ensure_signed(origin)?.into();

			// -- Validity checks.

			// There is a rotation happening.
			let mut rotation =
				PendingVaultRotation::<T, I>::get().ok_or(Error::<T, I>::NoActiveRotation)?;

			// Keygen is in progress, pull out the details.
			let (pending_ceremony_id, keygen_status) = ensure_variant!(
				VaultRotationStatus::<T, I>::AwaitingKeygen {
					keygen_ceremony_id, ref mut response_status
				} => (keygen_ceremony_id, response_status),
				rotation,
				Error::<T, I>::InvalidRotationStatus,
			);
			// Make sure the ceremony id matches
			ensure!(pending_ceremony_id == ceremony_id, Error::<T, I>::InvalidCeremonyId);

			// -- Tally the votes.

			match reported_outcome {
				KeygenOutcome::Success(key) => {
					keygen_status.add_success_vote(&reporter, key)?;
					Self::deposit_event(Event::<T, I>::KeygenSuccessReported(reporter));
				},
				KeygenOutcome::Failure(blamed) => {
					keygen_status.add_failure_vote(&reporter, blamed)?;
					Self::deposit_event(Event::<T, I>::KeygenFailureReported(reporter));
				},
			}

			PendingVaultRotation::<T, I>::put(rotation);

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
		/// - [UnsupportedChain](Error::UnsupportedChain)
		/// - [InvalidPublicKey](Error::InvalidPublicKey)
		///
		/// ## Dependencies
		///
		/// - [Epoch Info Trait](EpochInfo)
		#[pallet::weight(T::WeightInfo::vault_key_rotated())]
		pub fn vault_key_rotated(
			origin: OriginFor<T>,
			new_public_key: AggKeyFor<T, I>,
			block_number: u64,
			tx_hash: TransactionHashFor<T, I>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

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

			// We update the current epoch with an active window for the outgoers
			Vaults::<T, I>::try_mutate_exists(CurrentEpochIndex::<T>::get(), |maybe_vault| {
				if let Some(vault) = maybe_vault.as_mut() {
					vault.active_window.to = Some(block_number);
					Ok(())
				} else {
					Err(Error::<T, I>::UnsupportedChain)
				}
			})?;

			PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Complete { tx_hash });

			// For the new epoch we create a new vault with the new public key and its active
			// window at for the block after that reported
			Vaults::<T, I>::insert(
				CurrentEpochIndex::<T>::get().saturating_add(1),
				Vault {
					public_key: new_public_key,
					active_window: BlockHeightWindow {
						from: block_number.saturating_add(1),
						to: None,
					},
				},
			);

			Pallet::<T, I>::deposit_event(Event::VaultRotationCompleted);

			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		/// The provided Vec must be convertible to the chain's AggKey.
		///
		/// GenesisConfig members require `Serialize` and `Deserialize` which isn't
		/// implemented for the AggKey type, hence we use Vec<u8> and covert during genesis.
		pub vault_key: Vec<u8>,
		pub deployment_block: u64,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self { vault_key: Default::default(), deployment_block: Default::default() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> GenesisBuild<T, I> for GenesisConfig {
		fn build(&self) {
			use sp_std::convert::TryFrom;

			let public_key = AggKeyFor::<T, I>::try_from(self.vault_key.clone())
				// Note: Can't use expect() here without some type shenanigans, but would give
				// clearer error messages.
				.unwrap_or_else(|_| panic!("Can't build genesis without a valid vault key."));

			Vaults::<T, I>::insert(
				CurrentEpochIndex::<T>::get(),
				Vault {
					public_key,
					active_window: BlockHeightWindow { from: self.deployment_block, to: None },
				},
			);
		}
	}
}

impl<T: Config<I>, I: 'static> NonceProvider<T::Chain> for Pallet<T, I> {
	fn next_nonce() -> <T::Chain as ChainAbi>::Nonce {
		ChainNonce::<T, I>::mutate(|nonce| {
			*nonce += One::one();
			*nonce
		})
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn on_keygen_success(ceremony_id: CeremonyId, new_public_key: AggKeyFor<T, I>) {
		Self::deposit_event(Event::KeygenSuccess(ceremony_id));

		T::Broadcaster::threshold_sign_and_broadcast(
			<T::ApiCall as SetAggKeyWithAggKey<_>>::new_unsigned(
				<Self as NonceProvider<_>>::next_nonce(),
				new_public_key,
			),
		);

		PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::AwaitingRotation {
			new_public_key,
		});
	}

	fn on_keygen_failure(ceremony_id: CeremonyId, offenders: &[T::ValidatorId]) {
		Self::deposit_event(Event::KeygenFailure(ceremony_id));

		if offenders.len() < T::EpochInfo::consensus_threshold(T::EpochInfo::epoch_index()) as usize
		{
			T::OffenceReporter::report_many(PalletOffence::ParticipateKeygenFailed, offenders);
			T::OffenceReporter::report_many(PalletOffence::SigningOffence, offenders);
		}

		PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Failed);
	}

	fn has_grace_period_elapsed(block: BlockNumberFor<T>) -> bool {
		block.saturating_sub(KeygenResolutionPendingSince::<T, I>::get()) >=
			T::KeygenResponseGracePeriod::get()
	}
}

// TODO: Implement this on Runtime instead of pallet so that we can rotate multiple vaults.
impl<T: Config<I>, I: 'static> VaultRotator for Pallet<T, I> {
	type ValidatorId = T::ValidatorId;
	type RotationError = DispatchError;

	fn start_vault_rotation(candidates: Vec<Self::ValidatorId>) -> Result<(), Self::RotationError> {
		// Main entry point for the pallet
		ensure!(!candidates.is_empty(), Error::<T, I>::EmptyValidatorSet);
		ensure!(
			Self::get_vault_rotation_outcome() != AsyncResult::Pending,
			Error::<T, I>::DuplicateRotationRequest
		);

		let ceremony_id = T::CeremonyIdProvider::next_ceremony_id();

		PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::new(
			ceremony_id,
			BTreeSet::from_iter(candidates.clone()),
		));

		// Start the timer for resolving Keygen - we check this in the on_initialise() hook each
		// block
		KeygenResolutionPendingSince::<T, I>::put(frame_system::Pallet::<T>::current_block_number());

		Pallet::<T, I>::deposit_event(Event::KeygenRequest(ceremony_id, candidates));
		Ok(())
	}

	/// Get the status of the current key generation
	fn get_vault_rotation_outcome() -> AsyncResult<SuccessOrFailure> {
		match PendingVaultRotation::<T, I>::decode_variant() {
			Some(VaultRotationStatusVariant::AwaitingKeygen) => AsyncResult::Pending,
			Some(VaultRotationStatusVariant::AwaitingRotation) => AsyncResult::Pending,
			Some(VaultRotationStatusVariant::Complete) =>
				AsyncResult::Ready(SuccessOrFailure::Success),
			Some(VaultRotationStatusVariant::Failed) =>
				AsyncResult::Ready(SuccessOrFailure::Failure),
			None => AsyncResult::Void,
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
}

impl<T: Config<I>, I: 'static> EpochTransitionHandler for Pallet<T, I> {
	type ValidatorId = <T as Chainflip>::ValidatorId;

	fn on_new_epoch(_epoch_validators: &[Self::ValidatorId]) {
		PendingVaultRotation::<T, I>::kill();
		T::OffenceReporter::forgive_all(PalletOffence::ParticipateKeygenFailed);
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
