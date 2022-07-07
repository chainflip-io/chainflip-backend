#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]
#![feature(array_zip)]
#![feature(is_sorted)]

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
mod rotation_status;

pub use backup_triage::*;
use cf_traits::{
	offence_reporting::OffenceReporter, AsyncResult, Auctioneer, AuthorityCount, BackupNodes,
	BidderProvider, Bonding, Chainflip, ChainflipAccount, ChainflipAccountData,
	ChainflipAccountStore, EmergencyRotation, EpochIndex, EpochInfo, EpochTransitionHandler,
	ExecutionCondition, HistoricalEpoch, MissedAuthorshipSlots, QualifyNode, ReputationResetter,
	StakeHandler, SystemStateInfo, VaultRotator,
};
use cf_utilities::Port;
use frame_support::{
	pallet_prelude::*,
	traits::{EstimateNextSessionRotation, OnKilledAccount, OnRuntimeUpgrade, StorageVersion},
};
pub use pallet::*;
use sp_core::ed25519;
use sp_runtime::{
	traits::{BlockNumberProvider, CheckedDiv, One, Saturating, UniqueSaturatedInto, Zero},
	Percent,
};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	prelude::*,
};

use crate::rotation_status::RotationStatus;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(4);

type SessionIndex = u32;

#[derive(
	Clone, Debug, Default, PartialEq, Eq, PartialOrd, Encode, Decode, TypeInfo, MaxEncodedLen,
)]
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
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct PercentageRange {
	pub top: u8,
	pub bottom: u8,
}

type RuntimeRotationStatus<T> =
	RotationStatus<<T as Chainflip>::ValidatorId, <T as Chainflip>::Amount>;

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, RuntimeDebugNoBound)]
#[scale_info(skip_type_params(T))]
pub enum RotationPhase<T: Config> {
	Idle,
	VaultsRotating(RuntimeRotationStatus<T>),
	VaultsRotated(RuntimeRotationStatus<T>),
	SessionRotating(RuntimeRotationStatus<T>),
}

