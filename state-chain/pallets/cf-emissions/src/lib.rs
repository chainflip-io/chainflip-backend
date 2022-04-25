#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::UpdateFlipSupply;
use cf_traits::{Broadcaster, NonceProvider};
use frame_support::dispatch::Weight;
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod migrations;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

use cf_traits::{
	BlockEmissions, EmissionsTrigger, EpochTransitionHandler, Issuance, RewardsDistribution,
};
use codec::FullCodec;
use frame_support::traits::{Get, Imbalance, OnRuntimeUpgrade, StorageVersion};
use sp_arithmetic::traits::UniqueSaturatedFrom;
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, CheckedDiv, CheckedMul, UniqueSaturatedInto, Zero},
	SaturatedConversion,
};

use cf_traits::SystemStateInfo;

pub mod weights;
pub use weights::WeightInfo;

type BasisPoints = u32;

#[frame_support::pallet]
pub mod pallet {

	use super::*;
	use cf_chains::ChainAbi;
	use cf_traits::SystemStateInfo;
	use frame_support::pallet_prelude::*;
	use frame_system::{ensure_root, pallet_prelude::OriginFor};

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: cf_traits::Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The host chain to which we broadcast supply updates.
		///
		/// In practice this is always [Ethereum] but making this configurable simplifies
		/// testing.
		type HostChain: ChainAbi;

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

		/// An outgoing api call that supports UpdateFlipSupply.
		type ApiCall: UpdateFlipSupply<Self::HostChain>;

		/// Transaction broadcaster for the host chain.
		type Broadcaster: Broadcaster<Self::HostChain, ApiCall = Self::ApiCall>;

		/// Blocks per day.
		#[pallet::constant]
		type BlocksPerDay: Get<Self::BlockNumber>;

		/// Something that can provide a nonce for the threshold signature.
		type NonceProvider: NonceProvider<Self::HostChain>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;

