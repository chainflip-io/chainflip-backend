#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extended_key_value_attributes)]

#[doc = include_str!("../README.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

use cf_traits::{
	ActiveValidatorRange, Auction, AuctionError, AuctionPhase, BidderProvider, ChainflipAccount,
	ChainflipAccountState, Online, RemainingBid, StakerHandler, VaultRotationHandler, VaultRotator,
};
use frame_support::pallet_prelude::*;
use frame_support::sp_std::mem;
use frame_support::traits::ValidatorRegistration;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, Convert, One, Zero};
use sp_std::cmp::min;
use sp_std::prelude::*;

pub trait WeightInfo {
	fn set_auction_size_range() -> Weight;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::RemainingBid;
	use cf_traits::{ChainflipAccount, VaultRotator};
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
		type Amount: Member
			+ Parameter
			+ Default
			+ Eq
			+ Ord
			+ Copy
			+ AtLeast32BitUnsigned
			+ MaybeSerializeDeserialize;
		/// An identity for a validator
		type ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize;
		/// Providing bidders
		type BidderProvider: BidderProvider<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;
		/// To confirm we have a session key registered for a validator
		type Registrar: ValidatorRegistration<Self::ValidatorId>;
		/// An index for the current auction
		type AuctionIndex: Member + Parameter + Default + Add + One + Copy;
		/// Minimum amount of validators
		#[pallet::constant]
		type MinValidators: Get<u32>;
		/// Benchmark stuff
		type WeightInfo: WeightInfo;
		/// The lifecycle of a vault rotation
		type Handler: VaultRotator<ValidatorId = Self::ValidatorId>;
		/// For looking up Chainflip Account data.
		type ChainflipAccount: ChainflipAccount<AccountId = Self::AccountId>;
		/// Convert ValidatorId to AccountId
		type AccountIdOf: Convert<Self::ValidatorId, Self::AccountId>;
		/// An online validator
		type Online: Online<ValidatorId = Self::ValidatorId>;
		/// Ratio of backup validators
		#[pallet::constant]
		type ActiveToBackupValidatorRatio: Get<u32>;
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
		StorageValue<_, Vec<RemainingBid<T::ValidatorId, T::Amount>>, ValueQuery>;

	/// A size calculated for our backup validator group
	#[pallet::storage]
	#[pallet::getter(fn backup_group_size)]
	pub(super) type BackupGroupSize<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The lowest backup validator bid
	#[pallet::storage]
	#[pallet::getter(fn lowest_backup_validator_bid)]
	pub(super) type LowestBackupValidatorBid<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	/// The highest passive validator bid
	#[pallet::storage]
	#[pallet::getter(fn highest_passive_validator_bid)]
	pub(super) type HighestPassiveNodeBid<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

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
		#[pallet::weight(T::WeightInfo::set_auction_size_range())]
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
	pub struct GenesisConfig<T: Config> {
		pub validator_size_range: ActiveValidatorRange,
		pub winners: Vec<T::ValidatorId>,
		pub minimum_active_bid: T::Amount,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				validator_size_range: (Zero::zero(), Zero::zero()),
				winners: vec![],
				minimum_active_bid: Zero::zero(),
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			Pallet::<T>::set_active_range(self.validator_size_range).expect("valid range");
			CurrentPhase::<T>::set(AuctionPhase::WaitingForBids(
				self.winners.clone(),
				self.minimum_active_bid,
			));
		}
	}
}

