#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip Auction Module
//!
//! A module to manage auctions for the Chainflip State Chain
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Module`]
//!
//! ## Overview
//! The module contains functionality to run a contest or auction in which a set of bidders are
//! provided via the `BidderProvider` trait.  Calling `Auction::process()` we push forward the state
//! of our auction.
//!
//! The process starts with `AuctionPhase::WaitingForBids` requesting a set of bidders and filtering
//! them at a high level for the next phase `AuctionPhase::BidsTaken`.
//! During `AuctionPhase::BidsTaken` bidder classification starts where a set of viable candidates
//! for the next epoch are selected.  Those that don't qualify at this stage are grouped and stored in
//! `RemainingBidders` with a backup group size being calculated and stored in `BackupGroupSize`.
//! The pallet maintains a sorted list of these remaining bidders which can be viewed as two groups,
//! `ChainflipAccountState::Backup` and `ChainflipAccountState::Passive`, using the calculated `BackupGroupSize`.
//! This list and group size are recalculated everytime the process passes through `AuctionPhase::BidsTaken`.
//! Their final states are not updated until the process has completed.
//!
//! After completing the step `AuctionPhase::BidsTaken` the pallet moves forward to the
//! `AuctionPhase::ValidatorsSelected` phase.  At this point a request has been sent to start a vault
//! rotation with the proposed winning set via `VaultRotation::start_vault_rotation()`.
//! Once confirmation has been made via `VaultRotation::finalize_rotation()` the states for the
//! validators and the remaining set, backup and passive, are set using `ChainflipAccount::update_state`
//!
//! During the lifetime of a node its stake may vary.  This is shared via the `StakeHandler` trait in
//! which updates are received.  Updates to stakes are respected only during `AuctionPhase::WaitingForBids`
//! and depending on the nodes state being either `ChainflipAccountState::Passive` or
//! `ChainflipAccountState::Backup` we may see a change in their state if they rise above or fall
//! below the bid marked by `BackupGroupSize`
//!
//! At any point in time the auction can be aborted using `Auction::abort()` returning state to
//! `WaitingForBids`.
//!
//! ## Terminology
//! - **Bidder:** A staker that has put their bid forward to be considered in the auction
//! - **Winners:** Those bidders that have been evaluated and have been included in the the winning set
//!   to become the next set of validators in the next epoch.
//! - **Minimum Active Bid:** The minimum active bid required to be included in the Winners set
//! - **Backup Validator** A group of bidders who make up a group size of 1/3 of the desired validator
//!   group size.  They are expected to the reserve in that they are ready to become a validator during
//!   an emergency rotation.
//!

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

use cf_traits::{
	ActiveValidatorRange, Auction, AuctionError, AuctionPhase, BidderProvider, ChainflipAccount,
	ChainflipAccountState, StakerHandler, VaultRotation, VaultRotationHandler,
};
use frame_support::pallet_prelude::*;
use frame_support::sp_runtime::offchain::storage_lock::BlockNumberProvider;
use frame_support::sp_std::mem;
use frame_support::traits::ValidatorRegistration;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, Convert, One, Zero};
use sp_std::cmp::min;
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::Bid;
	use cf_traits::{ChainflipAccount, VaultRotation};
	use frame_support::traits::ValidatorRegistration;
	use sp_runtime::traits::Convert;
	use sp_std::ops::Add;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// An amount for a bid
		type Amount: Member + Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;
		/// An identity for a validator
		type ValidatorId: Member + Parameter;
		/// Providing bidders
		type BidderProvider: BidderProvider<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;
		/// To confirm we have a session key registered for a validator
		type Registrar: ValidatorRegistration<Self::ValidatorId>;
		/// An index for the current auction
		type AuctionIndex: Member + Parameter + Default + Add + One + Copy;
		/// Minimum amount of validators
		#[pallet::constant]
		type MinValidators: Get<u32>;
		/// The lifecycle of our auction
		type Handler: VaultRotation<ValidatorId = Self::ValidatorId>;
		/// A Chainflip Account
		type ChainflipAccount: ChainflipAccount<AccountId = Self::AccountId>;
		/// Convert ValidatorId to AccountId
		type AccountIdOf: Convert<Self::ValidatorId, Option<Self::AccountId>>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	/// Current phase of the auction
	#[pallet::storage]
	#[pallet::getter(fn current_phase)]
	pub(super) type CurrentPhase<T: Config> =
		StorageValue<_, AuctionPhase<T::ValidatorId, T::Amount>, ValueQuery>;

	/// Size range for number of validators we want in our validating set
	#[pallet::storage]
	#[pallet::getter(fn active_validator_size_range)]
	pub(super) type ActiveValidatorSizeRange<T: Config> =
		StorageValue<_, ActiveValidatorRange, ValueQuery>;

	/// The index of the auction we are in
	#[pallet::storage]
	#[pallet::getter(fn current_auction_index)]
	pub(super) type CurrentAuctionIndex<T: Config> = StorageValue<_, T::AuctionIndex, ValueQuery>;

	/// Validators that have been reported as being bad
	#[pallet::storage]
	#[pallet::getter(fn bad_validators)]
	pub(super) type BadValidators<T: Config> = StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

	/// The remaining set of bidders after an auction
	#[pallet::storage]
	#[pallet::getter(fn remaining_bidders)]
	pub(super) type RemainingBidders<T: Config> =
		StorageValue<_, Vec<Bid<T::ValidatorId, T::Amount>>, ValueQuery>;

	/// A size calculated for our backup validator group
	#[pallet::storage]
	#[pallet::getter(fn backup_group_size)]
	pub(super) type BackupGroupSize<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An auction phase has started \[auction_index\]
		AuctionStarted(T::AuctionIndex),
		/// An auction has a set of winners \[auction_index, winners\]
		AuctionCompleted(T::AuctionIndex, Vec<T::ValidatorId>),
		/// The auction has been confirmed off-chain \[auction_index\]
		AuctionConfirmed(T::AuctionIndex),
		/// Awaiting bidders for the auction
		AwaitingBidders,
		/// The active validator range upper limit has changed \[before, after\]
		ActiveValidatorRangeChanged(ActiveValidatorRange, ActiveValidatorRange),
		/// The auction was aborted \[auction_index\]
		AuctionAborted(T::AuctionIndex),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Invalid auction index used in confirmation
		InvalidAuction,
		/// Invalid range used for the active validator range
		InvalidRange,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets the size of our auction range
		///
		/// The dispatch origin of this function must be root.
		#[pallet::weight(10_000)]
		pub(super) fn set_active_validator_range(
			origin: OriginFor<T>,
			range: ActiveValidatorRange,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			match Self::set_active_range(range) {
				Ok(old) => {
					Self::deposit_event(Event::ActiveValidatorRangeChanged(old, range));
					Ok(().into())
				}
				Err(_) => Err(Error::<T>::InvalidRange.into()),
			}
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub validator_size_range: ActiveValidatorRange,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {
				validator_size_range: (Zero::zero(), Zero::zero()),
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			ActiveValidatorSizeRange::<T>::set(self.validator_size_range);
			// Run through an auction
			if Pallet::<T>::process().and(Pallet::<T>::process()).is_ok() {
				if let Err(err) = Pallet::<T>::process() {
					panic!("Failed to confirm auction: {:?}", err);
				}
			} else {
				panic!("Failed selecting winners in auction");
			}
		}
	}
}

impl<T: Config> Auction for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;
	type BidderProvider = T::BidderProvider;

	fn auction_range() -> ActiveValidatorRange {
		ActiveValidatorSizeRange::<T>::get()
	}

	/// Set new auction range, returning on success the old value
	fn set_active_range(range: ActiveValidatorRange) -> Result<ActiveValidatorRange, AuctionError> {
		let (low, high) = range;

		if low == high
			|| low < T::MinValidators::get()
			|| high < T::MinValidators::get()
			|| high < low
		{
			return Err(AuctionError::InvalidRange);
		}

		let old = ActiveValidatorSizeRange::<T>::get();
		if old == range {
			return Err(AuctionError::InvalidRange);
		}

		ActiveValidatorSizeRange::<T>::put(range);
		Ok(old)
	}

	fn phase() -> AuctionPhase<Self::ValidatorId, Self::Amount> {
		CurrentPhase::<T>::get()
	}

	fn waiting_on_bids() -> bool {
		mem::discriminant(&Self::phase()) == mem::discriminant(&AuctionPhase::default())
	}

	/// Move our auction process to the next phase returning success with phase completed
	///
	/// At each phase we assess the bidders based on a fixed set of criteria which results
	/// in us arriving at a winning list and a bond set for this auction
	fn process() -> Result<AuctionPhase<Self::ValidatorId, Self::Amount>, AuctionError> {
		return match <CurrentPhase<T>>::get() {
			// Run some basic rules on what we consider as valid bidders
			// At the moment this includes checking that their bid is more than 0, which
			// shouldn't be possible and whether they have registered their session keys
			// to be able to actual join the validating set.  If we manage to pass these tests
			// we kill the last set of winners stored, set the bond to 0, store this set of
			// bidders and change our state ready for an 'Auction' to be ran
			AuctionPhase::WaitingForBids(..) => {
				let mut bidders = T::BidderProvider::get_bidders();
				// Rule #1 - They are not bad
				bidders.retain(|(id, _)| !BadValidators::<T>::get().contains(id));
				// They aren't bad now
				BadValidators::<T>::kill();
				// Rule #2 - If we have a bid at 0 then please leave
				bidders.retain(|(_, amount)| !amount.is_zero());
				// Rule #3 - They are registered
				bidders.retain(|(id, _)| T::Registrar::is_registered(id));
				// Rule #4 - Confirm we have our set size
				if (bidders.len() as u32) < ActiveValidatorSizeRange::<T>::get().0 {
					return Err(AuctionError::MinValidatorSize);
				};

				let phase = AuctionPhase::BidsTaken(bidders);
				CurrentPhase::<T>::put(phase.clone());

				CurrentAuctionIndex::<T>::mutate(|idx| *idx + One::one());

				Self::deposit_event(Event::AuctionStarted(<CurrentAuctionIndex<T>>::get()));
				Ok(phase)
			}
			// We sort by bid and cut the size of the set based on auction size range
			// If we have a valid set, within the size range, we store this set as the
			// 'winners' of this auction, change the state to 'Completed' and store the
			// minimum bid needed to be included in the set.
			AuctionPhase::BidsTaken(mut bids) => {
				if !bids.is_empty() {
					bids.sort_unstable_by_key(|k| k.1);
					bids.reverse();
					let number_of_bidders = bids.len() as u32;
					let validator_set_size = ActiveValidatorSizeRange::<T>::get().1;
					let validator_group_size = min(validator_set_size, number_of_bidders);
					let backup_group_size = if validator_group_size < number_of_bidders {
						min(
							validator_set_size / 3,
							number_of_bidders - validator_group_size,
						)
					} else {
						0
					};

					let validating_set: (Vec<T::ValidatorId>, _) = bids
						.get(Zero::zero()..validator_group_size as usize)
						.map(|validators| {
							(
								validators.iter().map(|i| i.0.clone()).collect(),
								validators.last().map(|last| last.1).unwrap_or_default(),
							)
						})
						.ok_or(AuctionError::Empty)?;

					let remaining_bidders = bids
						.get(validator_group_size as usize..bids.len() as usize)
						.unwrap_or_default()
						.to_vec();

					let phase = AuctionPhase::ValidatorsSelected(
						validating_set.0.clone(),
						validating_set.1,
					);

					RemainingBidders::<T>::put(remaining_bidders);
					BackupGroupSize::<T>::put(backup_group_size);
					CurrentPhase::<T>::put(phase.clone());

					Self::deposit_event(Event::AuctionCompleted(
						<CurrentAuctionIndex<T>>::get(),
						validating_set.0.clone(),
					));

					T::Handler::start_vault_rotation(validating_set.0)
						.map_err(|_| AuctionError::Abort)?;

					return Ok(phase);
				}

				return Err(AuctionError::Empty);
			}
			// Things have gone well and we have a set of 'Winners', congratulations.
			// We are ready to call this an auction a day resetting the bidders in storage and
			// setting the state ready for a new set of 'Bidders'
			AuctionPhase::ValidatorsSelected(winners, minimum_active_bid) => {
				// If this is genesis we auto confirm
				let result = if frame_system::Pallet::<T>::current_block_number() == Zero::zero() {
					Ok(())
				} else {
					T::Handler::finalize_rotation()
				};

				match result {
					Ok(_) => {
						let update_validators = |validators, state| -> Result<(), AuctionError> {
							for validator_id in validators {
								let account_id = T::AccountIdOf::convert(validator_id)
									.ok_or_else(|| AuctionError::Abort)?;
								T::ChainflipAccount::update_state(&account_id, state);
							}
							Ok(())
						};

						let remaining_bidders = RemainingBidders::<T>::get();
						let backup_group_size = BackupGroupSize::<T>::get();

						let backup_validators: Vec<T::ValidatorId> = remaining_bidders
							.get(0usize..backup_group_size as usize)
							.unwrap_or_default()
							.iter()
							.map(|(validator_id, _)| validator_id.clone())
							.collect();

						let passive: Vec<T::ValidatorId> = remaining_bidders
							.get(backup_group_size as usize..remaining_bidders.len())
							.unwrap_or_default()
							.iter()
							.map(|(validator_id, _)| validator_id.clone())
							.collect();

						update_validators(winners.clone(), ChainflipAccountState::Validator)?;
						update_validators(backup_validators, ChainflipAccountState::Backup)?;
						update_validators(passive, ChainflipAccountState::Passive)?;

						let phase = AuctionPhase::WaitingForBids(winners, minimum_active_bid);

						CurrentPhase::<T>::put(phase.clone());

						Self::deposit_event(Event::AuctionConfirmed(
							CurrentAuctionIndex::<T>::get(),
						));

						Self::deposit_event(Event::AwaitingBidders);
						Ok(phase)
					}
					Err(_) => Err(AuctionError::NotConfirmed),
				}
			}
		};
	}
}

impl<T: Config> VaultRotationHandler for Pallet<T> {
	type ValidatorId = T::ValidatorId;

	fn abort() {
		CurrentPhase::<T>::put(AuctionPhase::default());
		Self::deposit_event(Event::AuctionAborted(CurrentAuctionIndex::<T>::get()));
	}

	fn penalise(bad_validators: Vec<Self::ValidatorId>) {
		BadValidators::<T>::set(bad_validators);
	}
}

pub struct HandleStakes<T>(PhantomData<T>);
impl<T: Config> StakerHandler for HandleStakes<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;

	fn stake_updated(validator_id: Self::ValidatorId, amount: Self::Amount) {
		if let Some(account_id) = T::AccountIdOf::convert(validator_id.clone()) {
			match CurrentPhase::<T>::get() {
				AuctionPhase::WaitingForBids(..) => {
					match T::ChainflipAccount::get(&account_id).state {
						ChainflipAccountState::Passive => {
							// Are we above the mark?
							let mut remaining_bids = RemainingBidders::<T>::get();
							let backup_group_size = BackupGroupSize::<T>::get();

							if let Some((_, lowest_backup_bid)) =
								remaining_bids.get(backup_group_size as usize)
							{
								if amount > *lowest_backup_bid {
									if let Some((passive_validator_id, _)) =
										remaining_bids.get((backup_group_size - 1) as usize)
									{
										if let Some(passive_account_id) =
											T::AccountIdOf::convert(passive_validator_id.clone())
										{
											// Set state of new validator to Backup
											T::ChainflipAccount::update_state(
												&account_id,
												ChainflipAccountState::Backup,
											);
											// Set state of backup_group_size - 1 to Passive
											T::ChainflipAccount::update_state(
												&passive_account_id,
												ChainflipAccountState::Passive,
											);
											// Sort and set state
											remaining_bids.sort_unstable_by_key(|k| k.1);
											remaining_bids.reverse();

											RemainingBidders::<T>::put(remaining_bids);
										}
									}
								}
							}
						}
						ChainflipAccountState::Backup => {
							let mut remaining_bids = RemainingBidders::<T>::get();
							let backup_group_size = BackupGroupSize::<T>::get();
							// Are we below the mark?
							if let Some((_, lowest_backup_bid)) =
								remaining_bids.get(backup_group_size as usize)
							{
								if amount < *lowest_backup_bid {
									if let Some((backup_validator_id, _)) =
										remaining_bids.get((backup_group_size + 1) as usize)
									{
										if let Some(backup_account_id) =
											T::AccountIdOf::convert(backup_validator_id.clone())
										{
											// Set state of new validator to Passive
											T::ChainflipAccount::update_state(
												&account_id,
												ChainflipAccountState::Passive,
											);
											// Set state of backup_group_size + 1 to Backup
											T::ChainflipAccount::update_state(
												&backup_account_id,
												ChainflipAccountState::Backup,
											);
											// Sort and set state
											remaining_bids.sort_unstable_by_key(|k| k.1);
											remaining_bids.reverse();
											RemainingBidders::<T>::put(remaining_bids);
										}
									}
								}
							}
						}
						_ => {}
					}
				}
				_ => {}
			}
		}
	}
}
