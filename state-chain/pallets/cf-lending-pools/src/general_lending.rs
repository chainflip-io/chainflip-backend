use cf_amm_math::{invert_price, Price};
use cf_primitives::{DcaParameters, SwapRequestId};
use cf_traits::{ExpiryBehaviour, LendingSwapType, PriceLimitsAndExpiry};
use core_lending_pool::ScaledAmountHP;
use frame_support::{
	fail,
	sp_runtime::{FixedI64, FixedPointNumber, FixedU64, PerThing},
};

use super::*;

#[cfg(test)]
mod general_lending_tests;

mod general_lending_pool;

pub use general_lending_pool::LendingPool;

pub enum LoanRepaymentOutcome {
	// In case of full repayment, we may have some excess amount left
	// over which the caller of `repay_loan` will need to allocate somewhere
	// (likely return to the borrower).
	FullyRepaid { excess_amount: AssetAmount },
	PartiallyRepaid,
}

/// Helps to link swap id in liquidation status to loan id
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct LiquidationSwap {
	loan_id: LoanId,
	from_asset: Asset,
	to_asset: Asset,
}

/// Whether the account's collateral is being liquidated (and if so, stores ids of liquidation
/// swaps)
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum LiquidationStatus {
	NoLiquidation,
	Liquidating { liquidation_swaps: BTreeMap<SwapRequestId, LiquidationSwap>, is_hard: bool },
}

/// High precision interest amounts broken down by type
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Default)]
struct InterestBreakdown {
	network: ScaledAmountHP,
	pool: ScaledAmountHP,
	broker: ScaledAmountHP,
	low_ltv_penalty: ScaledAmountHP,
}

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct LoanAccount<T: Config> {
	borrower_id: T::AccountId,
	primary_collateral_asset: Asset,
	collateral: BTreeMap<Asset, AssetAmount>,
	loans: BTreeMap<LoanId, GeneralLoan<T>>,
	liquidation_status: LiquidationStatus,
}

