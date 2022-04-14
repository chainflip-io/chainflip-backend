#![cfg_attr(not(feature = "std"), no_std)]
#![feature(bindings_after_at)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;

pub use weights::WeightInfo;

mod backup_triage;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod migrations;

pub use backup_triage::*;
use cf_traits::{
	offence_reporting::OffenceReporter, AsyncResult, AuctionOutcome, Auctioneer, Chainflip,
	ChainflipAccount, ChainflipAccountData, ChainflipAccountStore, EmergencyRotation, EpochIndex,
	EpochInfo, EpochTransitionHandler, ExecutionCondition, HistoricalEpoch, MissedAuthorshipSlots,
	QualifyValidator, StakeHandler, SuccessOrFailure, VaultRotator,
};
use frame_support::{
	pallet_prelude::*,
	traits::{EstimateNextSessionRotation, OnKilledAccount, OnRuntimeUpgrade, StorageVersion},
};
pub use pallet::*;
use sp_core::ed25519;
use sp_runtime::traits::{BlockNumberProvider, CheckedDiv, Convert, One, Saturating, Zero};
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

use cf_traits::Bonding;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(3);

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

#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub enum RotationStatus<T: Config> {
	Idle,
	RunAuction,
	AwaitingVaults(AuctionOutcome<T>),
	VaultsRotated(AuctionOutcome<T>),
	SessionRotating(AuctionOutcome<T>),
}

impl<T: Config> sp_std::fmt::Debug for RotationStatus<T> {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		match self {
			RotationStatus::Idle => write!(f, "Idle"),
			RotationStatus::RunAuction => write!(f, "RunAuction"),
			RotationStatus::AwaitingVaults(..) => write!(f, "AwaitingVaults(..)"),
			RotationStatus::VaultsRotated(..) => write!(f, "VaultsRotated(..)"),
			RotationStatus::SessionRotating(..) => write!(f, "SessionRotating(..)"),
		}
	}
}

impl<T: Config> Default for RotationStatus<T> {
	fn default() -> Self {
		RotationStatus::Idle
	}
}

/// Id type used for the Keygen and Signing ceremonies.
pub type CeremonyId = u64;

pub struct CeremonyIdProvider<T>(PhantomData<T>);

impl<T: Config> cf_traits::CeremonyIdProvider for CeremonyIdProvider<T> {
	type CeremonyId = CeremonyId;

	fn next_ceremony_id() -> Self::CeremonyId {
		CeremonyIdCounter::<T>::mutate(|id| {
			*id += 1;
			*id
		})
	}
}

type ValidatorIdOf<T> = <T as Chainflip>::ValidatorId;
type VanityName = Vec<u8>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum PalletOffence {
	MissedAuthorshipSlot,
}

pub const MAX_LENGTH_FOR_VANITY_NAME: usize = 64;

pub type Percentage = u8;
#[frame_support::pallet]
pub mod pallet {

