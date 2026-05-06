use cf_amm_math::Price;
use cf_primitives::{AccountRole, Beneficiary, DcaParameters, SwapRequestId, ONE_AS_BASIS_POINTS};
use cf_traits::{ExpiryBehaviour, LendingSwapType, LpRegistration, PriceLimitsAndExpiry};
use frame_support::{
	fail,
	sp_runtime::{
		helpers_128bit::multiply_by_rational_with_rounding, traits::Bounded, FixedI64,
		FixedPointNumber, FixedU64, PerThing, Rounding,
	},
	DefaultNoBound,
};

use crate::core_lending_pool::ScaledAmountHP;

use super::*;

#[cfg(test)]
mod general_lending_tests;

pub mod config;
mod general_lending_pool;
mod price_cache;
pub mod rpc;
mod whitelist;

pub use price_cache::OraclePriceCache;
pub use whitelist::{WhitelistStatus, WhitelistUpdate};

pub use general_lending_pool::{LendingPool, WithdrawnAndRemainingAmounts};

/// Maximum broker fee that a loan request can specify (10%).
pub const MAX_BROKER_FEE_BPS: cf_primitives::BasisPoints = 1_000;

/// Helps to link swap id in liquidation status to loan id
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct LiquidationSwap {
	loan_id: LoanId,
	from_asset: Asset,
	to_asset: Asset,
}

#[derive(
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Serialize,
	Deserialize,
)]
pub enum LiquidationType {
	SoftVoluntary,
	Soft,
	Hard,
}

/// Whether the account's collateral is being liquidated (and if so, stores ids of liquidation
/// swaps)
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub enum LiquidationStatus {
	/// No collateral is currently being liquidated. Does not necessarily mean that loans are
	/// "healthy" as we might be waiting for our collateral to become available (in case it was
	/// borrowed).
	NoLiquidation,
	Liquidating {
		liquidation_swaps: BTreeMap<SwapRequestId, LiquidationSwap>,
		liquidation_type: LiquidationType,
	},
}

/// High precision interest amounts broken down by type
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Default)]
pub struct InterestBreakdown {
	network: ScaledAmountHP,
	pool: ScaledAmountHP,
	broker: ScaledAmountHP,
	low_ltv_penalty: ScaledAmountHP,
}

#[derive(Clone, Copy, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, PartialEq, Eq)]
pub enum LiquidationCompletionReason {
	/// Full liquidation (loans are fully repaid and/or all collateral has been swapped)
	FullySwapped,
	/// Aborted to change liquidation state (e.g. to "no liquidation")
	LtvChange,
	/// Partial liquidation: manual liquidation aborted by the user
	ManualAbort,
}

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct LoanAccount<T: Config> {
	pub(super) borrower_id: T::AccountId,
	pub(super) loans: BTreeMap<LoanId, GeneralLoan<T>>,
	pub(super) liquidation_status: LiquidationStatus,
	pub(super) voluntary_liquidation_requested: bool,
}

#[derive(Clone, Debug)]
enum SurplusOrDeficit {
	Surplus(AssetAmount),
	Deficit,
}

#[derive(
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Serialize,
	Deserialize,
)]
pub enum LoanType<AccountId> {
	User(AccountId),
	Boost(PrewitnessedDepositId),
}

pub fn supply_funds<T: Config>(
	lp: T::AccountId,
	asset: Asset,
	amount: AssetAmount,
	action_type: SupplyAddedActionType,
) -> DispatchResult {
	GeneralLendingPools::<T>::try_mutate(asset, |maybe_pool| {
		let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

		pool.add_funds(&lp, amount);

		Ok::<_, DispatchError>(())
	})?;

	Pallet::deposit_event(Event::<T>::LendingFundsAdded {
		lender_id: lp,
		asset,
		amount,
		action_type,
	});

	Ok(())
}

impl<T: Config> LoanAccount<T> {
	pub fn new(borrower_id: T::AccountId) -> Self {
		Self {
			borrower_id,
			loans: BTreeMap::new(),
			liquidation_status: LiquidationStatus::NoLiquidation,
			voluntary_liquidation_requested: false,
		}
	}

	/// Convenience method to get a loan and at the same time sanity check that the asset
	/// matches the expected asset.
	fn get_loan_and_check_asset(
		&mut self,
		loan_id: LoanId,
		asset: Asset,
	) -> Option<&mut GeneralLoan<T>> {
		match self.loans.get_mut(&loan_id) {
			Some(loan) if loan.asset == asset => Some(loan),
			Some(loan) => {
				log_or_panic!(
					"Loan {} has asset {}, but expected ({})",
					loan_id,
					loan.asset,
					asset
				);
				None
			},
			None => None,
		}
	}

	pub fn get_collateral_in_supply_pools(&self) -> BTreeMap<Asset, AssetAmount> {
		GeneralLendingPools::<T>::iter()
			.filter_map(|(asset, pool)| {
				pool.get_supply_position_for_account(&self.borrower_id)
					.ok()
					.map(|amount| (asset, amount))
			})
			.collect()
	}

	/// Returns the account's collateral including any amounts that are in liquidation swaps.
	pub fn get_total_collateral(&self) -> BTreeMap<Asset, AssetAmount> {
		// Note that in order to keep things simple we don't guarantee that all of the
		// all collateral is being liquidated (e.g. it is possible for the user to top
		// up collateral during liquidation in which case we currently don't update the
		// liquidation swaps), but we *do* include collateral that is not in liquidation
		// when determining account's collateralisation ratio.

		let mut total_collateral = self.get_collateral_in_supply_pools();

		// Add any collateral in liquidation swaps:
		if let LiquidationStatus::Liquidating { liquidation_swaps, .. } = &self.liquidation_status {
			for (swap_request_id, LiquidationSwap { from_asset, .. }) in liquidation_swaps {
				if let Some(swap_progress) =
					T::SwapRequestHandler::inspect_swap_request(*swap_request_id)
				{
					total_collateral
						.entry(*from_asset)
						.or_default()
						.saturating_accrue(swap_progress.remaining_input_amount);
				} else {
					log_or_panic!("Failed to inspect swap request: {swap_request_id}");
				}
			}
		}

		total_collateral
	}

	/// Supply funds after liquidation (either from unused input amount or from
	/// excess output amount).
	fn supply_from_liquidation(
		&mut self,
		asset: Asset,
		amount: AssetAmount,
		action_type: SupplyAddedActionType,
	) {
		if amount > 0 {
			Pallet::deposit_event(Event::<T>::LendingFundsAdded {
				lender_id: self.borrower_id.clone(),
				asset,
				amount,
				action_type,
			});

			Pallet::<T>::mutate_existing_pool(asset, |pool| {
				pool.add_funds(&self.borrower_id, amount);
			});
		}
	}

	/// Computes account's total collateral value in USD, including what's in liquidation swaps.
	pub fn total_collateral_usd_value(
		&self,
		price_cache: &OraclePriceCache<T>,
	) -> Result<AssetAmount, Error<T>> {
		self.get_total_collateral()
			.iter()
			.map(|(asset, amount)| price_cache.usd_value_of(*asset, *amount))
			.try_fold(0u128, |acc, x| Ok(acc.saturating_add(x?)))
	}