impl<T: Config> LoanAccount<T> {
	pub fn new(borrower_id: T::AccountId, primary_collateral_asset: Asset) -> Self {
		Self {
			borrower_id,
			primary_collateral_asset,
			collateral: BTreeMap::new(),
			loans: BTreeMap::new(),
			liquidation_status: LiquidationStatus::NoLiquidation,
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

	/// Returns the account's collateral including any amounts that are in liquidation swaps.
	pub fn get_total_collateral(&self) -> BTreeMap<Asset, AssetAmount> {
		// Note that in order to keep things simple we don't guarantee that all of the
		// all collateral is being liquidated (e.g. it is possible for the user to top
		// up collateral during liquidation in which case we currently don't update the
		// liquidation swaps), but we *do* include any collateral sitting in the account
		// when determining account's collateralisation ratio.

		// Start with any collateral that may be sitting in the account:
		let mut total_collateral = self.collateral.clone();

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

	/// Adds collateral to the account from borrower's free balance as long as it's enabled by
	/// safe mode.
	fn try_adding_collateral_from_free_balance(
		&mut self,
		collateral: &BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
		for (asset, amount) in collateral {
			ensure!(
				T::SafeMode::get().add_collateral.enabled(asset),
				Error::<T>::AddingCollateralDisabled
			);
			T::Balance::try_debit_account(&self.borrower_id, *asset, *amount)?;

			self.add_to_collateral(*asset, *amount);
		}

		Ok(())
	}

	/// Helper function that makes sure the collateral is added correctly (not overriding existing
	/// amounts and creating entries for new assets if necessary)
	fn add_to_collateral(&mut self, asset: Asset, amount: AssetAmount) {
		self.collateral.entry(asset).or_default().saturating_accrue(amount);
	}

	/// Computes account's total collateral value in USD, including what's in liquidation swaps.
	pub fn total_collateral_usd_value(&self) -> Result<AssetAmount, Error<T>> {
		self.get_total_collateral()
			.iter()
			.map(|(asset, amount)| usd_value_of::<T>(*asset, *amount))
			.try_fold(0u128, |acc, x| Ok(acc.saturating_add(x?)))
	}

	pub fn update_liquidation_status(&mut self, borrower_id: &T::AccountId, ltv: FixedU64) {
		let config = LendingConfig::<T>::get();

		// This will saturate at 100%, but that's good enough (none of our thresholds exceed 100%):
		let ltv: Permill = ltv.into_clamped_perthing();

		// Every time we transition from a liquidating state we abort all liquidation swaps
		// and repay any swapped into principal. If the next state is "NoLiquidation", the
		// collateral is returned into the loan account; if it is "Liquidating", the collateral
		// is used in the new liquidation swaps.
		match &mut self.liquidation_status {
			LiquidationStatus::NoLiquidation =>
				if ltv > config.ltv_thresholds.hard_liquidation {
					if let Ok(collateral) = self.prepare_collateral_for_liquidation() {
						self.init_liquidation_swaps(borrower_id, collateral, true);
					}
				} else if ltv > config.ltv_thresholds.soft_liquidation {
					if let Ok(collateral) = self.prepare_collateral_for_liquidation() {
						self.init_liquidation_swaps(borrower_id, collateral, false);
					}
				},
			LiquidationStatus::Liquidating { liquidation_swaps, is_hard } if *is_hard => {
				if ltv < config.ltv_thresholds.soft_liquidation_abort {
					// Transition from hard liquidation to "no liquidation":
					let swaps = core::mem::take(liquidation_swaps);
					self.abort_liquidation_swaps(&swaps);
					self.liquidation_status = LiquidationStatus::NoLiquidation;
				} else if ltv < config.ltv_thresholds.hard_liquidation_abort {
					// Transition from hard liquidation to soft liquidation:
					let swaps = core::mem::take(liquidation_swaps);
					self.abort_liquidation_swaps(&swaps);
					if let Ok(collateral) = self.prepare_collateral_for_liquidation() {
						self.init_liquidation_swaps(borrower_id, collateral, false);
					}
				}
			},
			LiquidationStatus::Liquidating { liquidation_swaps, .. } => {
				if ltv > config.ltv_thresholds.hard_liquidation {
					// Transition from soft liquidation to hard liquidation:
					let swaps = core::mem::take(liquidation_swaps);
					self.abort_liquidation_swaps(&swaps);
					if let Ok(collateral) = self.prepare_collateral_for_liquidation() {
						self.init_liquidation_swaps(borrower_id, collateral, true);
					}
				} else if ltv < config.ltv_thresholds.soft_liquidation_abort {
					// Transition from soft liquidation to "no liquidation":
					let swaps = core::mem::take(liquidation_swaps);
					self.abort_liquidation_swaps(&swaps);
					self.liquidation_status = LiquidationStatus::NoLiquidation;
				}
			},
		}
	}

	// Abort all provided liquidation swaps, repays any already swapped principal assets and
	// returns remaining collateral assets alongside the corresponding loan information.
	fn abort_liquidation_swaps(
		&mut self,
		liquidation_swaps: &BTreeMap<SwapRequestId, LiquidationSwap>,
	) {
		// It should be rare, but not impossible that a partial liquidation fully repays
		// the loan. We delay settling them until the end of this function to make sure that
		// all liquidations fees are correctly paid.
		let mut fully_repaid_loans = vec![];

		for (swap_request_id, LiquidationSwap { loan_id, from_asset, to_asset }) in
			liquidation_swaps
		{
			if let Some(swap_progress) = T::SwapRequestHandler::abort_swap_request(*swap_request_id)
			{
				let excess_amount = match self.get_loan_and_check_asset(*loan_id, *to_asset) {
					Some(loan) => {
						match loan.repay_principal(
							swap_progress.accumulated_output_amount,
							true, /* liquidation */
						) {
							LoanRepaymentOutcome::FullyRepaid { excess_amount } => {
								fully_repaid_loans.push(loan_id);
								excess_amount
							},
							LoanRepaymentOutcome::PartiallyRepaid => {
								// On partial repayment the full amount has been consumed.
								0
							},
						}
					},
					None => swap_progress.accumulated_output_amount,
				};

				if excess_amount > 0 {
					// In case we have liquidated more than necessary the excess amount
					// is added to the account's collateral balance:
					self.add_to_collateral(*to_asset, excess_amount);
				}

				// Any input funds not yet liquidated are returned to the
				// account's collateral balance:
				self.add_to_collateral(*from_asset, swap_progress.remaining_input_amount);
			} else {
				log_or_panic!("Failed to abort swap request: {swap_request_id}");
			}
		}

		for loan_id in fully_repaid_loans {
			self.settle_loan(*loan_id, true /* via liquidation */);
		}
	}

	/// Computes the total amount owed in account's loans in USD adjusting for the amount that will
	/// be repaid by the collateral that has already been swapped into the loan asset.
	pub fn total_owed_usd_value(&self) -> Result<AssetAmount, Error<T>> {
		let total_owed = self
			.loans
			.values()
			.map(|loan| loan.owed_principal_usd_value())
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
						total_principal_usd_value_in_swaps.saturating_accrue(usd_value_of::<T>(
							*to_asset,
							swap_progress.accumulated_output_amount,
						)?);
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
	pub fn derive_ltv(&self) -> Result<FixedU64, Error<T>> {
		let collateral = self.total_collateral_usd_value()?;
		let principal = self.total_owed_usd_value()?;

		if collateral == 0 {
			log_or_panic!("Account has no collateral: {:?}", self.borrower_id);
			return Err(Error::<T>::InsufficientCollateral);
		}

		Ok(FixedU64::from_rational(principal, collateral))
	}

	#[transactional]
	pub fn derive_and_charge_interest(&mut self, ltv: FixedU64) -> DispatchResult {
		let config = LendingConfig::<T>::get();

		if self.liquidation_status != LiquidationStatus::NoLiquidation {
			// For simplicity, we don't charge interest during liquidations
			return Ok(())
		}

		let current_block = frame_system::Pallet::<T>::block_number();

		for loan in self.loans.values_mut() {
			let blocks_since_last_payment: u32 = current_block
				.saturating_sub(loan.last_interest_payment_at)
				.try_into()
				.unwrap_or(u32::MAX);

			if blocks_since_last_payment >= config.interest_payment_interval_blocks {
				let loan_asset = loan.asset;

				let base_interest_rate = {
					let utilisation = GeneralLendingPools::<T>::get(loan_asset)
						.map(|pool| pool.get_utilisation())
						.unwrap_or_default();

					config.derive_base_interest_rate_per_payment_interval(
						loan_asset,
						utilisation,
						blocks_since_last_payment,
					)
				};

				let network_interest_rate = config
					.derive_network_interest_rate_per_payment_interval(blocks_since_last_payment);

				let low_ltv_penalty_rate = config.derive_low_ltv_penalty_rate_per_payment_interval(
					ltv,
					blocks_since_last_payment,
				);

				// Calculating interest in scaled amounts for better precision
				let owed_principal = ScaledAmountHP::from_asset_amount(loan.owed_principal);

				// Work out how much interest has accrued in loan's asset terms:
				let network_interest_amount = owed_principal * network_interest_rate;
				let low_ltv_penalty_amount = owed_principal * low_ltv_penalty_rate;
				let pool_interest_amount = owed_principal * base_interest_rate;

				// Record the accrued interest amounts. We may or may not charge these immediately
				// depending on whether the amounts exceed some threshold.
				loan.pending_interest.network.saturating_accrue(network_interest_amount);
				loan.pending_interest.pool.saturating_accrue(pool_interest_amount);
				loan.pending_interest.low_ltv_penalty.saturating_accrue(low_ltv_penalty_amount);

				loan.last_interest_payment_at = current_block;

				loan.charge_pending_interest_if_above_threshold(
					config.interest_collection_threshold_usd,
				)?;
			}
		}

		Ok(())
	}

	/// Checks if a top up is required and if so, performs it. Returns
	/// boolean indicating whether a topup has been performed.
	#[transactional]
	pub fn process_auto_top_up(
		&mut self,
		borrower_id: &T::AccountId,
		ltv: FixedU64,
	) -> Result<bool, DispatchError> {
		let config = LendingConfig::<T>::get();

		if ltv <= config.ltv_thresholds.topup.into() {
			return Ok(false)
		}

		let top_up_required_in_usd = {
			let loan_value_in_usd = self.total_owed_usd_value()?;

			let collateral_required_in_usd = FixedU64::from(config.ltv_thresholds.target)
				.reciprocal()
				.map(|ltv_inverted| ltv_inverted.saturating_mul_int(loan_value_in_usd))
				// This effectively disables auto top up if the ltv target erroneously set to 0:
				.unwrap_or(0);

			collateral_required_in_usd.saturating_sub(self.total_collateral_usd_value()?)
		};

		// Auto top up is currently only possible from the primary collateral asset
		let top_up_required_in_collateral_asset =
			amount_from_usd_value::<T>(self.primary_collateral_asset, top_up_required_in_usd)?;

		try_sweep::<T>(borrower_id);

		// Don't attempt to charge more than what's available:
		let top_up_amount = core::cmp::min(
			T::Balance::get_balance(borrower_id, self.primary_collateral_asset),
			top_up_required_in_collateral_asset,
		);

		if top_up_amount > 0 {
			T::Balance::try_debit_account(
				borrower_id,
				self.primary_collateral_asset,
				top_up_amount,
			)
			.inspect_err(|_| {
				log_or_panic!("Unable to debit after checking balance");
			})?;

			self.add_to_collateral(self.primary_collateral_asset, top_up_amount);

			Ok(true)
		} else {
			Ok(false)
		}
	}

	/// Split collateral proportionally to the usd value of each loan (to give each loan a fair
	/// chance of being liquidated without a loss) as a preparation step for liquidation. Returns
	/// error if oracle prices aren't available.
	fn prepare_collateral_for_liquidation(
		&mut self,
	) -> Result<Vec<AssetCollateralForLoan>, Error<T>> {
		let mut prepared_collateral = vec![];

		let principal_amounts_usd = self
			.loans
			.iter()
			.map(|(loan_id, loan)| {
				loan.owed_principal_usd_value()
					.map(|usd_value| ((*loan_id, loan.asset), usd_value))
			})
			.collect::<Result<Vec<_>, Error<T>>>()?;

		for (collateral_asset, collateral_amount) in core::mem::take(&mut self.collateral) {
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
	fn init_liquidation_swaps(
		&mut self,
		borrower_id: &T::AccountId,
		collateral: Vec<AssetCollateralForLoan>,
		is_hard: bool,
	) {
		let config = LendingConfig::<T>::get();

		let mut liquidation_swaps = BTreeMap::new();

		let mut swaps_for_event = BTreeMap::<LoanId, Vec<SwapRequestId>>::new();

		for AssetCollateralForLoan { loan_id, loan_asset, collateral_asset, collateral_amount } in
			collateral
		{
			let from_asset = collateral_asset;
			let to_asset = loan_asset;

			let max_slippage = if is_hard {
				config.hard_liquidation_max_oracle_slippage
			} else {
				config.soft_liquidation_max_oracle_slippage
			};

			let swap_request_id = initiate_swap::<T>(
				from_asset,
				collateral_amount,
				to_asset,
				LendingSwapType::Liquidation { borrower_id: borrower_id.clone(), loan_id },
				max_slippage,
			);

			swaps_for_event.entry(loan_id).or_default().push(swap_request_id);

			liquidation_swaps
				.insert(swap_request_id, LiquidationSwap { loan_id, from_asset, to_asset });
		}

		Pallet::<T>::deposit_event(Event::LiquidationInitiated {
			borrower_id: borrower_id.clone(),
			swaps: swaps_for_event,
			is_hard,
		});

		self.liquidation_status = LiquidationStatus::Liquidating { liquidation_swaps, is_hard };
	}

	fn settle_loan(&mut self, loan_id: LoanId, via_liquidation: bool) {
		if let Some(loan) = self.loans.remove(&loan_id) {
			if loan.owed_principal > 0 {
				Pallet::<T>::mutate_existing_pool(loan.asset, |pool| {
					pool.write_off_unrecoverable_debt(loan.owed_principal);
				});
			}

			Pallet::<T>::deposit_event(Event::LoanSettled {
				loan_id,
				outstanding_principal: loan.owed_principal,
				via_liquidation,
			});
		}
	}

	fn expand_loan_inner(
		&mut self,
		mut loan: GeneralLoan<T>,
		extra_principal: AssetAmount,
		extra_collateral: BTreeMap<Asset, AssetAmount>,
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

		self.try_adding_collateral_from_free_balance(&extra_collateral)?;

		if !extra_collateral.is_empty() {
			Pallet::<T>::deposit_event(Event::CollateralAdded {
				borrower_id: self.borrower_id.clone(),
				collateral: extra_collateral,
			});
		}

		// Will need to request this much more from the pool
		let origination_fee_total = config.origination_fee(loan.asset) * extra_principal;

		let origination_fee_network =
			config.network_fee_contributions.from_origination_fee * origination_fee_total;

		let origination_fee_pool = origination_fee_total.saturating_sub(origination_fee_network);

		GeneralLendingPools::<T>::try_mutate(loan_asset, |pool| {
			let pool = pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

			pool.provide_funds_for_loan(extra_principal).map_err(Error::<T>::from)?;

			pool.record_pool_fee(origination_fee_pool);

			let network_fee_collected =
				pool.record_and_collect_network_fee(origination_fee_network);

			Pallet::<T>::credit_fees_to_network(loan_asset, network_fee_collected);

			Ok::<_, DispatchError>(())
		})?;

		loan.owed_principal.saturating_accrue(extra_principal);
		loan.owed_principal.saturating_accrue(origination_fee_total);

		Pallet::<T>::deposit_event(Event::OriginationFeeTaken {
			loan_id: loan.id,
			pool_fee: origination_fee_pool,
			network_fee: origination_fee_network,
			// TODO: add support for broker fees
			broker_fee: 0,
		});

		self.loans.insert(loan.id, loan);

		if self.derive_ltv()? > config.ltv_thresholds.target.into() {
			return Err(Error::<T>::InsufficientCollateral.into());
		}

		T::Balance::credit_account(&self.borrower_id, loan_asset, extra_principal);

		Ok(())
	}
}

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct GeneralLoan<T: Config> {
	pub id: LoanId,
	pub asset: Asset,
	pub last_interest_payment_at: BlockNumberFor<T>,
	pub created_at_block: BlockNumberFor<T>,
	pub owed_principal: AssetAmount,
	/// Interest owed on the loan but not yet taken (it is below the threshold)
	pending_interest: InterestBreakdown,
}

impl<T: Config> GeneralLoan<T> {
	fn owed_principal_usd_value(&self) -> Result<AssetAmount, Error<T>> {
		usd_value_of::<T>(self.asset, self.owed_principal)
	}

	/// Repays (fully or partially) the loan with `provided_amount` (that was either debited from
	/// the account or received during liquidation). Returns any unused amount. The caller is
	/// responsible for making sure that the provided asset is the same as the loan's asset.
	fn repay_principal(
		&mut self,
		provided_amount: AssetAmount,
		should_charge_liquidation_fee: bool,
	) -> LoanRepaymentOutcome {
		if provided_amount == 0 {
			// The name is slightly misleading, but the main point is that
			// we don't have any excess amount left (since 0 is provided).
			return LoanRepaymentOutcome::PartiallyRepaid;
		}

		let config = LendingConfig::<T>::get();

		// Collect any pending interest before any repayment. Note that in the unlikely scenario
		// where this fails (due to unavailable oracle prices), the protocol/pool may miss out
		// on the very last (and very small) interest payment, which should be OK.
		let _ = self.charge_pending_interest_if_above_threshold(
			1, /* collecting any non-zero amount */
		);

		let provided_amount_after_fees = if should_charge_liquidation_fee {
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
					// TODO: add support for broker fees
					broker_fee: 0,
				});
			}

			provided_amount.saturating_sub(liquidation_fee)
		} else {
			provided_amount
		};

		// Making sure the user doesn't pay more than the total principal plus liquidation fee:
		let repayment_amount = core::cmp::min(provided_amount_after_fees, self.owed_principal);

		Pallet::<T>::mutate_existing_pool(self.asset, |pool| {
			pool.receive_repayment(repayment_amount);
		});

		self.owed_principal.saturating_reduce(repayment_amount);

		Pallet::<T>::deposit_event(Event::LoanRepaid {
			loan_id: self.id,
			amount: repayment_amount,
		});

		if self.owed_principal == 0 {
			// NOTE: in some cases we may want to delay settling/removing the loan (e.g. there may
			// be pending liquidation swaps to process), so we let the caller settle it instead
			// of doing it here.
			LoanRepaymentOutcome::FullyRepaid {
				excess_amount: provided_amount_after_fees.saturating_sub(repayment_amount),
			}
		} else {
			LoanRepaymentOutcome::PartiallyRepaid
		}
	}

	fn charge_pending_interest_if_above_threshold(
		&mut self,
		threshold_usd: AssetAmount,
	) -> DispatchResult {
		let loan_asset = self.asset;

		// Making sure that the threshold isn't 0:
		let threshold_usd = core::cmp::max(threshold_usd, 1);

		if self.pending_interest == Default::default() {
			return Ok(());
		}

		let charge_fee_if_exceeds_threshold = |fee: &mut ScaledAmountHP| {
			// Only charge fees if the accumulated amount is greater than some threshold
			let fee_usd_value = usd_value_of::<T>(loan_asset, fee.into_asset_amount())?;

			let fee_taken = if fee_usd_value >= threshold_usd {
				fee.take_non_fractional_part()
			} else {
				Default::default()
			};

			Ok::<_, DispatchError>(fee_taken)
		};

		let network_interest = charge_fee_if_exceeds_threshold(&mut self.pending_interest.network)?;

		let low_ltv_penalty =
			charge_fee_if_exceeds_threshold(&mut self.pending_interest.low_ltv_penalty)?;

		let pool_interest = charge_fee_if_exceeds_threshold(&mut self.pending_interest.pool)?;

		let fees_owed_to_network = network_interest.saturating_add(low_ltv_penalty);

		self.owed_principal.saturating_accrue(pool_interest);
		self.owed_principal.saturating_accrue(fees_owed_to_network);

		Pallet::<T>::mutate_existing_pool(loan_asset, |pool| {
			pool.record_pool_fee(pool_interest);

			let network_fees_collected = pool.record_and_collect_network_fee(fees_owed_to_network);
			Pallet::<T>::credit_fees_to_network(loan_asset, network_fees_collected);
		});

		if pool_interest != 0 || network_interest != 0 || low_ltv_penalty != 0 {
			Pallet::<T>::deposit_event(Event::InterestTaken {
				loan_id: self.id,
				pool_interest,
				network_interest,
				// TODO: broker fees
				broker_interest: Default::default(),
				low_ltv_penalty,
			});
		}

		Ok(())
	}
}

/// Uses oracle API to get the price and makes sure that it is not stale
fn get_price<T: Config>(asset: Asset) -> Result<Price, Error<T>> {
	Ok(T::PriceApi::get_price(asset).ok_or(Error::<T>::OraclePriceUnavailable)?.price)
}

/// Uses oracle prices to calculate the USD value of the given asset amount
fn usd_value_of<T: Config>(asset: Asset, amount: AssetAmount) -> Result<AssetAmount, Error<T>> {
	let price_in_usd = get_price::<T>(asset)?;
	Ok(cf_amm_math::output_amount_ceil(amount.into(), price_in_usd).unique_saturated_into())
}

// Uses oracle prices to calculate the total USD value of the entire map of assets
fn total_usd_value_of<T: Config>(
	assets_amounts: &BTreeMap<Asset, AssetAmount>,
) -> Result<AssetAmount, DispatchError> {
	let mut total_collateral_usd = 0;
	for (asset, amount) in assets_amounts {
		total_collateral_usd.saturating_accrue(usd_value_of::<T>(*asset, *amount)?);
	}

	Ok(total_collateral_usd)
}

/// Uses oracle prices to calculate the amount of `asset` that's equivalent in USD value to
/// `amount` of USD
fn amount_from_usd_value<T: Config>(
	asset: Asset,
	usd_value: AssetAmount,
) -> Result<AssetAmount, Error<T>> {
	// The "price" of USD in terms of the asset:
	let price = invert_price(get_price::<T>(asset)?);
	Ok(cf_amm_math::output_amount_ceil(usd_value.into(), price).unique_saturated_into())
}

/// A wrapper around `init_swap_request` that uses parameter suitable for a lending pool swap
fn initiate_swap<T: Config>(
	from_asset: Asset,
	amount: AssetAmount,
	to_asset: Asset,
	swap_type: LendingSwapType<T::AccountId>,
	max_oracle_price_slippage: BasisPoints,
) -> SwapRequestId {
	let dca_params = match swap_type {
		LendingSwapType::Liquidation { .. } => {
			let number_of_chunks = match usd_value_of::<T>(from_asset, amount) {
				Ok(total_amount_usd) =>
					(total_amount_usd
						.div_ceil(LendingConfig::<T>::get().liquidation_swap_chunk_size_usd))
						as u32,
				Err(_) => {
					// It shouldn't be possible to not get the price here (we don't initiate
					// liquidations unless we can get prices), but if we do, let's fallback
					// to DEFAULT_LIQUIDATION_CHUNKS chunks
					log_or_panic!(
						"Failed to estimate optimal chunk size for a {}->{} swap",
						from_asset,
						to_asset
					);

					// This number is chosen in attempt to have individual chunks that aren't too
					// large and can be processed, while keeping the total liquidation time
					// reasonable, i.e. ~5 mins.
					const DEFAULT_LIQUIDATION_CHUNKS: u32 = 50;
					DEFAULT_LIQUIDATION_CHUNKS
				},
			};

			Some(DcaParameters { number_of_chunks, chunk_interval: 1 })
		},
	};

	T::SwapRequestHandler::init_swap_request(
		from_asset,
		amount,
		to_asset,
		SwapRequestType::Regular {
			output_action: SwapOutputAction::CreditLendingPool { swap_type },
		},
		Default::default(), // broker fees
		Some(PriceLimitsAndExpiry {
			expiry_behaviour: ExpiryBehaviour::NoExpiry,
			min_price: Default::default(),
			max_oracle_price_slippage: Some(max_oracle_price_slippage),
		}),
		dca_params,
		SwapOrigin::Internal,
	)
}

/// Sweeping but it is a no-op if it fails for whatever reason
fn try_sweep<T: Config>(account_id: &T::AccountId) {
	use frame_support::sp_runtime::TransactionOutcome;

	storage::with_transaction_unchecked(|| {
		if T::PoolApi::sweep(account_id).is_ok() {
			TransactionOutcome::Commit(())
		} else {
			TransactionOutcome::Rollback(())
		}
	})
}

/// Collateral amount linked to a specific loan
#[derive(Debug)]
struct AssetCollateralForLoan {
	loan_id: LoanId,
	loan_asset: Asset,
	collateral_asset: Asset,
	collateral_amount: AssetAmount,
}

/// Check collateralisation ratio (triggering/aborting liquidations if necessary) and
/// periodically swap collected fees into each pool's desired asset.
pub fn lending_upkeep<T: Config>(current_block: BlockNumberFor<T>) -> Weight {
	let config = LendingConfig::<T>::get();

	// Collecting keys to avoid undefined behaviour in `StorageMap`
	for borrower_id in LoanAccounts::<T>::iter_keys().collect::<Vec<_>>().iter() {
		LoanAccounts::<T>::mutate(borrower_id, |loan_account| {
			let loan_account = loan_account.as_mut().expect("Using keys read just above");

			// Some of these may fail due to oracle prices being unavailable, but that's
			// OK and doesn't need any specific error handling (they will simply be re-tried
			// at a later point).
			if let Ok(ltv) = loan_account.derive_ltv() {
				let _ = loan_account.derive_and_charge_interest(ltv);

				let new_ltv = if let Ok(true) = loan_account.process_auto_top_up(borrower_id, ltv) {
					// A successful topup means we have to re-derive LTV
					loan_account.derive_ltv()
				} else {
					Ok(ltv)
				};

				// This should always be Ok (otherwise we wouldn't be able to derive LTV the first
				// time), but let's check anyway as a defensive measure:
				if let Ok(new_ltv) = new_ltv {
					loan_account.update_liquidation_status(borrower_id, new_ltv);
				}
			}
		});
	}

	// Swap fees in every asset every FEE_CHECK_INTERVAL blocks, but only if they exceed
	// FEE_SWAP_THRESHOLD_USD in value
	if current_block % config.fee_swap_interval_blocks.into() == 0u32.into() {
		// Additionally swap all network fee contributions from fees:
		for asset in PendingNetworkFees::<T>::iter_keys().collect::<Vec<_>>() {
			PendingNetworkFees::<T>::mutate(asset, |fee_amount| {
				// NOTE: if asset is FLIP, we shouldn't need to swap, but it should still work,
				// and it seems easiest to not write a special case
				let Ok(fee_usd_value) = usd_value_of::<T>(asset, *fee_amount) else {
					// Don't swap yet if we can't determine asset's price
					return;
				};

				if fee_usd_value >= config.fee_swap_threshold_usd {
					let swap_request_id =
						T::SwapRequestHandler::init_network_fee_swap_request(asset, *fee_amount);

					Pallet::<T>::deposit_event(Event::LendingNetworkFeeSwapInitiated {
						swap_request_id,
					});

					*fee_amount = 0;
				}
			});
		}
	}

	Weight::zero()
}

impl<T: Config> LendingApi for Pallet<T> {
	type AccountId = T::AccountId;

	/// Create a new loan (assigning a new loan id) provided that the account's existing collateral
	/// plus any `extra_collateral` is sufficient. Will update the primary collateral asset if
	/// provided.
	#[transactional]
	fn new_loan(
		borrower_id: T::AccountId,
		asset: Asset,
		amount_to_borrow: AssetAmount,
		primary_collateral_asset: Option<Asset>,
		extra_collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<LoanId, DispatchError> {
		let config = LendingConfig::<T>::get();
		ensure!(
			amount_to_borrow >= amount_from_usd_value::<T>(asset, config.minimum_loan_amount_usd)?,
			Error::<T>::LoanBelowMinimumAmount
		);

		let loan_id = NextLoanId::<T>::get();
		NextLoanId::<T>::set(loan_id + 1);

		LoanAccounts::<T>::mutate(&borrower_id, |maybe_account| {
			let account = Self::create_or_update_loan_account(
				borrower_id.clone(),
				maybe_account,
				primary_collateral_asset,
			)?;

			// Creating a loan with 0 principal first, the using `expand_loan_inner` to update it
			let loan = GeneralLoan {
				id: loan_id,
				asset,
				last_interest_payment_at: frame_system::Pallet::<T>::current_block_number(),
				created_at_block: frame_system::Pallet::<T>::current_block_number(),
				owed_principal: 0,
				pending_interest: Default::default(),
			};

			// NOTE: it is important that this event is emitted before `OriginationFeeTaken` event
			Self::deposit_event(Event::LoanCreated {
				loan_id,
				borrower_id: borrower_id.clone(),
				asset,
				principal_amount: amount_to_borrow,
			});

			account.expand_loan_inner(loan, amount_to_borrow, extra_collateral)?;

			// Sanity check: the account either already had collateral or it was just added
			ensure_non_zero_collateral::<T>(&account.collateral)?;

			Ok::<_, DispatchError>(())
		})?;

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
		extra_collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
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
					amount_from_usd_value::<T>(
						loan.asset,
						config.minimum_update_loan_amount_usd
					)?,
				Error::<T>::AmountBelowMinimum
			);

			loan_account.expand_loan_inner(loan, extra_amount_to_borrow, extra_collateral)?;

			Ok::<_, DispatchError>(())
		})?;

		Ok(())
	}

	/// Repays (fully or partially) a loan. Must be left above the minimum loan amount and the
	/// repayment amount must be at least the minimum update amount, unless it's a full repayment.
	#[transactional]
	fn try_making_repayment(
		borrower_id: &T::AccountId,
		loan_id: LoanId,
		repayment_amount: AssetAmount,
	) -> Result<(), DispatchError> {
		LoanAccounts::<T>::mutate(borrower_id, |maybe_account| {
			let config = LendingConfig::<T>::get();
			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanNotFound)?;

			let Some(loan) = loan_account.loans.get_mut(&loan_id) else {
				fail!(Error::<T>::LoanNotFound);
			};

			let loan_asset = loan.asset;

			if repayment_amount < loan.owed_principal {
				ensure!(
					usd_value_of::<T>(loan.asset, repayment_amount)? >=
						config.minimum_update_loan_amount_usd,
					Error::<T>::AmountBelowMinimum
				);
			}

			T::Balance::try_debit_account(borrower_id, loan_asset, repayment_amount)?;

			if let LoanRepaymentOutcome::FullyRepaid { excess_amount } =
				loan.repay_principal(repayment_amount, false /* no liquidation fee */)
			{
				loan_account.settle_loan(loan_id, false /* not via liquidation */);

				if excess_amount > 0 {
					T::Balance::credit_account(borrower_id, loan_asset, excess_amount);
				}
			} else {
				ensure!(
					usd_value_of::<T>(loan.asset, loan.owed_principal)? >=
						config.minimum_loan_amount_usd,
					Error::<T>::LoanBelowMinimumAmount
				);
			}

			// NOTE: even if we settle the last loan here, we don't remove
			// the account as there must still be collateral in the account (it is not
			// released automatically).

			Ok::<_, DispatchError>(())
		})
	}

