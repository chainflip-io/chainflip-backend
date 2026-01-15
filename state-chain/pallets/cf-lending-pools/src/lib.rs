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
#![feature(map_try_insert)]

mod boost;
mod core_lending_pool;
mod general_lending;
mod utils;

use cf_chains::SwapOrigin;
use general_lending::LoanAccount;
pub use general_lending::{
	rpc::{
		before_v12, get_lending_pools, get_loan_accounts, LendingPoolAndSupplyPositions,
		LendingSupplyPosition, RpcLendingPool, RpcLiquidationStatus, RpcLiquidationSwap, RpcLoan,
		RpcLoanAccount,
	},
	LendingPool, LiquidationCompletionReason, LiquidationType, OraclePriceCache, WhitelistStatus,
	WhitelistUpdate, WithdrawnAndRemainingAmounts,
};

pub use general_lending::config::{
	InterestRateConfiguration, LendingConfiguration, LendingPoolConfiguration, LtvThresholds,
	NetworkFeeContributions,
};

pub use boost::{boost_pools_iter, get_boost_pool_details, BoostPoolDetails, OwedAmount};
use boost::{BoostPool, BoostPoolContribution, BoostPoolId};

pub mod migrations;
pub mod weights;

#[cfg(test)]
mod mocks;
#[cfg(test)]
mod tests;

mod benchmarking;

use cf_primitives::{
	define_wrapper_type, Asset, AssetAmount, BasisPoints, BoostPoolTier, PrewitnessedDepositId,
	SwapRequestId,
};
use cf_traits::{
	lending::{LendingApi, RepaymentAmount},
	AccountRoleRegistry, BalanceApi, Chainflip, LpRegistration, PoolApi, PriceFeedApi, SafeModeSet,
	SwapOutputAction, SwapRequestHandler, SwapRequestType,
};
use frame_support::{
	fail,
	pallet_prelude::*,
	sp_runtime::{
		traits::{BlockNumberProvider, Saturating, UniqueSaturatedInto, Zero},
		Percent, Permill, Perquintill,
	},
	transactional,
};

use cf_traits::lending::{BoostApi, BoostFinalisationOutcome, BoostOutcome, LoanId};

use cf_runtime_utilities::log_or_panic;
use frame_system::pallet_prelude::*;
use weights::WeightInfo;

pub use core_lending_pool::{CoreLendingPool, CoreLoanId};

use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec,
	vec::Vec,
};

pub use pallet::*;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(2);

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BoostConfiguration {
	/// The fraction of the network fee that is deducted from the boost fee.
	pub network_fee_deduction_from_boost_percent: Percent,
	/// The minimum amount that can be added to the boost pool.
	pub minimum_add_funds_amount: BTreeMap<Asset, AssetAmount>,
}

#[derive(Serialize, Deserialize, Encode, Decode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct PalletSafeMode {
	pub add_boost_funds_enabled: bool,
	pub stop_boosting_enabled: bool,
	// whether funds can be borrowed (stale oracle also disables this)
	pub borrowing: SafeModeSet<Asset>,
	// whether lenders can add funds to lending pools
	pub add_lender_funds: SafeModeSet<Asset>,
	// whether lenders can withdraw funds from lending pools (stale oracle also disables this)
	pub withdraw_lender_funds: SafeModeSet<Asset>,
	// whether borrowers can add collateral
	pub add_collateral: SafeModeSet<Asset>,
	// whether borrowers can withdraw collateral (stale oracle also disables this)
	pub remove_collateral: SafeModeSet<Asset>,
	// whether liquidations can be started, both voluntarily and system-initiated
	pub liquidations_enabled: bool,
}

impl cf_traits::SafeMode for PalletSafeMode {
	fn code_red() -> Self {
		Self {
			add_boost_funds_enabled: false,
			stop_boosting_enabled: false,
			borrowing: SafeModeSet::code_red(),
			add_lender_funds: SafeModeSet::code_red(),
			withdraw_lender_funds: SafeModeSet::code_red(),
			add_collateral: SafeModeSet::code_red(),
			remove_collateral: SafeModeSet::code_red(),
			liquidations_enabled: false,
		}
	}

