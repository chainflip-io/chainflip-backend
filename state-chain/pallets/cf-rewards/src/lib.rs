#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

use cf_traits::{RewardRollover, Rewarder, RewardsDistribution};
use frame_support::{
	ensure,
	traits::{Get, Imbalance},
};
use pallet_cf_flip::{Pallet as Flip, ReserveId, Surplus};
use sp_runtime::{
	traits::{Saturating, Zero},
	DispatchError,
};
use sp_std::{marker::PhantomData, prelude::*};

pub mod weights;
pub use weights::WeightInfo;

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
		/// Benchmark stuff
		type WeightInfoRewards: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	/// The total amount of rewards that have been created through emissions.
	#[pallet::storage]
	#[pallet::getter(fn offchain_funds)]
	pub type RewardsEntitlement<T: Config> =
		StorageMap<_, Twox64Concat, ReserveId, T::Balance, ValueQuery>;

	/// Rewards that have actually been apportioned to accounts.
	#[pallet::storage]
	#[pallet::getter(fn apportioned_rewards)]
	pub type ApportionedRewards<T: Config> =
		StorageDoubleMap<_, Twox64Concat, ReserveId, Blake2_128Concat, T::AccountId, T::Balance>;

	/// The number of beneficiaries that rewards will be distributed to this round.
	#[pallet::storage]
	#[pallet::getter(fn beneficiaries)]
	pub type Beneficiaries<T: Config> = StorageMap<_, Twox64Concat, ReserveId, u32, ValueQuery>;

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Outstanding rewards have been credited to an account. [beneficiary, amount]
		RewardsCredited(T::AccountId, T::Balance),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Not enough reserves to pay out expected rewards entitlements.
		InsufficientReserves,
		/// No current entitlement to any rewards.
		NoRewardEntitlement,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Credits any outstanding rewards to the caller's account.
		#[pallet::weight(T::WeightInfoRewards::redeem_rewards())]
		pub fn redeem_rewards(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;
			Self::try_apportion_full_entitlement(&account_id)?;
			Ok(().into())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// The amount of rewards still due to this account.
	fn rewards_due(account_id: &T::AccountId) -> T::Balance {
		if let Some(already_received) = ApportionedRewards::<T>::get(VALIDATOR_REWARDS, account_id)
		{
			Self::rewards_due_each() - already_received
		} else {
			Zero::zero()
		}
	}

	/// Credits up to the given amount to an account, depending on available reserves.
	fn apportion_amount(account_id: &T::AccountId, amount: T::Balance) {
		let reward = Flip::<T>::withdraw_reserves(VALIDATOR_REWARDS, amount);
		Self::settle_reward(account_id, reward);
	}

	/// Credits the full rewards entitlement to an account, if enough are available in reserves,
	/// otherwise errors.
	fn try_apportion_full_entitlement(account_id: &T::AccountId) -> Result<(), DispatchError> {
		let entitlement = Self::rewards_due(account_id);
		ensure!(!entitlement.is_zero(), Error::<T>::NoRewardEntitlement);
		let reward = Flip::<T>::try_withdraw_reserves(VALIDATOR_REWARDS, entitlement)?;
		Self::settle_reward(account_id, reward);
		Ok(())
	}

	/// Credits a reward amount to an account, up to the maximum reserves available.
	///
	/// *Note:* before calling this, you should:
	/// (a) check if sufficient funds are in the reserve.
	/// (b) ensure the account is entitled to the rewards.
	fn settle_reward(account_id: &T::AccountId, reward: Surplus<T>) {
		let reward_amount = reward.peek();
		Flip::settle_imbalance(account_id, reward);
		ApportionedRewards::<T>::mutate(&VALIDATOR_REWARDS, account_id, |maybe_balance| {
			*maybe_balance = maybe_balance.map(|balance| balance.saturating_add(reward_amount));
		});
		Self::deposit_event(Event::<T>::RewardsCredited(account_id.clone(), reward_amount));
	}

	/// The total rewards due to each beneficiary.
	pub fn rewards_due_each() -> T::Balance {
		let num_beneficiaries = Beneficiaries::<T>::get(VALIDATOR_REWARDS);
		if num_beneficiaries == 0 {
			return Zero::zero()
		}
		RewardsEntitlement::<T>::get(VALIDATOR_REWARDS) / T::Balance::from(num_beneficiaries)
	}

	/// Checks if we have enough reserves to honour the rewards entitlements.
	pub fn sufficient_reserves() -> bool {
		let due_per_beneficiary = Self::rewards_due_each();
		let total_entitlements = ApportionedRewards::<T>::iter_prefix_values(VALIDATOR_REWARDS)
			.fold(Zero::zero(), |total: T::Balance, already_received| {
				total.saturating_add(due_per_beneficiary - already_received)
			});
		total_entitlements <= Flip::<T>::reserved_balance(VALIDATOR_REWARDS)
	}
}