	#[transactional]
	fn add_collateral(
		borrower_id: &Self::AccountId,
		primary_collateral_asset: Option<Asset>,
		collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
		ensure_non_zero_collateral::<T>(&collateral)?;

		ensure!(
			total_usd_value_of::<T>(&collateral)? >=
				LendingConfig::<T>::get().minimum_update_collateral_amount_usd,
			Error::<T>::AmountBelowMinimum
		);

		LoanAccounts::<T>::mutate(borrower_id, |maybe_account| {
			let loan_account = Self::create_or_update_loan_account(
				borrower_id.clone(),
				maybe_account,
				primary_collateral_asset,
			)?;

			loan_account.try_adding_collateral_from_free_balance(&collateral)?;

			Self::deposit_event(Event::CollateralAdded {
				borrower_id: borrower_id.clone(),
				collateral,
			});

			Ok(())
		})
	}

	#[transactional]
	fn remove_collateral(
		borrower_id: &Self::AccountId,
		collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
		LoanAccounts::<T>::mutate(borrower_id, |maybe_account| {
			let chp_config = LendingConfig::<T>::get();

			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanAccountNotFound)?;

			ensure!(
				loan_account.liquidation_status == LiquidationStatus::NoLiquidation,
				Error::<T>::LiquidationInProgress
			);

			// If a larger than or equal amount of collateral is being removed from all collateral
			// assets, then we are removing all collateral and do not need to check the minimum
			// amount. Being able to specify a large (eg u128::MAX) amount for all assets lets the
			// user avoid exact values (useful because of fees).
			if !loan_account.collateral.iter().all(|(asset, loan_amount)| {
				collateral
					.get(asset)
					.map(|remove_amount| remove_amount >= loan_amount)
					.unwrap_or(false)
			}) {
				let total_collateral_usd = total_usd_value_of::<T>(&collateral)?;
				ensure!(
					total_collateral_usd >=
						LendingConfig::<T>::get().minimum_update_collateral_amount_usd,
					Error::<T>::AmountBelowMinimum
				);
			}

			for (asset, amount) in &collateral {
				ensure!(
					T::SafeMode::get().remove_collateral.enabled(asset),
					Error::<T>::RemovingCollateralDisabled
				);

				let current_amount = loan_account
					.collateral
					.get_mut(asset)
					.ok_or(Error::<T>::InsufficientCollateral)?;

				*current_amount = current_amount
					.checked_sub(*amount)
					.ok_or(Error::<T>::InsufficientCollateral)?;

				if *current_amount == 0 {
					loan_account.collateral.remove(asset);
				}

				T::Balance::credit_account(borrower_id, *asset, *amount);
			}

			// Only check LTV if there are loans:
			if !loan_account.loans.is_empty() &&
				loan_account.derive_ltv()? > chp_config.ltv_thresholds.target.into()
			{
				fail!(Error::<T>::InsufficientCollateral);
			}

			Self::deposit_event(Event::CollateralRemoved {
				borrower_id: borrower_id.clone(),
				collateral,
			});

			if loan_account.collateral.is_empty() && loan_account.loans.is_empty() {
				*maybe_account = None;
			}

			Ok(())
		})
	}

	fn update_primary_collateral_asset(
		borrower_id: &Self::AccountId,
		primary_collateral_asset: Asset,
	) -> Result<(), DispatchError> {
		LoanAccounts::<T>::try_mutate(borrower_id, |maybe_account| {
			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanAccountNotFound)?;

			if loan_account.primary_collateral_asset != primary_collateral_asset {
				loan_account.primary_collateral_asset = primary_collateral_asset;

				Self::deposit_event(Event::PrimaryCollateralAssetUpdated {
					borrower_id: borrower_id.clone(),
					primary_collateral_asset,
				});
			}

			Ok(())
		})
	}
}

