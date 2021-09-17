#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip Validator Module
//!
//! A module to manage the validator set for the Chainflip State Chain
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Module`]
//!
//! ## Overview
//!
//! The module contains functionality to manage the validator set used to ensure the Chainflip
//! State Chain network.  It extends on the functionality offered by the `session` pallet provided by
//! Parity.  At every epoch block length, or if forced, the `Auction` pallet proposes a set of new
//! validators.  The process of auction runs over 2 blocks to achieve a finalised candidate set and
//! anytime after this, based on confirmation of the auction(see `AuctionConfirmation`) the new set
//! will become the validating set.
//!
//! ## Terminology
//!
//! - **Validator:** A node that has staked an amount of `FLIP` ERC20 token.
//!
//! - **Validator ID:** Equivalent to an Account ID
//!
//! - **Epoch:** A period in blocks in which a constant set of validators ensure the network.
//!
//! - **Auction** A non defined period of blocks in which we continue with the existing validators
//!   and assess the new candidate set of their validity as validators.  This functionality is provided
//!   by the `Auction` pallet.  We rotate the set of validators on each `AuctionPhase::Completed` phase
//!   completed by the `Auction` pallet.
//!
//! - **Session:** A session as defined by the `session` pallet.
//!
//! - **Sudo:** A single account that is also called the "sudo key" which allows "privileged functions"
//!
//! ### Dispatchable Functions
//!
//! - `set_blocks_for_epoch` - Set the number of blocks an Epoch should run for.
//! - `force_rotation` - Force a rotation of validators to start on the next block.
//!

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use cf_traits::{Auction, AuctionPhase, EmergencyRotation, EpochInfo};
use frame_support::pallet_prelude::*;
use frame_support::sp_runtime::traits::{Saturating, Zero};
pub use pallet::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, Convert, One, OpaqueKeys};
use sp_std::prelude::*;

pub trait WeightInfo {
	fn set_blocks_for_epoch() -> Weight;
	fn force_rotation() -> Weight;
}

pub type ValidatorSize = u32;
type SessionIndex = u32;

/// Handler for Epoch life cycle events.
pub trait EpochTransitionHandler {
	/// The id type used for the validators.
	type ValidatorId;
	type Amount: Copy;
	/// A new epoch has started
	///
	/// The new set of validator `new_validators` are now validating
	fn on_new_epoch(_new_validators: &Vec<Self::ValidatorId>, _new_bond: Self::Amount) {}
}

impl<T: Config> EpochTransitionHandler for PhantomData<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::ChainflipAccount;
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

		/// An index describing the epoch
		type EpochIndex: Member + codec::FullCodec + Copy + AtLeast32BitUnsigned + Default;

		/// An amount
		type Amount: Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;

		/// An auction type
		type Auction: Auction<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;

		/// A Chainflip Account
		type ChainflipAccount: ChainflipAccount<AccountId = Self::AccountId>;

		/// Convert ValidatorId to AccountId
		type AccountIdOf: Convert<Self::ValidatorId, Option<Self::AccountId>>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new epoch has started \[epoch_index\]
		NewEpoch(T::EpochIndex),
		/// The number of blocks has changed for our epoch \[from, to\]
		EpochDurationChanged(T::BlockNumber, T::BlockNumber),
		/// A new epoch has been forced
		ForceRotationRequested(),
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
		#[pallet::weight(T::ValidatorWeightInfo::set_blocks_for_epoch())]
		pub(super) fn set_blocks_for_epoch(
			origin: OriginFor<T>,
			number_of_blocks: T::BlockNumber,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(T::Auction::waiting_on_bids(), Error::<T>::AuctionInProgress);
			ensure!(
				number_of_blocks >= T::MinEpoch::get(),
				Error::<T>::InvalidEpoch
			);
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
		#[pallet::weight(10_000)]
		pub(super) fn force_rotation(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(T::Auction::waiting_on_bids(), Error::<T>::AuctionInProgress);
			Self::force_validator_rotation();
			Ok(().into())
		}

		/// Allow a validator to set their keys for upcoming sessions
		///
		/// The dispatch origin of this function must be signed.
		#[pallet::weight(< T as pallet_session::Config >::WeightInfo::set_keys())]
		pub(super) fn set_keys(
			origin: OriginFor<T>,
			keys: T::Keys,
			proof: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			<pallet_session::Module<T>>::set_keys(origin, keys, proof)?;
			Ok(().into())
		}
	}

	/// Force auction on next block
	#[pallet::storage]
	#[pallet::getter(fn force)]
	pub(super) type Force<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// The starting block number for the current epoch
	#[pallet::storage]
	#[pallet::getter(fn last_block_number)]
	pub(super) type LastBlockNumber<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// The number of blocks an epoch runs for
	#[pallet::storage]
	#[pallet::getter(fn epoch_number_of_blocks)]
	pub(super) type BlocksPerEpoch<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// Current epoch index
	#[pallet::storage]
	#[pallet::getter(fn current_epoch)]
	pub(super) type CurrentEpoch<T: Config> = StorageValue<_, T::EpochIndex, ValueQuery>;

	/// Validator lookup
	#[pallet::storage]
	pub(super) type ValidatorLookup<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, ()>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub epoch_number_of_blocks: T::BlockNumber,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				epoch_number_of_blocks: Zero::zero(),
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			// The auction pallet should have ran through an auction
			if let AuctionPhase::WaitingForBids(winners, min_bid, ..) = T::Auction::phase() {
				T::EpochTransitionHandler::on_new_epoch(&winners, min_bid);
			}
			Pallet::<T>::generate_lookup();
		}
	}
}

impl<T: Config> EpochInfo for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;
	type EpochIndex = T::EpochIndex;

	fn current_validators() -> Vec<Self::ValidatorId> {
		<pallet_session::Module<T>>::validators()
	}

	fn is_validator(account: &Self::ValidatorId) -> bool {
		ValidatorLookup::<T>::contains_key(account)
	}

	fn next_validators() -> Vec<Self::ValidatorId> {
		<pallet_session::Module<T>>::queued_keys()
			.into_iter()
			.map(|(k, _)| k)
			.collect()
	}

	fn bond() -> Self::Amount {
		match T::Auction::phase() {
			AuctionPhase::ValidatorsSelected(_, min_bid) => min_bid,
			_ => Zero::zero(),
		}
	}

	fn epoch_index() -> Self::EpochIndex {
		CurrentEpoch::<T>::get()
	}

	fn is_auction_phase() -> bool {
		!T::Auction::waiting_on_bids()
	}
}

impl<T: Config> pallet_session::SessionHandler<T::ValidatorId> for Pallet<T> {
	/// TODO look at the key management
	const KEY_TYPE_IDS: &'static [sp_runtime::KeyTypeId] = &[];
	fn on_genesis_session<Ks: OpaqueKeys>(_validators: &[(T::ValidatorId, Ks)]) {}
	fn on_new_session<Ks: OpaqueKeys>(
		_changed: bool,
		_validators: &[(T::ValidatorId, Ks)],
		_queued_validators: &[(T::ValidatorId, Ks)],
	) {
	}
	fn on_before_session_ending() {}
	fn on_disabled(_validator_index: usize) {}
}

/// Indicates to the session module if the session should be rotated.
impl<T: Config> pallet_session::ShouldEndSession<T::BlockNumber> for Pallet<T> {
	fn should_end_session(now: T::BlockNumber) -> bool {
		// If we are waiting on bids let's see if we want to start a new rotation
		return match T::Auction::phase() {
			AuctionPhase::WaitingForBids(..) => {
				// If the session should end, run through an auction
				// two steps- validate and select winners
				Self::should_rotate(now) && T::Auction::process().and(T::Auction::process()).is_ok()
			}
			AuctionPhase::ValidatorsSelected(..) => {
				// Confirmation of winners, we need to finally process them
				// This checks whether this is confirmable via the `AuctionConfirmation` trait
				T::Auction::process().is_ok()
			}
			// Failing that do nothing
			_ => false,
		};
	}
}

impl<T: Config> Pallet<T> {
	/// Check whether we should based on either a force rotation or we have reach the epoch
	/// block number
	fn should_rotate(now: T::BlockNumber) -> bool {
		if Force::<T>::get() {
			Force::<T>::set(false);
			return true;
		}

		let epoch_blocks = BlocksPerEpoch::<T>::get();
		if epoch_blocks == Zero::zero() {
			return false;
		}
		let last_block_number = LastBlockNumber::<T>::get();
		let diff = now.saturating_sub(last_block_number);
		let end = diff >= epoch_blocks;
		if end {
			LastBlockNumber::<T>::set(now);
		}

		return end;
	}

	/// Generate our validator lookup list
	fn generate_lookup() {
		// Update our internal list of validators
		ValidatorLookup::<T>::remove_all();
		for validator in <pallet_session::Module<T>>::validators() {
			ValidatorLookup::<T>::insert(validator, ());
		}
	}

	fn force_validator_rotation() {
		Force::<T>::set(true);
		Pallet::<T>::deposit_event(Event::ForceRotationRequested());
	}
}

/// Provides the new set of validators to the session module when session is being rotated.
impl<T: Config> pallet_session::SessionManager<T::ValidatorId> for Pallet<T> {
	/// Prepare candidates for a new session
	fn new_session(_new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		return match T::Auction::phase() {
			// Successfully completed the process, these are the next set of validators to be used
			AuctionPhase::ValidatorsSelected(winners, _) => Some(winners),
			// A rotation has occurred, we emit an event of the new epoch and compile a list of
			// validators for validator lookup
			AuctionPhase::WaitingForBids(winners, min_bid) => {
				// If we have a set of winners
				if !winners.is_empty() {
					// Calculate our new epoch index
					let new_epoch = CurrentEpoch::<T>::mutate(|epoch| {
						*epoch = epoch.saturating_add(One::one());
						*epoch
					});
					// Emit an event
					Self::deposit_event(Event::NewEpoch(new_epoch));
					// Generate our lookup list of validators
					Self::generate_lookup();
					// Our trait callback
					T::EpochTransitionHandler::on_new_epoch(&winners, min_bid);
				}

				None
			}
			// Return
			_ => None,
		};
	}

	/// The current session is ending
	fn end_session(_end_index: SessionIndex) {}
	/// The session is starting
	fn start_session(_start_index: SessionIndex) {}
}

impl<T: Config> frame_support::traits::EstimateNextSessionRotation<T::BlockNumber> for Pallet<T> {
	fn estimate_next_session_rotation(_now: T::BlockNumber) -> Option<T::BlockNumber> {
		None
	}

	// The validity of this weight depends on the implementation of `estimate_next_session_rotation`
	fn weight(_now: T::BlockNumber) -> u64 {
		0
	}
}

/// In this module, for simplicity, we just return the same AccountId.
pub struct ValidatorOf<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> Convert<T::AccountId, Option<T::AccountId>> for ValidatorOf<T> {
	fn convert(account: T::AccountId) -> Option<T::AccountId> {
		Some(account)
	}
}

pub struct EmergencyRotationOf<T>(PhantomData<T>);

impl<T: Config> EmergencyRotation for EmergencyRotationOf<T> {
	fn request_emergency_rotation() {
		Pallet::<T>::force_validator_rotation();
	}
}