	#[transactional]
	pub fn update_liquidation_status(
		&mut self,
		borrower_id: &T::AccountId,
		ltv: FixedU64,
		price_cache: &OraclePriceCache<T>,
		weight_used: &mut Weight,
	) -> DispatchResult {
		let config = LendingConfig::<T>::get();

		// This will saturate at 100%, but that's good enough (none of our thresholds exceed 100%):
		let ltv: Permill = ltv.into_clamped_perthing();

		#[derive(Debug)]
		enum LiquidationStatusChange {
			NoChange,
			HealthyToLiquidation { liquidation_type: LiquidationType },
			ChangeLiquidationType { liquidation_type: LiquidationType },
			AbortLiquidation { reason: LiquidationCompletionReason },
		}

		// Every time we transition from a liquidating state we abort all liquidation swaps
		// and repay any swapped into principal. If the next state is "NoLiquidation", the
		// collateral is returned into the loan account; if it is "Liquidating", the collateral
		// is used in the new liquidation swaps.
		let new_status = match &mut self.liquidation_status {
			LiquidationStatus::NoLiquidation => {
				// If LTV requires us to initiate liquidation, we start a forced liquidation.
				// Otherwise, if check if voluntary liquidation is requested and initiate it if so.
				if ltv > config.ltv_thresholds.hard_liquidation {
					LiquidationStatusChange::HealthyToLiquidation {
						liquidation_type: LiquidationType::Hard,
					}
				} else if ltv > config.ltv_thresholds.soft_liquidation {
					LiquidationStatusChange::HealthyToLiquidation {
						liquidation_type: LiquidationType::Soft,
					}
				} else if self.voluntary_liquidation_requested {
					LiquidationStatusChange::HealthyToLiquidation {
						liquidation_type: LiquidationType::SoftVoluntary,
					}
				} else {
					LiquidationStatusChange::NoChange
				}
			},
			LiquidationStatus::Liquidating { liquidation_type: LiquidationType::Hard, .. } => {
				// If LTV requires us to either stay in hard liquidation or deescalate to soft
				// liquidation, we do so (this will still be a "forced" liquidation).
				// The only time need to check voluntary liquidation flag is when we would be
				// aborting liquidation: if it is set, we transition to voluntary liquidation.
				if ltv < config.ltv_thresholds.soft_liquidation_abort {
					if self.voluntary_liquidation_requested {
						LiquidationStatusChange::ChangeLiquidationType {
							liquidation_type: LiquidationType::SoftVoluntary,
						}
					} else {
						LiquidationStatusChange::AbortLiquidation {
							reason: LiquidationCompletionReason::LtvChange,
						}
					}
				} else if ltv < config.ltv_thresholds.hard_liquidation_abort {
					LiquidationStatusChange::ChangeLiquidationType {
						liquidation_type: LiquidationType::Soft,
					}
				} else {
					LiquidationStatusChange::NoChange
				}
			},
			LiquidationStatus::Liquidating { liquidation_type: LiquidationType::Soft, .. } => {
				// If LTV requires us to either stay in soft liquidation or escalate to hard
				// liquidation, we do so (this will still be a "forced" liquidation).
				// The only time need to check voluntary liquidation flag is when we would be
				// aborting liquidation: if it is set, we transition to voluntary liquidation.

				if ltv > config.ltv_thresholds.hard_liquidation {
					LiquidationStatusChange::ChangeLiquidationType {
						liquidation_type: LiquidationType::Hard,
					}
				} else if ltv < config.ltv_thresholds.soft_liquidation_abort {
					if self.voluntary_liquidation_requested {
						LiquidationStatusChange::ChangeLiquidationType {
							liquidation_type: LiquidationType::SoftVoluntary,
						}
					} else {
						LiquidationStatusChange::AbortLiquidation {
							reason: LiquidationCompletionReason::LtvChange,
						}
					}
				} else {
					LiquidationStatusChange::NoChange
				}
			},
			LiquidationStatus::Liquidating {
				liquidation_type: LiquidationType::SoftVoluntary,
				..
			} => {
				// If according to LTV we should be in soft/hard liquidation, abort the current
				// voluntary liquidation and start a forced liquidation.
				if ltv > config.ltv_thresholds.hard_liquidation {
					LiquidationStatusChange::ChangeLiquidationType {
						liquidation_type: LiquidationType::Hard,
					}
				} else if ltv > config.ltv_thresholds.soft_liquidation {
					LiquidationStatusChange::ChangeLiquidationType {
						liquidation_type: LiquidationType::Soft,
					}
				} else if !self.voluntary_liquidation_requested {
					// If the user switched off the manual liquidation flag (and LTV is "healthy"),
					// abort liquidation and transition to the "no liquidation" state.
					LiquidationStatusChange::AbortLiquidation {
						reason: LiquidationCompletionReason::ManualAbort,
					}
				} else if ltv == Permill::zero() {
					// LTV of 0 implies that liquidation swaps have resulted in enough of
					// loan assets to fully repay all outstanding loans, in which case
					// we abort the liquidation (this will reset the voluntary liquidation
					// flag).
					LiquidationStatusChange::AbortLiquidation {
						reason: LiquidationCompletionReason::FullySwapped,
					}
				} else {
					LiquidationStatusChange::NoChange
				}
			},
		};

		match new_status {
			LiquidationStatusChange::HealthyToLiquidation { liquidation_type } => {
				ensure!(T::SafeMode::get().liquidations_enabled, Error::<T>::LiquidationsDisabled);
				let collateral =
					self.prepare_collateral_for_liquidation(price_cache, liquidation_type)?;
				self.init_liquidation_swaps(
					borrower_id,
					collateral,
					liquidation_type,
					price_cache,
				)?;
				weight_used.saturating_accrue(T::WeightInfo::start_liquidation_swaps());
			},
			LiquidationStatusChange::AbortLiquidation { reason } => {
				self.abort_liquidation_swaps(reason);
				weight_used.saturating_accrue(T::WeightInfo::abort_liquidation_swaps());
			},
			LiquidationStatusChange::ChangeLiquidationType { liquidation_type } => {
				// Going from one liquidation type to another is always due to LTV change
				self.abort_liquidation_swaps(LiquidationCompletionReason::LtvChange);
				let collateral =
					self.prepare_collateral_for_liquidation(price_cache, liquidation_type)?;
				self.init_liquidation_swaps(
					borrower_id,
					collateral,
					liquidation_type,
					price_cache,
				)?;
				weight_used.saturating_accrue(T::WeightInfo::start_liquidation_swaps());
				weight_used.saturating_accrue(T::WeightInfo::abort_liquidation_swaps());
			},
			LiquidationStatusChange::NoChange => { /* nothing to do */ },
		}

		Ok(())
	}

	/// Aborts all current liquidation swaps, repays any already swapped principal assets and
	/// returns remaining collateral assets alongside the corresponding loan information.
	pub(super) fn abort_liquidation_swaps(&mut self, reason: LiquidationCompletionReason) {
		let (is_voluntary, liquidation_swaps) = match &mut self.liquidation_status {
			LiquidationStatus::NoLiquidation => {
				log_or_panic!("Attempting to abort liquidation swaps in no-liquidation state");
				(false, Default::default())
			},
			LiquidationStatus::Liquidating { liquidation_swaps, liquidation_type } => {
				let is_voluntary = *liquidation_type == LiquidationType::SoftVoluntary;

				(is_voluntary, core::mem::take(liquidation_swaps))
			},
		};

		self.liquidation_status = LiquidationStatus::NoLiquidation;

		Pallet::<T>::deposit_event(Event::LiquidationCompleted {
			borrower_id: self.borrower_id.clone(),
			reason,
		});

		// It should be rare, but not impossible that a partial liquidation fully repays
		// the loan. We delay settling them until the end of this function to make sure that
		// all liquidations fees are correctly paid.
		let mut fully_repaid_loans = vec![];

		for (swap_request_id, LiquidationSwap { loan_id, from_asset, to_asset }) in
			liquidation_swaps
		{
			if let Some(swap_progress) = T::SwapRequestHandler::abort_swap_request(swap_request_id)
			{
				let excess_amount = match self.get_loan_and_check_asset(loan_id, to_asset) {
					Some(loan) => {
						let excess_amount = loan.repay_via_liquidation(
							swap_progress.accumulated_output_amount,
							swap_request_id,
							is_voluntary,
						);

						if loan.owed_principal == 0 {
							fully_repaid_loans.push(loan_id);
						}

						excess_amount
					},
					None => swap_progress.accumulated_output_amount,
				};

				// In case we have liquidated more than necessary the excess amount
				// is added to the supply pool:
				self.supply_from_liquidation(
					to_asset,
					excess_amount,
					SupplyAddedActionType::SystemLiquidationExcessAmount {
						loan_id,
						swap_request_id,
					},
				);

				// Any input funds not yet liquidated are returned to the
				// supply pool:
				self.supply_from_liquidation(
					from_asset,
					swap_progress.remaining_input_amount,
					SupplyAddedActionType::SystemLiquidationUnusedAmount {
						loan_id,
						swap_request_id,
					},
				);
			} else {
				log_or_panic!("Failed to abort swap request: {swap_request_id}");
			}
		}

		for loan_id in fully_repaid_loans {
			self.settle_loan(loan_id, true /* via liquidation */);
		}
	}

	/// Computes the total amount owed in account's loans in USD adjusting for the amount that will
	/// be repaid by the collateral that has already been swapped into the loan asset.
	pub fn total_owed_usd_value(
		&self,
		price_cache: &OraclePriceCache<T>,
	) -> Result<AssetAmount, Error<T>> {
		let total_owed = self
			.loans
			.values()
			.map(|loan| loan.owed_principal_usd_value(price_cache))
			.try_fold(0u128, |acc, x| x.map(|v| acc.saturating_add(v)))?;

		let swapped_from_collateral = match &self.liquidation_status {
			LiquidationStatus::NoLiquidation => 0,
			LiquidationStatus::Liquidating { liquidation_swaps, .. } => {
				let mut total_principal_usd_value_in_swaps = 0;
				// If we are liquidating loans, we might already have some collateral swapped
				// into the loan asset that will be used to repay the loan:
				for (swap_request_id, LiquidationSwap { to_asset, .. }) in liquidation_swaps {
					if let Some(swap_progress) =
						T::SwapRequestHandler::inspect_swap_request(*swap_request_id)
					{
						total_principal_usd_value_in_swaps.saturating_accrue(
							price_cache
								.usd_value_of(*to_asset, swap_progress.accumulated_output_amount)?,
						);
					} else {
						log_or_panic!("Failed to inspect swap request: {swap_request_id}");
					}
				}
				total_principal_usd_value_in_swaps
			},
		};

		Ok(total_owed.saturating_sub(swapped_from_collateral))
	}

	/// Computes account's LTV (Loan-to-Value) ratio. Takes liquidations into account.
	/// Returns error if oracle prices are not available, or if collateral is zero.
	pub fn derive_ltv(&self, price_cache: &OraclePriceCache<T>) -> Result<FixedU64, Error<T>> {
		let collateral = self.total_collateral_usd_value(price_cache)?;
		let principal = self.total_owed_usd_value(price_cache)?;

		if collateral == 0 {
			if principal == 0 {
				return Ok(FixedU64::zero());
			} else {
				return Ok(FixedU64::max_value());
			}
		}

		Ok(FixedU64::from_rational(principal, collateral))
	}