	use super::*;
	use frame_system::pallet_prelude::*;
	use pallet_session::WeightInfo as SessionWeightInfo;
	use sp_runtime::app_crypto::RuntimePublic;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::storage_version(PALLET_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config<AccountData = ChainflipAccountData>
		+ Chainflip
		+ pallet_session::Config<ValidatorId = ValidatorIdOf<Self>>
	{
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The top-level offence type must support this pallet's offence type.
		type Offence: From<PalletOffence>;

		/// A handler for epoch lifecycle events
		type EpochTransitionHandler: EpochTransitionHandler<ValidatorId = ValidatorIdOf<Self>>;

		/// Minimum amount of blocks an epoch can run for
		#[pallet::constant]
		type MinEpoch: Get<<Self as frame_system::Config>::BlockNumber>;

		/// Benchmark stuff
		type ValidatorWeightInfo: WeightInfo;

		/// Resolves auctions.
		type Auctioneer: Auctioneer<Self>;

		/// The lifecycle of a vault rotation
		type VaultRotator: VaultRotator<ValidatorId = <Self as Chainflip>::ValidatorId>;

		/// For looking up Chainflip Account data.
		type ChainflipAccount: ChainflipAccount<AccountId = Self::AccountId>;

		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// For retrieving missed authorship slots.
		type MissedAuthorshipSlots: MissedAuthorshipSlots;

		/// For reporting missed authorship slots.
		type OffenceReporter: OffenceReporter<
			ValidatorId = ValidatorIdOf<Self>,
			Offence = Self::Offence,
		>;

		/// The range of online validators we would trigger an emergency rotation
		#[pallet::constant]
		type EmergencyRotationPercentageRange: Get<PercentageRange>;

		/// Updates the bond of a validator
		type Bonder: Bonding<ValidatorId = ValidatorIdOf<Self>, Amount = Self::Amount>;
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
		RotationStatusUpdated(RotationStatus<T>),
		/// An emergency rotation has been requested
		EmergencyRotationRequested(),
		/// The CFE version has been updated \[Validator, Old Version, New Version]
		CFEVersionUpdated(ValidatorIdOf<T>, Version, Version),
		/// A validator has register her current PeerId \[account_id, public_key, port,
		/// ip_address\]
		PeerIdRegistered(T::AccountId, Ed25519PublicKey, u16, Ipv6Addr),
		/// A validator has unregistered her current PeerId \[account_id, public_key\]
		PeerIdUnregistered(T::AccountId, Ed25519PublicKey),
		/// Ratio of claim period updated \[percentage\]
		ClaimPeriodUpdated(Percentage),
		/// Vanity Name for an account has been set \[account_id, vanity_name\]
		VanityNameSet(T::AccountId, VanityName),
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
		/// Invalid claim period
		InvalidClaimPeriod,
		/// Vanity name length exceeds the limit of 64 characters
		NameTooLong,
		/// Invalid characters in the name
		InvalidCharactersInName,
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(block_number: BlockNumberFor<T>) -> Weight {
			// Check expiry of epoch and store last expired
			if let Some(epoch_index) = EpochExpiries::<T>::take(block_number) {
				LastExpiredEpoch::<T>::set(epoch_index);
				Self::expire_epoch(epoch_index);
			}

			// Punish any validators that missed their authorship slot.
			for slot in T::MissedAuthorshipSlots::missed_slots() {
				let validator_index = slot % <Self as EpochInfo>::current_validator_count() as u64;
				if let Some(id) =
					<Self as EpochInfo>::current_validators().get(validator_index as usize)
				{
					T::OffenceReporter::report(PalletOffence::MissedAuthorshipSlot, id.clone());
				} else {
					log::error!(
						"Invalid slot index {:?} when processing missed authorship slots.",
						slot
					);
				}
			}

			match RotationPhase::<T>::get() {
				RotationStatus::Idle => {
					let blocks_per_epoch = BlocksPerEpoch::<T>::get();
					if blocks_per_epoch > Zero::zero() {
						let current_epoch_started_at = CurrentEpochStartedAt::<T>::get();
						let diff = block_number.saturating_sub(current_epoch_started_at);
						if diff >= blocks_per_epoch {
							Self::set_rotation_status(RotationStatus::RunAuction);
						}
					}
				},
				RotationStatus::RunAuction => match T::Auctioneer::resolve_auction() {
					Ok(auction_outcome) => {
						match T::VaultRotator::start_vault_rotation(auction_outcome.winners.clone())
						{
							Ok(_) => Self::set_rotation_status(RotationStatus::AwaitingVaults(
								auction_outcome,
							)),
							// We are assuming here that this is unlikely as the only reason it
							// would fail is if we have no validators, which is already checked by
							// the auction pallet, of if there is already a rotation in progress
							// which isn't possible.
							Err(e) => {
								log::warn!(target: "cf-validator", "starting a vault rotation failed due to error: {:?}", e.into())
							},
						}
					},
					Err(e) =>
						log::warn!(target: "cf-validator", "auction failed due to error: {:?}", e.into()),
				},
				RotationStatus::AwaitingVaults(auction_result) =>
					match T::VaultRotator::get_vault_rotation_outcome() {
						AsyncResult::Ready(SuccessOrFailure::Success) => {
							Self::set_rotation_status(RotationStatus::VaultsRotated(
								auction_result,
							));
						},
						AsyncResult::Ready(SuccessOrFailure::Failure) => {
							Self::deposit_event(Event::RotationAborted);
							Self::set_rotation_status(RotationStatus::RunAuction);
						},
						AsyncResult::Void => {
							log::error!(target: "cf-validator", "no vault rotation pending, returning to auction state");
						},
						AsyncResult::Pending => {
							log::debug!(target: "cf-validator", "awaiting vault rotations");
						},
					},
				RotationStatus::VaultsRotated(auction_result) => {
					Self::set_rotation_status(RotationStatus::SessionRotating(auction_result));
				},
				RotationStatus::SessionRotating(_) => {
					Self::set_rotation_status(RotationStatus::Idle);
				},
			}
			0
		}

		fn on_runtime_upgrade() -> Weight {
			migrations::PalletMigration::<T>::on_runtime_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::pre_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::post_upgrade()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Update the percentage of the epoch period that claims are permitted
		///
		/// The dispatch origin of this function must be governance
		///
		/// ## Events
		///
		/// - [ClaimPeriodUpdated](Event::ClaimPeriodUpdated)
		///
		/// ## Errors
		///
		/// - [InvalidClaimPeriod](Error::InvalidClaimPeriod)
		#[pallet::weight(T::ValidatorWeightInfo::set_blocks_for_epoch())]
		pub fn update_period_for_claims(
			origin: OriginFor<T>,
			percentage: Percentage,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(percentage <= 100, Error::<T>::InvalidClaimPeriod);
			ClaimPeriodAsPercentage::<T>::set(percentage);
			Self::deposit_event(Event::ClaimPeriodUpdated(percentage));

			Ok(().into())
		}
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
			T::EnsureGovernance::ensure_origin(origin)?;
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
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				RotationPhase::<T>::get() == RotationStatus::Idle,
				Error::<T>::RotationInProgress
			);
			Self::set_rotation_status(RotationStatus::RunAuction);

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
			// TODO Consider ensuring is non-private IP / valid IP

			let account_id = ensure_signed(origin)?;

			// Note this signature verify doesn't need replay protection as you need the
			// account_id's private key to pass the above ensure_signed which has replay protection.
			ensure!(
				RuntimePublic::verify(&peer_id, &account_id.encode(), &signature),
				Error::<T>::InvalidAccountPeerMappingSignature
			);

			if let Some((_, existing_peer_id, existing_port, existing_ip_address)) =
				AccountPeerMapping::<T>::get(&account_id)
			{
				if (existing_peer_id, existing_port, existing_ip_address) ==
					(peer_id, port, ip_address)
				{
					// Mapping hasn't changed
					return Ok(().into())
				}

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
			let validator_id: ValidatorIdOf<T> = account_id.into();
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

		#[pallet::weight(T::ValidatorWeightInfo::set_vanity_name())]
		pub fn set_vanity_name(origin: OriginFor<T>, name: Vec<u8>) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;
			ensure!(name.len() <= MAX_LENGTH_FOR_VANITY_NAME, Error::<T>::NameTooLong);
			ensure!(sp_std::str::from_utf8(&name).is_ok(), Error::<T>::InvalidCharactersInName);
			let mut vanity_names = VanityNames::<T>::get();
			vanity_names.insert(account_id.clone(), name.clone());
			VanityNames::<T>::put(vanity_names);
			Self::deposit_event(Event::VanityNameSet(account_id, name));
			Ok(().into())
		}
	}

	/// Percentage of epoch we allow claims
	#[pallet::storage]
	#[pallet::getter(fn claim_period_as_percentage)]
	pub type ClaimPeriodAsPercentage<T: Config> = StorageValue<_, Percentage, ValueQuery>;

	/// An emergency rotation has been requested
	#[pallet::storage]
	#[pallet::getter(fn emergency_rotation_requested)]
	pub(super) type EmergencyRotationRequested<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// The starting block number for the current epoch
	#[pallet::storage]
	#[pallet::getter(fn current_epoch_started_at)]
	pub type CurrentEpochStartedAt<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// The number of blocks an epoch runs for
	#[pallet::storage]
	#[pallet::getter(fn epoch_number_of_blocks)]
	pub type BlocksPerEpoch<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// Current epoch index
	#[pallet::storage]
	#[pallet::getter(fn current_epoch)]
	pub type CurrentEpoch<T: Config> = StorageValue<_, EpochIndex, ValueQuery>;

	/// Defines a unique index for each validator for every epoch.
	#[pallet::storage]
	#[pallet::getter(fn validator_index)]
	pub(super) type ValidatorIndex<T: Config> =
		StorageDoubleMap<_, Twox64Concat, EpochIndex, Blake2_128Concat, ValidatorIdOf<T>, u16>;

	/// Track epochs and their associated validator count
	#[pallet::storage]
	#[pallet::getter(fn epoch_validator_count)]
	pub type EpochValidatorCount<T: Config> = StorageMap<_, Twox64Concat, EpochIndex, u32>;

	/// The rotation phase we are currently at
	#[pallet::storage]
	#[pallet::getter(fn rotation_phase)]
	pub type RotationPhase<T: Config> = StorageValue<_, RotationStatus<T>, ValueQuery>;

	/// A list of the current validators
	#[pallet::storage]
	#[pallet::getter(fn validators)]
	pub type Validators<T: Config> = StorageValue<_, Vec<ValidatorIdOf<T>>, ValueQuery>;

	/// Vanity names of the validators stored as a Map with the current validator IDs as key
	#[pallet::storage]
	#[pallet::getter(fn vanity_names)]
	pub type VanityNames<T: Config> =
		StorageValue<_, BTreeMap<T::AccountId, VanityName>, ValueQuery>;

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
		StorageMap<_, Blake2_128Concat, ValidatorIdOf<T>, Version, ValueQuery>;

	/// The last expired epoch index
	#[pallet::storage]
	pub type LastExpiredEpoch<T: Config> = StorageValue<_, EpochIndex, ValueQuery>;

	/// A map storing the expiry block numbers for old epochs
	#[pallet::storage]
	pub type EpochExpiries<T: Config> =
		StorageMap<_, Twox64Concat, T::BlockNumber, EpochIndex, OptionQuery>;

	/// A map between an epoch and an vector of validators (participating in this epoch)
	#[pallet::storage]
	pub type HistoricalValidators<T: Config> =
		StorageMap<_, Twox64Concat, EpochIndex, Vec<ValidatorIdOf<T>>, ValueQuery>;

	/// A map between an epoch and the bonded balance (MAB)
	#[pallet::storage]
	pub type HistoricalBonds<T: Config> =
		StorageMap<_, Twox64Concat, EpochIndex, T::Amount, ValueQuery>;

	/// A map between an validator and an vector of epoch he attended
	#[pallet::storage]
	pub type HistoricalActiveEpochs<T: Config> =
		StorageMap<_, Twox64Concat, ValidatorIdOf<T>, Vec<EpochIndex>, ValueQuery>;

	/// Counter for generating unique ceremony ids.
	#[pallet::storage]
	#[pallet::getter(fn ceremony_id_counter)]
	pub type CeremonyIdCounter<T> = StorageValue<_, CeremonyId, ValueQuery>;

	/// Backup validator triage state.
	#[pallet::storage]
	#[pallet::getter(fn backup_validator_triage)]
	pub type BackupValidatorTriage<T> = StorageValue<_, RuntimeBackupTriage<T>, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub blocks_per_epoch: T::BlockNumber,
		pub bond: T::Amount,
		pub claim_period_as_percentage: Percentage,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				blocks_per_epoch: Zero::zero(),
				bond: Default::default(),
				claim_period_as_percentage: Zero::zero(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			LastExpiredEpoch::<T>::set(Default::default());
			BlocksPerEpoch::<T>::set(self.blocks_per_epoch);
			RotationPhase::<T>::set(RotationStatus::default());
			CurrentEpochStartedAt::<T>::set(Default::default());
			ClaimPeriodAsPercentage::<T>::set(self.claim_period_as_percentage);
			let genesis_validators = pallet_session::Pallet::<T>::validators();
			Pallet::<T>::start_new_epoch(AuctionOutcome {
				winners: genesis_validators,
				bond: self.bond,
				..Default::default()
			});
		}
	}
}