impl<T: Config> Auction for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;
	type BidderProvider = T::BidderProvider;

	fn active_range() -> ActiveValidatorRange {
		ActiveValidatorSizeRange::<T>::get()
	}

	/// Set new auction range, returning on success the old value
	fn set_active_range(range: ActiveValidatorRange) -> Result<ActiveValidatorRange, AuctionError> {
		let (low, high) = range;

		if low >= high || low < T::MinValidators::get() {
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
				// Rule #4 - Confirm that the validators are 'online'
				bidders.retain(|(id, _)| T::Online::is_online(id));
				// Rule #5 - Confirm we have our set size
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

					let validator_set_target_size = ActiveValidatorSizeRange::<T>::get().1;
					let number_of_bidders = bids.len() as u32;
					let validator_group_size = min(validator_set_target_size, number_of_bidders);
					let validating_set: Vec<_> =
						bids.iter().take(validator_group_size as usize).collect();
					let minimum_active_bid = validating_set
						.last()
						.map(|(_, bid)| *bid)
						.unwrap_or_default();
					let validating_set: Vec<_> = validating_set
						.iter()
						.map(|(validator_id, _)| (*validator_id).clone())
						.collect();
					let backup_group_size = min(
						number_of_bidders - validator_group_size,
						validator_set_target_size / T::ActiveToBackupValidatorRatio::get(),
					);

					let remaining_bidders: Vec<_> =
						bids.iter().skip(validator_group_size as usize).collect();

					let phase = AuctionPhase::ValidatorsSelected(
						validating_set.clone(),
						minimum_active_bid,
					);

					RemainingBidders::<T>::put(remaining_bidders);
					BackupGroupSize::<T>::put(backup_group_size);
					CurrentPhase::<T>::put(phase.clone());

					Self::deposit_event(Event::AuctionCompleted(
						<CurrentAuctionIndex<T>>::get(),
						validating_set.clone(),
					));

					T::Handler::start_vault_rotation(validating_set)
						.map_err(|_| AuctionError::Abort)?;

					return Ok(phase);
				}

				return Err(AuctionError::Empty);
			}
			// Things have gone well and we have a set of 'Winners', congratulations.
			// We are ready to call this an auction a day resetting the bidders in storage and
			// setting the state ready for a new set of 'Bidders'
			AuctionPhase::ValidatorsSelected(winners, minimum_active_bid) => {
				match T::Handler::finalize_rotation() {
					Ok(_) => {
						let update_status = |validators, state| {
							for validator_id in validators {
								T::ChainflipAccount::update_state(
									&T::AccountIdOf::convert(validator_id),
									state,
								);
							}
						};

						let remaining_bidders = RemainingBidders::<T>::get();
						let backup_validators = current_backup_validators::<T>(&remaining_bidders);
						let passive_nodes = current_passive_nodes::<T>(&remaining_bidders);
						let lowest_backup_validator_bid = lowest_bid::<T>(&backup_validators);
						let highest_passive_node_bid = highest_bid::<T>(&passive_nodes);

						LowestBackupValidatorBid::<T>::put(lowest_backup_validator_bid);
						HighestPassiveNodeBid::<T>::put(highest_passive_node_bid);

						update_status(winners.clone(), ChainflipAccountState::Validator);

						update_status(
							backup_validators
								.iter()
								.map(|(validator_id, _)| validator_id.clone())
								.collect(),
							ChainflipAccountState::Backup,
						);

						update_status(
							passive_nodes
								.iter()
								.map(|(validator_id, _)| validator_id.clone())
								.collect(),
							ChainflipAccountState::Passive,
						);

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

fn current_backup_validators<T: Config>(
	remaining_bidders: &Vec<RemainingBid<T::ValidatorId, T::Amount>>,
) -> Vec<RemainingBid<T::ValidatorId, T::Amount>> {
	remaining_bidders
		.iter()
		.take(BackupGroupSize::<T>::get() as usize)
		.map(|bid| bid.clone())
		.collect()
}

fn current_passive_nodes<T: Config>(
	remaining_bidders: &Vec<RemainingBid<T::ValidatorId, T::Amount>>,
) -> Vec<RemainingBid<T::ValidatorId, T::Amount>> {
	remaining_bidders
		.iter()
		.skip(BackupGroupSize::<T>::get() as usize)
		.map(|bid| bid.clone())
		.collect()
}

fn lowest_bid<T: Config>(bidders: &Vec<RemainingBid<T::ValidatorId, T::Amount>>) -> T::Amount {
	bidders
		.last()
		.map(|(_, amount)| *amount)
		.unwrap_or_default()
}

fn highest_bid<T: Config>(bidders: &Vec<RemainingBid<T::ValidatorId, T::Amount>>) -> T::Amount {
	bidders
		.first()
		.map(|(_, amount)| *amount)
		.unwrap_or_default()
}

pub struct HandleStakes<T>(PhantomData<T>);
impl<T: Config> StakerHandler for HandleStakes<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;

	fn stake_updated(validator_id: Self::ValidatorId, amount: Self::Amount) {
		if BackupGroupSize::<T>::get() == 0 {
			return;
		}

		let update_stake_for_bidder =
			|remaining_bidders: &mut Vec<RemainingBid<T::ValidatorId, T::Amount>>,
			 bid: RemainingBid<T::ValidatorId, T::Amount>| {
				if let Ok(index) =
					remaining_bidders.binary_search_by(|bid| validator_id.cmp(&bid.0))
				{
					remaining_bidders[index] = bid
				}
			};

		let sort_remaining_bidders =
			|remaining_bids: &mut Vec<RemainingBid<T::ValidatorId, T::Amount>>| {
				// Sort and set state
				remaining_bids.sort_unstable_by_key(|k| k.1);
				remaining_bids.reverse();

				let lowest_backup_validator_bid =
					lowest_bid::<T>(&current_backup_validators::<T>(&remaining_bids));

				let highest_passive_node_bid =
					highest_bid::<T>(&current_passive_nodes::<T>(&remaining_bids));

				LowestBackupValidatorBid::<T>::put(lowest_backup_validator_bid);
				HighestPassiveNodeBid::<T>::set(highest_passive_node_bid);
				RemainingBidders::<T>::put(remaining_bids);
			};

		let promote_or_demote = |promote: bool, validator_id: T::ValidatorId| {
			T::ChainflipAccount::update_state(
				&T::AccountIdOf::convert(validator_id.clone()),
				if promote {
					ChainflipAccountState::Backup
				} else {
					ChainflipAccountState::Passive
				},
			);
		};

		let adjust_group =
			|promote: bool,
			 remaining_bidders: &mut Vec<RemainingBid<T::ValidatorId, T::Amount>>| {
				// Update bid for bidder and state
				update_stake_for_bidder(remaining_bidders, (validator_id.clone(), amount));
				promote_or_demote(promote, validator_id.clone());

				sort_remaining_bidders(remaining_bidders);

				let index_of_shifted = if !promote {
					BackupGroupSize::<T>::get().saturating_sub(One::one())
				} else {
					BackupGroupSize::<T>::get()
				};

				match remaining_bidders.get(index_of_shifted as usize) {
					Some((adjusted_validator_id, _)) => {
						promote_or_demote(!promote, adjusted_validator_id.clone());
					}
					_ => {}
				}
			};

		if Pallet::<T>::waiting_on_bids() {
			match T::ChainflipAccount::get(&T::AccountIdOf::convert(validator_id.clone())).state
			{
				ChainflipAccountState::Passive => {
					if amount > LowestBackupValidatorBid::<T>::get() {
						adjust_group(true, &mut RemainingBidders::<T>::get());
					} else if amount > HighestPassiveNodeBid::<T>::get() {
						sort_remaining_bidders(&mut RemainingBidders::<T>::get());
					}
				}
				ChainflipAccountState::Backup => {
					if amount < LowestBackupValidatorBid::<T>::get() {
						adjust_group(false, &mut RemainingBidders::<T>::get());
					} else if amount > LowestBackupValidatorBid::<T>::get() {
						sort_remaining_bidders(&mut RemainingBidders::<T>::get());
					}
				}
				_ => {}
			}
		}
	}
}