impl<T: Config> cf_traits::lending::LendingSystemApi for Pallet<T> {
	type AccountId = T::AccountId;

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

					let mut is_last_liquidation_swap = false;

					let liquidation_swap = match &mut loan_account.liquidation_status {
						LiquidationStatus::NoLiquidation => {
							log_or_panic!("Unexpected liquidation (swap request id: {swap_request_id}, loan_id: {loan_id})");
							return;
						},
						LiquidationStatus::Liquidating { liquidation_swaps, .. } => {
							if let Some(swap) = liquidation_swaps.remove(&swap_request_id) {
								if liquidation_swaps
									.values()
									.filter(|swap| swap.loan_id == loan_id)
									.count() == 0
								{
									is_last_liquidation_swap = true;
								}

								if liquidation_swaps.is_empty() {
									loan_account.liquidation_status =
										LiquidationStatus::NoLiquidation;
								}

								swap
							} else {
								log_or_panic!("Unable to find liquidation swap (swap request id: {swap_request_id}) for loan_id: {loan_id})");
								return;
							}
						},
					};

					let remaining_amount = match loan_account
						.get_loan_and_check_asset(loan_id, liquidation_swap.to_asset)
					{
						Some(loan) => {
							match loan.repay_principal(output_amount, true /* liquidation */) {
								LoanRepaymentOutcome::FullyRepaid { excess_amount } => {
									// NOTE: we don't need to worry about settling the loan just yet
									// as there may be more liquidation swaps to process for the
									// loan.
									excess_amount
								},
								LoanRepaymentOutcome::PartiallyRepaid => {
									// On partial repayment the full amount has been consumed.
									0
								},
							}
						},
						None => {
							// In rare cases it may be possible for the loan to no longer exist if
							// e.g. the principal was fully covered by a prior liquidation swap.
							output_amount
						},
					};

