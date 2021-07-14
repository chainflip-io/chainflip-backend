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

use cf_traits::{EmissionsTrigger, EpochInfo, Issuance, RewardsDistribution, Witnesser};
use codec::FullCodec;
use frame_support::traits::{Get, Imbalance};
use sp_arithmetic::traits::UniqueSaturatedFrom;
use sp_runtime::traits::{AtLeast32BitUnsigned, CheckedMul, Zero};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	pub type EthTransactionHash = [u8; 32];

	#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
	pub struct EthToNative(u32, u32);

	impl EthToNative {
		pub fn convert_eth_to_native<T: Config>(&self, amount: T::FlipBalance) -> T::FlipBalance {
			amount * T::FlipBalance::from(self.1) / T::FlipBalance::from(self.0)
		}

		pub fn convert_native_to_eth<T: Config>(&self, amount: T::FlipBalance) -> T::FlipBalance {
			amount * T::FlipBalance::from(self.0) / T::FlipBalance::from(self.1)
		}
	}

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

		/// An imbalance type representing freshly minted, unallocated funds.
		type Surplus: Imbalance<Self::FlipBalance>;

		/// An implmentation of the [Issuance] trait.
		type Issuance: Issuance<
			Balance = Self::FlipBalance,
			AccountId = Self::AccountId,
			Surplus = Self::Surplus,
		>;

		/// Provides an origin check for witness transactions.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

		/// An implementation of the witnesser, allows us to define witness_* helper extrinsics.
		type Witnesser: Witnesser<Call = <Self as Config>::Call, AccountId = Self::AccountId>;

		/// An implementation of `RewardsDistribution` defining how to distribute the emissions.
		type RewardsDistribution: RewardsDistribution<
			Balance = Self::FlipBalance,
			Surplus = Self::Surplus,
		>;

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
	#[pallet::getter(fn block_time_ratio)]
	/// The ratio of eth block time to our native block time, expressed as a tuple.
	pub type BlockTimeRatio<T: Config> = StorageValue<_, EthToNative, ValueQuery>;

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
			let (should_mint, mut weight) = Self::should_mint_at(current_block);

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
			emissions_per_eth_block: T::FlipBalance,
			_tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let emissions_per_block = Self::convert_emissions_rate(emissions_per_eth_block);
			EmissionPerBlock::<T>::set(emissions_per_block);

			Ok(().into())
		}

		/// A proxy call for witnessing an emission rate update from the StakeManager contract.
		#[pallet::weight(10_000)]
		pub fn witness_emission_rate_changed(
			origin: OriginFor<T>,
			emissions_per_eth_block: T::FlipBalance,
			tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = Call::emission_rate_changed(emissions_per_eth_block, tx_hash);
			T::Witnesser::witness(who, call.into())?;
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		/// Emission rate at genesis.
		pub emission_per_block: T::FlipBalance,
		pub eth_block_time: u32,
		pub native_block_time: u32,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				emission_per_block: Zero::zero(),
				eth_block_time: 13,
				native_block_time: 6,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			EmissionPerBlock::<T>::set(self.emission_per_block);
			BlockTimeRatio::<T>::set(EthToNative(self.eth_block_time, self.native_block_time));
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Converts the emissions rate per eth block to emissions per state chain block.
	fn convert_emissions_rate(emissions_per_eth_block: T::FlipBalance) -> T::FlipBalance {
		let ratio = Self::block_time_ratio();
		ratio.convert_eth_to_native::<T>(emissions_per_eth_block)
	}

	/// Determines if we should mint at block number `block_number`.
	fn should_mint_at(block_number: T::BlockNumber) -> (bool, Weight) {
		let mint_frequency = T::MintFrequency::get();
		let blocks_elapsed = block_number - LastMintBlock::<T>::get();
		let should_mint = Self::should_mint(blocks_elapsed, mint_frequency);
		let weight = T::DbWeight::get().reads(2);

		(should_mint, weight)
	}

	/// Checks if we should mint.
	fn should_mint(
		blocks_elapsed_since_last_mint: T::BlockNumber,
		mint_frequency: T::BlockNumber,
	) -> bool {
		blocks_elapsed_since_last_mint >= mint_frequency
	}

	/// Based on the last block at which rewards were minted, calculates how much issuance needs to be
	/// minted and distributes this a as a reward via [RewardsDistribution].
	fn mint_rewards_for_block(block_number: T::BlockNumber) -> Result<Weight, Weight> {
		// Calculate the outstanding reward amount.
		let blocks_elapsed = block_number - LastMintBlock::<T>::get();
		let blocks_elapsed = T::FlipBalance::unique_saturated_from(blocks_elapsed);

		let reward_amount = EmissionPerBlock::<T>::get()
			.checked_mul(&blocks_elapsed)
			.ok_or(T::DbWeight::get().reads(2))?;

		// Mint the rewards
		let reward = T::Issuance::mint(reward_amount);

		// Delegate the distribution.
		T::RewardsDistribution::distribute(reward);
		let exec_weight = T::RewardsDistribution::execution_weight();

		// Update this pallet's state.
		LastMintBlock::<T>::set(block_number);

		Self::deposit_event(Event::EmissionsDistributed(block_number, reward_amount));

		let weight = exec_weight + T::DbWeight::get().reads_writes(2, 1);
		Ok(weight)
	}
}

impl<T: Config> EmissionsTrigger for Pallet<T> {
	type BlockNumber = BlockNumberFor<T>;

	fn trigger_emissions(block_number: Self::BlockNumber) {
		let _ = Self::mint_rewards_for_block(block_number);
	}
}