impl<T: Config> EpochInfo for Pallet<T> {
	type ValidatorId = ValidatorIdOf<T>;
	type Amount = T::Amount;

	fn last_expired_epoch() -> EpochIndex {
		LastExpiredEpoch::<T>::get()
	}

	fn current_validators() -> Vec<Self::ValidatorId> {
		Validators::<T>::get()
	}

	fn current_validator_count() -> u32 {
		Validators::<T>::decode_len().unwrap_or_default() as u32
	}

	fn validator_index(epoch_index: EpochIndex, account: &Self::ValidatorId) -> Option<u16> {
		ValidatorIndex::<T>::get(epoch_index, account)
	}

	fn bond() -> Self::Amount {
		Bond::<T>::get()
	}

	fn epoch_index() -> EpochIndex {
		CurrentEpoch::<T>::get()
	}

	// TODO: This logic is currently duplicated in the CLI. Using an RPC could fix this
	// https://github.com/chainflip-io/chainflip-backend/issues/1462
	fn is_auction_phase() -> bool {
		if RotationPhase::<T>::get() != RotationStatus::Idle {
			return true
		}

		// start + ((epoch * percentage) / 100)
		let last_block_for_claims = CurrentEpochStartedAt::<T>::get().saturating_add(
			BlocksPerEpoch::<T>::get()
				.saturating_mul(ClaimPeriodAsPercentage::<T>::get().into())
				.checked_div(&100u32.into())
				.unwrap_or_default(),
		);

		let current_block_number = frame_system::Pallet::<T>::current_block_number();
		last_block_for_claims <= current_block_number
	}