		/// Access to information about the current system state
		type SystemState: SystemStateInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::storage_version(PALLET_VERSION)]
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
	/// Annual inflation set aside for *active* validators, expressed as basis points ie. hundredths
	/// of a percent.
	pub(super) type ValidatorEmissionInflation<T: Config> =
		StorageValue<_, BasisPoints, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn backup_validator_emission_inflation)]
	/// Annual inflation set aside for *backup* validators, expressed as basis points ie. hundredths
	/// of a percent.
	pub(super) type BackupValidatorEmissionInflation<T: Config> =
		StorageValue<_, BasisPoints, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn mint_interval)]
	/// Mint interval in blocks
	pub(super) type MintInterval<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Emissions have been distributed. \[block_number, amount_minted\]
		EmissionsDistributed(BlockNumberFor<T>, T::FlipBalance),
		/// Validator inflation emission has been updated \[new\]
		ValidatorInflationEmissionsUpdated(BasisPoints),
		/// Backup Validator inflation emission has been updated \[new\]
		BackupValidatorInflationEmissionsUpdated(BasisPoints),
		/// MintInterval has been updated [block_number]
		MintIntervalUpdated(BlockNumberFor<T>),
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
		fn on_runtime_upgrade() -> Weight {
			migrations::PalletMigration::<T>::on_runtime_upgrade();
			T::WeightInfo::on_runtime_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::pre_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::post_upgrade()
		}
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			let should_mint = Self::should_mint_at(current_block);

			if should_mint {
				Self::mint_rewards_for_block(current_block);
				Self::broadcast_update_total_supply(T::Issuance::total_issuance(), current_block);
				T::WeightInfo::rewards_minted()
			} else {
				T::WeightInfo::no_rewards_minted()
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Updates the emission rate to Validators.
		///
		/// Can only be called by the root origin.
		///
		/// ## Events
		///
		/// - [ValidatorInflationEmissionsUpdated](Event::ValidatorInflationEmissionsUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::update_validator_emission_inflation())]
		pub fn update_validator_emission_inflation(
			origin: OriginFor<T>,
			inflation: BasisPoints,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ValidatorEmissionInflation::<T>::set(inflation);
			Self::deposit_event(Event::<T>::ValidatorInflationEmissionsUpdated(inflation));
			Ok(().into())
		}

		/// Updates the emission rate to Backup Validators.
		///
		/// ## Events
		///
		/// - [BackupValidatorInflationEmissionsUpdated](Event::
		///   BackupValidatorInflationEmissionsUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::update_backup_validator_emission_inflation())]
		pub fn update_backup_validator_emission_inflation(
			origin: OriginFor<T>,
			inflation: BasisPoints,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			BackupValidatorEmissionInflation::<T>::set(inflation);
			Self::deposit_event(Event::<T>::BackupValidatorInflationEmissionsUpdated(inflation));
			Ok(().into())
		}

		/// Updates the mint interval.
		///
		/// ## Events
		///
		/// - [MintIntervalUpdated](Event:: MintIntervalUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::update_mint_interval())]
		pub fn update_mint_interval(
			origin: OriginFor<T>,
			value: BlockNumberFor<T>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			MintInterval::<T>::put(value);
			Self::deposit_event(Event::<T>::MintIntervalUpdated(value));
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
			Self { validator_emission_inflation: 0, backup_validator_emission_inflation: 0 }
		}
	}

	/// At genesis we need to set the inflation rates for active and passive validators.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			ValidatorEmissionInflation::<T>::put(self.validator_emission_inflation);
			BackupValidatorEmissionInflation::<T>::put(self.backup_validator_emission_inflation);
			MintInterval::<T>::put(T::BlockNumber::from(100_u32));
			<Pallet<T> as BlockEmissions>::calculate_block_emissions();
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Determines if we should mint at block number `block_number`.
	fn should_mint_at(block_number: T::BlockNumber) -> bool {
		let mint_interval = MintInterval::<T>::get();
		let blocks_elapsed = block_number - LastMintBlock::<T>::get();
		Self::should_mint(blocks_elapsed, mint_interval)
	}

	/// Checks if we should mint.
	fn should_mint(
		blocks_elapsed_since_last_mint: T::BlockNumber,
		mint_interval: T::BlockNumber,
	) -> bool {
		blocks_elapsed_since_last_mint >= mint_interval
	}

	/// Updates the total supply on the ETH blockchain
	fn broadcast_update_total_supply(total_supply: T::FlipBalance, block_number: T::BlockNumber) {
		// Emit a threshold signature request.
		// TODO: See if we can replace an old request if there is one.
		if T::SystemState::ensure_no_maintanace().is_ok() {
			T::Broadcaster::threshold_sign_and_broadcast(T::ApiCall::new_unsigned(
				T::NonceProvider::next_nonce(),
				total_supply.unique_saturated_into(),
				block_number.saturated_into(),
			));
		}
	}

	/// Based on the last block at which rewards were minted, calculates how much issuance needs to
	/// be minted and distributes this as a reward via [RewardsDistribution].
	fn mint_rewards_for_block(block_number: T::BlockNumber) {
		// Calculate the outstanding reward amount.
		let blocks_elapsed = block_number - LastMintBlock::<T>::get();
		if blocks_elapsed == Zero::zero() {
			return
		}

		let blocks_elapsed = T::FlipBalance::unique_saturated_from(blocks_elapsed);

		let reward_amount = ValidatorEmissionPerBlock::<T>::get().checked_mul(&blocks_elapsed);

		let reward_amount = reward_amount.unwrap_or_else(|| {
			log::error!("Overflow while trying to mint rewards at block {:?}.", block_number);
			Zero::zero()
		});

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

	fn update_validator_block_emission(emission: Self::Balance) {
		ValidatorEmissionPerBlock::<T>::put(emission);
	}

	fn update_backup_validator_block_emission(emission: Self::Balance) {
		BackupValidatorEmissionPerBlock::<T>::put(emission);
	}

	fn calculate_block_emissions() {
		fn inflation_to_block_reward<T: Config>(inflation: BasisPoints) -> T::FlipBalance {
			const DAYS_IN_YEAR: u32 = 365;

			((T::Issuance::total_issuance() * inflation.into()) /
				10_000u32.into() / DAYS_IN_YEAR.into())
			.checked_div(&T::FlipBalance::unique_saturated_from(T::BlocksPerDay::get()))
			.unwrap_or_else(|| {
				log::error!("blocks per day should be greater than zero");
				Zero::zero()
			})
		}

		Self::update_validator_block_emission(inflation_to_block_reward::<T>(
			ValidatorEmissionInflation::<T>::get(),
		));

		Self::update_backup_validator_block_emission(inflation_to_block_reward::<T>(
			BackupValidatorEmissionInflation::<T>::get(),
		));
	}
}

impl<T: Config> EmissionsTrigger for Pallet<T> {
	fn trigger_emissions() {
		let current_block_number = frame_system::Pallet::<T>::block_number();
		Self::mint_rewards_for_block(current_block_number);
	}
}

impl<T: Config> EpochTransitionHandler for Pallet<T> {
	type ValidatorId = <T as frame_system::Config>::AccountId;

	fn on_new_epoch(_epoch_validators: &[Self::ValidatorId]) {
		// Calculate block emissions on every epoch
		Self::calculate_block_emissions();
		// Process any outstanding emissions.
		Self::trigger_emissions();
	}
}
