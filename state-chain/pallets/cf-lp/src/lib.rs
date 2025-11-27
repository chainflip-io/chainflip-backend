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
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{address::AddressConverter, AccountOrAddress, AnyChain, ForeignChainAddress};
use cf_primitives::{
	AccountRole, Asset, AssetAmount, BasisPoints, DcaParameters, ForeignChain, SECONDS_PER_BLOCK,
};
use cf_traits::{
	impl_pallet_safe_mode, AccountRoleRegistry, BalanceApi, BoostBalancesApi, Chainflip,
	DepositApi, EgressApi, LpRegistration, LpStatsApi, PoolApi, ScheduledEgressDetails,
	SwapRequestHandler,
};
use serde::{Deserialize, Serialize};

use frame_support::{
	fail,
	pallet_prelude::*,
	sp_runtime::{traits::Zero, DispatchResult, FixedU128, Perbill},
};
use frame_system::pallet_prelude::*;
pub use pallet::*;

mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod migrations;
pub mod weights;
pub use weights::WeightInfo;

use cf_chains::address::EncodedAddress;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(3);

impl_pallet_safe_mode!(PalletSafeMode; deposit_enabled, withdrawal_enabled, internal_swaps_enabled);

pub const STATS_UPDATE_INTERVAL_IN_BLOCKS: u64 = 24 * 3600 / SECONDS_PER_BLOCK; // 24 hours

// Alpha half-life factors for exponential moving averages calculated as:
// Alpha = 1 - e^(-ln 2 * sampling_interval / half_life_period)
// using a sampling interval defined in `STATS_UPDATE_INTERVAL_IN_BLOCKS`. Make sure to update
// these half-life values if `STATS_UPDATE_INTERVAL_IN_BLOCKS` is changed.
pub const ALPHA_HALF_LIFE_1_DAY: Perbill = Perbill::from_parts(500_000_000);
pub const ALPHA_HALF_LIFE_7_DAYS: Perbill = Perbill::from_parts(94_276_335);
pub const ALPHA_HALF_LIFE_30_DAYS: Perbill = Perbill::from_parts(22_840_031);

pub const MAX_NUM_ACOUNTS_TO_PURGE: u32 = 100;

#[frame_support::pallet]
pub mod pallet {
	use cf_amm_math::PriceLimits;
	use cf_chains::{AccountOrAddress, Chain};
	use cf_primitives::{BlockNumber, ChannelId, EgressId};
	use cf_traits::MinimumDeposit;
	use frame_support::sp_runtime::{traits::Zero, FixedU128, SaturatedConversion, Saturating};
	use sp_std::collections::btree_map::BTreeMap;

	use super::*;