	fn validator_count_at_epoch(epoch: EpochIndex) -> Option<u32> {
		EpochValidatorCount::<T>::get(epoch)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_validator_index(epoch_index: EpochIndex, account: &Self::ValidatorId, index: u16) {
		ValidatorIndex::<T>::insert(epoch_index, account, index);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_validator_count_for_epoch(epoch_index: EpochIndex, count: u32) {
		EpochValidatorCount::<T>::insert(epoch_index, count);
	}
}

/// Indicates to the session module if the session should be rotated.
///
/// Note: We need to rotate the session pallet twice in order to rotate in the new set of
///       validators due to a limitation in the design of the session pallet. See the
///       substrate issue https://github.com/paritytech/substrate/issues/8650 for context.
///
///       Also see `SessionManager::new_session` impl below.
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
	fn start_new_epoch(auction_outcome: AuctionOutcome<T>) {
		let epoch_validators = auction_outcome.winners;
		let new_bond = auction_outcome.bond;
		let backup_candidates = auction_outcome.losers;

		// Calculate the new epoch index
		let (old_epoch, new_epoch) = CurrentEpoch::<T>::mutate(|epoch| {
			*epoch = epoch.saturating_add(One::one());
			(*epoch - 1, *epoch)
		});

		let mut old_validators = Validators::<T>::get();
		// Update state of current validators
		Validators::<T>::put(&epoch_validators);

		epoch_validators.iter().enumerate().for_each(|(index, account_id)| {
			ValidatorIndex::<T>::insert(&new_epoch, account_id, index as u16);
		});

		EpochValidatorCount::<T>::insert(new_epoch, epoch_validators.len() as u32);

		// The new bond set
		Bond::<T>::set(new_bond);
		// Set the expiry block number for the outgoing set
		EpochExpiries::<T>::insert(
			frame_system::Pallet::<T>::current_block_number() + BlocksPerEpoch::<T>::get(),
			old_epoch,
		);

		// Set the block this epoch starts at
		CurrentEpochStartedAt::<T>::set(frame_system::Pallet::<T>::current_block_number());

		// If we were in an emergency, mark as completed
		Self::emergency_rotation_completed();

		// Save the epoch -> validators map
		HistoricalValidators::<T>::insert(new_epoch, &epoch_validators);

		// Save the bond for each epoch
		HistoricalBonds::<T>::insert(new_epoch, new_bond);

		for validator in epoch_validators.iter() {
			// Remember in which epoch an validator was active
			EpochHistory::<T>::activate_epoch(validator, new_epoch);
			// Bond the validators
			let bond = EpochHistory::<T>::active_bond(validator);
			T::Bonder::update_validator_bond(validator, bond);

			ChainflipAccountStore::<T>::set_current_authority(validator.into_ref());
		}

		// find all the valitators moving out of the epoch
		old_validators.retain(|validator| !epoch_validators.contains(validator));

		old_validators.iter().for_each(|validator| {
			ChainflipAccountStore::<T>::set_historical_validator(validator.into_ref());
		});

		// We've got new validators, which means the backups and passives may have changed
		// TODO configurable parameter to replace '3'.
		Self::update_backup_and_passive_states(backup_candidates, epoch_validators.len() / 3);

		// Handler for a new epoch
		T::EpochTransitionHandler::on_new_epoch(&epoch_validators);

		// Emit that a new epoch will be starting
		Self::deposit_event(Event::NewEpoch(new_epoch));
	}

	fn expire_epoch(epoch: EpochIndex) {
		for validator in EpochHistory::<T>::epoch_validators(epoch).iter() {
			EpochHistory::<T>::deactivate_epoch(validator, epoch);
			if EpochHistory::<T>::number_of_active_epochs_for_validator(validator) == 0 {
				ChainflipAccountStore::<T>::from_historical_to_backup_or_passive(
					validator.into_ref(),
				);
			}
			T::Bonder::update_validator_bond(validator, EpochHistory::<T>::active_bond(validator));
		}
	}

	fn set_rotation_status(new_status: RotationStatus<T>) {
		RotationPhase::<T>::put(new_status.clone());
		Self::deposit_event(Event::RotationStatusUpdated(new_status));
	}

	fn update_backup_and_passive_states(
		backup_candidates: Vec<(ValidatorIdOf<T>, T::Amount)>,
		backup_group_size_target: usize,
	) {
		let triage = RuntimeBackupTriage::<T>::new(backup_candidates, backup_group_size_target);
		triage.update_account_statuses::<T::ChainflipAccount>();
		BackupValidatorTriage::<T>::put(triage);
	}
}

pub struct EpochHistory<T>(PhantomData<T>);

impl<T: Config> HistoricalEpoch for EpochHistory<T> {
	type ValidatorId = ValidatorIdOf<T>;
	type EpochIndex = EpochIndex;
	type Amount = T::Amount;
	fn epoch_validators(epoch: Self::EpochIndex) -> Vec<Self::ValidatorId> {
		HistoricalValidators::<T>::get(epoch)
	}