	pub fn check_low_ltv_penalty_and_collect_interest(
		&mut self,
		ltv: FixedU64,
		price_cache: &OraclePriceCache<T>,
		config: &LendingConfiguration,
		weight_used: &mut Weight,
	) {
		let current_block = frame_system::Pallet::<T>::block_number();
		weight_used.saturating_accrue(T::DbWeight::get().reads(1));

		for loan in self.loans.values_mut() {
			if loan.should_charge_interest(current_block, config) {
				loan.charge_low_ltv_penalty(ltv, config);
				weight_used.saturating_accrue(T::WeightInfo::loan_charge_low_ltv_penalty());

				// Error can be ignored (likely due to oracle being unavailable), we will simply
				// collect interest next time:
				let _ = loan.collect_pending_interest_if_above_threshold(Some(
					PriceCacheAndThreshold {
						threshold_usd: config.interest_collection_threshold_usd,
						price_cache,
					},
				));
				weight_used.saturating_accrue(T::WeightInfo::collect_pending_interest());
			}
		}
	}

	pub fn derive_and_charge_interest(&mut self, weight_used: &mut Weight) {
		let config = LendingConfig::<T>::get();

		let current_block = frame_system::Pallet::<T>::block_number();
		weight_used.saturating_accrue(T::DbWeight::get().reads(1));

		for loan in self.loans.values_mut() {
			if loan.should_charge_interest(current_block, &config) {
				weight_used.saturating_accrue(T::WeightInfo::loan_charge_interest());
				loan.charge_interest(&config);
			}
		}
	}

	fn calculate_collateral_surplus_or_deficit(
		&self,
		ltv_threshold_target: Permill,
		price_cache: &OraclePriceCache<T>,
	) -> Result<SurplusOrDeficit, Error<T>> {
		let collateral_value_in_usd = self.total_collateral_usd_value(price_cache)?;
		let loan_value_in_usd = self.total_owed_usd_value(price_cache)?;

		let collateral_required_in_usd = FixedU64::from(ltv_threshold_target)
			.reciprocal()
			.map(|ltv_inverted| ltv_inverted.saturating_mul_int(loan_value_in_usd))
			// This fails if the ltv target erroneously set to 0:
			.ok_or(Error::<T>::InvalidConfigurationParameters)?;

		if collateral_required_in_usd >= collateral_value_in_usd {
			Ok(SurplusOrDeficit::Deficit)
		} else {
			Ok(SurplusOrDeficit::Surplus(collateral_value_in_usd - collateral_required_in_usd))
		}
	}

	/// Collect just enough collateral from supply pools to cover the outstanding loan
	/// principal plus a buffer for the maximum oracle price slippage allowed for the
	/// upcoming liquidation swaps, then split the collected collateral proportionally to
	/// the usd value of each loan (to give each loan a fair chance of being liquidated
	/// without a loss). When multiple collateral assets are involved, each is drawn
	/// proportionally to its available USD value. Returns error if oracle prices aren't
	/// available.
	pub(super) fn prepare_collateral_for_liquidation(
		&mut self,
		price_cache: &OraclePriceCache<T>,
		liquidation_type: LiquidationType,
	) -> Result<Vec<AssetCollateralForLoan>, Error<T>> {
		let config = LendingConfig::<T>::get();
		let max_oracle_price_slippage_bps = match liquidation_type {
			LiquidationType::Hard => config.hard_liquidation_max_oracle_slippage,
			LiquidationType::Soft | LiquidationType::SoftVoluntary =>
				config.soft_liquidation_max_oracle_slippage,
		};

		// Gather available collateral positions per asset, paired with their USD value.
		let positions_with_usd: Vec<(Asset, AssetAmount, AssetAmount)> = self
			.get_collateral_in_supply_pools()
			.into_iter()
			.filter(|(_, amount)| *amount > 0)
			.map(|(asset, amount)| {
				price_cache.usd_value_of(asset, amount).map(|usd| (asset, amount, usd))
			})
			.collect::<Result<Vec<_>, Error<T>>>()?;

		let total_owed_usd = self.total_owed_usd_value(price_cache)?;

		let mut collateral_to_liquidate = BTreeMap::<Asset, AssetAmount>::new();

		for (asset, amount_to_request) in compute_per_asset_liquidation_estimates(
			total_owed_usd,
			max_oracle_price_slippage_bps,
			&positions_with_usd,
		) {
			Pallet::<T>::mutate_existing_pool(asset, |pool| {
				if let Ok(WithdrawnAndRemainingAmounts { withdrawn_amount, .. }) =
					pool.remove_funds(&self.borrower_id, Some(amount_to_request))
				{
					if withdrawn_amount > 0 {
						collateral_to_liquidate
							.entry(asset)
							.or_default()
							.saturating_accrue(withdrawn_amount);

						Pallet::<T>::deposit_event(Event::<T>::LendingFundsRemoved {
							lender_id: self.borrower_id.clone(),
							asset,
							unlocked_amount: withdrawn_amount,
							action_type: SupplyRemovedActionType::SystemLiquidation,
						});
					}
				}
			});
		}

		if is_zero_collateral(&collateral_to_liquidate) {
			// Don't bother splitting zero collateral between loans:
			return Ok(Default::default());
		}

		let principal_amounts_usd = self
			.loans
			.iter()
			.map(|(loan_id, loan)| {
				loan.owed_principal_usd_value(price_cache)
					.map(|usd_value| ((*loan_id, loan.asset), usd_value))
			})
			.collect::<Result<Vec<_>, Error<T>>>()?;

		let mut prepared_collateral = vec![];

		for (collateral_asset, collateral_amount) in collateral_to_liquidate {
			let distribution = utils::distribute_proportionally(
				collateral_amount,
				principal_amounts_usd.iter().map(|(k, v)| (k, *v)),
			);

			for ((loan_id, loan_asset), collateral_amount) in distribution {
				prepared_collateral.push(AssetCollateralForLoan {
					loan_id: *loan_id,
					loan_asset: *loan_asset,
					collateral_asset,
					collateral_amount,
				});
			}
		}

		Ok(prepared_collateral)
	}

	/// Initiate liquidation swaps for each collateral asset (from collateral either
	/// prepared by [prepare_collateral_for_liquidation] or collected from previous
	/// liquidation swaps via [abort_liquidation_swaps]).
	pub(super) fn init_liquidation_swaps(
		&mut self,
		borrower_id: &T::AccountId,
		collateral: Vec<AssetCollateralForLoan>,
		liquidation_type: LiquidationType,
		price_cache: &OraclePriceCache<T>,
	) -> Result<(), Error<T>> {
		if self.liquidation_status != LiquidationStatus::NoLiquidation {
			log_or_panic!("Account {:?} is already in a liquidation state", borrower_id);
			fail!(Error::<T>::InternalInvariantViolation);
		}

		if collateral.is_empty() {
			// Collateral may not be immediately available (e.g. a lending pool
			// we supplied into may have 100% utilisation). In this case there is nothing to do
			// yet (we will try again later).
			return Ok(());
		}

		let config = LendingConfig::<T>::get();

		let mut liquidation_swaps = BTreeMap::new();

		let mut swaps_for_event = BTreeMap::<LoanId, Vec<SwapRequestId>>::new();

		for AssetCollateralForLoan {
			loan_id,
			loan_asset: to_asset,
			collateral_asset: from_asset,
			collateral_amount: amount_to_swap,
		} in collateral
		{
			let (chunk_size, max_oracle_price_slippage) =
				if liquidation_type == LiquidationType::Hard {
					(
						config.hard_liquidation_swap_chunk_size_usd,
						config.hard_liquidation_max_oracle_slippage,
					)
				} else {
					(
						config.soft_liquidation_swap_chunk_size_usd,
						config.soft_liquidation_max_oracle_slippage,
					)
				};

			let dca_params = {
				// This number is chosen in attempt to have individual chunks that aren't
				// too large and can be processed, while keeping the total liquidation
				// time reasonable, i.e. ~5 mins.
				const DEFAULT_LIQUIDATION_CHUNKS: u32 = 50;

				let number_of_chunks = match price_cache.usd_value_of(from_asset, amount_to_swap) {
					Ok(total_amount_usd) => {
						// Making sure that we don't divide by 0
						if chunk_size == 0 {
							DEFAULT_LIQUIDATION_CHUNKS
						} else {
							total_amount_usd.div_ceil(chunk_size) as u32
						}
					},
					Err(_) => {
						// It shouldn't be possible to not get the price here (we don't initiate
						// liquidations unless we can get prices), but if we do, let's fallback
						// to DEFAULT_LIQUIDATION_CHUNKS chunks
						log_or_panic!(
							"Failed to estimate optimal chunk size for a {}->{} swap",
							from_asset,
							to_asset
						);

						DEFAULT_LIQUIDATION_CHUNKS
					},
				};

				DcaParameters { number_of_chunks, chunk_interval: 1 }
			};

			let swap_request_id = T::SwapRequestHandler::init_swap_request(
				from_asset,
				amount_to_swap,
				to_asset,
				SwapRequestType::Regular {
					output_action: SwapOutputAction::CreditLendingPool {
						swap_type: LendingSwapType::Liquidation {
							borrower_id: borrower_id.clone(),
							loan_id,
						},
					},
				},
				Default::default(), // broker fees
				Some(PriceLimitsAndExpiry {
					expiry_behaviour: ExpiryBehaviour::NoExpiry,
					min_price: Default::default(),
					max_oracle_price_slippage: Some(max_oracle_price_slippage),
				}),
				Some(dca_params),
				SwapOrigin::Internal,
			);

			swaps_for_event.entry(loan_id).or_default().push(swap_request_id);

			liquidation_swaps
				.insert(swap_request_id, LiquidationSwap { loan_id, from_asset, to_asset });
		}

		Pallet::<T>::deposit_event(Event::LiquidationInitiated {
			borrower_id: borrower_id.clone(),
			swaps: swaps_for_event,
			liquidation_type,
		});

		self.liquidation_status =
			LiquidationStatus::Liquidating { liquidation_swaps, liquidation_type };

		Ok(())
	}

