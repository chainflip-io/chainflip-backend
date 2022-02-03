#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;

pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod migrations;

use cf_traits::{
	AuctionResult, Auctioneer, EmergencyRotation, EpochIndex, EpochInfo, EpochTransitionHandler,
	HasPeerMapping, VaultRotationHandler,
};
use frame_support::{
	pallet_prelude::*,
	traits::{EstimateNextSessionRotation, OnKilledAccount},
};
pub use pallet::*;
use sp_core::ed25519;
use sp_runtime::traits::{
	AtLeast32BitUnsigned, BlockNumberProvider, Convert, One, Saturating, Zero,
};
use sp_std::prelude::*;

pub mod releases {
	use frame_support::traits::StorageVersion;
	// Genesis version
	pub const V0: StorageVersion = StorageVersion::new(0);
	// Version 1 - adds Bond and Validator storage items
	pub const V1: StorageVersion = StorageVersion::new(1);
}

pub type ValidatorSize = u32;
type SessionIndex = u32;

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Encode, Decode)]
pub struct SemVer {
	pub major: u8,
	pub minor: u8,
	pub patch: u8,
}

type Version = SemVer;
type Ed25519PublicKey = ed25519::Public;
type Ed25519Signature = ed25519::Signature;
pub type Ipv6Addr = u128;