	fn epoch_bond(epoch: Self::EpochIndex) -> Self::Amount {
		HistoricalBonds::<T>::get(epoch)
	}

	fn active_epochs_for_validator(validator_id: &Self::ValidatorId) -> Vec<Self::EpochIndex> {
		HistoricalActiveEpochs::<T>::get(validator_id)
	}

	fn number_of_active_epochs_for_validator(validator_id: &Self::ValidatorId) -> u32 {
		HistoricalActiveEpochs::<T>::decode_len(validator_id).unwrap_or_default() as u32
	}

	fn deactivate_epoch(validator_id: &Self::ValidatorId, epoch: EpochIndex) {
		HistoricalActiveEpochs::<T>::mutate(validator_id, |active_epochs| {
			active_epochs.retain(|&x| x != epoch);
		});
	}

	fn activate_epoch(validator_id: &Self::ValidatorId, epoch: EpochIndex) {
		HistoricalActiveEpochs::<T>::mutate(validator_id, |epochs| {
			epochs.push(epoch);
		});
	}

	fn active_bond(validator_id: &Self::ValidatorId) -> Self::Amount {
		Self::active_epochs_for_validator(validator_id)
			.iter()
			.map(|epoch| Self::epoch_bond(*epoch))
			.max()
			.unwrap_or_else(|| Self::Amount::from(0_u32))
	}
}

/// Provides the new set of validators to the session module when session is being rotated.
impl<T: Config> pallet_session::SessionManager<ValidatorIdOf<T>> for Pallet<T> {
	/// If we have a set of confirmed validators we roll them in over two blocks. See the comment
	/// on `ShouldEndSession` for further context.
	///
	/// The first rotation queues the new validators, the next rotation queues `None`, and
	/// activates the queued validators.
	fn new_session(_new_index: SessionIndex) -> Option<Vec<ValidatorIdOf<T>>> {
		match RotationPhase::<T>::get() {
			RotationStatus::VaultsRotated(auction_outcome) => Some(auction_outcome.winners),
			_ => None,
		}
	}