	#[derive(
		Copy,
		Clone,
		Debug,
		Default,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		PartialEq,
		Eq,
		Deserialize,
		Serialize,
	)]
	pub struct DeltaStats {
		/// The delta in swap volume since the last sample in USD
		pub limit_orders_swap_usd_volume: FixedU128,
	}

	impl DeltaStats {
		pub fn reset(&mut self) {
			self.limit_orders_swap_usd_volume = FixedU128::zero();
		}

		pub fn on_limit_order(&mut self, usd_amount: FixedU128) {
			self.limit_orders_swap_usd_volume =
				self.limit_orders_swap_usd_volume.saturating_add(usd_amount);
		}
	}

	#[derive(
		Copy,
		Clone,
		Debug,
		Default,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		PartialEq,
		Eq,
		Deserialize,
		Serialize,
	)]
	pub struct WindowedEma {
		pub one_day: FixedU128,
		pub seven_days: FixedU128,
		pub thirty_days: FixedU128,
	}

	impl WindowedEma {
		pub fn new(initial_val: FixedU128) -> Self {
			Self { one_day: initial_val, seven_days: initial_val, thirty_days: initial_val }
		}

		/// Updates the ema values using the new sample.
		pub fn update(&mut self, sample: &FixedU128) {
			self.one_day = Self::calculate_ema(&self.one_day, sample, ALPHA_HALF_LIFE_1_DAY);
			self.seven_days = Self::calculate_ema(&self.seven_days, sample, ALPHA_HALF_LIFE_7_DAYS);
			self.thirty_days =
				Self::calculate_ema(&self.thirty_days, sample, ALPHA_HALF_LIFE_30_DAYS);
		}

		/// Ema is calculated using the formula:
		/// EMA_t = alpha * new_sample + (1 - alpha) * EMA_(t-1)
		fn calculate_ema(
			current_val: &FixedU128,
			new_val: &FixedU128,
			alpha_perbill: Perbill,
		) -> FixedU128 {
			let alpha = FixedU128::from(alpha_perbill);
			let one_minus_alpha = FixedU128::from_u32(1).saturating_sub(alpha);
			new_val
				.saturating_mul(alpha)
				.saturating_add(current_val.saturating_mul(one_minus_alpha))
		}
	}

	#[derive(
		Copy,
		Clone,
		Debug,
		Default,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		PartialEq,
		Eq,
		Deserialize,
		Serialize,
	)]
	pub struct AggStats {
		pub avg_limit_usd_volume: WindowedEma,
	}

	impl AggStats {
		pub fn new(delta: DeltaStats) -> Self {
			Self { avg_limit_usd_volume: WindowedEma::new(delta.limit_orders_swap_usd_volume) }
		}

		pub fn update(&mut self, delta: &DeltaStats) {
			self.avg_limit_usd_volume.update(&delta.limit_orders_swap_usd_volume);
		}
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because we want to emit events when there is a config change during
		/// a runtime upgrade
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// API for handling asset deposits.
		type DepositHandler: DepositApi<
			AnyChain,
			AccountId = <Self as frame_system::Config>::AccountId,
			Amount = <Self as Chainflip>::Amount,
		>;

		/// API for handling asset egress.
		type EgressHandler: EgressApi<AnyChain>;

		/// A converter to convert address to and from human readable to internal address
		/// representation.
		type AddressConverter: AddressConverter;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode>;

		/// The interface for sweeping funds from pools into free balance
		type PoolApi: PoolApi<AccountId = <Self as frame_system::Config>::AccountId>;

		/// The interface to managing balances.
		type BalanceApi: BalanceApi<AccountId = <Self as frame_system::Config>::AccountId>;

		/// The interface to access boosted balances
		type BoostBalancesApi: BoostBalancesApi<
			AccountId = <Self as frame_system::Config>::AccountId,
		>;

		type SwapRequestHandler: SwapRequestHandler<AccountId = Self::AccountId>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;

		#[cfg(feature = "runtime-benchmarks")]
		type FeePayment: cf_traits::FeePayment<
			Amount = <Self as Chainflip>::Amount,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;

		/// The interface to access the minimum deposit amount for each asset
		type MinimumDeposit: MinimumDeposit;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The user does not have enough funds.
		InsufficientBalance,
		/// The user has reached the maximum balance.
		BalanceOverflow,
		/// The caller is not authorized to modify the trading position.
		UnauthorisedToModify,
		/// The Asset cannot be egressed because the destination address is not invalid.
		InvalidEgressAddress,
		/// Then given encoded address cannot be decoded into a valid ForeignChainAddress.
		InvalidEncodedAddress,
		/// A liquidity refund address must be set by the user for the chain before a
		/// deposit address can be requested.
		NoLiquidityRefundAddressRegistered,
		/// Liquidity deposit is disabled due to Safe Mode.
		LiquidityDepositDisabled,
		/// Withdrawals are disabled due to Safe Mode.
		WithdrawalsDisabled,
		/// The account still has open orders remaining.
		OpenOrdersRemaining,
		/// The account still has funds remaining in the free balances.
		FundsRemaining,
		/// The destination account is not a liquidity provider.
		DestinationAccountNotLiquidityProvider,
		/// The account cannot transfer to itself.
		CannotTransferToOriginAccount,
		/// The account still has funds remaining in the boost pools
		BoostedFundsRemaining,
		/// The input amount of on-chain swaps must be at least the minimum deposit amount.
		InternalSwapBelowMinimumDepositAmount,
		/// Internal swaps disabled due to safe mode.
		InternalSwapsDisabled,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		LiquidityDepositAddressReady {
			channel_id: ChannelId,
			asset: Asset,
			deposit_address: EncodedAddress,
			// account the funds will be credited to upon deposit
			account_id: T::AccountId,
			deposit_chain_expiry_block: <AnyChain as Chain>::ChainBlockNumber,
			boost_fee: BasisPoints,
			channel_opening_fee: T::Amount,
		},
		WithdrawalEgressScheduled {
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			destination_address: EncodedAddress,
			fee: AssetAmount,
		},
		LiquidityRefundAddressRegistered {
			account_id: T::AccountId,
			chain: ForeignChain,
			address: ForeignChainAddress,
		},
		AssetTransferred {
			from: T::AccountId,
			to: T::AccountId,
			asset: Asset,
			amount: AssetAmount,
		},
		AssetBalancePurged {
			account_id: T::AccountId,
			asset: Asset,
			amount: AssetAmount,
			egress_id: EgressId,
			destination_address: EncodedAddress,
			fee: AssetAmount,
		},
		AssetBalancePurgeFailed {
			account_id: T::AccountId,
			asset: Asset,
			amount: AssetAmount,
			error: DispatchError,
		},
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Stores the registered emergency withdrawal address for an Account
	#[pallet::storage]
	pub type LiquidityRefundAddress<T: Config> = StorageDoubleMap<
		_,
		Identity,
		T::AccountId,
		Twox64Concat,
		ForeignChain,
		ForeignChainAddress,
	>;

	#[pallet::storage]
	/// Last block number when stats were updated
	pub type StatsLastUpdatedAt<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// Stores intermediate stats for liquidity providers per asset since last update
	#[pallet::storage]
	pub type LpDeltaStats<T: Config> =
		StorageDoubleMap<_, Identity, T::AccountId, Twox64Concat, Asset, DeltaStats>;

	/// Stores exponential moving average stats for liquidity providers per asset
	#[pallet::storage]
	pub type LpAggStats<T: Config> =
		StorageValue<_, BTreeMap<T::AccountId, BTreeMap<Asset, AggStats>>, ValueQuery>;

	//pub type LpAggStats<T: Config> = StorageDoubleMap<_, Identity, T::AccountId, Twox64Concat,
	// Asset, AggStats>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			let mut weight_used: Weight = T::DbWeight::get().reads(1);

			let blocks_elapsed = current_block.saturating_sub(StatsLastUpdatedAt::<T>::get());

			if blocks_elapsed.saturated_into::<u64>() >= STATS_UPDATE_INTERVAL_IN_BLOCKS {
				weight_used += Self::update_agg_stats();

				StatsLastUpdatedAt::<T>::put(current_block);
				weight_used += T::DbWeight::get().writes(1);
			}
			weight_used
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// For when the user wants to deposit assets into the Chain.
		/// Generates a new deposit address for the user to deposit their assets.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::request_liquidity_deposit_address())]
		pub fn request_liquidity_deposit_address(
			origin: OriginFor<T>,
			asset: Asset,
			boost_fee: BasisPoints,
		) -> DispatchResult {
			ensure!(T::SafeMode::get().deposit_enabled, Error::<T>::LiquidityDepositDisabled);

			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			if let Some(refund_address) =
				LiquidityRefundAddress::<T>::get(&account_id, ForeignChain::from(asset))
			{
				let (channel_id, deposit_address, expiry_block, channel_opening_fee) =
					T::DepositHandler::request_liquidity_deposit_address(
						account_id.clone(),
						account_id.clone(),
						asset,
						boost_fee,
						refund_address,
						None,
					)?;

				Self::deposit_event(Event::LiquidityDepositAddressReady {
					channel_id,
					asset,
					deposit_address: T::AddressConverter::to_encoded_address(deposit_address),
					account_id,
					deposit_chain_expiry_block: expiry_block,
					boost_fee,
					channel_opening_fee,
				});

				Ok(())
			} else {
				Err(Error::<T>::NoLiquidityRefundAddressRegistered.into())
			}
		}

		/// Withdraw some amount of an asset from the free balance to an external address.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::withdraw_asset())]
		pub fn withdraw_asset(
			origin: OriginFor<T>,
			amount: AssetAmount,
			asset: Asset,
			destination_address: EncodedAddress,
		) -> DispatchResult {
			Self::transfer_or_withdraw(
				origin,
				amount,
				asset,
				AccountOrAddress::ExternalAddress(destination_address),
			)
		}

		/// Register the account as a Liquidity Provider.
		/// Account roles are immutable once registered.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::register_lp_account())]
		pub fn register_lp_account(who: OriginFor<T>) -> DispatchResult {
			let account_id = ensure_signed(who)?;

			T::AccountRoleRegistry::register_as_liquidity_provider(&account_id)?;

			Ok(())
		}

		/// Registers a Liquidity Refund Address(LRA) for an account.
		///
		/// To request a deposit address for a chain, an LRA must be registered for that chain.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::register_liquidity_refund_address())]
		pub fn register_liquidity_refund_address(
			origin: OriginFor<T>,
			address: EncodedAddress,
		) -> DispatchResult {
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let decoded_address = T::AddressConverter::try_from_encoded_address(address)
				.map_err(|()| Error::<T>::InvalidEncodedAddress)?;

			LiquidityRefundAddress::<T>::insert(
				&account_id,
				decoded_address.chain(),
				decoded_address.clone(),
			);

			Self::deposit_event(Event::<T>::LiquidityRefundAddressRegistered {
				account_id,
				chain: decoded_address.chain(),
				address: decoded_address,
			});
			Ok(())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::deregister_lp_account())]
		pub fn deregister_lp_account(who: OriginFor<T>) -> DispatchResult {
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(who)?;

			ensure!(
				T::PoolApi::pools().iter().all(|asset_pair| {
					T::PoolApi::open_order_count(&account_id, asset_pair).unwrap_or_default() == 0
				}),
				Error::<T>::OpenOrdersRemaining
			);
			ensure!(
				T::BalanceApi::free_balances(&account_id).iter().all(|(_, amount)| *amount == 0),
				Error::<T>::FundsRemaining
			);

			for asset in Asset::all() {
				ensure!(
					T::BoostBalancesApi::boost_pool_account_balance(&account_id, asset) == 0,
					Error::<T>::BoostedFundsRemaining
				);
			}

			let _ = LiquidityRefundAddress::<T>::clear_prefix(&account_id, u32::MAX, None);

			T::AccountRoleRegistry::deregister_as_liquidity_provider(&account_id)?;

			Ok(())
		}

		/// Transfer some amount of an asset from the free balance to the free balance of another LP
		/// account on the Chainflip network.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::withdraw_asset())]
		pub fn transfer_asset(
			origin: OriginFor<T>,
			amount: AssetAmount,
			asset: Asset,
			destination: T::AccountId,
		) -> DispatchResult {
			Self::transfer_or_withdraw(
				origin,
				amount,
				asset,
				AccountOrAddress::InternalAccount(destination),
			)
		}

		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::schedule_swap())]
		pub fn schedule_swap(
			origin: OriginFor<T>,
			amount: AssetAmount,
			input_asset: Asset,
			output_asset: Asset,
			retry_duration: BlockNumber,
			price_limits: PriceLimits,
			dca_params: Option<DcaParameters>,
		) -> DispatchResult {
			ensure!(T::SafeMode::get().internal_swaps_enabled, Error::<T>::InternalSwapsDisabled);

			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			ensure!(
				amount >= T::MinimumDeposit::get(input_asset),
				Error::<T>::InternalSwapBelowMinimumDepositAmount
			);

			Self::ensure_has_refund_address_for_asset(&account_id, output_asset)?;

			T::BalanceApi::try_debit_account(&account_id, input_asset, amount)
				.map_err(|_| Error::<T>::InsufficientBalance)?;

			T::SwapRequestHandler::init_internal_swap_request(
				input_asset,
				amount,
				output_asset,
				retry_duration,
				price_limits,
				dca_params,
				account_id,
			);

			Ok(())
		}

		/// Purges LP asset balances to their refund addresss via egress
		/// Requires Governance
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::schedule_swap())]
		//#[pallet::weight(T::WeightInfo::purge_balances(accounts.len() as u32))]
		pub fn purge_balances(
			origin: OriginFor<T>,
			accounts: BoundedVec<
				(T::AccountId, Asset, AssetAmount),
				ConstU32<MAX_NUM_ACOUNTS_TO_PURGE>,
			>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			for (account_id, asset, amount) in accounts {
				if let Err(error) = Self::purge_account_balance(account_id.clone(), asset, amount) {
					Self::deposit_event(Event::<T>::AssetBalancePurgeFailed {
						account_id,
						asset,
						amount,
						error,
					});
				}
			}

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn transfer_or_withdraw(
		origin: OriginFor<T>,
		amount: AssetAmount,
		asset: Asset,
		destination: AccountOrAddress<T::AccountId, EncodedAddress>,
	) -> DispatchResult {
		ensure!(T::SafeMode::get().withdrawal_enabled, Error::<T>::WithdrawalsDisabled);
		let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

		if amount > 0 {
			match destination {
				AccountOrAddress::InternalAccount(destination_account) => {
					ensure!(
						account_id != destination_account,
						Error::<T>::CannotTransferToOriginAccount
					);
					// Check if the destination account has the role liquidity provider.
					ensure!(
						T::AccountRoleRegistry::has_account_role(
							&destination_account,
							AccountRole::LiquidityProvider,
						),
						Error::<T>::DestinationAccountNotLiquidityProvider
					);
					ensure!(
						LiquidityRefundAddress::<T>::contains_key(
							&destination_account,
							ForeignChain::from(asset)
						),
						Error::<T>::NoLiquidityRefundAddressRegistered
					);

					// Debit the asset from the account.
					T::BalanceApi::try_debit_account(&account_id, asset, amount)?;

					// Credit the asset to the destination account.
					T::BalanceApi::credit_account(&destination_account, asset, amount);

					Self::deposit_event(Event::AssetTransferred {
						from: account_id,
						to: destination_account,
						asset,
						amount,
					});
				},
				AccountOrAddress::ExternalAddress(destination_address) => {
					let destination_address_internal =
						T::AddressConverter::try_from_encoded_address(destination_address.clone())
							.map_err(|_| Error::<T>::InvalidEgressAddress)?;

					// Check validity of Chain and Asset
					ensure!(
						destination_address_internal.chain() == ForeignChain::from(asset),
						Error::<T>::InvalidEgressAddress
					);

					// Debit the asset from the account.
					T::BalanceApi::try_debit_account(&account_id, asset, amount)?;

					let ScheduledEgressDetails { egress_id, egress_amount, fee_withheld } =
						T::EgressHandler::schedule_egress(
							asset,
							amount,
							destination_address_internal,
							None,
						)
						.map_err(Into::into)?;

					Self::deposit_event(Event::<T>::WithdrawalEgressScheduled {
						egress_id,
						asset,
						amount: egress_amount,
						destination_address,
						fee: fee_withheld,
					});
				},
			}
		}
		Ok(())
	}

	fn update_agg_stats() -> Weight {
		let mut execution_weight = Weight::zero();

		LpAggStats::<T>::mutate(|agg_stats_map| {
			// For every existing Lp, update their Aggregate stats from accumulated delta stats
			for (lp, lp_stats) in agg_stats_map.iter_mut() {
				for (asset, agg_stats) in lp_stats.iter_mut() {
					let lp_delta = match LpDeltaStats::<T>::get(lp, asset) {
						Some(delta) => {
							execution_weight.saturating_accrue(T::DbWeight::get().writes(1));
							LpDeltaStats::<T>::remove(lp, asset);
							delta
						},
						None => Default::default(),
					};
					// TODO add weight for update function
					agg_stats.update(&lp_delta);
				}
			}

			// Any left-over deltas correspond to LPs that didn't have Aggregate entries yet
			for (lp, asset, delta) in LpDeltaStats::<T>::iter() {
				let lp_stats = agg_stats_map.entry(lp.clone()).or_default();
				lp_stats.insert(asset, AggStats::new(delta));

				LpDeltaStats::<T>::remove(&lp, asset);
				execution_weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 2));
			}
		});

		execution_weight
	}

	fn purge_account_balance(
		account_id: T::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult {
		ensure!(
			T::AccountRoleRegistry::has_account_role(&account_id, AccountRole::LiquidityProvider,),
			Error::<T>::DestinationAccountNotLiquidityProvider
		);

		let Some(refund_address) =
			LiquidityRefundAddress::<T>::get(&account_id, ForeignChain::from(asset))
		else {
			fail!(Error::<T>::NoLiquidityRefundAddressRegistered);
		};
		ensure!(
			refund_address.chain() == ForeignChain::from(asset),
			Error::<T>::InvalidEgressAddress
		);
		let destination_address = T::AddressConverter::to_encoded_address(refund_address.clone());

		// Sweep earned fees and Debit the asset from the account.
		T::PoolApi::sweep(&account_id)?;
		T::BalanceApi::try_debit_account(&account_id, asset, amount)?;

		let ScheduledEgressDetails { egress_id, egress_amount, fee_withheld } =
			T::EgressHandler::schedule_egress(asset, amount, refund_address, None)
				.map_err(Into::into)?;

		Self::deposit_event(Event::<T>::AssetBalancePurged {
			account_id,
			asset,
			amount: egress_amount,
			egress_id,
			destination_address,
			fee: fee_withheld,
		});

		Ok(())
	}
}

impl<T: Config> LpRegistration for Pallet<T> {
	type AccountId = <T as frame_system::Config>::AccountId;

	fn register_liquidity_refund_address(
		account_id: &Self::AccountId,
		address: ForeignChainAddress,
	) {
		LiquidityRefundAddress::<T>::insert(account_id, address.chain(), address);
	}

	fn ensure_has_refund_address_for_asset(
		account_id: &Self::AccountId,
		asset: Asset,
	) -> DispatchResult {
		ensure!(
			LiquidityRefundAddress::<T>::contains_key(account_id, ForeignChain::from(asset)),
			Error::<T>::NoLiquidityRefundAddressRegistered
		);
		Ok(())
	}
}

impl<T: Config> LpStatsApi for Pallet<T> {
	type AccountId = <T as frame_system::Config>::AccountId;

	fn on_limit_order_filled(who: &Self::AccountId, asset: &Asset, usd_amount: AssetAmount) {
		if usd_amount != AssetAmount::zero() {
			LpDeltaStats::<T>::mutate(who, asset, |maybe_stats| {
				let delta_stats = maybe_stats.get_or_insert_default();

				delta_stats.on_limit_order(FixedU128::from_inner(usd_amount));
			});
		}
	}
}