					// Any amount left after repaying the loan is added to the borrower's
					// collateral balance:
					if remaining_amount > 0 {
						loan_account.add_to_collateral(liquidation_swap.to_asset, remaining_amount);
					}

					// If this swap is the last liquidation swap for the loan, we should
					// "settle" it (even if it hasn't been repaid in full):
					if is_last_liquidation_swap {
						loan_account.settle_loan(loan_id, true /* via liquidation */);

						// If account has no loans and no collateral, it should now be removed
						if loan_account.loans.is_empty() && loan_account.collateral.is_empty() {
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

	fn credit_fees_to_network(fee_asset: Asset, fee_amount: AssetAmount) {
		PendingNetworkFees::<T>::mutate(fee_asset, |pending_amount| {
			pending_amount.saturating_accrue(fee_amount);
		});
	}

	fn create_or_update_loan_account(
		borrower_id: T::AccountId,
		maybe_account: &mut Option<LoanAccount<T>>,
		primary_collateral_asset: Option<Asset>,
	) -> Result<&mut LoanAccount<T>, Error<T>> {
		let mut primary_collateral_asset_updated = false;

		let account = match maybe_account {
			Some(account) => {
				// If the user provides primary collateral asset, we update it:
				if let Some(asset) = primary_collateral_asset {
					if account.primary_collateral_asset != asset {
						account.primary_collateral_asset = asset;
						primary_collateral_asset_updated = true;
					}
				}
				account
			},
			None => {
				// If the user has no account, they must provide primary collateral asset
				// in order for us to create a new account for them
				let primary_collateral_asset =
					primary_collateral_asset.ok_or(Error::<T>::InvalidLoanParameters)?;

				primary_collateral_asset_updated = true;

				let account = LoanAccount::new(borrower_id.clone(), primary_collateral_asset);
				maybe_account.insert(account)
			},
		};

		if primary_collateral_asset_updated {
			Self::deposit_event(Event::PrimaryCollateralAssetUpdated {
				borrower_id,
				primary_collateral_asset: account.primary_collateral_asset,
			});
		}

		Ok(account)
	}

	/// Mutates for pool for `asset` expecting it to exist.
	fn mutate_existing_pool<F>(asset: Asset, f: F)
	where
		F: FnOnce(&mut LendingPool<T::AccountId>),
	{
		GeneralLendingPools::<T>::mutate(asset, |maybe_pool| {
			if let Some(pool) = maybe_pool.as_mut() {
				f(pool)
			} else {
				log_or_panic!("Lending Pool must exist for asset {}", asset);
			}
		});
	}
}

pub use rpc::{RpcLendingPool, RpcLiquidationStatus, RpcLiquidationSwap, RpcLoan, RpcLoanAccount};

pub mod rpc {

	use super::*;
	use cf_primitives::{AssetAndAmount, SwapRequestId};
	use cf_traits::lending::LoanId;
	use serde::{Deserialize, Serialize};

	#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
	pub struct RpcLoan<Amount> {
		pub loan_id: LoanId,
		pub asset: Asset,
		pub created_at: u32,
		pub principal_amount: Amount,
	}

	// TODO: see what other parameters are needed
	#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
	pub struct RpcLendingPool<Amount> {
		pub asset: Asset,
		pub total_amount: Amount,
		pub available_amount: Amount,
		pub utilisation_rate: Permill,
		pub current_interest_rate: Permill,
		#[serde(flatten)]
		pub config: LendingPoolConfiguration,
	}

	#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
	pub struct RpcLiquidationSwap {
		pub swap_request_id: SwapRequestId,
		pub loan_id: LoanId,
	}

	#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
	pub struct RpcLiquidationStatus {
		pub liquidation_swaps: Vec<RpcLiquidationSwap>,
		pub is_hard: bool,
	}

	#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
	pub struct RpcLoanAccount<AccountId, Amount> {
		pub account: AccountId,
		pub primary_collateral_asset: Asset,
		pub ltv_ratio: Option<FixedU64>,
		pub collateral: Vec<AssetAndAmount<Amount>>,
		pub loans: Vec<RpcLoan<Amount>>,
		pub liquidation_status: Option<RpcLiquidationStatus>,
	}

	fn build_rpc_loan_account<T: Config>(
		borrower_id: T::AccountId,
		loan_account: LoanAccount<T>,
	) -> RpcLoanAccount<T::AccountId, AssetAmount> {
		let mut loans = loan_account.loans.clone();

		// Accounting for any partially executed liquidation swaps
		// when reporting on the outstanding principal amount:
		if let LiquidationStatus::Liquidating { liquidation_swaps, .. } =
			&loan_account.liquidation_status
		{
			for (swap_request_id, LiquidationSwap { loan_id, .. }) in liquidation_swaps {
				if let Some(swap_progress) =
					T::SwapRequestHandler::inspect_swap_request(*swap_request_id)
				{
					if let Some(loan) = loans.get_mut(loan_id) {
						loan.owed_principal
							.saturating_reduce(swap_progress.accumulated_output_amount);
					}
				} else {
					log_or_panic!("Failed to inspect swap request: {swap_request_id}");
				}
			}
		}

		RpcLoanAccount {
			account: borrower_id,
			primary_collateral_asset: loan_account.primary_collateral_asset,
			ltv_ratio: loan_account.derive_ltv().ok(),
			collateral: loan_account
				.get_total_collateral()
				.into_iter()
				.map(|(asset, amount)| AssetAndAmount { asset, amount })
				.collect(),
			loans: loans
				.into_iter()
				.map(|(loan_id, loan)| RpcLoan {
					loan_id,
					asset: loan.asset,
					created_at: loan.created_at_block.unique_saturated_into(),
					principal_amount: loan.owed_principal,
				})
				.collect(),
			liquidation_status: match loan_account.liquidation_status {
				LiquidationStatus::NoLiquidation => None,
				LiquidationStatus::Liquidating { liquidation_swaps, is_hard } =>
					Some(RpcLiquidationStatus {
						liquidation_swaps: liquidation_swaps
							.into_iter()
							.map(|(swap_request_id, swap)| RpcLiquidationSwap {
								swap_request_id,
								loan_id: swap.loan_id,
							})
							.collect(),
						is_hard,
					}),
			},
		}
	}

	pub fn get_loan_accounts<T: Config>(
		borrower_id: Option<T::AccountId>,
	) -> Vec<RpcLoanAccount<T::AccountId, AssetAmount>> {
		if let Some(borrower_id) = borrower_id {
			LoanAccounts::<T>::get(&borrower_id)
				.into_iter()
				.map(|loan_account| build_rpc_loan_account(borrower_id.clone(), loan_account))
				.collect()
		} else {
			LoanAccounts::<T>::iter()
				.map(|(borrower_id, loan_account)| {
					build_rpc_loan_account(borrower_id.clone(), loan_account)
				})
				.collect()
		}
	}

	fn build_rpc_lending_pool<T: Config>(
		asset: Asset,
		pool: &LendingPool<T::AccountId>,
	) -> RpcLendingPool<AssetAmount> {
		let config = LendingConfig::<T>::get();

		let utilisation = pool.get_utilisation();

		let current_interest_rate = config.derive_interest_rate_per_year(asset, utilisation);

		RpcLendingPool {
			asset,
			total_amount: pool.total_amount,
			available_amount: pool.available_amount,
			utilisation_rate: utilisation,
			current_interest_rate,
			config: config.get_config_for_asset(asset).clone(),
		}
	}

	pub fn get_lending_pools<T: Config>(asset: Option<Asset>) -> Vec<RpcLendingPool<AssetAmount>> {
		if let Some(asset) = asset {
			GeneralLendingPools::<T>::get(asset)
				.iter()
				.map(|pool| build_rpc_lending_pool::<T>(asset, pool))
				.collect()
		} else {
			GeneralLendingPools::<T>::iter()
				.map(|(asset, pool)| build_rpc_lending_pool::<T>(asset, &pool))
				.collect()
		}
	}
}

/// Interest "curve" is defined as two linear segments. One is in effect from 0% to
/// `junction_utilisation`, and the second is in effect from `junction_utilisation` to 100%.
#[derive(
	Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize,
)]
pub struct InterestRateConfiguration {
	pub interest_at_zero_utilisation: Permill,
	pub junction_utilisation: Permill,
	pub interest_at_junction_utilisation: Permill,
	pub interest_at_max_utilisation: Permill,
}

