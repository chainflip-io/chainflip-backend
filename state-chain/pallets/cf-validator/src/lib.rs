#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use cf_traits::{
	AuctionPhase, Auctioneer, EmergencyRotation, EpochIndex, EpochInfo, EpochTransitionHandler,
};
use frame_support::{pallet_prelude::*, traits::EstimateNextSessionRotation};
pub use pallet::*;
use sp_runtime::traits::{
	AtLeast32BitUnsigned, BlockNumberProvider, Convert, One, Saturating, Zero,
};
use sp_std::prelude::*;

pub type ValidatorSize = u32;
type SessionIndex = u32;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct SemVer {
	pub major: u8,
	pub minor: u8,
	pub patch: u8,
}

type Version = SemVer;

/// A percentage range
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct PercentageRange {
	pub top: u8,
	pub bottom: u8,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;
	use pallet_session::WeightInfo as SessionWeightInfo;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_session::Config {
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

		/// The range of online validators we would trigger an emergency rotation
		#[pallet::constant]
		type EmergencyRotationPercentageRange: Get<PercentageRange>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new epoch has started \[epoch_index\]
		NewEpoch(EpochIndex),
		/// The number of blocks has changed for our epoch \[from, to\]
		EpochDurationChanged(T::BlockNumber, T::BlockNumber),
		/// A new epoch has been forced
		ForceRotationRequested(),
		/// An emergency rotation has been requested
		EmergencyRotationRequested(),
		/// An validator has send his current CFE version
		ValidatorCFEVersionRecorded(T::AccountId, Version),
	}

	#[pallet::error]
	pub enum Error<T> {
		NoValidators,
		/// Epoch block number supplied is invalid
		InvalidEpoch,
		/// During an auction we can't update certain state
		AuctionInProgress,
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

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
		/// - [AuctionInProgress](Error::AuctionInProgress)
		/// - [InvalidEpoch](Error::InvalidEpoch)
		#[pallet::weight(T::ValidatorWeightInfo::set_blocks_for_epoch())]
		pub fn set_blocks_for_epoch(
			origin: OriginFor<T>,
			number_of_blocks: T::BlockNumber,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(T::Auctioneer::waiting_on_bids(), Error::<T>::AuctionInProgress);
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
		/// - [AuctionInProgress](Error::AuctionInProgress)
		#[pallet::weight(T::ValidatorWeightInfo::force_rotation())]
		pub fn force_rotation(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(T::Auctioneer::waiting_on_bids(), Error::<T>::AuctionInProgress);
			Self::force_validator_rotation();
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

		/// Allow a validator to send their current cfe version.
		///
		/// The dispatch origin of this function must be signed.
		///
		/// ## Events
		///
		/// - [ValidatorCFEVersionRecorded](Event::ValidatorCFEVersionRecorded)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::error::BadOrigin)
		///
		/// ## Dependencies
		///
		/// - None
		#[pallet::weight(T::ValidatorWeightInfo::cfe_version())]
		pub fn cfe_version(origin: OriginFor<T>, semver: Version) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;
			ValidatorCFEVersion::<T>::insert(account_id.clone(), semver.clone());
			Self::deposit_event(Event::ValidatorCFEVersionRecorded(account_id, semver));
			Ok(().into())
		}
	}

	/// Force auction on next block
	#[pallet::storage]
	#[pallet::getter(fn force)]
	pub(super) type Force<T: Config> = StorageValue<_, bool, ValueQuery>;

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
	pub(super) type CurrentEpoch<T: Config> = StorageValue<_, EpochIndex, ValueQuery>;

	/// Validator lookup
	#[pallet::storage]
	#[pallet::getter(fn validator_lookup)]
	pub type ValidatorLookup<T: Config> = StorageMap<_, Blake2_128Concat, T::ValidatorId, ()>;

	/// Validator CFE version
	#[pallet::storage]
	#[pallet::getter(fn validator_cfe_version)]
	pub type ValidatorCFEVersion<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, Version>;

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
			if let Some(auction_result) = T::Auctioneer::auction_result() {
				T::EpochTransitionHandler::on_new_epoch(
					&[],
					&auction_result.winners,
					auction_result.minimum_active_bid,
				);
			}
			CurrentEpoch::<T>::set(0);
			Pallet::<T>::generate_lookup();
		}
	}
}

