#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extended_key_value_attributes)]
#![doc = include_str!("../README.md")]

use frame_support::dispatch::Weight;
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use cf_traits::{BlockEmissions, EmissionsTrigger, Issuance, RewardsDistribution};
use codec::FullCodec;
use core::convert::TryInto;
use frame_support::traits::{Get, Imbalance};
use sp_arithmetic::traits::UniqueSaturatedFrom;
use sp_runtime::traits::CheckedDiv;
use sp_runtime::{
	offchain::storage_lock::BlockNumberProvider,
	traits::{AtLeast32BitUnsigned, CheckedMul, Zero},
};

pub mod weights;
pub use weights::WeightInfo;

type BasisPoints = u32;

#[frame_support::pallet]
pub mod pallet {

	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::ensure_root;
	use frame_system::pallet_prelude::OriginFor;

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

		/// An implementation of the [Issuance] trait.
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

		/// Blocks per day.
		#[pallet::constant]
		type BlocksPerDay: Get<Self::BlockNumber>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn last_mint_block)]
	/// The block number at which we last minted Flip.
	pub type LastMintBlock<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn validator_emission_per_block)]
	/// The amount of Flip we mint to validators per block.
	pub type ValidatorEmissionPerBlock<T: Config> = StorageValue<_, T::FlipBalance, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn backup_validator_emission_per_block)]
	/// The block number at which we last minted Flip.
	pub type BackupValidatorEmissionPerBlock<T: Config> =
		StorageValue<_, T::FlipBalance, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn validator_emission_inflation)]
	/// Annual inflation set aside for *active* validators, expressed as basis points ie. hundredths of a percent.
	pub(super) type ValidatorEmissionInflation<T: Config> =
		StorageValue<_, BasisPoints, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn backup_validator_emission_inflation)]
	/// Annual inflation set aside for *backup* validators, expressed as basis points ie. hundredths of a percent.
	pub(super) type BackupValidatorEmissionInflation<T: Config> =
		StorageValue<_, BasisPoints, ValueQuery>;

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Emissions have been distributed. [block_number, amount_minted]
		EmissionsDistributed(BlockNumberFor<T>, T::FlipBalance),
		/// Validator inflation emission has been updated [new]
		ValidatorInflationEmissionsUpdated(BasisPoints),
		/// Backup Validator inflation emission has been updated [new]
		BackupValidatorInflationEmissionsUpdated(BasisPoints),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Emissions calculation resulted in overflow.
		Overflow,
		/// Invalid percentage
		InvalidPercentage,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			let should_mint = Self::should_mint_at(current_block);

			if should_mint {
				Self::mint_rewards_for_block(current_block);
				T::WeightInfo::rewards_minted(current_block.try_into().unwrap_or_default())
			} else {
				T::WeightInfo::no_rewards_minted()
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(T::WeightInfo::update_validator_emission_inflation(1))]
		pub(super) fn update_validator_emission_inflation(
			origin: OriginFor<T>,
			inflation: BasisPoints,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ValidatorEmissionInflation::<T>::set(inflation);
			Self::deposit_event(Event::<T>::ValidatorInflationEmissionsUpdated(inflation));
			Ok(().into())
		}

		#[pallet::weight(T::WeightInfo::update_backup_validator_emission_inflation(1))]
		pub(super) fn update_backup_validator_emission_inflation(
			origin: OriginFor<T>,
			inflation: BasisPoints,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			BackupValidatorEmissionInflation::<T>::set(inflation);
			Self::deposit_event(Event::<T>::BackupValidatorInflationEmissionsUpdated(
				inflation,
			));
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub validator_emission_inflation: BasisPoints,
		pub backup_validator_emission_inflation: BasisPoints,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {
				validator_emission_inflation: 0,
				backup_validator_emission_inflation: 0,
			}
		}
	}

	/// At genesis we need to set the inflation rates for active and passive validators.
	///
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			ValidatorEmissionInflation::<T>::put(self.validator_emission_inflation);
			BackupValidatorEmissionInflation::<T>::put(self.backup_validator_emission_inflation);
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
	/// minted and distributes this as a reward via [RewardsDistribution].
	fn mint_rewards_for_block(block_number: T::BlockNumber) {
		// Calculate the outstanding reward amount.
		let blocks_elapsed = block_number - LastMintBlock::<T>::get();
		if blocks_elapsed == Zero::zero() {
			return;
		}

		let blocks_elapsed = T::FlipBalance::unique_saturated_from(blocks_elapsed);

		let reward_amount = ValidatorEmissionPerBlock::<T>::get().checked_mul(&blocks_elapsed);

		// Check if an overflow occurred during the multiplication
		if reward_amount.is_none() {
			// TODO: log an error here
			frame_support::debug::error!("Failed to mint rewards at block {:?}", block_number);
			return;
		}

		// Save unwrap the result and shadow the variable
		let reward_amount = reward_amount.expect("");

		if !reward_amount.is_zero() {
			// Mint the rewards
			let reward = T::Issuance::mint(reward_amount);

			// Delegate the distribution.
			T::RewardsDistribution::distribute(reward);
		}

		// Update this pallet's state.
		LastMintBlock::<T>::set(block_number);

		Self::deposit_event(Event::EmissionsDistributed(block_number, reward_amount));
	}
}

impl<T: Config> BlockEmissions for Pallet<T> {
	type Balance = T::FlipBalance;

	fn update_validator_block_emission(emission: Self::Balance) -> Weight {
		ValidatorEmissionPerBlock::<T>::put(emission);
		T::DbWeight::get().writes(1)
	}

	fn update_backup_validator_block_emission(emission: Self::Balance) -> Weight {
		BackupValidatorEmissionPerBlock::<T>::put(emission);
		T::DbWeight::get().writes(1)
	}

	fn calculate_block_emissions() -> Weight {
		fn inflation_to_block_reward<T: Config>(inflation: BasisPoints) -> T::FlipBalance {
			const DAYS_IN_YEAR: u32 = 365;

			((T::Issuance::total_issuance() * inflation.into())
				/ 10_000u32.into()
				/ DAYS_IN_YEAR.into())
			.checked_div(&T::FlipBalance::unique_saturated_from(
				T::BlocksPerDay::get(),
			))
			.expect("blocks per day should be greater than zero")
		}

		Self::update_validator_block_emission(inflation_to_block_reward::<T>(
			ValidatorEmissionInflation::<T>::get(),
		));

		Self::update_backup_validator_block_emission(inflation_to_block_reward::<T>(
			BackupValidatorEmissionInflation::<T>::get(),
		));

		0
	}
}

impl<T: Config> EmissionsTrigger for Pallet<T> {
	// TODO: remove weight and delegate benchmarking to the calling components
	fn trigger_emissions() -> Weight {
		let current_block_number = frame_system::Pallet::<T>::current_block_number();
		Self::mint_rewards_for_block(current_block_number);
		0
	}
}
