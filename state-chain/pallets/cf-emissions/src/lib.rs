// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{eth::api::StateChainGatewayAddressProvider, UpdateFlipSupply};
use cf_primitives::{AssetAmount, EgressId};
use cf_traits::{
	impl_pallet_safe_mode, Broadcaster, EgressApi, FlipBurnOrMoveInfo, Issuance,
	RewardsDistribution, ScheduledEgressDetails,
};
use codec::MaxEncodedLen;
use frame_support::storage::transactional::with_storage_layer;
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;

mod benchmarking;
pub mod migrations;
mod mock;
mod tests;

use frame_support::{
	sp_runtime::{
		traits::{AtLeast32BitUnsigned, UniqueSaturatedInto, Zero},
		Rounding, SaturatedConversion,
	},
	traits::Get,
};
use sp_arithmetic::traits::UniqueSaturatedFrom;

pub mod weights;
pub use weights::WeightInfo;

/// In order to trigger the buy and burn, the amount to burn must be a factor of [BURN_MULTIPLE]
/// greater than the egress fee.
const BURN_FEE_MULTIPLE: AssetAmount = 100;

impl_pallet_safe_mode!(PalletSafeMode; emissions_sync_enabled);

#[frame_support::pallet]
pub mod pallet {
	pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

	use super::*;
	use cf_chains::{eth::api::StateChainGatewayAddressProvider, Chain, Ethereum};
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

		/// An implementation of the [Issuance] trait.
		type Issuance: Issuance<Balance = Self::FlipBalance, AccountId = Self::AccountId>;

		/// An implementation of `RewardsDistribution` defining how to distribute the emissions.
		type RewardsDistribution: RewardsDistribution<
			Balance = Self::FlipBalance,
			AccountId = Self::AccountId,
		>;

		/// An outgoing api call that supports UpdateFlipSupply.
		type ApiCall: UpdateFlipSupply<<<Self as pallet::Config>::HostChain as Chain>::ChainCrypto>;

		/// Transaction broadcaster for the host chain.
		type Broadcaster: Broadcaster<Self::HostChain, ApiCall = Self::ApiCall>;

		#[pallet::constant]
		type CompoundingInterval: Get<BlockNumberFor<Self>>;

		/// Something that can provide the state chain gateway address.
		type EthEnvironment: StateChainGatewayAddressProvider;

		/// The interface for accessing the amount of Flip we want burn.
		type FlipToBurnOrMove: FlipBurnOrMoveInfo;

		/// API for handling asset egress. Emissions only interacts with Ethereum.
		type EgressHandler: EgressApi<Ethereum>;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode>;