	fn settle_loan(&mut self, loan_id: LoanId, via_liquidation: bool) {
		if let Some(loan) = self.loans.remove(&loan_id) {
			loan.settle(via_liquidation);
		}

		if self.loans.is_empty() {
			// Reset the voluntary liquidation flag in case it was set
			// (otherwise any future loan will immediately get liquidated)
			self.voluntary_liquidation_requested = false;
		}
	}

	fn expand_loan_inner(
		&mut self,
		mut loan: GeneralLoan<T>,
		extra_principal: AssetAmount,
		price_cache: &OraclePriceCache<T>,
	) -> Result<(), DispatchError> {
		let config = LendingConfig::<T>::get();
		let loan_asset = loan.asset;

		ensure!(
			T::SafeMode::get().borrowing.enabled(&loan_asset),
			Error::<T>::LoanCreationDisabled
		);

		// To avoid unnecessary edge cases we disable creation of new loans
		// while the account is being liquidation (even if the user provides
		// sufficient collateral)
		ensure!(
			self.liquidation_status == LiquidationStatus::NoLiquidation,
			Error::<T>::LiquidationInProgress
		);

		let origination_fee_total = config.origination_fee(loan.asset) * extra_principal;

		let origination_fee_network =
			config.network_fee_contributions.from_origination_fee * origination_fee_total;

		let origination_fee_pool = origination_fee_total.saturating_sub(origination_fee_network);

		fund_loan::<T>(
			&mut loan,
			extra_principal,
			origination_fee_pool,
			origination_fee_network,
			price_cache,
		)?;

		self.loans.insert(loan.id, loan);

		if self.derive_ltv(price_cache)? > config.ltv_thresholds.target.into() {
			return Err(Error::<T>::LtvTooHigh.into());
		}

		if self.voluntary_liquidation_requested {
			log_or_panic!("Voluntary liquidation flag is set on loan creation");
			// If the user requests any additional loans, we assume they no longer want to
			// be in voluntary liquidation mode (if for whatever reason it was active)
			self.voluntary_liquidation_requested = false;
		}

		T::Balance::credit_account(&self.borrower_id, loan_asset, extra_principal);

		Ok(())
	}
}

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct GeneralLoan<T: Config> {
	pub id: LoanId,
	pub asset: Asset,
	pub created_at_block: BlockNumberFor<T>,
	pub owed_principal: AssetAmount,
	/// Interest owed on the loan but not yet taken (it is either below the threshold or waiting
	/// for funds to become available)
	pub pending_interest: InterestBreakdown,
	/// Broker and their fee/interest (if any)
	pub broker: Option<Beneficiary<T::AccountId>>,
}

/// A parameter into [collect_pending_interest_if_above_threshold] grouping the threshold
/// and price cache together so they can both be wrapped in an Option.
pub struct PriceCacheAndThreshold<'a, T: Config> {
	pub threshold_usd: AssetAmount,
	pub price_cache: &'a OraclePriceCache<T>,
}

impl<T: Config> GeneralLoan<T> {
	fn owed_principal_usd_value(
		&self,
		price_cache: &OraclePriceCache<T>,
	) -> Result<AssetAmount, Error<T>> {
		price_cache.usd_value_of(self.asset, self.owed_principal)
	}

	fn collect_pending_interest(&mut self) {
		if self
			.collect_pending_interest_if_above_threshold(None /* no threshold */)
			.is_err()
		{
			log_or_panic!(
				"Final interest charge should not fail since the price oracle is not required here"
			);
		}
	}

	pub(super) fn charge_low_ltv_penalty(&mut self, ltv: FixedU64, config: &LendingConfiguration) {
		// Calculating interest in scaled amounts for better precision
		let owed_principal = ScaledAmountHP::from_asset_amount(self.owed_principal);

		let low_ltv_penalty_rate = config.derive_low_ltv_penalty_rate_per_payment_interval(
			ltv,
			config.interest_payment_interval_blocks,
		);

		let low_ltv_penalty_amount = owed_principal * low_ltv_penalty_rate;

		self.pending_interest.low_ltv_penalty.saturating_accrue(low_ltv_penalty_amount);
	}

	/// Determines whether we should charge interest (or low ltv penalty) at a given block
	fn should_charge_interest(
		&self,
		current_block: BlockNumberFor<T>,
		config: &LendingConfiguration,
	) -> bool {
		current_block.saturating_sub(self.created_at_block) %
			config.interest_payment_interval_blocks.into() ==
			0u32.into()
	}

	pub(super) fn charge_interest(&mut self, config: &LendingConfiguration) {
		let loan_asset = self.asset;

		let payment_interval = config.interest_payment_interval_blocks;

		let base_interest_rate = {
			let utilisation = GeneralLendingPools::<T>::get(loan_asset)
				.map(|pool| pool.get_utilisation())
				.unwrap_or_default();

			config.derive_base_interest_rate_per_payment_interval(
				loan_asset,
				utilisation,
				payment_interval,
			)
		};

		let network_interest_rate =
			config.derive_network_interest_rate_per_payment_interval(payment_interval);

		// Calculating interest in scaled amounts for better precision
		let owed_principal = ScaledAmountHP::from_asset_amount(self.owed_principal);

		// Work out how much interest has accrued in loan's asset terms:
		let network_interest_amount = owed_principal * network_interest_rate;

		let pool_interest_amount = owed_principal * base_interest_rate;

		// Record the accrued interest amounts. We may or may not charge these immediately
		// depending on whether the amounts exceed some threshold.
		self.pending_interest.network.saturating_accrue(network_interest_amount);
		self.pending_interest.pool.saturating_accrue(pool_interest_amount);

		if let Some(broker) = &self.broker {
			use cf_primitives::BASIS_POINTS_PER_MILLION;
			let broker_interest_rate = config.interest_per_year_to_per_payment_interval(
				Permill::from_parts(broker.bps as u32 * BASIS_POINTS_PER_MILLION),
				payment_interval,
			);
			let broker_interest_amount = owed_principal * broker_interest_rate;
			self.pending_interest.broker.saturating_accrue(broker_interest_amount);
		}
	}

	/// Repays the loan after collecting any pending interest and deducting liquidation fee
	/// from the provided amount. Returns any excess/unused funds.
	fn repay_via_liquidation(
		&mut self,
		provided_amount: AssetAmount,
		swap_request_id: SwapRequestId,
		is_voluntary: bool,
	) -> AssetAmount {
		let config = LendingConfig::<T>::get();

		self.collect_pending_interest();

		// Only charge liquidation fee if liquidation was not voluntary
		let provided_amount_after_fees = if is_voluntary {
			provided_amount
		} else {
			let liquidation_fee = config.get_config_for_asset(self.asset).liquidation_fee *
				core::cmp::min(provided_amount, self.owed_principal);

			if liquidation_fee > 0 {
				let liquidation_fee_network =
					config.network_fee_contributions.from_liquidation_fee * liquidation_fee;
				Pallet::<T>::credit_fees_to_network(self.asset, liquidation_fee_network);

				let liquidation_fee_pool = liquidation_fee.saturating_sub(liquidation_fee_network);
				Pallet::<T>::credit_fees_to_pool(self.asset, liquidation_fee_pool);

				Pallet::<T>::deposit_event(Event::LiquidationFeeTaken {
					loan_id: self.id,
					pool_fee: liquidation_fee_pool,
					network_fee: liquidation_fee_network,
					// TODO: add support for broker fees (see https://linear.app/chainflip/issue/PRO-2851/decide-whether-to-support-broker-fees-from-origination-and-liquidation)
					broker_fee: 0,
				});
			}

			provided_amount.saturating_sub(liquidation_fee)
		};

		self.repay_principal(
			provided_amount_after_fees,
			LoanRepaidActionType::Liquidation { swap_request_id },
		)
	}

	/// Repays (fully or partially) the loan with `provided_amount` (that was either debited from
	/// the account or received during liquidation). Returns any unused amount. The caller is
	/// responsible for making sure that all pending interest has already been collected (via
	/// [collect_pending_interest]) and that the provided asset is the same as the loan's asset.
	pub(super) fn repay_principal(
		&mut self,
		provided_amount: AssetAmount,
		action_type: LoanRepaidActionType,
	) -> AssetAmount {
		// Making sure the user doesn't pay more than the total principal plus liquidation fee:
		let repayment_amount = core::cmp::min(provided_amount, self.owed_principal);

		if repayment_amount > 0 {
			Pallet::<T>::mutate_existing_pool(self.asset, |pool| {
				pool.receive_repayment(repayment_amount);
			});

			self.owed_principal.saturating_reduce(repayment_amount);

			Pallet::<T>::deposit_event(Event::LoanRepaid {
				loan_id: self.id,
				amount: repayment_amount,
				action_type,
			});
		}

		provided_amount.saturating_sub(repayment_amount)
	}