impl<T: Config> Default for RotationPhase<T> {
	fn default() -> Self {
		RotationPhase::Idle
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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
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
	#[pallet::without_storage_info]
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

		/// Resolves auctions.
		type Auctioneer: Auctioneer<Self>;

		/// The lifecycle of a vault rotation
		type VaultRotator: VaultRotator<ValidatorId = ValidatorIdOf<Self>>;

		/// For looking up Chainflip Account data.
		type ChainflipAccount: ChainflipAccount<AccountId = Self::AccountId>;

		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// For retrieving missed authorship slots.
		type MissedAuthorshipSlots: MissedAuthorshipSlots;

		/// Used to get the list of bidding nodes in order initialise the backup set post-rotation.
		type BidderProvider: BidderProvider<
			ValidatorId = <Self as Chainflip>::ValidatorId,
			Amount = Self::Amount,
		>;

		/// Criteria that need to be fulfilled to qualify as a validator node (authority, backup or
		/// passive).
		type ValidatorQualification: QualifyNode<ValidatorId = ValidatorIdOf<Self>>;

		/// For reporting missed authorship slots.
		type OffenceReporter: OffenceReporter<
			ValidatorId = ValidatorIdOf<Self>,
			Offence = Self::Offence,
		>;

		/// The range of online authorities we would trigger an emergency rotation
		#[pallet::constant]
		type EmergencyRotationPercentageRange: Get<PercentageRange>;

		/// Updates the bond of an authority.
		type Bonder: Bonding<ValidatorId = ValidatorIdOf<Self>, Amount = Self::Amount>;

		/// This is used to reset the validator's reputation
		type ReputationResetter: ReputationResetter<ValidatorId = ValidatorIdOf<Self>>;

		/// Benchmark weights.
		type ValidatorWeightInfo: WeightInfo;
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
		/// Rotation phase updated.
		RotationPhaseUpdated { new_phase: RotationPhase<T> },
		/// An emergency rotation has been initiated.
		EmergencyRotationInitiated,
		/// The CFE version has been updated \[Validator, Old Version, New Version]
		CFEVersionUpdated(ValidatorIdOf<T>, Version, Version),
		/// An authority has register her current PeerId \[account_id, public_key, port,
		/// ip_address\]
		PeerIdRegistered(T::AccountId, Ed25519PublicKey, Port, Ipv6Addr),
		/// A authority has unregistered her current PeerId \[account_id, public_key\]
		PeerIdUnregistered(T::AccountId, Ed25519PublicKey),
		/// Ratio of claim period updated \[percentage\]
		ClaimPeriodUpdated(Percentage),
		/// Vanity Name for a node has been set \[account_id, vanity_name\]
		VanityNameSet(T::AccountId, VanityName),
		/// The backup node percentage has been updated \[percentage\].
		BackupNodePercentageUpdated(Percentage),
		/// The minimum authority set size has been updated.
		AuthoritySetMinSizeUpdated { min_size: u8 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Epoch block number supplied is invalid.
		InvalidEpoch,
		/// A rotation is in progress.
		RotationInProgress,
		/// Validator Peer mapping overlaps with an existing mapping.
		AccountPeerMappingOverlap,
		/// Invalid signature.
		InvalidAccountPeerMappingSignature,
		/// Invalid claim period.
		InvalidClaimPeriod,
		/// Vanity name length exceeds the limit of 64 characters.
		NameTooLong,
		/// Invalid characters in the name.
		InvalidCharactersInName,
		/// Invalid minimum authority set size.
		InvalidAuthoritySetMinSize,
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(block_number: BlockNumberFor<T>) -> Weight {
			log::trace!(target: "cf-validator", "on_initialize: {:?}",CurrentRotationPhase::<T>::get());
			let mut weight = 0;

			// Check expiry of epoch and store last expired
			if let Some(epoch_index) = EpochExpiries::<T>::take(block_number) {
				LastExpiredEpoch::<T>::set(epoch_index);
				Self::expire_epoch(epoch_index);
			}

			// Punish any validators that missed their authorship slot.
			for slot in T::MissedAuthorshipSlots::missed_slots() {
				let validator_index = slot % <Self as EpochInfo>::current_authority_count() as u64;
				if let Some(id) =
					<Self as EpochInfo>::current_authorities().get(validator_index as usize)
				{
					T::OffenceReporter::report(PalletOffence::MissedAuthorshipSlot, id.clone());
				} else {
					log::error!(
						"Invalid slot index {:?} when processing missed authorship slots.",
						slot
					);
				}
			}

			// Progress the authority rotation if necessary.
			weight += match CurrentRotationPhase::<T>::get() {
				RotationPhase::Idle => {
					if block_number.saturating_sub(CurrentEpochStartedAt::<T>::get()) >=
						BlocksPerEpoch::<T>::get()
					{
						Self::start_authority_rotation()
					} else {
						T::ValidatorWeightInfo::rotation_phase_idle()
					}
				},
				RotationPhase::VaultsRotating(mut rotation_status) => {
					match T::VaultRotator::get_vault_rotation_outcome() {
						AsyncResult::Ready(Ok(_)) => {
							let weight =
								T::ValidatorWeightInfo::rotation_phase_vaults_rotating_success(
									rotation_status.weight_params(),
								);
							Self::set_rotation_phase(RotationPhase::VaultsRotated(rotation_status));
							weight
						},
						AsyncResult::Ready(Err(offenders)) => {
							let weight =
								T::ValidatorWeightInfo::rotation_phase_vaults_rotating_failure(
									offenders.len() as u32,
								);
							rotation_status.ban(offenders);
							Self::start_vault_rotation(rotation_status);
							weight
						},
						AsyncResult::Void => {
							debug_assert!(false, "Void state should be unreachable.");
							log::error!(target: "cf-validator", "no vault rotation pending");
							Self::set_rotation_phase(RotationPhase::Idle);
							// Use the weight of the pending phase.
							T::ValidatorWeightInfo::rotation_phase_vaults_rotating_pending(
								rotation_status.weight_params(),
							)
						},
						AsyncResult::Pending => {
							log::debug!(target: "cf-validator", "awaiting vault rotations");
							T::ValidatorWeightInfo::rotation_phase_vaults_rotating_pending(
								rotation_status.weight_params(),
							)
						},
					}
				},
				RotationPhase::VaultsRotated(rotation_status) =>
					T::ValidatorWeightInfo::rotation_phase_vaults_rotated(
						rotation_status.weight_params(),
					),
				RotationPhase::SessionRotating(rotation_status) =>
					T::ValidatorWeightInfo::rotation_phase_vaults_rotated(
						rotation_status.weight_params(),
					),
			};
			weight
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
		///
		/// ## Dependencies
		///
		/// - [EnsureGovernance]
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
		///
		/// ## Dependencies
		///
		/// - [EnsureGovernance]
		#[pallet::weight(T::ValidatorWeightInfo::set_blocks_for_epoch())]
		pub fn set_blocks_for_epoch(
			origin: OriginFor<T>,
			number_of_blocks: T::BlockNumber,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				CurrentRotationPhase::<T>::get() == RotationPhase::Idle,
				Error::<T>::RotationInProgress
			);
			ensure!(number_of_blocks >= T::MinEpoch::get(), Error::<T>::InvalidEpoch);
			let old_epoch = BlocksPerEpoch::<T>::get();
			ensure!(old_epoch != number_of_blocks, Error::<T>::InvalidEpoch);
			BlocksPerEpoch::<T>::set(number_of_blocks);
			Self::deposit_event(Event::EpochDurationChanged(old_epoch, number_of_blocks));

			Ok(().into())
		}

		/// Force a new epoch. From the next block we will try to move to a new
		/// epoch and rotate our validators.
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
		///
		/// ## Weight
		///
		/// The weight is related to the number of bidders. Getting that number is quite expensive
		/// so we use 2 * authority_count as an approximation.
		#[pallet::weight(T::ValidatorWeightInfo::start_authority_rotation(
			<Pallet<T> as EpochInfo>::current_authority_count() * 2
		))]
		pub fn force_rotation(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				CurrentRotationPhase::<T>::get() == RotationPhase::Idle,
				Error::<T>::RotationInProgress
			);
			Self::start_authority_rotation();

			Ok(().into())
		}

		/// Allow a node to set their keys for upcoming sessions
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

		/// Allow a node to link their validator id to a peer id
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
			port: Port,
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

		/// Allow a node to send their current cfe version.  We validate that the version is a
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
		///
		/// ## Dependencies
		///
		/// - None
		#[pallet::weight(T::ValidatorWeightInfo::cfe_version())]
		pub fn cfe_version(origin: OriginFor<T>, version: Version) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;
			let validator_id = <ValidatorIdOf<T> as IsType<
				<T as frame_system::Config>::AccountId,
			>>::from_ref(&account_id);
			NodeCFEVersion::<T>::try_mutate(&validator_id, |current_version| {
				if *current_version != version {
					Self::deposit_event(Event::CFEVersionUpdated(
						validator_id.clone(),
						current_version.clone(),
						version.clone(),
					));
					*current_version = version;
				}
				Ok(().into())
			})
		}

		/// Allow a node to set a "Vanity Name" for themselves. This is functionally
		/// useless but can be used to make the network a bit more friendly for
		/// observers. Names are required to be <= MAX_LENGTH_FOR_VANITY_NAME (64)
		/// UTF-8 bytes.
		///
		/// The dispatch origin of this function must be signed.
		///
		/// ## Events
		///
		/// - [VanityNameSet](Event::VanityNameSet)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::error::BadOrigin)
		/// - [NameTooLong](Error::NameTooLong)
		/// - [InvalidCharactersInName](Error::InvalidCharactersInName)
		///
		/// ## Dependencies
		///
		/// - None
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

		/// Allow governance to set the percentage of Validators that should be set
		/// as backup Validators. This percentage is relative to the total permitted
		/// number of Authorities.
		///
		/// The dispatch origin of this function must be governance.
		///
		/// ## Events
		///
		/// - [BackupNodePercentageUpdated](Event::BackupNodePercentageUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::error::BadOrigin)
		///
		/// ## Dependencies
		///
		/// - [EnsureGovernance]
		#[pallet::weight(T::ValidatorWeightInfo::set_backup_node_percentage())]
		pub fn set_backup_node_percentage(
			origin: OriginFor<T>,
			percentage: Percentage,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			BackupNodePercentage::<T>::put(percentage);

			Self::deposit_event(Event::BackupNodePercentageUpdated(percentage));
			Ok(().into())
		}

		/// Allow governance to set the minimum size of the authority set.
		///
		/// The dispatch origin of this function must be governance.
		///
		/// ## Events
		///
		/// - [BackupNodePercentageUpdated](Event::BackupNodePercentageUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::error::BadOrigin)
		/// - [InvalidAuthoritySetSize](Error::InvalidAuthoritySetSize)
		///
		/// ## Dependencies
		///
		/// - [EnsureGovernance]
		#[pallet::weight(T::ValidatorWeightInfo::set_authority_set_min_size())]
		pub fn set_authority_set_min_size(
			origin: OriginFor<T>,
			min_size: u8,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				u32::from(min_size) <= <Self as EpochInfo>::current_authority_count(),
				Error::<T>::InvalidAuthoritySetMinSize
			);

			AuthoritySetMinSize::<T>::put(min_size);

			Self::deposit_event(Event::AuthoritySetMinSizeUpdated { min_size });
			Ok(().into())
		}
	}

	/// Percentage of epoch we allow claims
	#[pallet::storage]
	#[pallet::getter(fn claim_period_as_percentage)]
	pub type ClaimPeriodAsPercentage<T: Config> = StorageValue<_, Percentage, ValueQuery>;

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

	/// Defines a unique index for each authority for each epoch.
	#[pallet::storage]
	#[pallet::getter(fn authority_index)]
	pub type AuthorityIndex<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		EpochIndex,
		Blake2_128Concat,
		ValidatorIdOf<T>,
		AuthorityCount,
	>;

	/// Track epochs and their associated authority count
	#[pallet::storage]
	#[pallet::getter(fn epoch_authority_count)]
	pub type EpochAuthorityCount<T: Config> =
		StorageMap<_, Twox64Concat, EpochIndex, AuthorityCount>;

	/// The rotation phase we are currently at
	#[pallet::storage]
	#[pallet::getter(fn current_rotation_phase)]
	pub type CurrentRotationPhase<T: Config> = StorageValue<_, RotationPhase<T>, ValueQuery>;

	/// A list of the current authorites
	#[pallet::storage]
	pub type CurrentAuthorities<T: Config> = StorageValue<_, Vec<ValidatorIdOf<T>>, ValueQuery>;

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
	#[pallet::getter(fn node_peer_id)]
	pub type AccountPeerMapping<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		(T::AccountId, Ed25519PublicKey, Port, Ipv6Addr),
	>;

	/// Peers that are associated with account ids
	#[pallet::storage]
	#[pallet::getter(fn mapped_peer)]
	pub type MappedPeers<T: Config> = StorageMap<_, Blake2_128Concat, Ed25519PublicKey, ()>;

	/// Node CFE version
	#[pallet::storage]
	#[pallet::getter(fn node_cfe_version)]
	pub type NodeCFEVersion<T: Config> =
		StorageMap<_, Blake2_128Concat, ValidatorIdOf<T>, Version, ValueQuery>;

	/// The last expired epoch index
	#[pallet::storage]
	pub type LastExpiredEpoch<T: Config> = StorageValue<_, EpochIndex, ValueQuery>;

	/// A map storing the expiry block numbers for old epochs
	#[pallet::storage]
	pub type EpochExpiries<T: Config> =
		StorageMap<_, Twox64Concat, T::BlockNumber, EpochIndex, OptionQuery>;

	/// A map between an epoch and an vector of authorities (participating in this epoch)
	#[pallet::storage]
	pub type HistoricalAuthorities<T: Config> =
		StorageMap<_, Twox64Concat, EpochIndex, Vec<ValidatorIdOf<T>>, ValueQuery>;

	/// A map between an epoch and the bonded balance (MAB)
	#[pallet::storage]
	pub type HistoricalBonds<T: Config> =
		StorageMap<_, Twox64Concat, EpochIndex, T::Amount, ValueQuery>;

	/// A map between an authority and a set of all the active epochs a node was an authority in
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

	/// Determines the target size for the set of backup nodes. Expressed as a percentage of the
	/// authority set size.
	#[pallet::storage]
	#[pallet::getter(fn backup_node_percentage)]
	pub type BackupNodePercentage<T> = StorageValue<_, Percentage, ValueQuery>;

	/// The absolute minimum number of authority nodes for the next epoch.
	#[pallet::storage]
	#[pallet::getter(fn authority_set_min_size)]
	pub type AuthoritySetMinSize<T> = StorageValue<_, u8, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub genesis_authorities: Vec<ValidatorIdOf<T>>,
		pub blocks_per_epoch: T::BlockNumber,
		pub bond: T::Amount,
		pub claim_period_as_percentage: Percentage,
		pub backup_node_percentage: Percentage,
		pub authority_set_min_size: u8,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				genesis_authorities: Default::default(),
				blocks_per_epoch: Zero::zero(),
				bond: Default::default(),
				claim_period_as_percentage: Zero::zero(),
				backup_node_percentage: Zero::zero(),
				authority_set_min_size: Zero::zero(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			LastExpiredEpoch::<T>::set(Default::default());
			BlocksPerEpoch::<T>::set(self.blocks_per_epoch);
			AuthoritySetMinSize::<T>::set(self.authority_set_min_size);
			CurrentRotationPhase::<T>::set(RotationPhase::Idle);
			ClaimPeriodAsPercentage::<T>::set(self.claim_period_as_percentage);
			BackupNodePercentage::<T>::set(self.backup_node_percentage);

			const GENESIS_EPOCH: u32 = 1;
			CurrentEpoch::<T>::set(GENESIS_EPOCH);
			for id in &self.genesis_authorities {
				ChainflipAccountStore::<T>::set_current_authority(id.into_ref());
			}
			Pallet::<T>::initialise_new_epoch(
				GENESIS_EPOCH,
				&self.genesis_authorities,
				self.bond,
				RuntimeBackupTriage::<T>::new::<T::ChainflipAccount>(vec![], 0),
			);
		}
	}
}