		/// Benchmark stuff.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
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
	#[pallet::getter(fn current_authority_emission_inflation)]
	/// Inflation per `COMPOUNDING_INTERVAL` set aside for current authorities in parts per billion.
	pub(super) type CurrentAuthorityEmissionInflation<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn supply_update_interval)]
	/// Mint interval in blocks
	pub(super) type SupplyUpdateInterval<T: Config> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub fn deposit_event)]
	pub enum Event<T: Config> {
		/// Supply Update has been Broadcasted [block_number]
		SupplyUpdateBroadcastRequested(BlockNumberFor<T>),
		/// Current authority inflation emission has been updated \[new\]
		CurrentAuthorityInflationEmissionsUpdated(u32),
		/// SupplyUpdateInterval has been updated [block_number]
		SupplyUpdateIntervalUpdated(BlockNumberFor<T>),
		/// The Flip that was bought using the network fee has been burned.
		NetworkFeeBurned { amount: AssetAmount, egress_id: EgressId },
		/// The Flip burn was skipped.
		FlipBurnSkipped { reason: DispatchError },
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Emissions calculation resulted in overflow.
		Overflow,
		/// Invalid percentage
		InvalidPercentage,
		/// The Flip balance was below the burn threshold.
		FlipBalanceBelowBurnThreshold,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			if current_block % T::CompoundingInterval::get() == Zero::zero() {
				Self::update_block_emissions();
			}
			if Self::should_update_supply_at(current_block) {
				if T::SafeMode::get().emissions_sync_enabled {
					Self::burn_flip_network_fee();
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
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::update_current_authority_emission_inflation())]
		pub fn update_current_authority_emission_inflation(
			origin: OriginFor<T>,
			inflation: u32,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			CurrentAuthorityEmissionInflation::<T>::set(inflation);
			Self::deposit_event(Event::<T>::CurrentAuthorityInflationEmissionsUpdated(inflation));
			Ok(())
		}

		/// Updates the Supply Update interval.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::update_supply_update_interval())]
		pub fn update_supply_update_interval(
			origin: OriginFor<T>,
			value: BlockNumberFor<T>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			SupplyUpdateInterval::<T>::put(value);
			Self::deposit_event(Event::<T>::SupplyUpdateIntervalUpdated(value));
			Ok(())
		}
	}

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T> {
		pub current_authority_emission_inflation: u32,
		pub supply_update_interval: u32,
		pub _phantom: PhantomData<T>,
	}

	/// At genesis we need to set the inflation rates for active validators.
	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			CurrentAuthorityEmissionInflation::<T>::put(self.current_authority_emission_inflation);
			SupplyUpdateInterval::<T>::put(BlockNumberFor::<T>::from(self.supply_update_interval));
			Pallet::<T>::update_block_emissions();
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

	//TODO: rename function and trait/use separate trait to get the FLIP to be sent to the state
	// chain gateway
	fn burn_flip_network_fee() {
		match with_storage_layer(|| {
			let flip_to_be_sent_to_gateway = T::FlipToBurnOrMove::take_flip_to_be_sent_to_gateway();
			let flip_to_offset = T::FlipToBurnOrMove::take_flip_deficit();
			let flip_to_burn = T::FlipToBurnOrMove::take_flip_to_burn();
			if flip_to_burn == Zero::zero() {
				return Err(Error::<T>::FlipBalanceBelowBurnThreshold.into())
			}
			T::EgressHandler::schedule_egress(
				cf_chains::assets::eth::Asset::Flip,
				flip_to_burn
					.saturating_add(flip_to_be_sent_to_gateway)
					.saturating_sub(flip_to_offset),
				T::EthEnvironment::state_chain_gateway_address(),
				None,
			)
			.map_err(Into::into)
			.and_then(
				|result @ ScheduledEgressDetails { egress_amount, fee_withheld, .. }| {
					if egress_amount < BURN_FEE_MULTIPLE * fee_withheld {
						Err(Error::<T>::FlipBalanceBelowBurnThreshold.into())
					} else {
						Ok((result, flip_to_be_sent_to_gateway))
					}
				},
			)
		}) {
			Ok((
				ScheduledEgressDetails { egress_id, egress_amount, .. },
				flip_to_be_sent_to_gateway,
			)) => {
				let flip_to_burn = egress_amount.saturating_sub(flip_to_be_sent_to_gateway);
				T::Issuance::burn_offchain(flip_to_burn.into());
				Self::deposit_event(Event::NetworkFeeBurned { amount: flip_to_burn, egress_id });
			},
			Err(e) => {
				Self::deposit_event(Event::FlipBurnSkipped { reason: e });
			},
		}
	}

	fn update_block_emissions() {
		CurrentAuthorityEmissionPerBlock::<T>::put(calculate_inflation_to_block_reward(
			T::Issuance::total_issuance(),
			CurrentAuthorityEmissionInflation::<T>::get().into(),
			T::FlipBalance::unique_saturated_from(T::CompoundingInterval::get()),
		));
	}
}

fn calculate_inflation_to_block_reward<T>(
	issuance: T,
	inflation_per_bill: T,
	compounding_interval: T,
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
	.checked_div(compounding_interval.into())
	.unwrap_or_else(|| {
		log::error!("Compounding Interval should be greater than zero");
		Zero::zero()
	})
	.into()
}

impl<T: Config> pallet_authorship::EventHandler<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
	fn note_author(author: T::AccountId) {
		let reward_amount = CurrentAuthorityEmissionPerBlock::<T>::get();
		if reward_amount != Zero::zero() {
			T::RewardsDistribution::distribute(reward_amount, &author);
		}
	}
}