impl InterestRateConfiguration {
	pub fn validate(&self) -> DispatchResult {
		// Ensure that the interest rate increases with utilization, which
		// we rely on when interpolating the curve.
		ensure!(
			self.interest_at_zero_utilisation <= self.interest_at_junction_utilisation &&
				self.interest_at_junction_utilisation <= self.interest_at_max_utilisation,
			"Invalid curve"
		);

		Ok(())
	}
}

#[derive(
	Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize,
)]
pub struct LendingPoolConfiguration {
	pub origination_fee: Permill,
	/// Portion of the amount of principal asset obtained via liquidation that's
	/// paid as a fee (instead of reducing the loan's principal)
	pub liquidation_fee: Permill,
	/// Determines how interest rate is calculated based on the utilization rate.
	pub interest_rate_curve: InterestRateConfiguration,
}

#[derive(
	Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize,
)]
pub struct LtvThresholds {
	/// Borrowers aren't allowed to borrow more (or withdraw collateral) if their Loan-to-value
	/// ratio (principal/collateral) would exceed this threshold.
	pub target: Permill,
	/// Reaching this threshold will trigger a top-up of the collateral
	pub topup: Permill,
	/// Reaching this threshold will trigger soft liquidation account's loans
	pub soft_liquidation: Permill,
	/// If a loan that's being liquidated reaches this threshold, it will be considered
	/// "healthy" again and the liquidation will be aborted. This is meant to be slightly
	/// lower than the soft threshold to avoid frequent oscillations between liquidating and
	/// not liquidating.
	pub soft_liquidation_abort: Permill,
	/// Reaching this threshold will trigger hard liquidation of the loan
	pub hard_liquidation: Permill,
	/// Same as overcollateralisation_soft_liquidation_abort_threshold, but for
	/// transitioning from hard to soft liquidation
	pub hard_liquidation_abort: Permill,
	/// The max value for LTV that doesn't lead to borrowers paying extra interest to the network
	/// on their collateral
	pub low_ltv: Permill,
}