	#[transactional]
	pub fn collect_pending_interest_if_above_threshold(
		&mut self,
		price_cache_and_threshold: Option<PriceCacheAndThreshold<T>>,
	) -> DispatchResult {
		let loan_asset = self.asset;

		if self.pending_interest == Default::default() {
			return Ok(());
		}

		let charge_fee_if_exceeds_threshold = |fee: &mut ScaledAmountHP| {
			let fee_taken = if let Some(PriceCacheAndThreshold { threshold_usd, price_cache }) =
				price_cache_and_threshold
			{
				// If the threshold is provided, take fees only if they exceed it:
				if price_cache.usd_value_of(loan_asset, fee.into_asset_amount())? > threshold_usd {
					fee.take_non_fractional_part()
				} else {
					Default::default()
				}
			} else {
				// If no threshold is provided, take the fees unconditionally:
				fee.take_non_fractional_part()
			};

			Ok::<_, DispatchError>(fee_taken)
		};

		let network_interest = charge_fee_if_exceeds_threshold(&mut self.pending_interest.network)?;

		let low_ltv_penalty =
			charge_fee_if_exceeds_threshold(&mut self.pending_interest.low_ltv_penalty)?;

		let pool_interest = charge_fee_if_exceeds_threshold(&mut self.pending_interest.pool)?;

		let broker_interest = charge_fee_if_exceeds_threshold(&mut self.pending_interest.broker)?;

		let fees_owed_to_network = network_interest.saturating_add(low_ltv_penalty);

		self.owed_principal.saturating_accrue(pool_interest);
		self.owed_principal.saturating_accrue(fees_owed_to_network);

		let broker_interest_collected = Pallet::<T>::mutate_existing_pool(loan_asset, |pool| {
			pool.record_pool_fee(pool_interest);

			let network_fees_collected = pool.record_and_collect_network_fee(fees_owed_to_network);
			Pallet::<T>::credit_fees_to_network(loan_asset, network_fees_collected);

			pool.collect_fee_from_available(broker_interest)
		})
		.unwrap_or_default();

		// Any portion of the broker fee that the pool couldn't cover stays in pending so the
		// next collection round can retry once the pool has more liquidity.
		let broker_interest_uncollected = broker_interest.saturating_sub(broker_interest_collected);
		if broker_interest_uncollected > 0 {
			self.pending_interest
				.broker
				.saturating_accrue(ScaledAmountHP::from_asset_amount(broker_interest_uncollected));
		}

		self.owed_principal.saturating_accrue(broker_interest_collected);

		if broker_interest_collected > 0 {
			match &self.broker {
				Some(broker) => T::Balance::credit_account(
					&broker.account,
					loan_asset,
					broker_interest_collected,
				),
				None =>
					log_or_panic!("Broker interest collected without broker on loan {:?}", self.id),
			}
		}

		if pool_interest != 0 ||
			network_interest != 0 ||
			low_ltv_penalty != 0 ||
			broker_interest_collected != 0
		{
			Pallet::<T>::deposit_event(Event::InterestTaken {
				loan_id: self.id,
				pool_interest,
				network_interest,
				broker_interest: broker_interest_collected,
				low_ltv_penalty,
			});
		}

		Ok(())
	}

	/// Last method to be called on a loan. Checks outstanding debt and emits LoanSettled event.
	pub fn settle(self, via_liquidation: bool) {
		if self.owed_principal > 0 {
			Pallet::<T>::mutate_existing_pool(self.asset, |pool| {
				pool.write_off_unrecoverable_debt(self.owed_principal);
			});
		}

		Pallet::<T>::deposit_event(Event::LoanSettled {
			loan_id: self.id,
			outstanding_principal: self.owed_principal,
			via_liquidation,
		});
	}
}

/// Collateral amount linked to a specific loan
#[derive(Debug)]
pub(super) struct AssetCollateralForLoan {
	loan_id: LoanId,
	loan_asset: Asset,
	collateral_asset: Asset,
	collateral_amount: AssetAmount,
}

/// Inflate `total_owed_usd` by the maximum oracle slippage allowed for the upcoming
/// liquidation swaps. In the worst case the swaps return `oracle_value * (1 - slippage)`
/// of loan asset per unit of oracle USD value of collateral, so to fully cover the debt
/// we must collect collateral worth `total_owed_usd / (1 - slippage)`. A slippage of 100%
/// (or more) means the swap could return nothing, so we'd need an unbounded amount —
/// represented here as `AssetAmount::MAX`.
fn required_collateral_with_slippage(
	total_owed_usd: AssetAmount,
	max_oracle_slippage_bps: BasisPoints,
) -> AssetAmount {
	let slippage_denom = ONE_AS_BASIS_POINTS.saturating_sub(max_oracle_slippage_bps) as u128;
	if slippage_denom == 0 {
		AssetAmount::MAX
	} else {
		multiply_by_rational_with_rounding(
			total_owed_usd,
			ONE_AS_BASIS_POINTS as u128,
			slippage_denom,
			Rounding::Up,
		)
		.unwrap_or(AssetAmount::MAX)
	}
}

/// Decide how much of each available collateral asset to pull into liquidation swaps.
/// `positions` is `(asset, available_amount, available_usd_value)` for each pool the
/// borrower has a positive supply position in. When the available collateral exceeds the
/// owed-plus-slippage target, each asset is drawn proportionally to its share of the
/// total available USD value (rounded up so the collected oracle value never falls short
/// of the per-asset target). Otherwise the function returns the full available amount of
/// each position.
fn compute_per_asset_liquidation_estimates(
	total_owed_usd: AssetAmount,
	max_oracle_slippage_bps: BasisPoints,
	positions: &[(Asset, AssetAmount, AssetAmount)],
) -> Vec<(Asset, AssetAmount)> {
	let total_available_usd =
		positions.iter().fold(0u128, |acc, (_, _, usd)| acc.saturating_add(*usd));

	if total_available_usd == 0 {
		return Vec::new();
	}

	let total_required_with_slippage_usd =
		required_collateral_with_slippage(total_owed_usd, max_oracle_slippage_bps);
	let take_all = total_required_with_slippage_usd >= total_available_usd;

	positions
		.iter()
		.map(|&(asset, available_amount, _)| {
			// In the proportional branch we know `total_required_with_slippage_usd <
			// total_available_usd` (otherwise `take_all` would be true), so the rounded-up
			// per-asset share never exceeds `available_amount` mathematically — but we
			// still cap with `min` to be safe against rounding overflow.
			let amount = if take_all {
				available_amount
			} else {
				core::cmp::min(
					multiply_by_rational_with_rounding(
						available_amount,
						total_required_with_slippage_usd,
						total_available_usd,
						Rounding::Up,
					)
					.unwrap_or(available_amount),
					available_amount,
				)
			};
			(asset, amount)
		})
		.filter(|(_, amount)| *amount > 0)
		.collect()
}

#[cfg(test)]
mod liquidation_math_tests {
	use super::*;

	const ETH: Asset = Asset::Eth;
	const BTC: Asset = Asset::Btc;
	const SOL: Asset = Asset::Sol;
	const USDC: Asset = Asset::Usdc;

	#[test]
	fn required_collateral_with_zero_slippage_equals_owed() {
		assert_eq!(required_collateral_with_slippage(1_000, 0), 1_000);
	}

	#[test]
	fn required_collateral_with_slippage_inflates_with_ceil_rounding() {
		// 1_000 / (1 - 0.005) = 1_005.025... -> 1_006 (ceil).
		assert_eq!(required_collateral_with_slippage(1_000, 50), 1_006);
		// 1_000 / (1 - 0.05) = 1_052.63... -> 1_053 (ceil).
		assert_eq!(required_collateral_with_slippage(1_000, 500), 1_053);
	}

	#[test]
	fn required_collateral_saturates_when_slippage_is_total_loss() {
		// 100% slippage means any swap could return zero, so no finite amount is enough.
		assert_eq!(required_collateral_with_slippage(1_000, ONE_AS_BASIS_POINTS), AssetAmount::MAX);
		// Above 100% saturates the same way.
		assert_eq!(
			required_collateral_with_slippage(1_000, ONE_AS_BASIS_POINTS + 1_000),
			AssetAmount::MAX
		);
	}

	#[test]
	fn empty_positions_give_no_takes() {
		assert!(compute_per_asset_liquidation_estimates(1_000, 50, &[]).is_empty());
	}

	#[test]
	fn single_asset_with_excess_takes_only_required_with_buffer() {
		// Owed 1_000 USD, slippage 0.5%, available 10_000 USD worth of ETH (10_000 ETH @ $1).
		// Required = ceil(1_000 / 0.995) = 1_006. ETH price is 1, so amount equals USD.
		assert_eq!(
			compute_per_asset_liquidation_estimates(1_000, 50, &[(ETH, 10_000, 10_000)]),
			vec![(ETH, 1_006)]
		);
	}

	#[test]
	fn single_asset_with_deficit_takes_everything() {
		// Owed 1_000 USD, slippage 0.5% → required 1_006 USD. Only 800 USD available.
		assert_eq!(
			compute_per_asset_liquidation_estimates(1_000, 50, &[(ETH, 800, 800)]),
			vec![(ETH, 800)]
		);
	}

	#[test]
	fn at_required_threshold_takes_all() {
		// Exactly enough collateral (in USD) to cover the slippage-buffered owed amount.
		assert_eq!(
			compute_per_asset_liquidation_estimates(1_000, 50, &[(ETH, 1_006, 1_006)]),
			vec![(ETH, 1_006)]
		);
	}

	#[test]
	fn zero_owed_takes_nothing() {
		assert!(compute_per_asset_liquidation_estimates(0, 50, &[(ETH, 1_000, 1_000)]).is_empty());
	}