	fn code_green() -> Self {
		Self {
			add_boost_funds_enabled: true,
			stop_boosting_enabled: true,
			borrowing: SafeModeSet::code_green(),
			add_lender_funds: SafeModeSet::code_green(),
			withdraw_lender_funds: SafeModeSet::code_green(),
			add_collateral: SafeModeSet::code_green(),
			remove_collateral: SafeModeSet::code_green(),
			liquidations_enabled: true,
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum PalletConfigUpdate {
	SetBoostConfig {
		config: BoostConfiguration,
	},
	/// Updates pool/asset specific configuration. If `asset` is `None`, updates the default
	/// configuration (one that applies to all assets). If `config` is `None`, removes
	/// configuration override for the specified asset (asset must not be `None`).
	SetLendingPoolConfiguration {
		asset: Option<Asset>,
		config: Option<LendingPoolConfiguration>,
	},
	SetLtvThresholds {
		ltv_thresholds: LtvThresholds,
	},
	SetNetworkFeeContributions {
		contributions: NetworkFeeContributions,
	},
	SetFeeSwapIntervalBlocks(u32),
	SetInterestPaymentIntervalBlocks(u32),
	SetFeeSwapThresholdUsd(AssetAmount),
	SetInterestCollectionThresholdUsd(AssetAmount),
	SetOracleSlippageForSwaps {
		soft_liquidation: BasisPoints,
		hard_liquidation: BasisPoints,
		fee_swap: BasisPoints,
	},
	/// Both values must be non-zero
	SetLiquidationSwapChunkSizeUsd {
		soft: AssetAmount,
		hard: AssetAmount,
	},
	SetMinimumAmounts {
		minimum_loan_amount_usd: AssetAmount,
		minimum_update_loan_amount_usd: AssetAmount,
		minimum_update_collateral_amount_usd: AssetAmount,
		minimum_supply_amount_usd: AssetAmount,
	},
}

define_wrapper_type!(CorePoolId, u32);

const MAX_PALLET_CONFIG_UPDATE: u32 = 100; // used to bound no. of updates per extrinsic

// Rename this to LoanPurpose?
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, PartialOrd, Ord)]
pub enum LoanUsage {
	Boost(PrewitnessedDepositId),
}

/// Indicates how the action of adding collateral was triggered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum CollateralAddedActionType {
	/// Triggered manually by the user. Collateral is taken from the user's free balance.
	Manual,
	/// Triggered by the protocol due to high LTV. Collateral is taken from the user's free
	/// balance.
	SystemTopup,
	/// Triggered by the protocol as a result of liquidation obtaining more of the loan asset
	/// than was required.
	SystemLiquidationExcessAmount { loan_id: LoanId, swap_request_id: SwapRequestId },
}

/// Indicates how the action of repaying a loan was triggered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum LoanRepaidActionType {
	/// Triggered manually by the user. Loan is repaid from the user's free balance.
	Manual,
	/// Triggered by the protocol as a result of liquidation. Loan is repaid from a liquidation
	/// swap's output.
	Liquidation { swap_request_id: SwapRequestId },
}

pub struct BoostConfigDefault {}

impl Get<BoostConfiguration> for BoostConfigDefault {
	fn get() -> BoostConfiguration {
		BoostConfiguration {
			network_fee_deduction_from_boost_percent: Percent::from_percent(50),
			minimum_add_funds_amount: BTreeMap::from([(Asset::Btc, 11000_u128)]), // ~$10 usd
		}
	}
}

pub struct LendingConfigDefault {}

const DEFAULT_ORIGINATION_FEE: Permill = Permill::from_parts(100); // 1 bps

