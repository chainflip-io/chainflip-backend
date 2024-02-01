#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{address::ForeignChainAddress, evm::api::EthEnvironmentProvider, UpdateFlipSupply};
use cf_traits::{
	impl_pallet_safe_mode, BackupRewardsNotifier, BlockEmissions, Broadcaster, EgressApi,
	FlipBurnInfo, Issuance, RewardsDistribution,
};
use codec::MaxEncodedLen;
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;

mod benchmarking;
mod mock;
mod tests;

use frame_support::{
	sp_runtime::{
		traits::{AtLeast32BitUnsigned, UniqueSaturatedInto, Zero},
		Rounding, SaturatedConversion,
	},
	traits::{Get, Imbalance},
};
use sp_arithmetic::traits::UniqueSaturatedFrom;

use cf_primitives::{chains::AnyChain, Asset};

pub mod weights;
pub use weights::WeightInfo;

impl_pallet_safe_mode!(PalletSafeMode; emissions_sync_enabled);

#[frame_support::pallet]
pub mod pallet {

	use super::*;
	use cf_chains::Chain;
	use frame_support::{pallet_prelude::*, DefaultNoBound};
	use frame_system::pallet_prelude::OriginFor;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: cf_traits::Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The host chain to which we broadcast supply updates.
		///
		/// In practice this is always [Ethereum] but making this configurable simplifies
		/// testing.
		type HostChain: Chain;

		/// The Flip token denomination.
		type FlipBalance: Member
			+ Parameter
			+ MaxEncodedLen
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ AtLeast32BitUnsigned
			+ UniqueSaturatedFrom<BlockNumberFor<Self>>
			+ Into<u128>
			+ From<u128>;

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
			Issuance = Self::Issuance,
		>;

		/// An outgoing api call that supports UpdateFlipSupply.
		type ApiCall: UpdateFlipSupply<<<Self as pallet::Config>::HostChain as Chain>::ChainCrypto>;

		/// Transaction broadcaster for the host chain.
		type Broadcaster: Broadcaster<Self::HostChain, ApiCall = Self::ApiCall>;

		/// The number of blocks for the time frame we would test liveliness within
		#[pallet::constant]
		type CompoundingInterval: Get<BlockNumberFor<Self>>;

		/// Something that can provide the state chain gateway address.
		type EthEnvironment: EthEnvironmentProvider;

		/// The interface for accessing the amount of Flip we want burn.
		type FlipToBurn: FlipBurnInfo;

		/// API for handling asset egress.
		type EgressHandler: EgressApi<AnyChain>;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode>;