	#[test]
	fn two_assets_with_equal_usd_split_evenly() {
		// Owed 1_000 USD, slippage 0%, available: 100 ETH @ $5 and 50 BTC @ $10 = $500 each.
		// target = 1_000, total_avail = 1_000 → take everything.
		assert_eq!(
			compute_per_asset_liquidation_estimates(1_000, 0, &[(ETH, 100, 500), (BTC, 50, 500)]),
			vec![(ETH, 100), (BTC, 50)]
		);
	}

	#[test]
	fn two_assets_drawn_proportionally_to_usd_share() {
		// ETH: 100 units @ $1 = $100 USD. SOL: 200 units @ $5 = $1000 USD. Total = $1100.
		// Owed 550 USD, slippage 0% → target = 550. ETH share = 100/1100 ≈ 9.09%, SOL = 90.91%.
		// ETH take = ceil(100 * 550 / 1100) = 50. SOL take = ceil(200 * 550 / 1100) = 100.
		assert_eq!(
			compute_per_asset_liquidation_estimates(550, 0, &[(ETH, 100, 100), (SOL, 200, 1_000)]),
			vec![(ETH, 50), (SOL, 100)]
		);
	}

	#[test]
	fn three_assets_proportional_split_with_slippage_buffer() {
		// Total available: 1_000 ETH ($2_000) + 500 SOL ($500) + 1_000_000 USDC ($1_000_000) =
		// $1_002_500. Owed 500_000 USD, slippage 0.5% → required = ceil(500_000 / 0.995) =
		// 502_513. Per-asset: ETH = ceil(1_000 * 502_513 / 1_002_500) ≈ 502, SOL = ceil(500 *
		// 502_513 / 1_002_500) ≈ 251, USDC = ceil(1_000_000 * 502_513 / 1_002_500) ≈ 501_260.
		assert_eq!(
			compute_per_asset_liquidation_estimates(
				500_000,
				50,
				&[(ETH, 1_000, 2_000), (SOL, 500, 500), (USDC, 1_000_000, 1_000_000)],
			),
			vec![(ETH, 502), (SOL, 251), (USDC, 501_260)]
		);
	}

	#[test]
	fn rounds_up_so_collected_usd_meets_target() {
		// ETH and SOL each contribute $50 USD with awkward unit counts that force rounding.
		// Owed 33 USD, slippage 0% → target 33 USD, total avail 100 USD. Each share = 33/2 USD
		// rounded up. ETH: ceil(7 * 33 / 100) = ceil(2.31) = 3. SOL: ceil(13 * 33 / 100) =
		// ceil(4.29) = 5. The sum (8 + leftover from rounding) covers the target.
		let takes = compute_per_asset_liquidation_estimates(33, 0, &[(ETH, 7, 50), (SOL, 13, 50)]);
		assert_eq!(takes, vec![(ETH, 3), (SOL, 5)]);
	}

	#[test]
	fn deficit_in_some_assets_still_takes_all_when_total_required_exceeds_available() {
		// Required 100_000 USD with 0% slippage but only $300 + $200 = $500 available → take all.
		assert_eq!(
			compute_per_asset_liquidation_estimates(100_000, 0, &[(ETH, 30, 300), (SOL, 100, 200)],),
			vec![(ETH, 30), (SOL, 100)]
		);
	}

	#[test]
	fn extreme_slippage_forces_take_all() {
		// 99.5% slippage means required ≈ 200x owed → almost certainly exceeds available.
		assert_eq!(
			compute_per_asset_liquidation_estimates(100, 9_950, &[(ETH, 50, 50)]),
			vec![(ETH, 50)]
		);
	}
}

/// Check collateralisation ratio (triggering/aborting liquidations if necessary) and
/// periodically swap collected fees into each pool's desired asset.
pub fn lending_upkeep<T: Config>(current_block: BlockNumberFor<T>) -> Weight {
	let config = LendingConfig::<T>::get();
	let mut weight_used = T::DbWeight::get().reads(1);

	let price_cache = OraclePriceCache::<T>::default();

	// Collecting keys to avoid undefined behaviour in `StorageMap`
	for borrower_id in LoanAccounts::<T>::iter_keys()
		.inspect(|_| weight_used += T::DbWeight::get().reads(1))
		.collect::<Vec<_>>()
		.iter()
	{
		// Not being able to process a loan account is acceptable (expected when oracle
		// prices are down).
		let _ = LoanAccounts::<T>::try_mutate_exists(borrower_id, |maybe_account| {
			let loan_account = maybe_account.as_mut().expect("Using keys read just above");

			loan_account.derive_and_charge_interest(&mut weight_used);

			// Some of these may fail due to oracle prices being unavailable, but that's
			// OK and doesn't need any specific error handling (they will simply be re-tried
			// at a later point).
			weight_used.saturating_accrue(T::WeightInfo::derive_ltv());
			let result: DispatchResult =
				loan_account.derive_ltv(&price_cache).map_err(Into::into).and_then(|ltv| {
					loan_account.check_low_ltv_penalty_and_collect_interest(
						ltv,
						&price_cache,
						&config,
						&mut weight_used,
					);

					loan_account.update_liquidation_status(
						borrower_id,
						ltv,
						&price_cache,
						&mut weight_used,
					)
				});

			// If all loans are repaid manually while in liquidation, liquidation swaps will be
			// aborted above and we need to delete the account:
			if loan_account.loans.is_empty() &&
				loan_account.liquidation_status == LiquidationStatus::NoLiquidation
			{
				*maybe_account = None;
			}

			result
		});
	}

	// Swap fees in every asset every fee_swap_interval_blocks, but only if they exceed
	// fee_swap_threshold_usd in value
	if current_block % config.fee_swap_interval_blocks.into() == 0u32.into() {
		// Swap all network fee contributions from fees:
		for asset in PendingNetworkFees::<T>::iter_keys()
			.inspect(|_| weight_used += T::DbWeight::get().reads(1))
			.collect::<Vec<_>>()
		{
			PendingNetworkFees::<T>::mutate(asset, |fee_amount| {
				// NOTE: if asset is FLIP, we shouldn't need to swap, but it should still work,
				// and it seems easiest to not write a special case
				weight_used.saturating_accrue(
					T::WeightInfo::usd_value_of().saturating_add(T::DbWeight::get().reads(1)),
				);
				let Ok(fee_usd_value) = price_cache.usd_value_of(asset, *fee_amount) else {
					// Don't swap yet if we can't determine asset's price
					return;
				};

				if fee_usd_value >= config.fee_swap_threshold_usd {
					initiate_network_fee_swap::<T>(asset, *fee_amount);
					*fee_amount = 0;

					weight_used.saturating_accrue(
						T::WeightInfo::initiate_network_fee_swap()
							.saturating_add(T::DbWeight::get().writes(1)),
					);
				}
			});
		}
	}

	weight_used
}

pub(super) fn initiate_network_fee_swap<T: Config>(asset: Asset, fee_amount: AssetAmount) {
	let swap_request_id = T::SwapRequestHandler::init_network_fee_swap_request(asset, fee_amount);

	Pallet::<T>::deposit_event(Event::LendingNetworkFeeSwapInitiated { swap_request_id });
}

/// Draws `principal` from the lending pool and records the origination fees against the loan.
///
/// The total amount owed (`loan.owed_principal`) is increased by `principal +
/// origination_fee_pool + origination_fee_network`. The pool fee stays in the pool (increasing
/// lenders' share), while the network fee is immediately credited to the network.
///
/// Emits [`Event::OriginationFeeTaken`].
///
/// Fails if the pool for `loan.asset` does not exist or has insufficient available funds.
pub fn fund_loan<T: Config>(
	loan: &mut GeneralLoan<T>,
	principal: AssetAmount,
	origination_fee_pool: AssetAmount,
	origination_fee_network: AssetAmount,
	price_cache: &OraclePriceCache<T>,
) -> Result<(), DispatchError> {
	let utilisation_cap = compute_utilisation_cap::<T>(
		loan.asset,
		LendingConfig::<T>::get().liquidation_coverage_factor,
		price_cache,
	)?;

	GeneralLendingPools::<T>::try_mutate(loan.asset, |pool| {
		let pool = pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

		pool.provide_funds_for_loan(principal).map_err(Error::<T>::from)?;

		ensure!(pool.get_utilisation() <= utilisation_cap, Error::<T>::UtilisationCapExceeded);

		pool.record_pool_fee(origination_fee_pool);

		let network_fee_collected = pool.record_and_collect_network_fee(origination_fee_network);

		Pallet::<T>::credit_fees_to_network(loan.asset, network_fee_collected);

		Ok::<_, DispatchError>(())
	})?;

	loan.owed_principal.saturating_accrue(principal);
	loan.owed_principal
		.saturating_accrue(origination_fee_pool.saturating_add(origination_fee_network));

	Pallet::<T>::deposit_event(Event::OriginationFeeTaken {
		loan_id: loan.id,
		pool_fee: origination_fee_pool,
		network_fee: origination_fee_network,
		// TODO: add support for broker fees (see https://linear.app/chainflip/issue/PRO-2851/decide-whether-to-support-broker-fees-from-origination-and-liquidation)
		broker_fee: 0,
	});

	Ok(())
}

impl<T: Config> LendingApi for Pallet<T> {
	type AccountId = T::AccountId;

