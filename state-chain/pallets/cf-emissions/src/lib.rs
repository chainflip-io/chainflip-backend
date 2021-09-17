#![cfg_attr(not(feature = "std"), no_std)]

//! A pallet for managing the FLIP emissions schedule.

use frame_support::dispatch::Weight;
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use cf_traits::{EmissionsTrigger, Issuance, RewardsDistribution};
use codec::FullCodec;
use frame_support::traits::{Get, Imbalance};
use sp_arithmetic::traits::UniqueSaturatedFrom;
use sp_runtime::{
	offchain::storage_lock::BlockNumberProvider,
	traits::{AtLeast32BitUnsigned, CheckedMul, Zero},
};

pub trait WeightInfo {
	fn on_initialize() -> Weight;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The Flip token denomination.
		type FlipBalance: Member
			+ FullCodec
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ AtLeast32BitUnsigned
			+ UniqueSaturatedFrom<Self::BlockNumber>;

		/// An imbalance type representing freshly minted, unallocated funds.
		type Surplus: Imbalance<Self::FlipBalance>;

		/// An implmentation of the [Issuance] trait.
		type Issuance: Issuance<
			Balance = Self::FlipBalance,
			AccountId = Self::AccountId,
			Surplus = Self::Surplus,
		>;

		/// An implementation of `RewardsDistribution` defining how to distribute the emissions.
		type RewardsDistribution: RewardsDistribution<
			Balance = Self::FlipBalance,
			Surplus = Self::Surplus,
		>;

		/// How frequently to mint.
		#[pallet::constant]
		type MintInterval: Get<Self::BlockNumber>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn emissions_per_block)]
	/// The amount of Flip to mint per block.
	pub type EmissionPerBlock<T: Config> = StorageValue<_, T::FlipBalance, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn last_mint_block)]
	/// The block number at which we last minted Flip.
	pub type LastMintBlock<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Emissions have been distributed. [block_number, amount_minted]
		EmissionsDistributed(BlockNumberFor<T>, T::FlipBalance),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Emissions calculation resulted in overflow.
		Overflow,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			let should_mint = Self::should_mint_at(current_block);

			if should_mint {
				Self::mint_rewards_for_block(current_block);
			}
			T::WeightInfo::on_initialize()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		/// Emission rate at genesis.
		pub emission_per_block: T::FlipBalance,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				emission_per_block: Zero::zero(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			EmissionPerBlock::<T>::set(self.emission_per_block);
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Determines if we should mint at block number `block_number`.
	fn should_mint_at(block_number: T::BlockNumber) -> bool {
		let mint_interval = T::MintInterval::get();
		let blocks_elapsed = block_number - LastMintBlock::<T>::get();
		let should_mint = Self::should_mint(blocks_elapsed, mint_interval);

		should_mint
	}

	/// Checks if we should mint.
	fn should_mint(
		blocks_elapsed_since_last_mint: T::BlockNumber,
		mint_interval: T::BlockNumber,
	) -> bool {
		blocks_elapsed_since_last_mint >= mint_interval
	}

	/// Based on the last block at which rewards were minted, calculates how much issuance needs to be
	/// minted and distributes this a as a reward via [RewardsDistribution].
	fn mint_rewards_for_block(block_number: T::BlockNumber) {
		// Calculate the outstanding reward amount.
		let blocks_elapsed = block_number - LastMintBlock::<T>::get();
		let blocks_elapsed = T::FlipBalance::unique_saturated_from(blocks_elapsed);

		match EmissionPerBlock::<T>::get().checked_mul(&blocks_elapsed) {
			Some(reward_amount) if !reward_amount.is_zero() => {
				// Mint the rewards
				let reward = T::Issuance::mint(reward_amount);

				// Delegate the distribution.
				T::RewardsDistribution::distribute(reward);

				// Update this pallet's state.
				LastMintBlock::<T>::set(block_number);

				Self::deposit_event(Event::EmissionsDistributed(block_number, reward_amount));
			}
			_ => (),
		}
		// if let Some(reward_amount) = EmissionPerBlock::<T>::get().checked_mul(&blocks_elapsed) {
		// 	let exec_weight = if reward_amount.is_zero() {
		// 		0
		// 	} else {
		// 		// Mint the rewards
		// 		let reward = T::Issuance::mint(reward_amount);

		// 		// Delegate the distribution.
		// 		T::RewardsDistribution::distribute(reward);
		// 		T::RewardsDistribution::execution_weight()
		// 	};

		// 	// Update this pallet's state.
		// 	LastMintBlock::<T>::set(block_number);

		// 	Self::deposit_event(Event::EmissionsDistributed(block_number, reward_amount));
		// }
	}
}

impl<T: Config> EmissionsTrigger for Pallet<T> {
	fn trigger_emissions() {
		let block_number = frame_system::Pallet::<T>::current_block_number();
		let _ = Self::mint_rewards_for_block(block_number);
	}
}