const LENDING_DEFAULT_CONFIG: LendingConfiguration = LendingConfiguration {
	default_pool_config: LendingPoolConfiguration {
		origination_fee: DEFAULT_ORIGINATION_FEE,
		liquidation_fee: Permill::from_parts(500), // 5 bps
		interest_rate_curve: InterestRateConfiguration {
			interest_at_zero_utilisation: Permill::from_percent(2),
			junction_utilisation: Permill::from_percent(90),
			interest_at_junction_utilisation: Permill::from_percent(8),
			interest_at_max_utilisation: Permill::from_percent(50),
		},
	},
	ltv_thresholds: LtvThresholds {
		target: Permill::from_percent(80),
		topup: None,
		soft_liquidation: Permill::from_percent(90),
		soft_liquidation_abort: Permill::from_percent(88),
		hard_liquidation: Permill::from_percent(95),
		hard_liquidation_abort: Permill::from_percent(93),
		low_ltv: Permill::from_percent(50),
	},
	network_fee_contributions: NetworkFeeContributions {
		// A fixed 1% per year is added to the base interest rate (the latter determined by the
		// interest rate curve) and paid to the network.
		extra_interest: Permill::from_percent(1),
		// 20% of all origination fees is paid to the network.
		from_origination_fee: Permill::from_percent(20),
		// 20% of all liquidation fees is paid to the network.
		from_liquidation_fee: Permill::from_percent(20),
		low_ltv_penalty_max: Permill::from_percent(1),
	},
	// don't swap more often than every 10 blocks
	fee_swap_interval_blocks: 10,
	interest_payment_interval_blocks: 10,
	fee_swap_threshold_usd: 20_000_000, // don't swap fewer than 20 USD
	interest_collection_threshold_usd: 100_000, // don't collect less than 0.1 USD
	soft_liquidation_max_oracle_slippage: 50, // 0.5%
	hard_liquidation_max_oracle_slippage: 500, // 5%
	soft_liquidation_swap_chunk_size_usd: 10_000_000_000, //10k USD
	hard_liquidation_swap_chunk_size_usd: 50_000_000_000, //50k USD
	fee_swap_max_oracle_slippage: 50,   // 0.5%
	pool_config_overrides: BTreeMap::new(),
	minimum_loan_amount_usd: 100_000_000,             // 100 USD
	minimum_update_loan_amount_usd: 10_000_000,       // 10 USD
	minimum_supply_amount_usd: 100_000_000,           // 100 USD
	minimum_update_collateral_amount_usd: 10_000_000, // 10 USD
};

impl Get<LendingConfiguration> for LendingConfigDefault {
	fn get() -> LendingConfiguration {
		LENDING_DEFAULT_CONFIG
	}
}

#[frame_support::pallet]
pub mod pallet {

	use cf_primitives::SwapRequestId;

	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;

		type Balance: BalanceApi<AccountId = Self::AccountId>;

		type SwapRequestHandler: SwapRequestHandler<AccountId = Self::AccountId>;

		type PoolApi: PoolApi<AccountId = <Self as frame_system::Config>::AccountId>;

		type PriceApi: PriceFeedApi;

		type LpRegistrationApi: LpRegistration<AccountId = Self::AccountId>;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode>;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	pub type NextCorePoolId<T: Config> = StorageValue<_, CorePoolId, ValueQuery>;

	#[pallet::storage]
	pub type CorePools<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		Asset,
		Twox64Concat,
		CorePoolId,
		CoreLendingPool<T::AccountId>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type BoostPools<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		Asset,
		Twox64Concat,
		BoostPoolTier,
		BoostPool,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type BoostedDeposits<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		Asset,
		Twox64Concat,
		PrewitnessedDepositId,
		BTreeMap<BoostPoolTier, BoostPoolContribution>,
		OptionQuery,
	>;

	/// Stores Lending pools for each asset.
	#[pallet::storage]
	pub type GeneralLendingPools<T: Config> =
		StorageMap<_, Twox64Concat, Asset, LendingPool<T::AccountId>, OptionQuery>;

	/// The next loan id to assign to a new loan.
	#[pallet::storage]
	pub type NextLoanId<T: Config> = StorageValue<_, LoanId, ValueQuery>;

	/// Stores the configuration for lending (updatable by governance).
	#[pallet::storage]
	pub type LendingConfig<T: Config> =
		StorageValue<_, LendingConfiguration, ValueQuery, LendingConfigDefault>;

	/// Stores loan accounts for borrowers and their loans.
	#[pallet::storage]
	pub type LoanAccounts<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, LoanAccount<T>, OptionQuery>;

	/// Stores collected network fees awaiting to be swapped into FLIP at regular intervals.
	#[pallet::storage]
	pub type PendingNetworkFees<T: Config> =
		StorageMap<_, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	/// Determines which accounts are allowed to use lending extrinsics.
	#[pallet::storage]
	pub type Whitelist<T: Config> = StorageValue<_, WhitelistStatus<T::AccountId>, ValueQuery>;