		/// Benchmark stuff.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn last_supply_update_block)]
	/// The block number at which we last updated supply to the Eth Chain.
	pub type LastSupplyUpdateBlock<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn current_authority_emission_per_block)]
	/// The amount of Flip we mint to validators per block.
	pub type CurrentAuthorityEmissionPerBlock<T: Config> =
		StorageValue<_, T::FlipBalance, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn backup_node_emission_per_block)]
	/// The amount of Flip we mint to backup nodes per block.
	pub type BackupNodeEmissionPerBlock<T: Config> = StorageValue<_, T::FlipBalance, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn current_authority_emission_inflation)]
	/// Inflation per `COMPOUNDING_INTERVAL` set aside for current authorities in parts per billion.
	pub(super) type CurrentAuthorityEmissionInflation<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn backup_node_emission_inflation)]
	/// Inflation per `COMPOUNDING_INTERVAL` set aside for *backup* nodes, in parts per billion.
	pub(super) type BackupNodeEmissionInflation<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn supply_update_interval)]
	/// Mint interval in blocks
	pub(super) type SupplyUpdateInterval<T: Config> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Supply Update has been Broadcasted [block_number]
		SupplyUpdateBroadcastRequested(BlockNumberFor<T>),
		/// Current authority inflation emission has been updated \[new\]
		CurrentAuthorityInflationEmissionsUpdated(u32),
		/// Backup node inflation emission has been updated \[new\]
		BackupNodeInflationEmissionsUpdated(u32),
		/// SupplyUpdateInterval has been updated [block_number]
		SupplyUpdateIntervalUpdated(BlockNumberFor<T>),
		/// Rewards have been distributed to [account_id] \[amount\]
		BackupRewardsDistributed { account_id: T::AccountId, amount: T::FlipBalance },
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
			T::RewardsDistribution::distribute();
			if Self::should_update_supply_at(current_block) {
				if T::SafeMode::get().emissions_sync_enabled {
					let flip_to_burn = T::FlipToBurn::take_flip_to_burn();
					if flip_to_burn > Zero::zero() {
						T::EgressHandler::schedule_egress(
							Asset::Flip,
							flip_to_burn,
							ForeignChainAddress::Eth(
								T::EthEnvironment::state_chain_gateway_address(),
							),
							None,
						);
						T::Issuance::burn(flip_to_burn.into());
					}
					Self::broadcast_update_total_supply(
						T::Issuance::total_issuance(),
						current_block,
					);
					Self::deposit_event(Event::SupplyUpdateBroadcastRequested(current_block));
					LastSupplyUpdateBlock::<T>::set(current_block);
					return T::WeightInfo::rewards_minted()
				} else {
					log::info!("Runtime Safe Mode is CODE RED: Flip total issuance update broadcast are paused for now.");
				}
			}
			T::WeightInfo::rewards_not_minted()
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
		/// - [CurrentAuthorityInflationEmissionsUpdated](Event::
		///   CurrentAuthorityInflationEmissionsUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::update_current_authority_emission_inflation())]
		pub fn update_current_authority_emission_inflation(
			origin: OriginFor<T>,
			inflation: u32,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			CurrentAuthorityEmissionInflation::<T>::set(inflation);
			Self::deposit_event(Event::<T>::CurrentAuthorityInflationEmissionsUpdated(inflation));
			Ok(().into())
		}

		/// Updates the emission rate to Backup nodes.
		///
		/// ## Events
		///
		/// - [BackupNodeInflationEmissionsUpdated](Event:: BackupNodeInflationEmissionsUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::update_backup_node_emission_inflation())]
		pub fn update_backup_node_emission_inflation(
			origin: OriginFor<T>,
			inflation: u32,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			BackupNodeEmissionInflation::<T>::set(inflation);
			Self::deposit_event(Event::<T>::BackupNodeInflationEmissionsUpdated(inflation));
			Ok(().into())
		}

		/// Updates the Supply Update interval.
		///
		/// ## Events
		///
		/// - [SupplyUpdateIntervalUpdated](Event:: SupplyUpdateIntervalUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::update_supply_update_interval())]
		pub fn update_supply_update_interval(
			origin: OriginFor<T>,
			value: BlockNumberFor<T>,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			SupplyUpdateInterval::<T>::put(value);
			Self::deposit_event(Event::<T>::SupplyUpdateIntervalUpdated(value));
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T> {
		pub current_authority_emission_inflation: u32,
		pub backup_node_emission_inflation: u32,
		pub supply_update_interval: u32,
		pub _config: PhantomData<T>,
	}

	/// At genesis we need to set the inflation rates for active and backup validators.
	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			CurrentAuthorityEmissionInflation::<T>::put(self.current_authority_emission_inflation);
			BackupNodeEmissionInflation::<T>::put(self.backup_node_emission_inflation);
			SupplyUpdateInterval::<T>::put(BlockNumberFor::<T>::from(self.supply_update_interval));
			<Pallet<T> as BlockEmissions>::calculate_block_emissions();
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Determines if we should broadcast supply update at block number `block_number`.
	fn should_update_supply_at(block_number: BlockNumberFor<T>) -> bool {
		let supply_update_interval = SupplyUpdateInterval::<T>::get();
		let blocks_elapsed = block_number - LastSupplyUpdateBlock::<T>::get();
		blocks_elapsed >= supply_update_interval
	}

	/// Updates the total supply on the ETH blockchain
	fn broadcast_update_total_supply(
		total_supply: T::FlipBalance,
		block_number: BlockNumberFor<T>,
	) {
		// Emit a threshold signature request.
		// TODO: See if we can replace an old request if there is one.
		T::Broadcaster::threshold_sign_and_broadcast(T::ApiCall::new_unsigned(
			total_supply.unique_saturated_into(),
			block_number.saturated_into(),
		));
	}
}

impl<T: Config> BackupRewardsNotifier for Pallet<T> {
	type Balance = T::FlipBalance;
	type AccountId = T::AccountId;

	fn emit_event(account_id: &Self::AccountId, amount: Self::Balance) {
		Self::deposit_event(Event::BackupRewardsDistributed {
			account_id: account_id.clone(),
			amount,
		});
	}
}

impl<T: Config> BlockEmissions for Pallet<T> {
	type Balance = T::FlipBalance;

	fn update_authority_block_emission(emission: Self::Balance) {
		CurrentAuthorityEmissionPerBlock::<T>::put(emission);
	}

	fn update_backup_node_block_emission(emission: Self::Balance) {
		BackupNodeEmissionPerBlock::<T>::put(emission);
	}

	fn calculate_block_emissions() {
		fn inflation_to_block_reward<T: Config>(inflation_per_bill: u32) -> T::FlipBalance {
			calculate_inflation_to_block_reward(
				T::Issuance::total_issuance(),
				inflation_per_bill.into(),
				T::FlipBalance::unique_saturated_from(T::CompoundingInterval::get()),
			)
		}

		Self::update_authority_block_emission(inflation_to_block_reward::<T>(
			CurrentAuthorityEmissionInflation::<T>::get(),
		));

		Self::update_backup_node_block_emission(inflation_to_block_reward::<T>(
			BackupNodeEmissionInflation::<T>::get(),
		));
	}
}

fn calculate_inflation_to_block_reward<T>(
	issuance: T,
	inflation_per_bill: T,
	heartbeat_interval: T,
) -> T
where
	T: Into<u128> + From<u128>,
{
	use frame_support::sp_runtime::helpers_128bit::multiply_by_rational_with_rounding;

	multiply_by_rational_with_rounding(
		issuance.into(),
		inflation_per_bill.into(),
		1_000_000_000u128,
		Rounding::Down,
	)
	.unwrap_or_else(|| {
		log::error!("Error calculating block rewards, Either Issuance or inflation value too big",);
		0_u128
	})
	.checked_div(heartbeat_interval.into())
	.unwrap_or_else(|| {
		log::error!("Heartbeat Interval should be greater than zero");
		Zero::zero()
	})
	.into()
}
