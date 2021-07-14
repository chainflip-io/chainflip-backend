#![cfg_attr(not(feature = "std"), no_std)]

//! A pallet for distributing validator rewards.


pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use cf_traits::RewardsDistribution;
use frame_support::traits::{Get, Imbalance};
use pallet_cf_flip::{Pallet as Flip, ReserveId, Surplus};
use sp_runtime::traits::Saturating;
use sp_std::{marker::PhantomData, prelude::*};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	pub const VALIDATOR_REWARDS: ReserveId = *b"VALR";

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_cf_flip::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	/// The total amount of rewards that have been created through emissions.
	#[pallet::storage]
	#[pallet::getter(fn offchain_funds)]
	pub type RewardsEntitlement<T: Config> =
		StorageMap<_, Identity, ReserveId, T::Balance, ValueQuery>;

	/// Rewards that have actually been apportioned to accounts.
	#[pallet::storage]
	#[pallet::getter(fn apportioned_rewards)]
	pub type ApportionedRewards<T: Config> = StorageDoubleMap<
		_,
		Identity,
		ReserveId,
		Blake2_128Concat,
		T::AccountId,
		T::Balance,
		ValueQuery,
	>;

	/// The beneficiaries that rewards will be distributed to.
	#[pallet::storage]
	#[pallet::getter(fn beneficiaries)]
	pub type Beneficiaries<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event documentation should end with an array that provides descriptive names for event
		/// parameters. [something, who]
		SomethingStored(u32, T::AccountId),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Error names should be descriptive.
		NoneValue,
		/// Errors should have helpful documentation associated with them.
		StorageOverflow,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		// No external calls for this pallet.
	}
}

impl<T: Config> Pallet<T> {
	/// The amount of rewards still due to this account.
	fn rewards_due(account_id: &T::AccountId) -> T::Balance {
		let num_validators = Beneficiaries::<T>::decode_len().unwrap_or(0) as u32;
		let total_entitlement = RewardsEntitlement::<T>::get(VALIDATOR_REWARDS);
		let already_received = ApportionedRewards::<T>::get(VALIDATOR_REWARDS, account_id);

		total_entitlement / T::Balance::from(num_validators) - already_received
	}

	/// Credits the full rewards entitlement to an account.
	fn apportion_entitlement(account_id: &T::AccountId) {
		let entitlement = Self::rewards_due(account_id);
		let reward = Flip::<T>::withdraw_reserves(VALIDATOR_REWARDS, entitlement);
		// let reward = T::FlipReserves::withdraw_reserves(VALIDATOR_REWARDS, entitlement);
		ApportionedRewards::<T>::mutate(&VALIDATOR_REWARDS, account_id, |balance| {
			*balance = balance.saturating_add(reward.peek());
		});
		Flip::settle(account_id, reward.into());
	}

	/// Apportion all rewards and any other entitlements.
	fn apportion_outstanding_entitlements() {
		// Credit each validator any rewards still due.
		for account_id in Beneficiaries::<T>::get() {
			Self::apportion_entitlement(&account_id)
		}
	}

	/// Rolls over to another rewards period with a new set of beneficiaries:
	///
	/// 1. Credits all current beneficiaries with any remaining reward entitlements.
	/// 2. If any dust is left over in the reserve, keeps it for the next reward period.
	/// 3. Resets the apportioned rewards counter to zero.
	/// 4. Updates the list of beneficiaries.
	pub fn rollover(new_beneficiaries: &Vec<T::AccountId>) {
		Self::apportion_outstanding_entitlements();

		// Dust remaining in the reserve.
		let dust = Flip::<T>::reserved_balance(VALIDATOR_REWARDS);
		// let dust = T::FlipReserves::reserved_balance(&VALIDATOR_REWARDS);
		RewardsEntitlement::<T>::insert(VALIDATOR_REWARDS, dust);

		// Reset the accounting.
		ApportionedRewards::<T>::remove_prefix(VALIDATOR_REWARDS);

		// Set the new beneficiaries
		Beneficiaries::<T>::set(new_beneficiaries.clone());
	}
}

/// An implementation of [RewardsDistribution] that simply credits the rewards to an on-chain reserve so that it can be
/// allocated at a later point.
pub struct OnDemandRewardsDistribution<T>(PhantomData<T>);

impl<T: Config> RewardsDistribution for OnDemandRewardsDistribution<T> {
	type Balance = T::Balance;
	type Surplus = Surplus<T>;

	fn distribute(rewards: Self::Surplus) {
		let reward_amount = rewards.peek();
		let deposit = Flip::<T>::deposit_reserves(VALIDATOR_REWARDS, reward_amount);
		// let deposit = T::FlipReserves::deposit_reserves(VALIDATOR_REWARDS, rewards.peek());
		let _ = rewards.offset(deposit);
		RewardsEntitlement::<T>::mutate(VALIDATOR_REWARDS, |amount| {
			*amount = amount.saturating_add(reward_amount);
		});
	}

	fn execution_weight() -> frame_support::dispatch::Weight {
		T::DbWeight::get().reads_writes(1, 2)
	}
}

pub struct RewardRollover<T>(PhantomData<T>);

impl<T: Config> pallet_cf_validator::EpochTransitionHandler for RewardRollover<T> {
	type Amount = T::Balance;
	type ValidatorId = T::AccountId;

	fn on_new_epoch(new_validators: &Vec<Self::ValidatorId>, _new_bond: Self::Amount) {
		Pallet::<T>::rollover(new_validators);
	}
}