	/// Stores boost-related configuration (updatable by governance).
	#[pallet::storage]
	pub type BoostConfig<T: Config> =
		StorageValue<_, BoostConfiguration, ValueQuery, BoostConfigDefault>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		PalletConfigUpdated {
			update: PalletConfigUpdate,
		},
		BoostPoolCreated {
			boost_pool: BoostPoolId,
		},
		BoostFundsAdded {
			booster_id: T::AccountId,
			boost_pool: BoostPoolId,
			amount: AssetAmount,
		},
		StoppedBoosting {
			booster_id: T::AccountId,
			boost_pool: BoostPoolId,
			// When we stop boosting, the amount in the pool that isn't currently pending
			// finalisation can be returned immediately.
			unlocked_amount: AssetAmount,
			// The ids of the boosts that are pending finalisation, such that the funds can then be
			// returned to the user's free balance when the finalisation occurs.
			pending_boosts: BTreeSet<PrewitnessedDepositId>,
		},
		LendingPoolCreated {
			asset: Asset,
		},
		LendingFundsAdded {
			lender_id: T::AccountId,
			asset: Asset,
			amount: AssetAmount,
		},
		LendingFundsRemoved {
			lender_id: T::AccountId,
			asset: Asset,
			unlocked_amount: AssetAmount,
		},
		CollateralAdded {
			borrower_id: T::AccountId,
			collateral: BTreeMap<Asset, AssetAmount>,
			action_type: CollateralAddedActionType,
		},
		CollateralRemoved {
			borrower_id: T::AccountId,
			collateral: BTreeMap<Asset, AssetAmount>,
		},
		LoanCreated {
			loan_id: LoanId,
			asset: Asset,
			borrower_id: T::AccountId,
			principal_amount: AssetAmount,
		},
		LoanUpdated {
			loan_id: LoanId,
			extra_principal_amount: AssetAmount,
		},
		CollateralTopupAssetUpdated {
			borrower_id: T::AccountId,
			collateral_topup_asset: Option<Asset>,
		},
		OriginationFeeTaken {
			loan_id: LoanId,
			pool_fee: AssetAmount,
			network_fee: AssetAmount,
			broker_fee: AssetAmount,
		},
		InterestTaken {
			loan_id: LoanId,
			// Interest is always charged in the loan's asset (effectively increasing
			// the loan's principal)
			pool_interest: AssetAmount,
			network_interest: AssetAmount,
			broker_interest: AssetAmount,
			low_ltv_penalty: AssetAmount,
		},
		LiquidationInitiated {
			borrower_id: T::AccountId,
			swaps: BTreeMap<LoanId, Vec<SwapRequestId>>,
			liquidation_type: LiquidationType,
		},
		LiquidationCompleted {
			borrower_id: T::AccountId,
			reason: LiquidationCompletionReason,
		},
		LiquidationFeeTaken {
			loan_id: LoanId,
			pool_fee: AssetAmount,
			network_fee: AssetAmount,
			broker_fee: AssetAmount,
		},
		LoanRepaid {
			loan_id: LoanId,
			amount: AssetAmount,
			action_type: LoanRepaidActionType,
		},
		LoanSettled {
			loan_id: LoanId,
			/// The amount of principal that the borrower failed to repay at the time of settlement
			/// (can only be non-zero as a result of liquidation that didn't fully recover the
			/// principal)
			outstanding_principal: AssetAmount,
			/// Indicates whether the loan was settled as a result of liquidation.
			via_liquidation: bool,
		},
		LendingNetworkFeeSwapInitiated {
			swap_request_id: SwapRequestId,
		},
		WhitelistUpdated {
			update: WhitelistUpdate<T::AccountId>,
		},
	}

	#[derive(PartialEq)]
	#[pallet::error]
	pub enum Error<T> {
		/// Adding boost funds is disabled due to safe mode.
		AddBoostFundsDisabled,
		/// Retrieving boost funds disabled due to safe mode.
		StopBoostingDisabled,
		/// Cannot create a boost pool if it already exists.
		PoolAlreadyExists,
		/// Cannot create a boost pool of 0 bps
		InvalidBoostPoolTier,
		/// The specified pool does not exist.
		PoolDoesNotExist,
		/// The account id is not a member of the boost pool.
		AccountNotFoundInPool,
		/// Not enough available liquidity to boost a deposit
		InsufficientBoostLiquidity,
		// TODO: consolidate this with `InsufficientBoostLiquidity`?
		InsufficientLiquidity,
		/// Adding lending funds is disabled due to safe mode.
		AddLenderFundsDisabled,
		/// Removing lending funds is disabled due to safe mode.
		RemoveLenderFundsDisabled,
		/// Adding collateral is disabled due to safe mode.
		AddingCollateralDisabled,
		/// Removing collateral is disabled due to safe mode.
		RemovingCollateralDisabled,
		/// Creating general loans is disabled due to safe mode.
		LoanCreationDisabled,
		/// Requested loan not found
		LoanNotFound,
		/// Specified loan account not found (in methods where one should not be created by
		/// default)
		LoanAccountNotFound,
		/// Can't trigger voluntary liquidation because account has no loans
		AccountHasNoLoans,
		/// The borrower has insufficient collateral for the requested loan
		InsufficientCollateral,
		/// Action not allowed as it would lead to LTV that's above safe threshold
		LtvTooHigh,
		/// A catch-all error for invalid loan parameters where a more specific error is not
		/// available
		InvalidLoanParameters,
		/// Failed to read oracle price
		OraclePriceUnavailable,
		InternalInvariantViolation,
		InvalidConfigurationParameters,
		LenderNotFoundInPool,
		/// Certain actions (such as removing collateral) are disabled during liquidation.
		LiquidationInProgress,
		/// The provided collateral amount is empty/zero.
		EmptyCollateral,
		/// The loan/supplied amount would be below the minimum allowed.
		RemainingAmountBelowMinimum,
		/// The amount specified must be at least the minimum allowed.
		AmountBelowMinimum,
		/// No refund address has been set for the loan asset.
		NoRefundAddressSet,
		/// Access denied as account is not in the whitelist.
		AccountNotWhitelisted,
		/// Liquidations are currently disabled due to safe mode.
		LiquidationsDisabled,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			general_lending::lending_upkeep::<T>(current_block)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Apply a list of configuration updates to the pallet.
		///
		/// Requires Governance.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::update_pallet_config(updates.len() as u32))]
		pub fn update_pallet_config(
			origin: OriginFor<T>,
			updates: BoundedVec<PalletConfigUpdate, ConstU32<MAX_PALLET_CONFIG_UPDATE>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			LendingConfig::<T>::try_mutate(|config| {
				for update in updates {
					match &update {
						PalletConfigUpdate::SetBoostConfig { config } => {
							ensure!(
								config.minimum_add_funds_amount.values().all(|&min| min > 0),
								Error::<T>::InvalidConfigurationParameters
							);
							BoostConfig::<T>::put(config.clone());
						},
						PalletConfigUpdate::SetLendingPoolConfiguration {
							asset,
							config: pool_config,
						} => {
							if let Some(pool_config) = pool_config {
								pool_config
									.interest_rate_curve
									.validate()
									.map_err(|_| Error::<T>::InvalidConfigurationParameters)?;
							}

							match (asset, pool_config) {
								(None, Some(pool_config)) => {
									// Updating the default configuration for all assets:
									config.default_pool_config = pool_config.clone();
								},
								(Some(asset), Some(pool_config)) => {
									// Creating/updating override for the specified asset:
									config
										.pool_config_overrides
										.insert(*asset, pool_config.clone());
								},
								(Some(asset), None) => {
									// Removing config override for the specified asset:
									config.pool_config_overrides.remove(asset);
								},
								(None, None) => {
									fail!(Error::<T>::InvalidConfigurationParameters)
								},
							}
						},
						PalletConfigUpdate::SetLtvThresholds { ltv_thresholds } => {
							ltv_thresholds
								.validate()
								.map_err(|_| Error::<T>::InvalidConfigurationParameters)?;
							config.ltv_thresholds = ltv_thresholds.clone();
						},
						PalletConfigUpdate::SetNetworkFeeContributions { contributions } => {
							config.network_fee_contributions = contributions.clone();
						},
						PalletConfigUpdate::SetFeeSwapIntervalBlocks(interval) => {
							ensure!(*interval > 0, Error::<T>::InvalidConfigurationParameters);
							config.fee_swap_interval_blocks = *interval;
						},
						PalletConfigUpdate::SetInterestPaymentIntervalBlocks(interval) => {
							ensure!(*interval > 0, Error::<T>::InvalidConfigurationParameters);
							config.interest_payment_interval_blocks = *interval;
						},
						PalletConfigUpdate::SetFeeSwapThresholdUsd(amount_threshold) => {
							config.fee_swap_threshold_usd = *amount_threshold;
						},
						PalletConfigUpdate::SetInterestCollectionThresholdUsd(amount_threshold) => {
							config.interest_collection_threshold_usd = *amount_threshold;
						},
						PalletConfigUpdate::SetOracleSlippageForSwaps {
							soft_liquidation,
							hard_liquidation,
							fee_swap,
						} => {
							config.soft_liquidation_max_oracle_slippage = *soft_liquidation;
							config.hard_liquidation_max_oracle_slippage = *hard_liquidation;
							config.fee_swap_max_oracle_slippage = *fee_swap;
						},
						PalletConfigUpdate::SetLiquidationSwapChunkSizeUsd { soft, hard } => {
							ensure!(
								*soft > 0 && *hard > 0,
								Error::<T>::InvalidConfigurationParameters
							);

							config.soft_liquidation_swap_chunk_size_usd = *soft;
							config.hard_liquidation_swap_chunk_size_usd = *hard;
						},
						PalletConfigUpdate::SetMinimumAmounts {
							minimum_loan_amount_usd,
							minimum_update_loan_amount_usd,
							minimum_update_collateral_amount_usd,
							minimum_supply_amount_usd,
						} => {
							ensure!(
								minimum_supply_amount_usd > &0,
								Error::<T>::InvalidConfigurationParameters
							);
							config.minimum_loan_amount_usd = *minimum_loan_amount_usd;
							config.minimum_update_loan_amount_usd = *minimum_update_loan_amount_usd;
							config.minimum_update_collateral_amount_usd =
								*minimum_update_collateral_amount_usd;
							config.minimum_supply_amount_usd = *minimum_supply_amount_usd;
						},
					}
					Self::deposit_event(Event::<T>::PalletConfigUpdated { update });
				}

				Ok(())
			})
		}

		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::add_boost_funds())]
		pub fn add_boost_funds(
			origin: OriginFor<T>,
			asset: Asset,
			amount: AssetAmount,
			pool_tier: BoostPoolTier,
		) -> DispatchResult {
			let booster_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			ensure!(T::SafeMode::get().add_boost_funds_enabled, Error::<T>::AddBoostFundsDisabled);

			let minimum = BoostConfig::<T>::get()
				.minimum_add_funds_amount
				.get(&asset)
				.copied()
				.unwrap_or(1);
			ensure!(amount >= minimum, Error::<T>::AmountBelowMinimum);

			// `try_debit_account` does not account for any unswept open positions, so we sweep to
			// ensure we have the funds in our free balance before attempting to debit the account.
			T::PoolApi::sweep(&booster_id)?;

			T::Balance::try_debit_account(&booster_id, asset, amount)?;

			let boost_pool: BoostPool =
				BoostPools::<T>::get(asset, pool_tier).ok_or(Error::<T>::PoolDoesNotExist)?;

			let core_pool_id = boost_pool.core_pool_id;

			CorePools::<T>::mutate(asset, core_pool_id, |pool| {
				let pool = pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;
				pool.add_funds(booster_id.clone(), amount);
				Ok::<(), DispatchError>(())
			})?;

			Self::deposit_event(Event::<T>::BoostFundsAdded {
				booster_id,
				boost_pool: BoostPoolId { asset, tier: pool_tier },
				amount,
			});

			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::stop_boosting())]
		pub fn stop_boosting(
			origin: OriginFor<T>,
			asset: Asset,
			pool_tier: BoostPoolTier,
		) -> DispatchResult {
			let booster = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			ensure!(T::SafeMode::get().stop_boosting_enabled, Error::<T>::StopBoostingDisabled);

			let boost_pool: BoostPool =
				BoostPools::<T>::get(asset, pool_tier).ok_or(Error::<T>::PoolDoesNotExist)?;

			let core_pool_id = boost_pool.core_pool_id;

			let (unlocked_amount, pending_loans) =
				CorePools::<T>::mutate(asset, core_pool_id, |pool| {
					let pool = pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

					pool.stop_lending(booster.clone()).map_err(|e| match e {
						core_lending_pool::Error::AccountNotFoundInPool =>
							Error::<T>::AccountNotFoundInPool,
					})
				})?;

			T::Balance::credit_account(&booster, asset, unlocked_amount);

			let pending_boosts = pending_loans
				.into_iter()
				.map(|loan_usage| match loan_usage {
					LoanUsage::Boost(deposit_id) => deposit_id,
				})
				.collect();

			Self::deposit_event(Event::StoppedBoosting {
				booster_id: booster,
				boost_pool: BoostPoolId { asset, tier: pool_tier },
				unlocked_amount,
				pending_boosts,
			});

			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::create_boost_pools())]
		pub fn create_boost_pools(
			origin: OriginFor<T>,
			new_pools: Vec<BoostPoolId>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			new_pools.into_iter().try_for_each(|pool_id| Self::new_boost_pool(pool_id))?;
			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::create_lending_pool())]
		pub fn create_lending_pool(origin: OriginFor<T>, asset: Asset) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			Self::new_lending_pool(asset)
		}

		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::add_lender_funds())]
		pub fn add_lender_funds(
			origin: OriginFor<T>,
			asset: Asset,
			amount: AssetAmount,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().add_lender_funds.enabled(&asset),
				Error::<T>::AddLenderFundsDisabled
			);

			let lender_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			ensure!(
				Whitelist::<T>::get().is_allowed(&lender_id),
				Error::<T>::AccountNotWhitelisted
			);

			let config = LendingConfig::<T>::get();

			ensure!(
				OraclePriceCache::<T>::default().usd_value_of(asset, amount)? >=
					config.minimum_supply_amount_usd,
				Error::<T>::AmountBelowMinimum
			);

			// `try_debit_account` does not account for any unswept open positions, so we sweep to
			// ensure we have the funds in our free balance before attempting to debit the account.
			T::PoolApi::sweep(&lender_id)?;

			T::Balance::try_debit_account(&lender_id, asset, amount)?;

			GeneralLendingPools::<T>::try_mutate(asset, |maybe_pool| {
				let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

				pool.add_funds(&lender_id, amount);

				Ok::<_, DispatchError>(())
			})?;

			Self::deposit_event(Event::<T>::LendingFundsAdded { lender_id, asset, amount });

			Ok(())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::remove_lender_funds())]
		pub fn remove_lender_funds(
			origin: OriginFor<T>,
			asset: Asset,
			amount: Option<AssetAmount>,
		) -> DispatchResult {
			ensure!(
				T::SafeMode::get().withdraw_lender_funds.enabled(&asset),
				Error::<T>::RemoveLenderFundsDisabled
			);

			let config = LendingConfig::<T>::get();

			if let Some(amount) = amount {
				ensure!(
					OraclePriceCache::<T>::default().usd_value_of(asset, amount)? >=
						config.minimum_supply_amount_usd,
					Error::<T>::AmountBelowMinimum
				);
			}

			let lender_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let unlocked_amount = GeneralLendingPools::<T>::try_mutate(asset, |maybe_pool| {
				let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

				let WithdrawnAndRemainingAmounts { withdrawn_amount, remaining_amount } =
					pool.remove_funds(&lender_id, amount).map_err(Error::<T>::from)?;

				// Either the user removes everything, or they have to leave at least
				// the minimum required amount in the pool (to prevent dust amounts from
				// accumulating):
				ensure!(
					remaining_amount == 0 ||
						OraclePriceCache::<T>::default()
							.usd_value_of(asset, remaining_amount)? >=
							config.minimum_supply_amount_usd,
					Error::<T>::RemainingAmountBelowMinimum
				);

				Ok::<_, DispatchError>(withdrawn_amount)
			})?;

			T::Balance::credit_account(&lender_id, asset, unlocked_amount);

			Self::deposit_event(Event::<T>::LendingFundsRemoved {
				lender_id,
				asset,
				unlocked_amount,
			});

			Ok(())
		}

		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::add_collateral())]
		pub fn add_collateral(
			origin: OriginFor<T>,
			collateral_topup_asset: Option<Asset>,
			collateral: BTreeMap<Asset, AssetAmount>,
		) -> DispatchResult {
			let borrower_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			ensure!(
				Whitelist::<T>::get().is_allowed(&borrower_id),
				Error::<T>::AccountNotWhitelisted
			);

			<Self as LendingApi>::add_collateral(&borrower_id, collateral_topup_asset, collateral)
		}

		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::remove_collateral())]
		pub fn remove_collateral(
			origin: OriginFor<T>,
			collateral: BTreeMap<Asset, AssetAmount>,
		) -> DispatchResult {
			let borrower_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			<Self as LendingApi>::remove_collateral(&borrower_id, collateral)
		}

		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::request_loan())]
		pub fn request_loan(
			origin: OriginFor<T>,
			loan_asset: Asset,
			loan_amount: AssetAmount,
			collateral_topup_asset: Option<Asset>,
			extra_collateral: BTreeMap<Asset, AssetAmount>,
		) -> DispatchResult {
			let borrower_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			ensure!(
				Whitelist::<T>::get().is_allowed(&borrower_id),
				Error::<T>::AccountNotWhitelisted
			);

			Self::new_loan(
				borrower_id,
				loan_asset,
				loan_amount,
				collateral_topup_asset,
				extra_collateral,
			)?;

			Ok(())
		}

		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::update_collateral_topup_asset())]
		pub fn update_collateral_topup_asset(
			origin: OriginFor<T>,
			collateral_topup_asset: Option<Asset>,
		) -> DispatchResult {
			let borrower_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			<Self as LendingApi>::update_collateral_topup_asset(
				&borrower_id,
				collateral_topup_asset,
			)
		}

		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::expand_loan())]
		pub fn expand_loan(
			origin: OriginFor<T>,
			loan_id: LoanId,
			extra_amount_to_borrow: AssetAmount,
			extra_collateral: BTreeMap<Asset, AssetAmount>,
		) -> DispatchResult {
			let borrower_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			<Self as LendingApi>::expand_loan(
				borrower_id,
				loan_id,
				extra_amount_to_borrow,
				extra_collateral,
			)
		}

		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::make_repayment())]
		pub fn make_repayment(
			origin: OriginFor<T>,
			loan_id: LoanId,
			amount: RepaymentAmount,
		) -> DispatchResult {
			let borrower_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			Self::try_making_repayment(&borrower_id, loan_id, amount)
		}

		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::change_voluntary_liquidation())]
		pub fn initiate_voluntary_liquidation(origin: OriginFor<T>) -> DispatchResult {
			let borrower_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			ensure!(T::SafeMode::get().liquidations_enabled, Error::<T>::LiquidationsDisabled);

			<Self as LendingApi>::set_voluntary_liquidation_flag(borrower_id, true)
		}

		#[pallet::call_index(14)]
		#[pallet::weight(T::WeightInfo::change_voluntary_liquidation())]
		pub fn stop_voluntary_liquidation(origin: OriginFor<T>) -> DispatchResult {
			let borrower_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			<Self as LendingApi>::set_voluntary_liquidation_flag(borrower_id, false)
		}

		#[pallet::call_index(15)]
		#[pallet::weight(T::WeightInfo::update_whitelist())]
		pub fn update_whitelist(
			origin: OriginFor<T>,
			update: WhitelistUpdate<T::AccountId>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			Whitelist::<T>::mutate(|whitelist| whitelist.apply_update(update.clone()))
				.map_err(|_| Error::<T>::InvalidConfigurationParameters)?;

			Self::deposit_event(Event::WhitelistUpdated { update });

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn new_core_pool(asset: Asset) -> CorePoolId {
		let core_pool_id = NextCorePoolId::<T>::get();
		NextCorePoolId::<T>::set(CorePoolId(core_pool_id.0 + 1));

		CorePools::<T>::insert(asset, core_pool_id, CoreLendingPool::default());

		core_pool_id
	}

	pub fn new_boost_pool(pool_id: BoostPoolId) -> DispatchResult {
		ensure!(pool_id.tier != 0, Error::<T>::InvalidBoostPoolTier);
		Ok(BoostPools::<T>::try_mutate_exists(pool_id.asset, pool_id.tier, |pool| {
			ensure!(pool.is_none(), Error::<T>::PoolAlreadyExists);

			let core_pool_id = Self::new_core_pool(pool_id.asset);

			*pool = Some(BoostPool { core_pool_id, fee_bps: pool_id.tier });

			Self::deposit_event(Event::<T>::BoostPoolCreated { boost_pool: pool_id });

			Ok::<(), Error<T>>(())
		})?)
	}

	pub fn new_lending_pool(asset: Asset) -> DispatchResult {
		Ok(GeneralLendingPools::<T>::try_mutate_exists(asset, |pool| {
			ensure!(pool.is_none(), Error::<T>::PoolAlreadyExists);

			*pool = Some(LendingPool::new());

			Self::deposit_event(Event::<T>::LendingPoolCreated { asset });

			Ok::<(), Error<T>>(())
		})?)
	}
}