impl LtvThresholds {
	pub fn validate(&self) -> DispatchResult {
		ensure!(
			self.target <= self.topup &&
				self.topup <= self.soft_liquidation &&
				self.soft_liquidation <= self.hard_liquidation,
			"Invalid LTV thresholds"
		);

		ensure!(
			self.hard_liquidation_abort < self.hard_liquidation &&
				self.soft_liquidation_abort < self.soft_liquidation,
			"Invalid LTV thresholds"
		);

		Ok(())
	}
}

#[derive(
	Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize,
)]
pub struct NetworkFeeContributions {
	/// A fixed % that's added to the base interest to get the total borrow rate (as a % on the
	/// principal paid every year)
	pub extra_interest: Permill,
	/// The % of the origination fee that should be taken as a network fee.
	pub from_origination_fee: Permill,
	/// The % of the liquidation fee that should be taken as a network fee.
	pub from_liquidation_fee: Permill,
	/// Interest on collateral paid when LTV approaches 0
	pub interest_on_collateral_max: Permill,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct LendingConfiguration {
	/// This configuration is used unless it is overridden in `pool_config_overrides`.
	pub default_pool_config: LendingPoolConfiguration,
	/// Determines when events like liquidation should be triggered based on the account's
	/// loan-to-value ratio.
	pub ltv_thresholds: LtvThresholds,
	/// Determines what portion of each fee type should be taken as a network fee.
	pub network_fee_contributions: NetworkFeeContributions,
	/// Determines how frequently (in blocks) we check if fees should be swapped into the
	/// pools asset
	pub fee_swap_interval_blocks: u32,
	/// Determines how frequently (in blocks) we calculate and record interest payments from loans.
	pub interest_payment_interval_blocks: u32,
	/// If loan account's owed interest reaches this threshold, it will be taken from the account's
	/// collateral
	pub interest_collection_threshold_usd: AssetAmount,
	/// Fees collected in some asset will be swapped into the pool's asset once their usd value
	/// reaches this threshold
	pub fee_swap_threshold_usd: AssetAmount,
	/// Soft liquidation will be executed with this oracle slippage limit
	pub soft_liquidation_max_oracle_slippage: BasisPoints,
	/// Hard liquidation will be executed with this oracle slippage limit
	pub hard_liquidation_max_oracle_slippage: BasisPoints,
	/// Liquidation swaps will use chunks that are equivalent to this amount of USD
	pub liquidation_swap_chunk_size_usd: AssetAmount,
	/// All fee swaps from lending will be executed with this oracle slippage limit
	pub fee_swap_max_oracle_slippage: BasisPoints,
	/// If set for a pool/asset, this configuration will be used instead of the default
	pub pool_config_overrides: BTreeMap<Asset, LendingPoolConfiguration>,
	/// Minimum amount of principal that a loan must have at all times.
	pub minimum_loan_amount_usd: AssetAmount,
	/// Minimum equivalent amount of principal that can be used to expand or repay an existing
	/// loan.
	pub minimum_update_loan_amount_usd: AssetAmount,
	/// Minimum equivalent amount of collateral that can be added or removed from a loan account.
	pub minimum_update_collateral_amount_usd: AssetAmount,
}

impl LendingConfiguration {
	pub fn get_config_for_asset(&self, asset: Asset) -> &LendingPoolConfiguration {
		self.pool_config_overrides.get(&asset).unwrap_or(&self.default_pool_config)
	}

