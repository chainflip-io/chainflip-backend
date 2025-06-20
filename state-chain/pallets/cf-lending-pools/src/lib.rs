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

mod chp_lending;
mod core_lending_pool;

use cf_chains::SwapOrigin;
use chp_lending::ChpLoan;
// Temporarily exposing this for a migration
pub use core_lending_pool::{PendingLoan, ScaledAmount};

pub mod migrations;

pub mod weights;

#[cfg(test)]
mod mocks;
#[cfg(test)]
mod tests;

mod benchmarking;

use cf_primitives::{
	define_wrapper_type, Asset, AssetAmount, BasisPoints, BoostPoolTier, PrewitnessedDepositId,
};
use cf_traits::{
	impl_pallet_safe_mode, lending::ChpLendingApi, AccountRoleRegistry, BalanceApi, Chainflip,
	OraclePriceApi, PoolApi, SwapOutputAction, SwapRequestHandler, SwapRequestType,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{
		traits::{BlockNumberProvider, Saturating, UniqueSaturatedInto, Zero},
		Perbill, Percent, Permill, Perquintill,
	},
	transactional,
};

use cf_traits::lending::{BoostApi, BoostFinalisationOutcome, BoostOutcome, ChpLoanId};

use cf_runtime_utilities::log_or_panic;
use frame_system::{pallet_prelude::*, WeightInfo as SystemWeightInfo};
use weights::WeightInfo;

pub use core_lending_pool::{CoreLendingPool, LoanId};

use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec,
	vec::Vec,
};

pub use pallet::*;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

