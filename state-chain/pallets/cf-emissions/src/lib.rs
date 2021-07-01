#![cfg_attr(not(feature = "std"), no_std)]

//! A pallet for managing the FLIP emissions schedule.

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use cf_traits::Emissions;
use codec::FullCodec;
use sp_runtime::traits::{AtLeast32BitUnsigned, Zero};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::Witnesser;
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
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
			+ MaybeSerializeDeserialize
			+ AtLeast32BitUnsigned;

		/// An implmentation of the [Emissions] trait.
		type Emissions: Emissions<Balance = Self::FlipBalance, AccountId = Self::AccountId>;

		/// Provides an origin check for witness transactions.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

		/// An implementation of the witnesser, allows us to define witness_* helper extrinsics.
		type Witnesser: Witnesser<Call = <Self as Config>::Call, AccountId = Self::AccountId>;

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
	pub enum Error<T> {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> frame_support::weights::Weight {
			let mut weight = frame_support::weights::Weight::zero();

			let last_mint_block = LastMintBlock::<T>::get();
			let mint_frequency = T::MintFrequency::get();
			weight = weight.saturating_add(T::DbWeight::get().reads(2));

			if (current_block - last_mint_block) % mint_frequency == Zero::zero() {
				todo!("Print some billz.")
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
			let annual_issuance = T::Emissions::total_issuance() / T::FlipBalance::from(10);
			let seconds_per_year = T::FlipBalance::from(31_557_600); // Thank you google.
			let blocks_per_year = seconds_per_year / 6; // Assume 6-second target block size.
			let emission_per_block = annual_issuance / blocks_per_year;

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