/// A percentage range
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct PercentageRange {
	pub top: u8,
	pub bottom: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum RotationStatus<ValidatorId, Amount> {
	Idle,
	RunAuction,
	AwaitingVaults(AuctionResult<ValidatorId, Amount>),
	VaultsRotated(AuctionResult<ValidatorId, Amount>),
	SessionRotating(AuctionResult<ValidatorId, Amount>),
}

impl<ValidatorId, Amount> Default for RotationStatus<ValidatorId, Amount> {
	fn default() -> Self {
		RotationStatus::Idle
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{QualifyConfig, ValidatorChecked, ValidatorUnchecked, VaultRotator};
	use frame_system::pallet_prelude::*;
	use pallet_session::WeightInfo as SessionWeightInfo;
	use sp_runtime::app_crypto::RuntimePublic;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::storage_version(releases::V1)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ pallet_session::Config<ValidatorId = <Self as frame_system::Config>::AccountId>
	{
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// A handler for epoch lifecycle events
		type EpochTransitionHandler: EpochTransitionHandler<
			ValidatorId = Self::ValidatorId,
			Amount = Self::Amount,
		>;

		/// Minimum amount of blocks an epoch can run for
		#[pallet::constant]
		type MinEpoch: Get<<Self as frame_system::Config>::BlockNumber>;

		/// Benchmark stuff
		type ValidatorWeightInfo: WeightInfo;

		/// An amount
		type Amount: Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;

		/// An auction type
		type Auctioneer: Auctioneer<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;

		/// The lifecycle of a vault rotation
		type VaultRotator: VaultRotator<ValidatorId = Self::ValidatorId>;

		/// Configure what we are intested in qualifying
		type QualifyConfig: QualifyConfig<ValidatorId = Self::ValidatorId>;

		/// The range of online validators we would trigger an emergency rotation
		#[pallet::constant]
		type EmergencyRotationPercentageRange: Get<PercentageRange>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The rotation is aborted
		RotationAborted,
		/// A new epoch has started \[epoch_index\]
		NewEpoch(EpochIndex),
		/// The number of blocks has changed for our epoch \[from, to\]
		EpochDurationChanged(T::BlockNumber, T::BlockNumber),
		/// Rotation status updated \[rotation_status\]
		RotationStatusUpdated(RotationStatusOf<T>),
		/// An emergency rotation has been requested
		EmergencyRotationRequested(),
		/// The CFE version has been updated \[Validator, Old Version, New Version]
		CFEVersionUpdated(T::ValidatorId, Version, Version),
		/// A validator has register her current PeerId \[account_id, public_key, port,
		/// ip_address\]
		PeerIdRegistered(T::AccountId, Ed25519PublicKey, u16, Ipv6Addr),
		/// A validator has unregistered her current PeerId \[account_id, public_key\]
		PeerIdUnregistered(T::AccountId, Ed25519PublicKey),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Epoch block number supplied is invalid
		InvalidEpoch,
		/// A rotation is in progress
		RotationInProgress,
		/// Validator Peer mapping overlaps with an existing mapping
		AccountPeerMappingOverlap,
		/// Invalid signature
		InvalidAccountPeerMappingSignature,
	}

	type RotationStatusOf<T> =
		RotationStatus<<T as frame_system::Config>::AccountId, <T as Config>::Amount>;

	impl<T: Config> Pallet<T> {
		pub(crate) fn update_rotation_status(new_status: RotationStatusOf<T>) {
			RotationPhase::<T>::put(new_status.clone());
			Self::deposit_event(Event::RotationStatusUpdated(new_status));
		}
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(block_number: BlockNumberFor<T>) -> Weight {
			match RotationPhase::<T>::get() {
				RotationStatus::Idle => {
					let blocks_per_epoch = BlocksPerEpoch::<T>::get();
					if blocks_per_epoch > Zero::zero() {
						let current_epoch_started_at = CurrentEpochStartedAt::<T>::get();
						let diff = block_number.saturating_sub(current_epoch_started_at);
						if diff >= blocks_per_epoch {
							Self::update_rotation_status(RotationStatus::RunAuction);
						}
					}
				},
				RotationStatus::RunAuction => match T::Auctioneer::resolve_auction::<
					ValidatorChecked<T::QualifyConfig>,
				>() {
					Ok(auction_result) => {
						// TODO Do we handle an error at starting the vault rotation as an abort or
						// just simply log?
						match T::VaultRotator::start_vault_rotation(auction_result.winners.clone())
						{
							Ok(_) => Self::update_rotation_status(RotationStatus::AwaitingVaults(
								auction_result,
							)),
							Err(e) =>
								log::warn!(target: "cf-validator", "starting a vault rotation failed due to error: {:?}", e),
						}
					},
					Err(e) =>
						log::warn!(target: "cf-validator", "auction failed due to error: {:?}", e),
				},
				RotationStatus::AwaitingVaults(auction_result) =>
					match T::VaultRotator::finalize_rotation() {
						Ok(_) => match T::Auctioneer::confirm_auction(auction_result.clone()) {
							Ok(_) => Self::update_rotation_status(RotationStatus::VaultsRotated(
								auction_result,
							)),
							Err(e) =>
								log::warn!(target: "cf-validator", "failed to confirm auction {:?}", e),
						},
						// TODO is this too verbose as we will be seeing this on every block until
						// we finalize a vault rotation
						Err(e) =>
							log::warn!(target: "cf-validator", "finalizing a vault rotation failed due to error: {:?}", e),
					},
				RotationStatus::VaultsRotated(auction_result) => {
					Self::update_rotation_status(RotationStatus::SessionRotating(auction_result));
				},
				RotationStatus::SessionRotating(_) => {
					Self::update_rotation_status(RotationStatus::Idle);
				},
			}
			0
		}

		fn on_runtime_upgrade() -> Weight {
			if releases::V0 == <Pallet<T> as GetStorageVersion>::on_chain_storage_version() {
				releases::V1.put::<Pallet<T>>();
				migrations::v1::migrate::<T>().saturating_add(T::DbWeight::get().reads_writes(1, 1))
			} else {
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<(), &'static str> {
			if releases::V0 == <Pallet<T> as GetStorageVersion>::on_chain_storage_version() {
				migrations::v1::pre_migrate::<T, Self>()
			} else {
				Ok(())
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade() -> Result<(), &'static str> {
			if releases::V1 == <Pallet<T> as GetStorageVersion>::on_chain_storage_version() {
				migrations::v1::post_migrate::<T, Self>()
			} else {
				Ok(())
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets the number of blocks an epoch should run for
		///
		/// The dispatch origin of this function must be root.
		///
		/// ## Events
		///
		/// - [EpochDurationChanged](Event::EpochDurationChanged)
		///
		/// ## Errors
		///
		/// - [RotationInProgress](Error::RotationInProgress)
		/// - [InvalidEpoch](Error::InvalidEpoch)
		#[pallet::weight(T::ValidatorWeightInfo::set_blocks_for_epoch())]
		pub fn set_blocks_for_epoch(
			origin: OriginFor<T>,
			number_of_blocks: T::BlockNumber,
		) -> DispatchResultWithPostInfo {
			// TODO Governance
			ensure_root(origin)?;
			ensure!(
				RotationPhase::<T>::get() == RotationStatus::Idle,
				Error::<T>::RotationInProgress
			);
			ensure!(number_of_blocks >= T::MinEpoch::get(), Error::<T>::InvalidEpoch);
			let old_epoch = BlocksPerEpoch::<T>::get();
			ensure!(old_epoch != number_of_blocks, Error::<T>::InvalidEpoch);
			BlocksPerEpoch::<T>::set(number_of_blocks);
			Self::deposit_event(Event::EpochDurationChanged(old_epoch, number_of_blocks));

			Ok(().into())
		}

		/// Force a new epoch.  From the next block we will try to move to a new epoch and rotate
		/// our validators.
		///
		/// The dispatch origin of this function must be root.
		///
		/// ## Events
		///
		/// - [ForceRotationRequested](Event::ForceRotationRequested)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		/// - [RotationInProgress](Error::RotationInProgress)
		#[pallet::weight(T::ValidatorWeightInfo::force_rotation())]
		pub fn force_rotation(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			// TODO Governance
			ensure_root(origin)?;
			ensure!(
				RotationPhase::<T>::get() == RotationStatus::Idle,
				Error::<T>::RotationInProgress
			);
			Self::update_rotation_status(RotationStatus::RunAuction);

			Ok(().into())
		}

		/// Allow a validator to set their keys for upcoming sessions
		///
		/// The dispatch origin of this function must be signed.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - None
		///
		/// ## Dependencies
		///
		/// - [Session Pallet](pallet_session::Config)
		#[pallet::weight(< T as pallet_session::Config >::WeightInfo::set_keys())] // TODO: check if this is really valid
		pub fn set_keys(
			origin: OriginFor<T>,
			keys: T::Keys,
			proof: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			<pallet_session::Pallet<T>>::set_keys(origin, keys, proof)?;
			Ok(().into())
		}

		/// Allow a validator to link their validator id to a peer id
		///
		/// The dispatch origin of this function must be signed.
		///
		/// ## Events
		///
		/// - [PeerIdRegistered](Event::PeerIdRegistered)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::error::BadOrigin)
		/// - [InvalidAccountPeerMappingSignature](Error::InvalidAccountPeerMappingSignature)
		/// - [AccountPeerMappingOverlap](Error::AccountPeerMappingOverlap)
		///
		/// ## Dependencies
		///
		/// - None
		#[pallet::weight(T::ValidatorWeightInfo::register_peer_id())]
		pub fn register_peer_id(
			origin: OriginFor<T>,
			peer_id: Ed25519PublicKey,
			port: u16,
			ip_address: Ipv6Addr,
			signature: Ed25519Signature,
		) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;
			ensure!(
				RuntimePublic::verify(&peer_id, &account_id.encode(), &signature),
				Error::<T>::InvalidAccountPeerMappingSignature
			);

			if let Some((_, existing_peer_id, _, _)) = AccountPeerMapping::<T>::get(&account_id) {
				if existing_peer_id != peer_id {
					ensure!(
						!MappedPeers::<T>::contains_key(&peer_id),
						Error::<T>::AccountPeerMappingOverlap
					);
					MappedPeers::<T>::remove(&existing_peer_id);
					MappedPeers::<T>::insert(&peer_id, ());
				}
			} else {
				ensure!(
					!MappedPeers::<T>::contains_key(&peer_id),
					Error::<T>::AccountPeerMappingOverlap
				);
				MappedPeers::<T>::insert(&peer_id, ());
			}

			AccountPeerMapping::<T>::insert(
				&account_id,
				(account_id.clone(), peer_id, port, ip_address),
			);

			Self::deposit_event(Event::PeerIdRegistered(account_id, peer_id, port, ip_address));
			Ok(().into())
		}

		/// Allow a validator to send their current cfe version.  We validate that the version is a
		/// not the same version stored and if not we store and emit `CFEVersionUpdated`.
		///
		/// The dispatch origin of this function must be signed.
		///
		/// ## Events
		///
		/// - [CFEVersionUpdated](Event::CFEVersionUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::error::BadOrigin)
		/// ## Dependencies
		///
		/// - None
		#[pallet::weight(T::ValidatorWeightInfo::cfe_version())]
		pub fn cfe_version(origin: OriginFor<T>, version: Version) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;
			let validator_id: T::ValidatorId = account_id;
			ValidatorCFEVersion::<T>::try_mutate(validator_id.clone(), |current_version| {
				if *current_version != version {
					Self::deposit_event(Event::CFEVersionUpdated(
						validator_id,
						current_version.clone(),
						version.clone(),
					));
					*current_version = version;
				}
				Ok(().into())
			})
		}
	}

	/// An emergency rotation has been requested
	#[pallet::storage]
	#[pallet::getter(fn emergency_rotation_requested)]
	pub(super) type EmergencyRotationRequested<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// The starting block number for the current epoch
	#[pallet::storage]
	#[pallet::getter(fn current_epoch_started_at)]
	pub(super) type CurrentEpochStartedAt<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// The number of blocks an epoch runs for
	#[pallet::storage]
	#[pallet::getter(fn epoch_number_of_blocks)]
	pub(super) type BlocksPerEpoch<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// Current epoch index
	#[pallet::storage]
	#[pallet::getter(fn current_epoch)]
	pub type CurrentEpoch<T: Config> = StorageValue<_, EpochIndex, ValueQuery>;

	/// Active validator lookup
	#[pallet::storage]
	#[pallet::getter(fn validator_lookup)]
	pub type ValidatorLookup<T: Config> = StorageMap<_, Blake2_128Concat, T::ValidatorId, ()>;

	/// The rotation phase we are currently at
	#[pallet::storage]
	#[pallet::getter(fn rotation_phase)]
	pub type RotationPhase<T: Config> =
		StorageValue<_, RotationStatus<T::ValidatorId, T::Amount>, ValueQuery>;

	/// A list of the current validators
	#[pallet::storage]
	#[pallet::getter(fn validators)]
	pub type Validators<T: Config> = StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

	/// The current bond
	#[pallet::storage]
	#[pallet::getter(fn bond)]
	pub type Bond<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	/// Account to Peer Mapping
	#[pallet::storage]
	#[pallet::getter(fn validator_peer_id)]
	pub type AccountPeerMapping<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		(T::AccountId, Ed25519PublicKey, u16, Ipv6Addr),
	>;

	/// Peers that are associated with account ids
	#[pallet::storage]
	#[pallet::getter(fn mapped_peer)]
	pub type MappedPeers<T: Config> = StorageMap<_, Blake2_128Concat, Ed25519PublicKey, ()>;

	/// Validator CFE version
	#[pallet::storage]
	#[pallet::getter(fn validator_cfe_version)]
	pub type ValidatorCFEVersion<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, Version, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub blocks_per_epoch: T::BlockNumber,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { blocks_per_epoch: Zero::zero() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			BlocksPerEpoch::<T>::set(self.blocks_per_epoch);
			// Run an auction on genesis. We use `ValidatorUnchecked` to skip
			// any valdation for the bidders which are our genesis nodes which
			// are already staked
			let auction_result =
				T::Auctioneer::resolve_auction::<ValidatorUnchecked<T::ValidatorId>>()
					.expect("an auction is run for our genesis bidders");
			T::Auctioneer::confirm_auction(auction_result.clone())
				.expect("the auction is confirmed");
			Pallet::<T>::start_new_epoch(
				&auction_result.winners,
				auction_result.minimum_active_bid,
			);
		}
	}
}

impl<T: Config> EpochInfo for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;

	fn current_validators() -> Vec<Self::ValidatorId> {
		Validators::<T>::get()
	}

	fn is_validator(account: &Self::ValidatorId) -> bool {
		ValidatorLookup::<T>::contains_key(account)
	}

	fn bond() -> Self::Amount {
		Bond::<T>::get()
	}

	fn epoch_index() -> EpochIndex {
		CurrentEpoch::<T>::get()
	}

	// TODO refactor this name
	fn is_auction_phase() -> bool {
		RotationPhase::<T>::get() != RotationStatus::Idle
	}

	fn active_validator_count() -> u32 {
		Validators::<T>::decode_len().unwrap_or_default() as u32
	}
}

/// Indicates to the session module if the session should be rotated.
impl<T: Config> pallet_session::ShouldEndSession<T::BlockNumber> for Pallet<T> {
	fn should_end_session(_now: T::BlockNumber) -> bool {
		matches!(
			RotationPhase::<T>::get(),
			RotationStatus::VaultsRotated(_) | RotationStatus::SessionRotating(_)
		)
	}
}

impl<T: Config> Pallet<T> {
	/// Starting a new epoch we update the storage, emit the event and call
	/// `EpochTransitionHandler::on_new_epoch`
	fn start_new_epoch(new_validators: &[T::ValidatorId], new_bond: T::Amount) {
		let old_validators = Validators::<T>::get();
		// Update state of current validators
		Validators::<T>::set(new_validators.to_vec());
		ValidatorLookup::<T>::remove_all(None);
		for validator in new_validators {
			ValidatorLookup::<T>::insert(validator, ());
		}
		// The new bond set
		Bond::<T>::set(new_bond);
		// Set the block this epoch starts at
		CurrentEpochStartedAt::<T>::set(frame_system::Pallet::<T>::current_block_number());
		// If we were in an emergency, mark as completed
		Self::emergency_rotation_completed();

		// Calculate the new epoch index
		let new_epoch = CurrentEpoch::<T>::mutate(|epoch| {
			*epoch = epoch.saturating_add(One::one());
			*epoch
		});

		// Emit that a new epoch will be starting
		Self::deposit_event(Event::NewEpoch(new_epoch));

		// Handler for a new epoch
		T::EpochTransitionHandler::on_new_epoch(&old_validators, new_validators, new_bond);
	}
}

/// Provides the new set of validators to the session module when session is being rotated.
impl<T: Config> pallet_session::SessionManager<T::ValidatorId> for Pallet<T> {
	/// If we have a set of confirmed validators we roll them in over two blocks
	fn new_session(_new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		match RotationPhase::<T>::get() {
			RotationStatus::VaultsRotated(auction_result) => Some(auction_result.winners),
			_ => None,
		}
	}

	/// We provide an implementation for this as we already have a set of validators with keys at
	/// genesis
	fn new_session_genesis(_new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		None
	}

	/// The current session is ending
	fn end_session(_end_index: SessionIndex) {}

	/// The session is starting
	fn start_session(_start_index: SessionIndex) {
		if let RotationStatus::SessionRotating(AuctionResult {
			winners, minimum_active_bid, ..
		}) = RotationPhase::<T>::get()
		{
			Pallet::<T>::start_new_epoch(&winners, minimum_active_bid)
		}
	}
}

impl<T: Config> EstimateNextSessionRotation<T::BlockNumber> for Pallet<T> {
	fn average_session_length() -> T::BlockNumber {
		Self::epoch_number_of_blocks()
	}

	fn estimate_current_session_progress(
		_now: T::BlockNumber,
	) -> (Option<sp_runtime::Permill>, Weight) {
		// TODO
		(None, 0)
	}

	fn estimate_next_session_rotation(_now: T::BlockNumber) -> (Option<T::BlockNumber>, Weight) {
		// TODO
		(None, 0)
	}
}

/// In this module, for simplicity, we just return the same AccountId.
pub struct ValidatorOf<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> Convert<T::AccountId, Option<T::AccountId>> for ValidatorOf<T> {
	fn convert(account: T::AccountId) -> Option<T::AccountId> {
		Some(account)
	}
}

impl<T: Config> EmergencyRotation for Pallet<T> {
	fn request_emergency_rotation() -> Weight {
		if !EmergencyRotationRequested::<T>::get() {
			EmergencyRotationRequested::<T>::set(true);
			Pallet::<T>::deposit_event(Event::EmergencyRotationRequested());
			Self::update_rotation_status(RotationStatus::RunAuction);
			return T::DbWeight::get().reads_writes(1, 2)
		}

		T::DbWeight::get().reads_writes(1, 0)
	}

	fn emergency_rotation_in_progress() -> bool {
		EmergencyRotationRequested::<T>::get()
	}

	fn emergency_rotation_completed() {
		if Self::emergency_rotation_in_progress() {
			EmergencyRotationRequested::<T>::set(false);
		}
	}
}

pub struct DeletePeerMapping<T: Config>(PhantomData<T>);

/// Implementation of `OnKilledAccount` ensures that we reconcile any flip dust remaining in the
/// account by burning it.
impl<T: Config> OnKilledAccount<T::AccountId> for DeletePeerMapping<T> {
	fn on_killed_account(account_id: &T::AccountId) {
		if let Some((_, peer_id, _, _)) = AccountPeerMapping::<T>::take(&account_id) {
			MappedPeers::<T>::remove(&peer_id);
			Pallet::<T>::deposit_event(Event::PeerIdUnregistered(account_id.clone(), peer_id));
		}
	}
}

impl<T: Config> HasPeerMapping for Pallet<T> {
	type ValidatorId = T::ValidatorId;

	fn has_peer_mapping(validator_id: &Self::ValidatorId) -> bool {
		AccountPeerMapping::<T>::contains_key(validator_id)
	}
}

impl<T: Config> VaultRotationHandler for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	// The vault rotation has aborted, we reset the ongoing rotation
	fn vault_rotation_aborted() {
		// Quietly check state and reset
		if RotationPhase::<T>::get() != RotationStatus::Idle {
			Self::deposit_event(Event::RotationAborted);
			Self::update_rotation_status(RotationStatus::Idle);
		}
	}
}