impl<T: Config> EpochInfo for Pallet<T> {
	type ValidatorId = ValidatorIdOf<T>;
	type Amount = T::Amount;

	fn last_expired_epoch() -> EpochIndex {
		LastExpiredEpoch::<T>::get()
	}

	fn current_authorities() -> Vec<Self::ValidatorId> {
		CurrentAuthorities::<T>::get()
	}

	fn current_authority_count() -> AuthorityCount {
		CurrentAuthorities::<T>::decode_len().unwrap_or_default() as AuthorityCount
	}

	fn authority_index(
		epoch_index: EpochIndex,
		account: &Self::ValidatorId,
	) -> Option<AuthorityCount> {
		AuthorityIndex::<T>::get(epoch_index, account)
	}

	fn bond() -> Self::Amount {
		Bond::<T>::get()
	}

	fn epoch_index() -> EpochIndex {
		CurrentEpoch::<T>::get()
	}

	fn is_auction_phase() -> bool {
		if CurrentRotationPhase::<T>::get() != RotationPhase::Idle {
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

	fn authority_count_at_epoch(epoch: EpochIndex) -> Option<AuthorityCount> {
		EpochAuthorityCount::<T>::get(epoch)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add_authority_info_for_epoch(
		epoch_index: EpochIndex,
		new_authorities: Vec<Self::ValidatorId>,
	) {
		EpochAuthorityCount::<T>::insert(epoch_index, new_authorities.len() as AuthorityCount);
		for (i, authority) in new_authorities.iter().enumerate() {
			AuthorityIndex::<T>::insert(epoch_index, authority, i as AuthorityCount);
			HistoricalActiveEpochs::<T>::append(authority, epoch_index);
		}
		HistoricalAuthorities::<T>::insert(epoch_index, new_authorities);
	}
}

/// Indicates to the session module if the session should be rotated.
///
/// Note: We need to rotate the session pallet twice in order to rotate in the new set of
/// validators due to a limitation in the design of the session pallet. See the
/// substrate issue https://github.com/paritytech/substrate/issues/8650 for context.
///
/// Also see [SessionManager::new_session] impl below.
impl<T: Config> pallet_session::ShouldEndSession<T::BlockNumber> for Pallet<T> {
	fn should_end_session(_now: T::BlockNumber) -> bool {
		matches!(
			CurrentRotationPhase::<T>::get(),
			RotationPhase::VaultsRotated(_) | RotationPhase::SessionRotating(_)
		)
	}
}

impl<T: Config> Pallet<T> {
	/// Makes the transition to the next epoch.
	///
	/// Among other things, updates the authority, backup and passive sets.
	///
	/// Also triggers [T::EpochTransitionHandler::on_new_epoch] which may call into other pallets.
	///
	/// Note this function is not benchmarked - it is only ever triggered via the session pallet,
	/// which at the time of writing uses `T::BlockWeights::get().max_block` ie. it implicitly fills
	/// the block.
	fn transition_to_next_epoch(rotation_status: RuntimeRotationStatus<T>) {
		log::debug!(target: "cf-validator", "Starting new epoch");

		// Update epoch numbers.
		let (old_epoch, new_epoch) = CurrentEpoch::<T>::mutate(|epoch| {
			*epoch = epoch.saturating_add(One::one());
			(*epoch - 1, *epoch)
		});

		// Set the expiry block number for the old epoch.
		EpochExpiries::<T>::insert(
			frame_system::Pallet::<T>::current_block_number() + BlocksPerEpoch::<T>::get(),
			old_epoch,
		);

		// Update current / historical authority status.
		let new_authorities_lookup = rotation_status.authority_candidates::<BTreeSet<_>>();
		let old_authorities_lookup =
			BTreeSet::<ValidatorIdOf<T>>::from_iter(CurrentAuthorities::<T>::get().into_iter());
		for historical_authority in old_authorities_lookup
			.iter()
			.filter(|authority| !new_authorities_lookup.contains(authority))
		{
			ChainflipAccountStore::<T>::set_historical_authority(historical_authority.into_ref());
		}

		for incoming_authority in new_authorities_lookup
			.iter()
			.filter(|authority| !old_authorities_lookup.contains(authority))
		{
			ChainflipAccountStore::<T>::set_current_authority(incoming_authority.into_ref());
		}

		let new_authorities = rotation_status.authority_candidates::<Vec<_>>();
		Self::initialise_new_epoch(
			new_epoch,
			&new_authorities,
			rotation_status.bond,
			RuntimeBackupTriage::<T>::new::<T::ChainflipAccount>(
				T::BidderProvider::get_bidders()
					.into_iter()
					.filter(|bid| {
						!new_authorities_lookup.contains(&bid.bidder_id) &&
							T::ValidatorQualification::is_qualified(&bid.bidder_id)
					})
					.collect(),
				Self::backup_set_target_size(
					new_authorities_lookup.len(),
					BackupNodePercentage::<T>::get(),
				),
			),
		);

		// Trigger the new epoch handlers on other pallets.
		T::EpochTransitionHandler::on_new_epoch(&new_authorities);

		Self::deposit_event(Event::NewEpoch(new_epoch));
	}

	fn expire_epoch(epoch: EpochIndex) {
		for authority in EpochHistory::<T>::epoch_authorities(epoch).iter() {
			EpochHistory::<T>::deactivate_epoch(authority, epoch);
			if EpochHistory::<T>::number_of_active_epochs_for_authority(authority) == 0 {
				ChainflipAccountStore::<T>::from_historical_to_backup_or_passive(
					authority.into_ref(),
				);
				T::ReputationResetter::reset_reputation(authority);
			}
			T::Bonder::update_bond(authority, EpochHistory::<T>::active_bond(authority));
		}
	}

	/// Does all state updates related to the *new* epoch. Is also called at genesis to initialise
	/// pallet state. Should not update any external state that is not managed by the validator
	/// pallet, ie. should not call `on_new_epoch`. Also does not need to concern itself with
	/// expiries etc. that relate to the state of previous epochs.
	fn initialise_new_epoch(
		new_epoch: EpochIndex,
		new_authorities: &[ValidatorIdOf<T>],
		new_bond: T::Amount,
		backup_triage: RuntimeBackupTriage<T>,
	) {
		CurrentAuthorities::<T>::put(new_authorities);
		HistoricalAuthorities::<T>::insert(new_epoch, new_authorities);
		Bond::<T>::set(new_bond);

		new_authorities.iter().enumerate().for_each(|(index, account_id)| {
			AuthorityIndex::<T>::insert(&new_epoch, account_id, index as AuthorityCount);
		});

		EpochAuthorityCount::<T>::insert(new_epoch, new_authorities.len() as AuthorityCount);

		CurrentEpochStartedAt::<T>::set(frame_system::Pallet::<T>::current_block_number());

		// Save the bond for each epoch
		HistoricalBonds::<T>::insert(new_epoch, new_bond);

		for authority in new_authorities {
			EpochHistory::<T>::activate_epoch(authority, new_epoch);
			T::Bonder::update_bond(authority, EpochHistory::<T>::active_bond(authority));
		}

		// We've got new validators, which means the backups and passives may have changed.
		BackupValidatorTriage::<T>::put(backup_triage);
	}

	fn set_rotation_phase(new_phase: RotationPhase<T>) {
		log::debug!(target: "cf-validator", "Advancing rotation phase to: {new_phase:?}");
		CurrentRotationPhase::<T>::put(new_phase.clone());
		Self::deposit_event(Event::RotationPhaseUpdated { new_phase });
	}

	fn backup_set_target_size(num_authorities: usize, backup_node_percentage: Percentage) -> usize {
		Percent::from_percent(backup_node_percentage) * num_authorities
	}

	fn start_authority_rotation() -> Weight {
		if T::SystemState::is_maintenance_mode() {
			log::info!(
				target: "cf-validator",
				"Can't start rotation. System is in maintenance mode."
			);
			return T::ValidatorWeightInfo::start_authority_rotation_in_maintenance_mode()
		}
		log::info!(target: "cf-validator", "Starting rotation");
		match T::Auctioneer::resolve_auction() {
			Ok(auction_outcome) => {
				debug_assert!(!auction_outcome.winners.is_empty());
				debug_assert!({
					let bids = T::BidderProvider::get_bidders()
						.into_iter()
						.map(|bid| (bid.bidder_id, bid.amount))
						.collect::<BTreeMap<_, _>>();
					auction_outcome.winners.iter().map(|id| bids.get(id)).is_sorted_by_key(Reverse)
				});
				log::info!(
					target: "cf-validator",
					"Auction resolved with {} winners and {} losers. Bond will be {}FLIP.",
					auction_outcome.winners.len(),
					auction_outcome.losers.len(),
					UniqueSaturatedInto::<u128>::unique_saturated_into(auction_outcome.bond) /
						10u128.pow(18),
				);

				// Without reading the full list of bidders we can't know the real number.
				// Use the winners and losers as an approximation.
				let weight = T::ValidatorWeightInfo::start_authority_rotation(
					(auction_outcome.winners.len() + auction_outcome.losers.len()) as u32,
				);

				Self::start_vault_rotation(RotationStatus::from_auction_outcome::<T>(
					auction_outcome,
				));

				weight
			},
			Err(e) => {
				log::warn!(target: "cf-validator", "auction failed due to error: {:?}", e.into());
				// Use an approximation again - see comment above.
				T::ValidatorWeightInfo::start_authority_rotation(
					Self::current_authority_count() +
						<Self as BackupNodes>::backup_nodes().len() as u32,
				)
			},
		}
	}

	fn start_vault_rotation(rotation_status: RuntimeRotationStatus<T>) {
		let candidates: Vec<_> = rotation_status.authority_candidates();
		if candidates.len() < AuthoritySetMinSize::<T>::get().into() {
			log::warn!(
				target: "cf-validator",
				"Only {:?} authority candidates available, not enough to satisfy the minimum set size of {:?}. - aborting rotation.",
				candidates.len(),
				AuthoritySetMinSize::<T>::get()
			);
			Self::set_rotation_phase(RotationPhase::Idle);
		} else {
			match T::VaultRotator::start_vault_rotation(candidates) {
				Ok(()) => {
					log::info!(target: "cf-validator", "Vault rotation initiated.");
					Self::set_rotation_phase(RotationPhase::VaultsRotating(rotation_status));
				},
				Err(e) => {
					log::error!(target: "cf-validator", "Unable to start vault rotation: {:?}", e);
					#[cfg(not(test))]
					debug_assert!(false, "Unable to start vault rotation");
				},
			}
		}
	}
}

pub struct EpochHistory<T>(PhantomData<T>);

impl<T: Config> HistoricalEpoch for EpochHistory<T> {
	type ValidatorId = ValidatorIdOf<T>;
	type EpochIndex = EpochIndex;
	type Amount = T::Amount;
	fn epoch_authorities(epoch: Self::EpochIndex) -> Vec<Self::ValidatorId> {
		HistoricalAuthorities::<T>::get(epoch)
	}

	fn epoch_bond(epoch: Self::EpochIndex) -> Self::Amount {
		HistoricalBonds::<T>::get(epoch)
	}

	fn active_epochs_for_authority(authority: &Self::ValidatorId) -> Vec<Self::EpochIndex> {
		HistoricalActiveEpochs::<T>::get(authority)
	}

	fn number_of_active_epochs_for_authority(authority: &Self::ValidatorId) -> u32 {
		HistoricalActiveEpochs::<T>::decode_len(authority).unwrap_or_default() as u32
	}

	fn deactivate_epoch(authority: &Self::ValidatorId, epoch: EpochIndex) {
		HistoricalActiveEpochs::<T>::mutate(authority, |active_epochs| {
			active_epochs.retain(|&x| x != epoch);
		});
	}

	fn activate_epoch(authority: &Self::ValidatorId, epoch: EpochIndex) {
		HistoricalActiveEpochs::<T>::mutate(authority, |epochs| {
			epochs.push(epoch);
		});
	}

	fn active_bond(authority: &Self::ValidatorId) -> Self::Amount {
		Self::active_epochs_for_authority(authority)
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
		match CurrentRotationPhase::<T>::get() {
			RotationPhase::VaultsRotated(rotation_status) => {
				let next_authorities = rotation_status.authority_candidates();
				Self::set_rotation_phase(RotationPhase::SessionRotating(rotation_status));
				Some(next_authorities)
			},
			RotationPhase::SessionRotating(rotation_status) => {
				Self::set_rotation_phase(RotationPhase::Idle);
				None
			},
			_ => None,
		}
	}

	/// These Validators' keys must be registered as part of the session pallet genesis.
	fn new_session_genesis(_new_index: SessionIndex) -> Option<Vec<ValidatorIdOf<T>>> {
		let genesis_authorities = Self::current_authorities();
		assert!(
			!genesis_authorities.is_empty(),
			"No genesis authorities found! Make sure the Validator pallet is initialised before the Session pallet."
		);
		Some(genesis_authorities)
	}

	/// The current session is ending
	fn end_session(_end_index: SessionIndex) {}

	/// The session is starting
	fn start_session(_start_index: SessionIndex) {
		if let RotationPhase::SessionRotating(rotation_status) = CurrentRotationPhase::<T>::get() {
			Pallet::<T>::transition_to_next_epoch(rotation_status)
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

impl<T: Config> EmergencyRotation for Pallet<T> {
	fn request_emergency_rotation() {
		if CurrentRotationPhase::<T>::get() == RotationPhase::<T>::Idle {
			Pallet::<T>::deposit_event(Event::EmergencyRotationInitiated);
			Self::start_authority_rotation();
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

impl<T: Config> QualifyNode for PeerMapping<T> {
	type ValidatorId = ValidatorIdOf<T>;

	fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
		AccountPeerMapping::<T>::contains_key(validator_id.into_ref())
	}
}

pub struct NotDuringRotation<T: Config>(PhantomData<T>);

impl<T: Config> ExecutionCondition for NotDuringRotation<T> {
	fn is_satisfied() -> bool {
		CurrentRotationPhase::<T>::get() == RotationPhase::Idle
	}
}

pub struct UpdateBackupAndPassiveAccounts<T>(PhantomData<T>);

impl<T: Config> StakeHandler for UpdateBackupAndPassiveAccounts<T> {
	type ValidatorId = ValidatorIdOf<T>;
	type Amount = T::Amount;

	fn on_stake_updated(validator_id: &Self::ValidatorId, amount: Self::Amount) {
		if <Pallet<T> as EpochInfo>::current_authorities().contains(validator_id) {
			return
		}
		if T::ValidatorQualification::is_qualified(validator_id) {
			BackupValidatorTriage::<T>::mutate(|backup_triage| {
				backup_triage.adjust_bid::<T::ChainflipAccount>(validator_id.clone(), amount);
			});
		}
	}
}