impl<T: Config> EpochInfo for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;

	fn current_validators() -> Vec<Self::ValidatorId> {
		<pallet_session::Pallet<T>>::validators()
	}

	fn is_validator(account: &Self::ValidatorId) -> bool {
		ValidatorLookup::<T>::contains_key(account)
	}

	fn next_validators() -> Vec<Self::ValidatorId> {
		<pallet_session::Pallet<T>>::queued_keys().into_iter().map(|(k, _)| k).collect()
	}

	fn bond() -> Self::Amount {
		match T::Auctioneer::phase() {
			AuctionPhase::ValidatorsSelected(_, min_bid) => min_bid,
			_ => Zero::zero(),
		}
	}

	fn epoch_index() -> EpochIndex {
		CurrentEpoch::<T>::get()
	}

	fn is_auction_phase() -> bool {
		!T::Auctioneer::waiting_on_bids()
	}
}

/// Indicates to the session module if the session should be rotated.
impl<T: Config> pallet_session::ShouldEndSession<T::BlockNumber> for Pallet<T> {
	fn should_end_session(now: T::BlockNumber) -> bool {
		// If we are waiting on bids let's see if we want to start a new rotation
		match T::Auctioneer::phase() {
			AuctionPhase::WaitingForBids => {
				// If the session should end, start an auction.  We evaluate the first two steps
				// of the auction, validate and select winners, as one.  If this fails we force a
				// new rotation attempt.
				if Self::should_rotate(now) {
					let processed =
						T::Auctioneer::process().is_ok() && T::Auctioneer::process().is_ok();
					if !processed {
						Force::<T>::set(true);
					}
					return processed
				}

				false
			},
			AuctionPhase::ValidatorsSelected(..) => {
				// Confirmation of winners, we need to finally process them
				// This checks whether this is confirmable via the `AuctionConfirmation` trait
				T::Auctioneer::process().is_ok()
			},
			_ => {
				// Do nothing more
				false
			},
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Check whether we should based on either a force rotation or we have reach the epoch
	/// block number
	fn should_rotate(now: T::BlockNumber) -> bool {
		if Force::<T>::get() {
			Force::<T>::set(false);
			return true
		}

		let blocks_per_epoch = BlocksPerEpoch::<T>::get();
		if blocks_per_epoch == Zero::zero() {
			return false
		}
		let current_epoch_started_at = CurrentEpochStartedAt::<T>::get();
		let diff = now.saturating_sub(current_epoch_started_at);

		diff >= blocks_per_epoch
	}

	/// Generate our validator lookup list
	fn generate_lookup() {
		// Update our internal list of validators
		ValidatorLookup::<T>::remove_all(None);
		for validator in <pallet_session::Pallet<T>>::validators() {
			ValidatorLookup::<T>::insert(validator, ());
		}
	}

	fn force_validator_rotation() -> Weight {
		Force::<T>::set(true);
		Pallet::<T>::deposit_event(Event::ForceRotationRequested());

		T::DbWeight::get().reads_writes(0, 1)
	}
}

/// Provides the new set of validators to the session module when session is being rotated.
impl<T: Config> pallet_session::SessionManager<T::ValidatorId> for Pallet<T> {
	/// Prepare candidates for a new session
	fn new_session(_new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		match T::Auctioneer::phase() {
			// Successfully completed the process, these are the next set of validators to be used
			AuctionPhase::ValidatorsSelected(winners, _) => Some(winners),
			// A rotation has occurred, we emit an event of the new epoch and compile a list of
			// validators for validator lookup
			AuctionPhase::ConfirmedValidators(winners, minimum_active_bid) => {
				// If we have a set of winners
				if !winners.is_empty() {
					// If we were in an emergency, mark as completed
					Self::emergency_rotation_completed();
					// Calculate our new epoch index
					let new_epoch = CurrentEpoch::<T>::mutate(|epoch| {
						*epoch = epoch.saturating_add(One::one());
						*epoch
					});
					// Set the block this epoch starts at
					CurrentEpochStartedAt::<T>::set(
						frame_system::Pallet::<T>::current_block_number(),
					);
					// Emit an event
					Self::deposit_event(Event::NewEpoch(new_epoch));
					// Generate our lookup list of validators
					Self::generate_lookup();
					let old_validators = T::Auctioneer::auction_result()
						.expect("from genesis we would expect a previous auction")
						.winners;

					// Our trait callback
					T::EpochTransitionHandler::on_new_epoch(
						&old_validators,
						&winners,
						minimum_active_bid,
					);
				}

				let _ = T::Auctioneer::process();
				None
			},
			// Return
			_ => None,
		}
	}

	/// The current session is ending
	fn end_session(_end_index: SessionIndex) {}
	/// The session is starting
	fn start_session(_start_index: SessionIndex) {}
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
			return T::DbWeight::get().reads_writes(1, 0) + Pallet::<T>::force_validator_rotation()
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