	pub fn derive_interest_rate_per_year(&self, asset: Asset, utilisation: Permill) -> Permill {
		let InterestRateConfiguration {
			interest_at_zero_utilisation,
			junction_utilisation,
			interest_at_junction_utilisation,
			interest_at_max_utilisation,
		} = self.get_config_for_asset(asset).interest_rate_curve;

		if utilisation < junction_utilisation {
			interpolate_linear_segment(
				Permill::zero(),
				junction_utilisation,
				interest_at_zero_utilisation,
				interest_at_junction_utilisation,
				utilisation,
			)
		} else {
			interpolate_linear_segment(
				junction_utilisation,
				Permill::one(),
				interest_at_junction_utilisation,
				interest_at_max_utilisation,
				utilisation,
			)
		}
	}

	fn interest_per_year_to_per_payment_interval(
		&self,
		interest_per_year: Permill,
		interval_blocks: u32,
	) -> Perquintill {
		use cf_primitives::BLOCKS_IN_YEAR;

		Perquintill::from_parts(
			(interest_per_year.deconstruct() as u64 *
				(Perquintill::ACCURACY / Permill::ACCURACY as u64)) /
				(BLOCKS_IN_YEAR / interval_blocks) as u64,
		)
	}

	/// Computes the interest rate to be paid each payment interval. Uses Perquintill for better
	/// precision as the value is likely to be a very small fraction due to the interval being
	/// short.
	fn derive_base_interest_rate_per_payment_interval(
		&self,
		asset: Asset,
		utilisation: Permill,
		interval_blocks: u32,
	) -> Perquintill {
		let interest_rate = self.derive_interest_rate_per_year(asset, utilisation);

		self.interest_per_year_to_per_payment_interval(interest_rate, interval_blocks)
	}

	fn derive_network_interest_rate_per_payment_interval(
		&self,
		interval_blocks: u32,
	) -> Perquintill {
		self.interest_per_year_to_per_payment_interval(
			self.network_fee_contributions.extra_interest,
			interval_blocks,
		)
	}

	/// Computes an additional annual interest/penalty for loan accounts with LTV below
	/// `low_ltv` to incentivise capital efficiency. The penalty decreases linearly
	/// from `interest_on_collateral_max` at 0% LTV to zero at `low_ltv` threshold.
	fn derive_low_ltv_interest_rate_per_year(&self, ltv: FixedU64) -> Permill {
		let ltv: Permill = ltv.into_clamped_perthing();

		if ltv >= self.ltv_thresholds.low_ltv {
			return Permill::zero();
		}

		interpolate_linear_segment(
			Permill::zero(),
			self.ltv_thresholds.low_ltv,
			self.network_fee_contributions.interest_on_collateral_max,
			Permill::zero(),
			ltv,
		)
	}

	fn derive_low_ltv_penalty_rate_per_payment_interval(
		&self,
		ltv: FixedU64,
		interval_blocks: u32,
	) -> Perquintill {
		let interest_rate = self.derive_low_ltv_interest_rate_per_year(ltv);
		self.interest_per_year_to_per_payment_interval(interest_rate, interval_blocks)
	}

	pub fn origination_fee(&self, asset: Asset) -> Permill {
		self.get_config_for_asset(asset).origination_fee
	}

	pub fn liquidation_fee(&self, asset: Asset) -> Permill {
		self.get_config_for_asset(asset).liquidation_fee
	}
}

/// Computes the value of `f(x)` where f is a linear function defined by two points:
/// (x0, y0) and (x1, y1). The code assumes x0 <= x <= x1 and x0 != x1.
fn interpolate_linear_segment(
	x0: Permill,
	x1: Permill,
	y0: Permill,
	y1: Permill,
	x: Permill,
) -> Permill {
	if x0 > x || x > x1 || x0 == x1 {
		log_or_panic!("Invalid parameters");
		return Permill::zero();
	}

	let y0 = FixedI64::from(y0);
	let y1 = FixedI64::from(y1);
	let x0 = FixedI64::from(x0);
	let x1 = FixedI64::from(x1);
	let x = FixedI64::from(x);

	let slope = (y1 - y0) / (x1 - x0);

	let delta = slope * (x - x0);

	y0.saturating_add(delta).into_clamped_perthing()
}

fn ensure_non_zero_collateral<T: Config>(
	collateral: &BTreeMap<Asset, AssetAmount>,
) -> Result<(), Error<T>> {
	ensure!(!collateral.is_empty(), Error::<T>::EmptyCollateral);
	ensure!(collateral.values().all(|amount| *amount > 0), Error::<T>::EmptyCollateral);

	Ok(())
}