impl_pallet_safe_mode! {
	PalletSafeMode;
	add_boost_funds_enabled,
	stop_boosting_enabled,
	chp_loans_enabled,
	add_chp_funds_enabled,
	stop_chp_lending_enabled,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletConfigUpdate {
	SetNetworkFeeDeductionFromBoost { deduction_percent: Percent },
}

define_wrapper_type!(CorePoolId, u32);

const COLLATERAL_ASSET: Asset = Asset::Usdc;
const INTEREST_PAYMENT_INTERVAL: u32 = 10; // interest is charged every minute

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BoostPool {
	// Fee charged by the pool
	pub fee_bps: BasisPoints,
	pub core_pool_id: CorePoolId,
}

#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Eq, Clone)]
pub struct BoostPoolContribution {
	pub core_pool_id: CorePoolId,
	pub loan_id: LoanId,
	pub boosted_amount: AssetAmount,
	pub network_fee: AssetAmount,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BoostPoolId {
	pub asset: Asset,
	pub tier: BoostPoolTier,
}

// Rename this to LoanPurpose?
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, PartialOrd, Ord)]
pub enum LoanUsage {
	Boost(PrewitnessedDepositId),
	ChpLoan(ChpLoanId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct ChpPool {
	core_pool_id: CorePoolId,
}

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
enum LoanStatus {
	// Loan has not yet been repaid in full
	Active,
	// A swap has been created from USDC collateral into the borrowed asset
	// (using soft liquidation parameters)
	SoftLiquidation { usdc_collateral: AssetAmount },
	// A swap has been created from USDC collateral into the borrowed asset
	// (using hard liquidation parameters)
	HardLiquidation { usdc_collateral: AssetAmount },
	// Principal has been repaid in full and we are awaiting for collected
	// fees to be swapped into the pools asset
	Finalising,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct ChpPoolContribution {
	core_pool_id: CorePoolId,
	loan_id: LoanId,
	principal: AssetAmount,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct ChpConfiguration {
	/// Clearing fee is computed as clearing_fee_base + utilisation *
	/// clearing_fee_utilisation_factor
	pub clearing_fee_base: Permill,
	pub clearing_fee_utilisation_factor: Permill,
	/// Interest is computed as interest_base + utilisation *
	/// interest_utilisation_factor
	pub interest_base: Perbill,
	pub interest_utilisation_factor: Perbill,

	/// The % above 100% of the loan principal amount that must be covered
	/// by collateral in order to create a loan.
	pub overcollateralisation_target: Permill,
	/// Reaching this threshold will trigger a top-up of the collateral
	pub overcollateralisation_topup_threshold: Permill,
	/// Reaching this threshold will trigger soft liquidation of the loan
	pub overcollateralisation_soft_threshold: Permill,
	/// Reaching this threshold will trigger hard liquidation of the loan
	pub overcollateralisation_hard_threshold: Permill,

	/// Maximum duration of a loan in blocks upon reaching which the loan will be (soft) liquidated
	pub max_loan_duration: u32,
}

impl ChpConfiguration {
	fn derive_clearing_fee(&self, utilisation: Permill) -> Permill {
		self.clearing_fee_base + utilisation * self.clearing_fee_utilisation_factor
	}

	fn derive_interest_rate(&self, utilisation: Permill) -> Perbill {
		self.interest_base +
			Perbill::from_parts(utilisation.deconstruct() * 1000) *
				self.interest_utilisation_factor
	}
}

mod utils {

	use super::*;
	use frame_support::sp_runtime::{
		helpers_128bit::multiply_by_rational_with_rounding, Permill, Rounding,
	};

	/// Boosted amount is the amount provided by the pool plus boost fee,
	/// (and the sum of all boosted amounts from each participating pool
	/// must be equal the deposit amount being boosted). The fee is payed
	/// per boosted amount, and so here we multiply by fee_bps directly.
	pub(super) fn fee_from_boosted_amount(
		amount_to_boost: AssetAmount,
		fee_bps: u16,
	) -> AssetAmount {
		use cf_primitives::BASIS_POINTS_PER_MILLION;
		let fee_permill = Permill::from_parts(fee_bps as u32 * BASIS_POINTS_PER_MILLION);

		fee_permill * amount_to_boost
	}

	/// Unlike `fee_from_boosted_amount`, the boosted amount is not known here
	/// so we have to calculate it first from the provided amount in order to
	/// calculate the boost fee amount.
	pub(super) fn fee_from_provided_amount(
		provided_amount: AssetAmount,
		fee_bps: u16,
	) -> Result<AssetAmount, &'static str> {
		// Compute `boosted = provided / (1 - fee)`
		let boosted_amount = {
			const BASIS_POINTS_MAX: u16 = 10_000;

			let inverse_fee = BASIS_POINTS_MAX.saturating_sub(fee_bps);

			multiply_by_rational_with_rounding(
				provided_amount,
				BASIS_POINTS_MAX as u128,
				inverse_fee as u128,
				Rounding::Down,
			)
			.ok_or("invalid fee")?
		};

		let fee_amount = boosted_amount.checked_sub(provided_amount).ok_or("invalid fee")?;

		Ok(fee_amount)
	}
}

#[frame_support::pallet]
pub mod pallet {

	use cf_traits::OraclePriceApi;

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

		type PriceApi: OraclePriceApi;

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

	/// The fraction of the network fee that is deducted from the boost fee.
	#[pallet::storage]
	pub type NetworkFeeDeductionFromBoostPercent<T: Config> = StorageValue<_, Percent, ValueQuery>;

	#[pallet::storage]
	pub type ChpPools<T: Config> = StorageMap<_, Twox64Concat, Asset, ChpPool, OptionQuery>;

	#[pallet::storage]
	pub type NextChpLoanId<T: Config> = StorageValue<_, ChpLoanId, ValueQuery>;

	#[pallet::storage]
	pub type ChpConfig<T: Config> = StorageValue<_, ChpConfiguration, ValueQuery>;

	#[pallet::storage]
	pub type ChpLoans<T: Config> =
		StorageDoubleMap<_, Twox64Concat, Asset, Twox64Concat, ChpLoanId, ChpLoan<T>, OptionQuery>;

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
		ChpPoolCreated {
			asset: Asset,
		},
		ChpFundsAdded {
			lender_id: T::AccountId,
			asset: Asset,
			amount: AssetAmount,
		},
		StoppedChpLending {
			lender_id: T::AccountId,
			asset: Asset,
			// When we stop lending, the amount in the pool that isn't currently pending
			// finalisation can be returned immediately.
			unlocked_amount: AssetAmount,
			// The ids of the loans that are pending finalisation, such that the funds can then be
			// returned to the lender's free balance when the finalisation occurs.
			pending_loans: BTreeSet<ChpLoanId>,
		},
		ChpLoanCreated {
			loan_id: ChpLoanId,
			borrower_id: T::AccountId,
			asset: Asset,
			amount: AssetAmount,
		},
		ChpLoanSettled {
			loan_id: ChpLoanId,
		},
	}

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
		/// You cannot add 0 to a boost pool.
		AmountMustBeNonZero,
		/// Not enough available liquidity to boost a deposit
		InsufficientBoostLiquidity,
		/// The specified CHP Loan does not exist
		ChpLoanDoesNotExist,
		// TODO: consolidate this with `InsufficientBoostLiquidity`?
		InsufficientLiquidity,
		/// Adding CHP funds is disabled due to safe mode.
		AddChpFundsDisabled,
		/// Removing CHP funds is disabled due to safe mode.
		StopChpLendingDisabled,
		/// Creating CHP loans is disabled due to safe mode.
		ChpLoansDisabled,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			chp_lending::chp_upkeep::<T>(current_block)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Apply a list of configuration updates to the pallet.
		///
		/// Requires Governance.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as frame_system::Config>::SystemWeightInfo::set_storage(updates.len() as u32))]
		pub fn update_pallet_config(
			origin: OriginFor<T>,
			updates: BoundedVec<PalletConfigUpdate, ConstU32<10>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			for update in updates {
				match update {
					PalletConfigUpdate::SetNetworkFeeDeductionFromBoost { deduction_percent } =>
						NetworkFeeDeductionFromBoostPercent::<T>::set(deduction_percent),
				}
				Self::deposit_event(Event::<T>::PalletConfigUpdated { update });
			}

			Ok(())
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

			ensure!(amount > Zero::zero(), Error::<T>::AmountMustBeNonZero);

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
				.filter_map(|loan_usage| match loan_usage {
					LoanUsage::Boost(deposit_id) => Some(deposit_id),
					LoanUsage::ChpLoan(loan_id) => {
						log_or_panic!("Unexpected chp loan in boost pool: {}", loan_id);
						None
					},
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
		#[pallet::weight(T::WeightInfo::create_boost_pools())] // TODO: own benchmark
		pub fn create_chp_pool(origin: OriginFor<T>, asset: Asset) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			Self::new_chp_pool(asset)
		}

		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::add_boost_funds())] // TODO: own benchmark
		pub fn add_chp_funds(
			origin: OriginFor<T>,
			asset: Asset,
			amount: AssetAmount,
		) -> DispatchResult {
			ensure!(T::SafeMode::get().add_chp_funds_enabled, Error::<T>::AddChpFundsDisabled);

			let lender_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			ensure!(amount > Zero::zero(), Error::<T>::AmountMustBeNonZero);

			// `try_debit_account` does not account for any unswept open positions, so we sweep to
			// ensure we have the funds in our free balance before attempting to debit the account.
			T::PoolApi::sweep(&lender_id)?;

			T::Balance::try_debit_account(&lender_id, asset, amount)?;

			let chp_pool: ChpPool =
				ChpPools::<T>::get(asset).ok_or(Error::<T>::PoolDoesNotExist)?;

			let core_pool_id = chp_pool.core_pool_id;

			CorePools::<T>::mutate(asset, core_pool_id, |pool| {
				let pool = pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;
				pool.add_funds(lender_id.clone(), amount);
				Ok::<(), DispatchError>(())
			})?;

			Self::deposit_event(Event::<T>::ChpFundsAdded { lender_id, asset, amount });

			Ok(())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::add_boost_funds())] // TODO: own benchmark
		pub fn stop_chp_lending(origin: OriginFor<T>, asset: Asset) -> DispatchResult {
			ensure!(
				T::SafeMode::get().stop_chp_lending_enabled,
				Error::<T>::StopChpLendingDisabled
			);

			let lender_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let chp_pool = ChpPools::<T>::get(asset).ok_or(Error::<T>::PoolDoesNotExist)?;

			let core_pool_id = chp_pool.core_pool_id;

			let (unlocked_amount, pending_core_loans) =
				CorePools::<T>::mutate(asset, core_pool_id, |pool| {
					let pool = pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

					pool.stop_lending(lender_id.clone()).map_err(|e| match e {
						core_lending_pool::Error::AccountNotFoundInPool =>
							Error::<T>::AccountNotFoundInPool,
					})
				})?;

			T::Balance::credit_account(&lender_id, asset, unlocked_amount);

			let pending_loans = pending_core_loans
				.into_iter()
				.filter_map(|loan_usage| match loan_usage {
					LoanUsage::ChpLoan(loan_id) => Some(loan_id),
					LoanUsage::Boost(deposit_id) => {
						log_or_panic!("Unexpected boost in a chp pool: {}", deposit_id);
						None
					},
				})
				.collect();

			Self::deposit_event(Event::StoppedChpLending {
				lender_id,
				asset,
				unlocked_amount,
				pending_loans,
			});

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

	pub fn new_chp_pool(asset: Asset) -> DispatchResult {
		Ok(ChpPools::<T>::try_mutate_exists(asset, |pool| {
			ensure!(pool.is_none(), Error::<T>::PoolAlreadyExists);

			let core_pool_id = Self::new_core_pool(asset);

			*pool = Some(ChpPool { core_pool_id });

			Self::deposit_event(Event::<T>::ChpPoolCreated { asset });

			Ok::<(), Error<T>>(())
		})?)
	}
}

impl<T: Config> BoostApi for Pallet<T> {
	#[transactional]
	fn try_boosting(
		deposit_id: PrewitnessedDepositId,
		asset: Asset,
		deposit_amount: AssetAmount,
		max_boost_fee_bps: BasisPoints,
	) -> Result<BoostOutcome, DispatchError> {
		let mut remaining_amount = deposit_amount;
		let mut total_fee_amount: AssetAmount = 0;

		let mut used_pools = BTreeMap::new();

		let network_fee_portion = NetworkFeeDeductionFromBoostPercent::<T>::get();

		let sorted_boost_pools = BoostPools::<T>::iter_prefix(asset)
			.map(|(tier, pool)| (tier, pool.core_pool_id))
			.collect::<BTreeMap<_, _>>();

		for (boost_tier, core_pool_id) in sorted_boost_pools {
			if boost_tier > max_boost_fee_bps {
				break
			}

			let Some((loan_id, boosted_amount, fee)) =
				CorePools::<T>::mutate(asset, core_pool_id, |pool| {
					let core_pool: &mut CoreLendingPool<_> = match pool {
						Some(pool) if pool.get_available_amount() == Zero::zero() => {
							return Ok::<_, DispatchError>(None);
						},
						None => {
							// Pool not existing for some reason is equivalent to not having funds:
							return Ok::<_, DispatchError>(None);
						},
						Some(pool) => pool,
					};

					// 1. Derive the amount that needs to be borrowed:
					let full_amount_fee =
						utils::fee_from_boosted_amount(remaining_amount, boost_tier);
					let required_amount = remaining_amount.saturating_sub(full_amount_fee);

					let available_amount = core_pool.get_available_amount();

					let (amount_to_provide, fee_amount) = if available_amount >= required_amount {
						// Will borrow full required amount
						(required_amount, full_amount_fee)
					} else {
						// Will only borrow what is available
						let amount_to_provide = available_amount;
						let fee = utils::fee_from_provided_amount(amount_to_provide, boost_tier)?;

						(amount_to_provide, fee)
					};

					let loan_id =
						core_pool.new_loan(amount_to_provide, LoanUsage::Boost(deposit_id))?;

					Ok(Some((loan_id, amount_to_provide.saturating_add(fee_amount), fee_amount)))
				})?
			else {
				// Can't use the current pool, moving on to the next
				continue;
			};

			// NOTE: A portion of the boost pool fees will be charged as network fee:
			let network_fee = network_fee_portion * fee;
			used_pools.insert(
				boost_tier,
				BoostPoolContribution { core_pool_id, loan_id, boosted_amount, network_fee },
			);

			remaining_amount.saturating_reduce(boosted_amount);
			total_fee_amount.saturating_accrue(fee);

			if remaining_amount == 0u32.into() {
				let boost_output = BoostOutcome {
					used_pools: used_pools
						.iter()
						.map(|(tier, pool)| (*tier, pool.boosted_amount))
						.collect(),
					total_fee: total_fee_amount,
				};

				BoostedDeposits::<T>::insert(asset, deposit_id, used_pools);
				return Ok(boost_output);
			}
		}

		Err(Error::<T>::InsufficientBoostLiquidity.into())
	}

	fn finalise_boost(deposit_id: PrewitnessedDepositId, asset: Asset) -> BoostFinalisationOutcome {
		let Some(pool_contributions) = BoostedDeposits::<T>::take(asset, deposit_id) else {
			return Default::default();
		};

		let mut total_network_fee = 0;

		for BoostPoolContribution { core_pool_id, loan_id, boosted_amount, network_fee } in
			pool_contributions.values()
		{
			total_network_fee += network_fee;

			CorePools::<T>::mutate(asset, core_pool_id, |pool| {
				if let Some(pool) = pool {
					for (booster_id, unlocked_amount) in
						pool.make_repayment(*loan_id, boosted_amount.saturating_sub(*network_fee))
					{
						T::Balance::credit_account(&booster_id, asset, unlocked_amount);
					}
					pool.finalise_loan(*loan_id);
				}
			});
		}

		BoostFinalisationOutcome { network_fee: total_network_fee }
	}

	fn process_deposit_as_lost(deposit_id: PrewitnessedDepositId, asset: Asset) {
		let Some(pool_contributions) = BoostedDeposits::<T>::take(asset, deposit_id) else {
			log_or_panic!("Boost record for a lost deposit not found: {}", deposit_id);
			return;
		};

		for BoostPoolContribution { core_pool_id, loan_id, .. } in pool_contributions.values() {
			CorePools::<T>::mutate(asset, core_pool_id, |pool| {
				if let Some(pool) = pool {
					pool.finalise_loan(*loan_id);
				}
			});
		}
	}
}

impl<T: Config> cf_traits::BoostBalancesApi for Pallet<T> {
	type AccountId = T::AccountId;
	fn boost_pool_account_balance(who: &Self::AccountId, asset: Asset) -> AssetAmount {
		let available = BoostPools::<T>::iter_prefix(asset).fold(0, |acc, (_tier, pool)| {
			let Some(core_pool) = CorePools::<T>::get(asset, pool.core_pool_id) else {
				return 0;
			};

			acc + core_pool.get_available_amount_for_account(who).unwrap_or(0)
		});

		let in_all_boosted_deposits =
			BoostedDeposits::<T>::iter_prefix(asset).fold(0, |acc, (_, pool_contributions)| {
				let in_boosted_deposit = pool_contributions.iter().fold(
					0,
					|acc,
					 (
						_,
						BoostPoolContribution {
							core_pool_id,
							loan_id,
							boosted_amount,
							network_fee,
						},
					)| {
						let Some(core_pool) = CorePools::<T>::get(asset, core_pool_id) else {
							return 0;
						};

						let Some(loan) = core_pool.pending_loans.get(loan_id) else { return 0 };

						let Some(share) = loan.shares.get(who) else { return 0 };

						acc + *share * boosted_amount.saturating_sub(*network_fee)
					},
				);

				acc + in_boosted_deposit
			});

		available + in_all_boosted_deposits
	}
}

pub fn boost_pools_iter<T: Config>(
) -> impl Iterator<Item = (Asset, BoostPoolTier, CoreLendingPool<T::AccountId>)> {
	BoostPools::<T>::iter().filter_map(move |(asset, tier, pool)| {
		CorePools::<T>::get(asset, pool.core_pool_id).map(|core_pool| (asset, tier, core_pool))
	})
}

pub fn boost_pools_for_asset_iter<T: Config>(
	asset: Asset,
) -> impl Iterator<Item = (BoostPoolTier, CoreLendingPool<T::AccountId>)> {
	BoostPools::<T>::iter_prefix(asset).filter_map(move |(tier, pool)| {
		CorePools::<T>::get(asset, pool.core_pool_id).map(|core_pool| (tier, core_pool))
	})
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct OwedAmount<AmountT> {
	pub total: AmountT,
	pub fee: AmountT,
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct BoostPoolDetails<AccountId> {
	pub available_amounts: BTreeMap<AccountId, AssetAmount>,
	pub pending_boosts:
		BTreeMap<PrewitnessedDepositId, BTreeMap<AccountId, OwedAmount<AssetAmount>>>,
	pub pending_withdrawals: BTreeMap<AccountId, BTreeSet<PrewitnessedDepositId>>,
	pub network_fee_deduction_percent: Percent,
}

pub fn get_boost_pool_details<T: Config>(
	asset: Asset,
) -> BTreeMap<BoostPoolTier, BoostPoolDetails<T::AccountId>> {
	let network_fee_deduction_percent = NetworkFeeDeductionFromBoostPercent::<T>::get();

	boost_pools_for_asset_iter::<T>(asset)
		.map(|(tier, core_pool)| {
			let pending_boosts = core_pool
				.get_pending_loans()
				.iter()
				.filter_map(|(_loan_id, loan)| {
					if let LoanUsage::Boost(deposit_id) = loan.usage {
						Some((deposit_id, loan))
					} else {
						None
					}
				})
				.map(|(deposit_id, loan)| {
					let Some(contribution) = BoostedDeposits::<T>::get(asset, deposit_id)
						.and_then(|pools| pools.get(&tier).cloned())
					else {
						return (deposit_id, BTreeMap::default());
					};

					let BoostPoolContribution { boosted_amount, network_fee, .. } = contribution;

					let boosters_fee = utils::fee_from_boosted_amount(boosted_amount, tier)
						.saturating_sub(network_fee);

					let owed_amounts = loan
						.shares
						.iter()
						.map(|(acc_id, share)| {
							(
								acc_id.clone(),
								OwedAmount {
									total: *share * boosted_amount,
									fee: *share * boosters_fee,
								},
							)
						})
						.collect();

					(deposit_id, owed_amounts)
				})
				.collect();

			let pending_withdrawals = core_pool
				.pending_withdrawals
				.iter()
				.map(|(acc_id, loan_ids)| {
					let deposit_ids = loan_ids
						.iter()
						.filter_map(|loan_id| {
							core_pool.pending_loans.get(loan_id).and_then(|loan| {
								if let LoanUsage::Boost(deposit_id) = loan.usage {
									Some(deposit_id)
								} else {
									None
								}
							})
						})
						.collect();

					(acc_id.clone(), deposit_ids)
				})
				.collect();
			(
				tier,
				BoostPoolDetails {
					available_amounts: core_pool.get_amounts(),
					pending_boosts,
					pending_withdrawals,
					network_fee_deduction_percent,
				},
			)
		})
		.collect()
}

pub mod migration_support {

	use core_lending_pool::PendingLoan;
	use frame_support::sp_runtime::Perquintill;

	use super::*;

	pub mod old {

		use super::*;

		pub type OwedAmountScaled = OwedAmount<ScaledAmount>;

		#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
		pub struct BoostPool<AccountId> {
			// Fee charged by the pool
			pub fee_bps: BasisPoints,
			// Total available amount (not currently used in any boost)
			pub available_amount: ScaledAmount,
			// Mapping from booster to the available amount they own in `available_amount`
			pub amounts: BTreeMap<AccountId, ScaledAmount>,
			// Boosted deposits awaiting finalisation and how much of them is owed to which booster
			pub pending_boosts:
				BTreeMap<PrewitnessedDepositId, BTreeMap<AccountId, OwedAmountScaled>>,
			// Stores boosters who have indicated that they want to stop boosting along with
			// the pending deposits that they have to wait to be finalised
			pub pending_withdrawals: BTreeMap<AccountId, BTreeSet<PrewitnessedDepositId>>,
		}
	}

	pub fn migrate_boost_pools<T: Config>(
		asset: Asset,
		tier: BoostPoolTier,
		boost_pool: old::BoostPool<T::AccountId>,
	) {
		let core_pool_id = NextCorePoolId::<T>::get();
		NextCorePoolId::<T>::set(CorePoolId(core_pool_id.0 + 1));

		let (core_pool, pool_contributions) =
			deconstruct_legacy_boost_pool::<T>(core_pool_id, boost_pool);

		for (deposit_id, contributions) in pool_contributions {
			BoostedDeposits::<T>::mutate_exists(asset, deposit_id, |all_contributions| {
				let all_contributions = all_contributions.get_or_insert_default();

				all_contributions.insert(tier, contributions);
			})
		}

		BoostPools::<T>::insert(asset, tier, BoostPool { fee_bps: tier, core_pool_id });

		CorePools::<T>::insert(asset, core_pool_id, core_pool);
	}

	fn deconstruct_legacy_boost_pool<T: Config>(
		core_pool_id: CorePoolId,
		old_pool: old::BoostPool<T::AccountId>,
	) -> (CoreLendingPool<T::AccountId>, BTreeMap<PrewitnessedDepositId, BoostPoolContribution>) {
		let mut next_loan_id = 0;

		use frame_support::sp_runtime::{PerThing, Rounding};

		let mut boost_contributions: BTreeMap<PrewitnessedDepositId, BoostPoolContribution> =
			Default::default();

		let pending_loans: BTreeMap<_, _> = old_pool
			.pending_boosts
			.into_iter()
			.map(|(deposit_id, owed_amounts)| {
				// Each pending boost is assigned a loan id:
				let loan_id = next_loan_id;
				next_loan_id += 1;

				let total_boosted_amount: ScaledAmount =
					owed_amounts.values().map(|a| a.total).sum();

				let shares: BTreeMap<_, _> = owed_amounts
					.iter()
					.map(|(booster_id, OwedAmount { total: amount, .. })| {
						let share = Perquintill::from_rational_with_rounding::<u128>(
							(*amount).into(),
							total_boosted_amount.into(),
							// Round down to ensure the sum of shares does not exceed 1
							Rounding::Down,
						)
						.unwrap_or_default();

						(booster_id.clone(), share)
					})
					.collect();

				boost_contributions.insert(
					deposit_id,
					BoostPoolContribution {
						core_pool_id,
						loan_id: loan_id.into(),
						boosted_amount: total_boosted_amount.into_asset_amount(),
						network_fee: 0, // keeping things simple
					},
				);

				(loan_id.into(), PendingLoan { usage: LoanUsage::Boost(deposit_id), shares })
			})
			.collect();

		let pending_withdrawals = old_pool
			.pending_withdrawals
			.into_iter()
			.map(|(acc_id, deposit_ids)| {
				let loan_ids: BTreeSet<LoanId> = deposit_ids
					.into_iter()
					.filter_map(|deposit_id| {
						boost_contributions.get(&deposit_id).map(|c| c.loan_id)
					})
					.collect();

				(acc_id, loan_ids)
			})
			.collect();

		(
			CoreLendingPool {
				next_loan_id: next_loan_id.into(),
				available_amount: old_pool.available_amount,
				amounts: old_pool.amounts,
				pending_loans,
				pending_withdrawals,
			},
			boost_contributions,
		)
	}

	#[cfg(test)]
	mod tests {

		use super::*;

		use mocks::{BOOSTER_1, BOOSTER_2, BOOSTER_3};

		use old::OwedAmountScaled;

		const AMOUNT_1: ScaledAmount = ScaledAmount::from_raw(200_000);
		const AMOUNT_2: ScaledAmount = ScaledAmount::from_raw(300_000);

		const DEPOSIT_1: PrewitnessedDepositId = PrewitnessedDepositId(7);
		const DEPOSIT_2: PrewitnessedDepositId = PrewitnessedDepositId(8);

		const LOAN_1: LoanId = LoanId(0); // Corresponds to DEPOSIT_1
		const LOAN_2: LoanId = LoanId(1); // Corresponds to DEPOSIT_2

		fn old_pool_mock() -> old::BoostPool<u64> {
			let pending_boosts = BTreeMap::from_iter([
				(
					DEPOSIT_1,
					BTreeMap::from_iter([
						(
							BOOSTER_1,
							OwedAmountScaled {
								total: ScaledAmount::from_raw(20_000),
								fee: ScaledAmount::from_raw(10),
							},
						),
						(
							BOOSTER_2,
							OwedAmountScaled {
								total: ScaledAmount::from_raw(10_000),
								fee: ScaledAmount::from_raw(5),
							},
						),
						(
							BOOSTER_3,
							OwedAmountScaled {
								total: ScaledAmount::from_raw(50_000),
								fee: ScaledAmount::from_raw(15),
							},
						),
					]),
				),
				(
					DEPOSIT_2,
					BTreeMap::from_iter([(
						BOOSTER_1,
						OwedAmountScaled {
							total: ScaledAmount::from_raw(50_000),
							fee: ScaledAmount::from_raw(25),
						},
					)]),
				),
			]);

			let pending_withdrawals =
				BTreeMap::from_iter([(BOOSTER_3, BTreeSet::from_iter([DEPOSIT_1]))]);

			old::BoostPool {
				fee_bps: 5,
				available_amount: AMOUNT_1 + AMOUNT_2,
				amounts: BTreeMap::from_iter([(BOOSTER_1, AMOUNT_1), (BOOSTER_2, AMOUNT_2)]),
				pending_boosts,
				pending_withdrawals,
			}
		}

		#[test]
		fn test_core_pool_from_legacy_boost_pool() {
			const CORE_POOL_ID: CorePoolId = CorePoolId(3);

			let (core_pool, contributions) =
				deconstruct_legacy_boost_pool::<mocks::Test>(CORE_POOL_ID, old_pool_mock());

			assert_eq!(
				core_pool,
				CoreLendingPool {
					next_loan_id: LOAN_2 + 1,
					available_amount: AMOUNT_1 + AMOUNT_2,
					amounts: BTreeMap::from_iter([(BOOSTER_1, AMOUNT_1), (BOOSTER_2, AMOUNT_2)]),
					pending_loans: BTreeMap::from_iter([
						(
							LOAN_1,
							PendingLoan {
								usage: LoanUsage::Boost(DEPOSIT_1),
								shares: BTreeMap::from_iter([
									(BOOSTER_1, Perquintill::from_float(0.250)),
									(BOOSTER_2, Perquintill::from_float(0.125)),
									(BOOSTER_3, Perquintill::from_float(0.625))
								])
							}
						),
						(
							LOAN_2,
							PendingLoan {
								usage: LoanUsage::Boost(DEPOSIT_2),
								shares: BTreeMap::from_iter([(BOOSTER_1, Perquintill::one())])
							}
						),
					]),
					pending_withdrawals: BTreeMap::from_iter([(
						BOOSTER_3,
						BTreeSet::from_iter([LOAN_1])
					)]),
				}
			);

			assert_eq!(
				contributions,
				BTreeMap::from_iter([
					(
						DEPOSIT_1,
						BoostPoolContribution {
							core_pool_id: CORE_POOL_ID,
							loan_id: LOAN_1,
							boosted_amount: 80,
							network_fee: 0
						}
					),
					(
						DEPOSIT_2,
						BoostPoolContribution {
							core_pool_id: CORE_POOL_ID,
							loan_id: LOAN_2,
							boosted_amount: 50,
							network_fee: 0
						}
					)
				])
			);
		}
	}
}