	/// Create a new loan (assigning a new loan id) provided that the account's existing collateral
	/// is sufficient.
	#[transactional]
	fn new_loan(
		borrower_id: T::AccountId,
		asset: Asset,
		amount_to_borrow: AssetAmount,
		broker: Option<Beneficiary<T::AccountId>>,
	) -> Result<LoanId, DispatchError> {
		T::LpRegistrationApi::ensure_has_refund_address_for_asset(&borrower_id, asset)
			.map_err(|_| Error::<T>::NoRefundAddressSet)?;

		if let Some(broker) = &broker {
			ensure!(broker.bps <= MAX_BROKER_FEE_BPS, Error::<T>::BrokerFeeTooHigh);
			ensure!(broker.bps > 0, Error::<T>::InvalidZeroBrokerFee);
			ensure!(
				T::AccountRoleRegistry::has_account_role(&broker.account, AccountRole::Broker),
				Error::<T>::UnknownBroker
			);
		}

		let price_cache = OraclePriceCache::<T>::default();

		let config = LendingConfig::<T>::get();
		ensure!(
			amount_to_borrow >=
				price_cache.amount_from_usd_value(asset, config.minimum_loan_amount_usd)?,
			Error::<T>::AmountBelowMinimum
		);

		// Creating a loan with 0 principal first, then using `expand_loan_inner` to update it
		let loan = create_new_loan::<T>(asset, broker);
		let loan_id = loan.id;

		LoanAccounts::<T>::mutate(&borrower_id, |maybe_account| {
			let account = maybe_account.get_or_insert(LoanAccount::new(borrower_id.clone()));

			// NOTE: it is important that this event is emitted before `OriginationFeeTaken` event
			Self::deposit_event(Event::LoanCreated {
				loan_id,
				loan_type: LoanType::User(borrower_id.clone()),
				asset,
				principal_amount: amount_to_borrow,
			});

			account.expand_loan_inner(loan, amount_to_borrow, &price_cache)?;

			Ok::<_, DispatchError>(())
		})?;

		// Borrowing more raises this account's total_loans_usd, which lowers the utilisation
		// cap of every pool whose asset the account holds as collateral. Reject the loan if
		// any of those caps would drop below the pool's current utilisation. We check this
		// after the loan account is committed so `compute_utilisation_cap` sees the new
		// principal directly; the surrounding `#[transactional]` rolls back on failure.
		check_collateral_pool_caps::<T>(&borrower_id, &price_cache)?;

		Ok(loan_id)
	}

	/// Borrows `extra_amount_to_borrow` by expanding `loan_id`. Adds any extra collateral to the
	/// account (which may be required to cover the new total owed amount). The extra amount to
	/// borrow must be above the minimum update amount.
	#[transactional]
	fn expand_loan(
		borrower_id: Self::AccountId,
		loan_id: LoanId,
		extra_amount_to_borrow: AssetAmount,
	) -> Result<(), DispatchError> {
		let price_cache = OraclePriceCache::<T>::default();

		LoanAccounts::<T>::mutate(&borrower_id, |maybe_account| {
			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanNotFound)?;

			let loan = loan_account.loans.remove(&loan_id).ok_or(Error::<T>::LoanNotFound)?;

			Self::deposit_event(Event::LoanUpdated {
				loan_id,
				extra_principal_amount: extra_amount_to_borrow,
			});

			let config = LendingConfig::<T>::get();
			ensure!(
				extra_amount_to_borrow >=
					price_cache.amount_from_usd_value(
						loan.asset,
						config.minimum_update_loan_amount_usd
					)?,
				Error::<T>::AmountBelowMinimum
			);

			loan_account.expand_loan_inner(loan, extra_amount_to_borrow, &price_cache)?;

			Ok::<_, DispatchError>(())
		})?;

		check_collateral_pool_caps::<T>(&borrower_id, &price_cache)?;

		Ok(())
	}

	/// Repays (fully or partially) a loan. Must be left above the minimum loan amount and the
	/// repayment amount must be at least the minimum update amount, unless it's a full repayment.
	#[transactional]
	fn try_making_repayment(
		borrower_id: &T::AccountId,
		loan_id: LoanId,
		repayment_amount: RepaymentAmount,
	) -> Result<(), DispatchError> {
		let price_cache = OraclePriceCache::<T>::default();
		LoanAccounts::<T>::mutate(borrower_id, |maybe_account| {
			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanNotFound)?;

			let Some(loan) = loan_account.loans.get_mut(&loan_id) else {
				fail!(Error::<T>::LoanNotFound);
			};

			loan.collect_pending_interest();

			let config = LendingConfig::<T>::get();

			let loan_asset = loan.asset;

			let repayment_amount = match repayment_amount {
				RepaymentAmount::Full => loan.owed_principal,
				RepaymentAmount::Exact(amount) => {
					if amount < loan.owed_principal {
						ensure!(
							price_cache.usd_value_of_allow_stale(loan_asset, amount)? >=
								config.minimum_update_loan_amount_usd,
							Error::<T>::AmountBelowMinimum
						);
					}

					amount
				},
			};

			T::Balance::try_debit_account(borrower_id, loan_asset, repayment_amount)?;

			let excess_amount =
				loan.repay_principal(repayment_amount, LoanRepaidActionType::Manual);

			if excess_amount > 0 {
				T::Balance::credit_account(borrower_id, loan_asset, excess_amount);
			}

			if loan.owed_principal == 0 {
				loan_account.settle_loan(loan_id, false /* not via liquidation */);
			} else {
				ensure!(
					price_cache.usd_value_of_allow_stale(loan.asset, loan.owed_principal)? >=
						config.minimum_loan_amount_usd,
					Error::<T>::RemainingAmountBelowMinimum
				);
			}

			// Only remove account if it has no ongoing liquidation swaps (it will be removed
			// after the liquidation swaps have been aborted as a result of having no debt).
			if loan_account.loans.is_empty() &&
				loan_account.liquidation_status == LiquidationStatus::NoLiquidation
			{
				*maybe_account = None;
			}

			Ok::<_, DispatchError>(())
		})
	}

	fn set_voluntary_liquidation_flag(borrower_id: Self::AccountId, value: bool) -> DispatchResult {
		LoanAccounts::<T>::try_mutate(&borrower_id, |maybe_account| {
			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanAccountNotFound)?;

			ensure!(!loan_account.loans.is_empty(), Error::<T>::AccountHasNoLoans);

			loan_account.voluntary_liquidation_requested = value;
			Ok(())
		})
	}
}

#[transactional]
pub fn remove_lender_funds<T: Config>(
	lender_id: T::AccountId,
	asset: Asset,
	amount: Option<AssetAmount>,
) -> DispatchResult {
	let config = LendingConfig::<T>::get();

	let price_cache = OraclePriceCache::<T>::default();

	if let Some(amount) = amount {
		ensure!(
			price_cache.usd_value_of_allow_stale(asset, amount)? >=
				config.minimum_update_supply_amount_usd,
			Error::<T>::AmountBelowMinimum
		);
	}

	// Check if account also has loans: if it does, check "ltv surplus", i.e. the max amount
	// that can be withdrawn without LTV spiking above the target threshold.
	let ltv_surplus_asset = if let Some(loan_account) = LoanAccounts::<T>::get(&lender_id) {
		ensure!(
			loan_account.liquidation_status == LiquidationStatus::NoLiquidation,
			Error::<T>::LiquidationInProgress
		);

		let surplus_usd = match loan_account
			.calculate_collateral_surplus_or_deficit(config.ltv_thresholds.target, &price_cache)?
		{
			SurplusOrDeficit::Surplus(amount) => amount,
			SurplusOrDeficit::Deficit => 0,
		};

		Some(price_cache.amount_from_usd_value(asset, surplus_usd)?)
	} else {
		None
	};

	let unlocked_amount = GeneralLendingPools::<T>::try_mutate(asset, |maybe_pool| {
		let pool = maybe_pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

		// Adjust the amount taking into account how much we are limited by LTV
		let amount = match (amount, ltv_surplus_asset) {
			(Some(amount), Some(ltv_surplus)) => Some(core::cmp::min(amount, ltv_surplus)),
			_ => ltv_surplus_asset.or(amount),
		};

		if let Some(amount) = amount {
			ensure!(
				price_cache.usd_value_of(asset, amount)? >= config.minimum_update_supply_amount_usd,
				Error::<T>::InsufficientLtvHeadroom
			);
		}

		let WithdrawnAndRemainingAmounts { withdrawn_amount, remaining_amount } =
			pool.remove_funds(&lender_id, amount).map_err(Error::<T>::from)?;

		// Either the user removes everything, or they have to leave at least
		// the minimum required amount in the pool (to prevent dust amounts from
		// accumulating):
		ensure!(
			remaining_amount == 0 ||
				price_cache.usd_value_of_allow_stale(asset, remaining_amount)? >=
					config.minimum_supply_amount_usd,
			Error::<T>::RemainingAmountBelowMinimum
		);

		Ok::<_, DispatchError>(withdrawn_amount)
	})?;

	T::Balance::credit_account(&lender_id, asset, unlocked_amount);

	Pallet::<T>::deposit_event(Event::<T>::LendingFundsRemoved {
		lender_id,
		asset,
		unlocked_amount,
		action_type: SupplyRemovedActionType::Manual,
	});

	Ok(())
}