	/// We provide an implementation for this as we already have a set of validators with keys at
	/// genesis
	fn new_session_genesis(_new_index: SessionIndex) -> Option<Vec<ValidatorIdOf<T>>> {
		None
	}

	/// The current session is ending
	fn end_session(_end_index: SessionIndex) {}

	/// The session is starting
	fn start_session(_start_index: SessionIndex) {
		if let RotationStatus::SessionRotating(auction_outcome) = RotationPhase::<T>::get() {
			Pallet::<T>::start_new_epoch(auction_outcome)
		}
	}
}

impl<T: Config> EstimateNextSessionRotation<T::BlockNumber> for Pallet<T> {
	fn average_session_length() -> T::BlockNumber {
		Self::epoch_number_of_blocks()
	}

	fn estimate_current_session_progress(
		now: T::BlockNumber,
	) -> (Option<sp_runtime::Permill>, Weight) {
		(
			Some(sp_runtime::Permill::from_rational(
				now.saturating_sub(CurrentEpochStartedAt::<T>::get()),
				BlocksPerEpoch::<T>::get(),
			)),
			T::DbWeight::get().reads(2),
		)
	}

	fn estimate_next_session_rotation(_now: T::BlockNumber) -> (Option<T::BlockNumber>, Weight) {
		(
			Some(CurrentEpochStartedAt::<T>::get() + BlocksPerEpoch::<T>::get()),
			T::DbWeight::get().reads(2),
		)
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
	fn request_emergency_rotation() {
		if !EmergencyRotationRequested::<T>::get() {
			EmergencyRotationRequested::<T>::set(true);
			Pallet::<T>::deposit_event(Event::EmergencyRotationRequested());
			Self::set_rotation_status(RotationStatus::RunAuction);
		}
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

pub struct PeerMapping<T>(PhantomData<T>);

impl<T: Config> QualifyValidator for PeerMapping<T> {
	type ValidatorId = ValidatorIdOf<T>;

	fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
		AccountPeerMapping::<T>::contains_key(validator_id.into_ref())
	}
}

pub struct NotDuringRotation<T: Config>(PhantomData<T>);

impl<T: Config> ExecutionCondition for NotDuringRotation<T> {
	fn is_satisfied() -> bool {
		RotationPhase::<T>::get() == RotationStatus::Idle
	}
}

pub struct UpdateBackupAndPassiveAccounts<T>(PhantomData<T>);

impl<T: Config> StakeHandler for UpdateBackupAndPassiveAccounts<T> {
	type ValidatorId = ValidatorIdOf<T>;
	type Amount = T::Amount;

	fn stake_updated(validator_id: &Self::ValidatorId, amount: Self::Amount) {
		if <Pallet<T> as EpochInfo>::current_validators().contains(validator_id) {
			return
		}

		BackupValidatorTriage::<T>::mutate(|backup_triage| {
			backup_triage.adjust_validator::<T::ChainflipAccount>(validator_id.clone(), amount);
		});
	}
}
