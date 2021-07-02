#![cfg_attr(not(feature = "std"), no_std)]

//! A pallet for managing the FLIP emissions schedule.

use frame_support::dispatch::Weight;
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use cf_traits::{Emissions, EpochInfo, RewardsDistribution, Witnesser};
use codec::FullCodec;
use sp_arithmetic::traits::UniqueSaturatedFrom;
use sp_runtime::{DispatchError, SaturatedConversion, traits::{AtLeast32BitUnsigned, CheckedMul, Zero}};
use sp_std::marker::PhantomData;
use sp_std::ops::Div;
use frame_support::{weights, traits::Get};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		dispatch::DispatchResultWithPostInfo, pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;

	pub type EthTransactionHash = [u8; 32];

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Standard Call type. We need this so we can use it as a constraint in `Witnesser`.
		type Call: From<Call<Self>> + IsType<<Self as frame_system::Config>::Call>;

		/// The Flip token denomination.
		type FlipBalance: Member
			+ FullCodec
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ AtLeast32BitUnsigned
			+ UniqueSaturatedFrom<Self::BlockNumber>;

		/// An implmentation of the [Emissions] trait.
		type Emissions: Emissions<Balance = Self::FlipBalance, AccountId = Self::AccountId>;

		/// Provides an origin check for witness transactions.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

		/// An implementation of the witnesser, allows us to define witness_* helper extrinsics.
		type Witnesser: Witnesser<Call = <Self as Config>::Call, AccountId = Self::AccountId>;

		/// An implementation of `RewardsDistribution` defining how to distribute the emissions.
		type RewardsDistribution: RewardsDistribution<Balance = Self::FlipBalance>;

		/// Gives access to the current set of validators.
		type Validators: EpochInfo<ValidatorId = Self::AccountId>;

		/// How frequently to mint.
		#[pallet::constant]
		type MintFrequency: Get<Self::BlockNumber>;
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

	#[pallet::storage]
	#[pallet::getter(fn dust)]
	/// We keep any dust that could not be allocated on the last emission.
	pub type Dust<T: Config> = StorageValue<_, T::FlipBalance, ValueQuery>;

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event documentation should end with an array that provides descriptive names for event
		/// parameters. [something, who]
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
			let (should_mint, mut weight) = Self::should_mint(current_block);

			if should_mint {
				weight += Self::mint_rewards_for_block(current_block).unwrap_or_else(|w| w);
			}

			weight
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Apply a new emission rate.
		#[pallet::weight(10_000)]
		pub fn emission_rate_changed(
			origin: OriginFor<T>,
			emissions_per_block: T::FlipBalance,
			_tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			todo!("Check validity and update the emission rate.");
		}

		/// A proxy call for witnessing an emission rate update from the StakeManager contract.
		#[pallet::weight(10_000)]
		pub fn witness_emission_rate_changed(
			origin: OriginFor<T>,
			emissions_per_block: T::FlipBalance,
			tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = Call::emission_rate_changed(emissions_per_block, tx_hash);
			T::Witnesser::witness(who, call.into())?;
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		/// Emission rate at genesis.
		pub emission_per_block: T::FlipBalance,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			// 10% annual issuance
			let annual_issuance = T::Emissions::total_issuance().div(10u32.into());
			let seconds_per_year = 31_557_600u32; // Thank you google.
			let blocks_per_year = seconds_per_year / 6u32; // Assume 6-second target block size.
			let emission_per_block = annual_issuance.div(blocks_per_year.into());

			Self { emission_per_block }
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
	/// Determines if we should 
	fn should_mint(block_number: T::BlockNumber) -> (bool, Weight) {
		let mint_frequency = T::MintFrequency::get();
		let blocks_elapsed = block_number - LastMintBlock::<T>::get();
		let result = blocks_elapsed % mint_frequency == Zero::zero();
		let weight = T::DbWeight::get().reads(2);

		(result, weight)
	}

	/// Based on the last block at which rewards were minted, calculates how much issuance needs to be
	/// minted and distributes this a as a reward via [RewardsDistribution].
	fn mint_rewards_for_block(block_number: T::BlockNumber) -> Result<Weight, Weight> {
		// Calculate the outstanding reward amount.
		let blocks_elapsed = block_number - LastMintBlock::<T>::get();
		let blocks_elapsed = T::FlipBalance::unique_saturated_from(blocks_elapsed);

		let reward_amount = EmissionPerBlock::<T>::get().checked_mul(&blocks_elapsed)
			.ok_or(T::DbWeight::get().reads(3))?;
		let reward_amount = reward_amount + Dust::<T>::get();

		// Do the distribution.
		let remainder = T::RewardsDistribution::distribute(reward_amount);
		let exec_weight = T::RewardsDistribution::execution_weight();

		// Update this pallet's state.
		LastMintBlock::<T>::set(block_number);
		Dust::<T>::set(remainder);

		let weight = exec_weight + T::DbWeight::get().reads_writes(3, 2);
		Ok(weight)
	}

	fn distribute_to_validators(amount: T::FlipBalance) -> T::FlipBalance {
		let validators = T::Validators::current_validators();
		let num_validators: T::FlipBalance = (validators.len() as u32).into();
		let reward_per_validator = amount / num_validators;
		let actual_issuance = reward_per_validator * num_validators;
		let remainder = actual_issuance - amount;

		for validator in validators {
			T::Emissions::mint_to(&validator, reward_per_validator);
		}

		remainder
	}
}

struct NaiveRewardsDistribution<T>(PhantomData<T>);

impl<T: Config> RewardsDistribution for NaiveRewardsDistribution<T> {
	type Balance = T::FlipBalance;

	fn distribute(amount: Self::Balance) -> Self::Balance {
		Pallet::<T>::distribute_to_validators(amount)
	}

	fn execution_weight() -> Weight {
		// 1 Read to get the list of validators, and 1 read/write to update each balance.
		let rw: u64 = T::Validators::current_validators().len().saturated_into();
		T::DbWeight::get().reads_writes(rw + 1, rw)
	}
}