/// Computes the maximum utilisation ratio allowed for `asset`'s lending pool such that
/// enough of the pool's asset is still available to fully liquidate `coverage_factor`
/// of all outstanding loans at current oracle prices.
///
/// For each loan account, the share of its collateral (by USD value) held in `asset` is
/// multiplied by `coverage_factor * total_loans_usd` (capped by total collateral USD
/// value for undercollateralised accounts). The sum across accounts is the amount of
/// `asset` that may need to be released from the pool during liquidation; the cap is
/// `1 - required / pool.total_amount`, saturating at zero.
///
/// Returns `Permill::one()` if the pool does not exist or is empty. Allows stale oracle
/// prices (since we still want the cap to hold during price outages), and propagates
/// an error only when a price is completely unavailable.
pub fn compute_utilisation_cap<T: Config>(
	asset: Asset,
	coverage_factor: Percent,
	price_cache: &OraclePriceCache<T>,
) -> Result<Permill, DispatchError> {
	let Some(pool) = GeneralLendingPools::<T>::get(asset) else {
		fail!(Error::<T>::PoolDoesNotExist);
	};

	if pool.total_amount == 0 {
		return Ok(Permill::one());
	}

	// For each loan account, compute the amount of `asset` needed to cover `coverage_factor`
	// of its loans. When the account holds multiple collateral assets, we attribute the
	// required amount in proportion to each asset's USD share of the collateral.
	let total_required_amount_across_accounts = LoanAccounts::<T>::iter().try_fold(
		0 as AssetAmount,
		|acc, (_borrower_id, loan_account)| {
			let collateral = loan_account.get_total_collateral();
			let collateral_in_asset = collateral.get(&asset).copied().unwrap_or_default();

			let total_collateral_usd = price_cache.total_usd_value_of_allow_stale(&collateral)?;
			if total_collateral_usd == 0 {
				return Ok(acc);
			}

			let total_loans_usd =
				loan_account.loans.values().try_fold(0 as AssetAmount, |sum, loan| {
					price_cache
						.usd_value_of_allow_stale(loan.asset, loan.owed_principal)
						.map(|usd| sum.saturating_add(usd))
				})?;

			// For undercollateralised accounts we can extract at most the full collateral value.
			let target_liquidation_usd =
				core::cmp::min(coverage_factor * total_loans_usd, total_collateral_usd);

			let required_in_asset = multiply_by_rational_with_rounding(
				collateral_in_asset,
				target_liquidation_usd,
				total_collateral_usd,
				Rounding::Up,
			)
			.unwrap_or(u128::MAX);

			Ok::<_, DispatchError>(acc.saturating_add(required_in_asset))
		},
	)?;

	let required_fraction =
		Permill::from_rational(total_required_amount_across_accounts, pool.total_amount);

	Ok(Permill::one().saturating_sub(required_fraction))
}

/// Checks that, for every pool in which `borrower_id` holds collateral, the pool's current
/// utilisation does not exceed the (post-borrow) liquidation-coverage cap. Intended to be
/// called after the loan account has been committed to storage so that
/// [`compute_utilisation_cap`] sees the new principal directly.
fn check_collateral_pool_caps<T: Config>(
	borrower_id: &T::AccountId,
	price_cache: &OraclePriceCache<T>,
) -> DispatchResult {
	let Some(loan_account) = LoanAccounts::<T>::get(borrower_id) else {
		return Ok(());
	};

	let coverage_factor = LendingConfig::<T>::get().liquidation_coverage_factor;

	for collateral_asset in loan_account.get_total_collateral().into_keys() {
		let cap = compute_utilisation_cap::<T>(collateral_asset, coverage_factor, price_cache)?;

		let pool_utilisation = GeneralLendingPools::<T>::get(collateral_asset)
			.ok_or(Error::<T>::PoolDoesNotExist)?
			.get_utilisation();

		ensure!(pool_utilisation <= cap, Error::<T>::CollateralPoolUtilisationCapExceeded);
	}

	Ok(())
}

pub fn create_new_loan<T: Config>(
	asset: Asset,
	broker: Option<Beneficiary<T::AccountId>>,
) -> GeneralLoan<T> {
	let loan_id = NextLoanId::<T>::get();
	NextLoanId::<T>::set(loan_id + 1);

	GeneralLoan {
		id: loan_id,
		asset,
		created_at_block: frame_system::Pallet::<T>::current_block_number(),
		owed_principal: 0,
		pending_interest: Default::default(),
		broker,
	}
}

impl<T: Config> cf_traits::lending::LendingSystemApi for Pallet<T> {
	type AccountId = T::AccountId;

	/// Called when one of liquidation swaps completes. Liquidation status on loan account
	/// keeps track of all current liquidation swaps for each loan id. If the swap is the last
	/// one for a give loan id, we check if the loan has been fully repaid: if so, we "settle"
	/// it.
	fn process_loan_swap_outcome(
		swap_request_id: SwapRequestId,
		swap_type: LendingSwapType<Self::AccountId>,
		output_amount: AssetAmount,
	) {
		match swap_type {
			LendingSwapType::Liquidation { borrower_id, loan_id } => {
				LoanAccounts::<T>::mutate_exists(&borrower_id, |maybe_account| {
					let Some(loan_account) = maybe_account else {
						log_or_panic!("Loan account does not exist for {borrower_id:?}");
						return;
					};

					// See if the liquidation is voluntary to determine whether
					// liquidation fee should be paid:
					let is_voluntary = match &loan_account.liquidation_status {
						LiquidationStatus::Liquidating { liquidation_type, .. } =>
							*liquidation_type == LiquidationType::SoftVoluntary,
						LiquidationStatus::NoLiquidation => {
							log_or_panic!("Liquidation swap completed in no-liquidation state");
							false
						},
					};

					let mut is_last_liquidation_swap_for_loan = false;

					let liquidation_swap = match &mut loan_account.liquidation_status {
						LiquidationStatus::NoLiquidation => {
							log_or_panic!(
								"Unexpected liquidation (swap request id: {swap_request_id}, loan_id: {loan_id})"
							);
							return;
						},
						LiquidationStatus::Liquidating { liquidation_swaps, .. } => {
							if let Some(swap) = liquidation_swaps.remove(&swap_request_id) {
								if liquidation_swaps
									.values()
									.filter(|swap| swap.loan_id == loan_id)
									.count() == 0
								{
									is_last_liquidation_swap_for_loan = true;
								}

								if liquidation_swaps.is_empty() {
									loan_account.liquidation_status =
										LiquidationStatus::NoLiquidation;

									Pallet::<T>::deposit_event(Event::LiquidationCompleted {
										borrower_id: borrower_id.clone(),
										reason: LiquidationCompletionReason::FullySwapped,
									});
								}

								swap
							} else {
								log_or_panic!(
									"Unable to find liquidation swap (swap request id: {swap_request_id}) for loan_id: {loan_id})"
								);
								return;
							}
						},
					};

					let excess_amount = match loan_account
						.get_loan_and_check_asset(loan_id, liquidation_swap.to_asset)
					{
						// NOTE: this might fully repaid the loan, but we don't want to settle
						// the loan just yet as there may be more liquidation swaps to process for
						// the loan.
						Some(loan) =>
							loan.repay_via_liquidation(output_amount, swap_request_id, is_voluntary),
						None => {
							// In some cases it may be possible for the loan to no longer exist if
							// e.g. the principal was fully covered by a prior liquidation swap or
							// by user repaying it manually.
							output_amount
						},
					};

					// Any amount left after repaying the loan is added to the borrower's
					// collateral balance:
					loan_account.supply_from_liquidation(
						liquidation_swap.to_asset,
						excess_amount,
						SupplyAddedActionType::SystemLiquidationExcessAmount {
							loan_id,
							swap_request_id,
						},
					);

					let no_collateral_left =
						is_zero_collateral(&loan_account.get_collateral_in_supply_pools());

					// If this swap is the last liquidation swap for the loan, we should
					// "settle" it if it has been fully repaid:
					if is_last_liquidation_swap_for_loan {
						if let Some(loan) = loan_account.loans.get(&loan_id) {
							if loan.owed_principal == 0 {
								loan_account.settle_loan(loan_id, true /* via liquidation */);
							}
						}

						// If all liquidation swaps have finished and we have no collateral left,
						// write off any remaining loans:
						if loan_account.liquidation_status == LiquidationStatus::NoLiquidation &&
							no_collateral_left
						{
							let unrecoverable: Vec<LoanId> =
								loan_account.loans.keys().copied().collect();
							for loan_id in unrecoverable {
								loan_account.settle_loan(loan_id, true /* via liquidation */);
							}
						}

						if loan_account.loans.is_empty() &&
							loan_account.liquidation_status == LiquidationStatus::NoLiquidation
						{
							*maybe_account = None;
						}
					}
				});
			},
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Pays fee to the pool in the pool's asset.
	fn credit_fees_to_pool(loan_asset: Asset, fee_amount: AssetAmount) {
		Pallet::<T>::mutate_existing_pool(loan_asset, |pool| {
			pool.receive_fees_in_pools_asset(fee_amount);
		});
	}

	pub fn credit_fees_to_network(fee_asset: Asset, fee_amount: AssetAmount) {
		PendingNetworkFees::<T>::mutate(fee_asset, |pending_amount| {
			pending_amount.saturating_accrue(fee_amount);
		});
	}

	/// Mutates the pool for `asset` expecting it to exist. If the pool is missing the
	/// closure is skipped and `None` is returned.
	fn mutate_existing_pool<R, F>(asset: Asset, f: F) -> Option<R>
	where
		F: FnOnce(&mut LendingPool<T::AccountId>) -> R,
	{
		GeneralLendingPools::<T>::mutate(asset, |maybe_pool| {
			if let Some(pool) = maybe_pool.as_mut() {
				Some(f(pool))
			} else {
				log_or_panic!("Lending Pool must exist for asset {}", asset);
				None
			}
		})
	}
}

pub fn is_zero_collateral(collateral: &BTreeMap<Asset, AssetAmount>) -> bool {
	collateral.values().all(|amount| *amount == 0)
}
