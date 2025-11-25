use cf_amm_math::{invert_price, Price};
use cf_primitives::{DcaParameters, SwapRequestId};
use cf_traits::{ExpiryBehaviour, LendingSwapType, LpRegistration, PriceLimitsAndExpiry};
use core_lending_pool::ScaledAmountHP;
use frame_support::{
	fail,
	sp_runtime::{traits::Bounded, FixedI64, FixedPointNumber, FixedU64, PerThing},
	DefaultNoBound,
};

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

/// Helps to link swap id in liquidation status to loan id
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct LiquidationSwap {
	loan_id: LoanId,
	from_asset: Asset,
	to_asset: Asset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize)]
pub enum LiquidationType {
	SoftVoluntary,
	Soft,
	Hard,
}

/// Whether the account's collateral is being liquidated (and if so, stores ids of liquidation
/// swaps)
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum LiquidationStatus {
	NoLiquidation,
	Liquidating {
		liquidation_swaps: BTreeMap<SwapRequestId, LiquidationSwap>,
		liquidation_type: LiquidationType,
	},
}

/// High precision interest amounts broken down by type
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Default)]
struct InterestBreakdown {
	network: ScaledAmountHP,
	pool: ScaledAmountHP,
	broker: ScaledAmountHP,
	low_ltv_penalty: ScaledAmountHP,
}

#[derive(Clone, Copy, Debug, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub enum LiquidationCompletionReason {
	/// Full liquidation (loans are fully repaid and/or all collateral has been swapped)
	FullySwapped,
	/// Aborted to change liquidation state (e.g. to "no liquidation")
	LtvChange,
	/// Partial liquidation: manual liquidation aborted by the user
	ManualAbort,
}

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct LoanAccount<T: Config> {
	borrower_id: T::AccountId,
	collateral_topup_asset: Option<Asset>,
	collateral: BTreeMap<Asset, AssetAmount>,
	pub(super) loans: BTreeMap<LoanId, GeneralLoan<T>>,
	pub(super) liquidation_status: LiquidationStatus,
	pub(super) voluntary_liquidation_requested: bool,
}

impl<T: Config> LoanAccount<T> {
	pub fn new(borrower_id: T::AccountId) -> Self {
		Self {
			borrower_id,
			collateral_topup_asset: None,
			collateral: BTreeMap::new(),
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
		collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
		for (asset, amount) in &collateral {
			ensure!(
				T::SafeMode::get().add_collateral.enabled(asset),
				Error::<T>::AddingCollateralDisabled
			);
			T::Balance::try_debit_account(&self.borrower_id, *asset, *amount)?;
		}

		self.add_new_collateral(collateral, CollateralAddedActionType::Manual);

		Ok(())
	}

	/// Add to the user's collateral and emit the corresponding event.
	fn add_new_collateral(
		&mut self,
		collateral: BTreeMap<Asset, AssetAmount>,
		action_type: CollateralAddedActionType,
	) {
		for (asset, amount) in &collateral {
			self.collateral.entry(*asset).or_default().saturating_accrue(*amount);
		}

		Pallet::<T>::deposit_event(Event::CollateralAdded {
			borrower_id: self.borrower_id.clone(),
			collateral,
			action_type,
		});
	}

	/// Return user's existing collateral (what hasn't been swapped during liquidation).
	/// Unlike [add_new_collateral], we don't emit an event when collateral is returned.
	fn return_collateral(&mut self, asset: Asset, amount: AssetAmount) {
		self.collateral.entry(asset).or_default().saturating_accrue(amount);
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

	pub fn update_liquidation_status(
		&mut self,
		borrower_id: &T::AccountId,
		ltv: FixedU64,
		price_cache: &OraclePriceCache<T>,
		weight_used: &mut Weight,
	) -> Result<(), Error<T>> {
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
				// aborting liqiudation: if it is set, we transition to voluntary liquidation.
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
				// aborting liqiudation: if it is set, we transition to voluntary liquidation.

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
				let collateral = self.prepare_collateral_for_liquidation(price_cache)?;
				self.init_liquidation_swaps(borrower_id, collateral, liquidation_type, price_cache);
				weight_used.saturating_accrue(T::WeightInfo::start_liquidation_swaps());
			},
			LiquidationStatusChange::AbortLiquidation { reason } => {
				self.abort_liquidation_swaps(reason);
				weight_used.saturating_accrue(T::WeightInfo::abort_liquidation_swaps());
			},
			LiquidationStatusChange::ChangeLiquidationType { liquidation_type } => {
				// Going from one liquidation type to another is always due to LTV change
				self.abort_liquidation_swaps(LiquidationCompletionReason::LtvChange);
				let collateral = self.prepare_collateral_for_liquidation(price_cache)?;
				self.init_liquidation_swaps(borrower_id, collateral, liquidation_type, price_cache);
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
							is_voluntary,
						);

						if loan.owed_principal == 0 {
							fully_repaid_loans.push(loan_id);
						}

						excess_amount
					},
					None => swap_progress.accumulated_output_amount,
				};

				if excess_amount > 0 {
					// In case we have liquidated more than necessary the excess amount
					// is added to the account's collateral balance:
					self.add_new_collateral(
						BTreeMap::from([(to_asset, excess_amount)]),
						CollateralAddedActionType::SystemLiquidationExcessAmount,
					);
				}

				// Any input funds not yet liquidated are returned to the
				// account's collateral balance.
				self.return_collateral(from_asset, swap_progress.remaining_input_amount);
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

	#[transactional]
	pub fn derive_and_charge_interest(
		&mut self,
		ltv: FixedU64,
		price_cache: &OraclePriceCache<T>,
		weight_used: &mut Weight,
	) -> DispatchResult {
		let config = LendingConfig::<T>::get();

		if self.liquidation_status != LiquidationStatus::NoLiquidation {
			// For simplicity, we don't charge interest during liquidations
			return Ok(())
		}

		let current_block = frame_system::Pallet::<T>::block_number();
		weight_used.saturating_accrue(T::DbWeight::get().reads(1));

		for loan in self.loans.values_mut() {
			let blocks_since_last_payment: u32 = current_block
				.saturating_sub(loan.last_interest_payment_at)
				.try_into()
				.unwrap_or(u32::MAX);

			if current_block.saturating_sub(loan.created_at_block) %
				config.interest_payment_interval_blocks.into() ==
				0u32.into()
			{
				weight_used.saturating_accrue(T::WeightInfo::loan_charge_interest());
				loan.charge_interest(
					ltv,
					current_block,
					blocks_since_last_payment,
					&config,
					price_cache,
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
		price_cache: &OraclePriceCache<T>,
		weight_used: &mut Weight,
	) -> Result<bool, DispatchError> {
		let config = LendingConfig::<T>::get();

		if config.ltv_thresholds.topup.is_none_or(|threshold| ltv <= threshold.into()) {
			return Ok(false)
		}

		// No topup if topup asset is not specified:
		let Some(collateral_topup_asset) = self.collateral_topup_asset else { return Ok(false) };

		// Ignoring weight of sweep as we expect most borrowers to not have open orders
		try_sweep::<T>(borrower_id);

		weight_used.saturating_accrue(T::WeightInfo::loan_calculate_top_up_amount());
		let top_up_amount =
			self.calculate_top_up_amount(borrower_id, config.ltv_thresholds.target, price_cache)?;

		if top_up_amount > 0 {
			weight_used.saturating_accrue(T::DbWeight::get().reads_writes(2, 2));
			T::Balance::try_debit_account(borrower_id, collateral_topup_asset, top_up_amount)
				.inspect_err(|_| {
					log_or_panic!("Unable to debit after checking balance");
				})?;

			self.add_new_collateral(
				BTreeMap::from([(collateral_topup_asset, top_up_amount)]),
				CollateralAddedActionType::SystemTopup,
			);

			Ok(true)
		} else {
			Ok(false)
		}
	}

	pub(super) fn calculate_top_up_amount(
		&self,
		borrower_id: &T::AccountId,
		ltv_threshold_target: Permill,
		price_cache: &OraclePriceCache<T>,
	) -> Result<AssetAmount, Error<T>> {
		let Some(topup_asset) = self.collateral_topup_asset else {
			return Ok(0);
		};

		let top_up_required_in_usd = {
			let loan_value_in_usd = self.total_owed_usd_value(price_cache)?;

			let collateral_required_in_usd = FixedU64::from(ltv_threshold_target)
				.reciprocal()
				.map(|ltv_inverted| ltv_inverted.saturating_mul_int(loan_value_in_usd))
				// This effectively disables auto top up if the ltv target erroneously set to 0:
				.unwrap_or(0);

			collateral_required_in_usd.saturating_sub(self.total_collateral_usd_value(price_cache)?)
		};

		// Auto top up is currently only possible from the primary collateral asset
		let top_up_required_in_collateral_asset =
			price_cache.amount_from_usd_value(topup_asset, top_up_required_in_usd)?;

		// Don't attempt to charge more than what's available:
		Ok(core::cmp::min(
			T::Balance::get_balance(borrower_id, topup_asset),
			top_up_required_in_collateral_asset,
		))
	}

	/// Split collateral proportionally to the usd value of each loan (to give each loan a fair
	/// chance of being liquidated without a loss) as a preparation step for liquidation. Returns
	/// error if oracle prices aren't available.
	pub(super) fn prepare_collateral_for_liquidation(
		&mut self,
		price_cache: &OraclePriceCache<T>,
	) -> Result<Vec<AssetCollateralForLoan>, Error<T>> {
		let mut prepared_collateral = vec![];

		let principal_amounts_usd = self
			.loans
			.iter()
			.map(|(loan_id, loan)| {
				loan.owed_principal_usd_value(price_cache)
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
	pub(super) fn init_liquidation_swaps(
		&mut self,
		borrower_id: &T::AccountId,
		collateral: Vec<AssetCollateralForLoan>,
		liquidation_type: LiquidationType,
		price_cache: &OraclePriceCache<T>,
	) {
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
		extra_collateral: BTreeMap<Asset, AssetAmount>,
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

		if !extra_collateral.is_empty() {
			self.try_adding_collateral_from_free_balance(extra_collateral)?;
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

/// A parameter into [charge_pending_interest_if_above_threshold] grouping the threshold
/// and price cache together so they can both be wrapped in an Option.
struct PriceCacheAndThreshold<'a, T: Config> {
	threshold_usd: AssetAmount,
	price_cache: &'a OraclePriceCache<T>,
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
			.charge_pending_interest_if_above_threshold(None /* no threshold */)
			.is_err()
		{
			log_or_panic!(
				"Final interest charge should not fail since the price oracle is not required here"
			);
		}
	}

	pub(super) fn charge_interest(
		&mut self,
		ltv: FixedU64,
		current_block: BlockNumberFor<T>,
		blocks_since_last_payment: u32,
		config: &LendingConfiguration,
		price_cache: &OraclePriceCache<T>,
	) -> DispatchResult {
		let loan_asset = self.asset;

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

		let network_interest_rate =
			config.derive_network_interest_rate_per_payment_interval(blocks_since_last_payment);

		let low_ltv_penalty_rate =
			config.derive_low_ltv_penalty_rate_per_payment_interval(ltv, blocks_since_last_payment);

		// Calculating interest in scaled amounts for better precision
		let owed_principal = ScaledAmountHP::from_asset_amount(self.owed_principal);

		// Work out how much interest has accrued in loan's asset terms:
		let network_interest_amount = owed_principal * network_interest_rate;
		let low_ltv_penalty_amount = owed_principal * low_ltv_penalty_rate;
		let pool_interest_amount = owed_principal * base_interest_rate;

		// Record the accrued interest amounts. We may or may not charge these immediately
		// depending on whether the amounts exceed some threshold.
		self.pending_interest.network.saturating_accrue(network_interest_amount);
		self.pending_interest.pool.saturating_accrue(pool_interest_amount);
		self.pending_interest.low_ltv_penalty.saturating_accrue(low_ltv_penalty_amount);

		self.last_interest_payment_at = current_block;

		self.charge_pending_interest_if_above_threshold(Some(PriceCacheAndThreshold {
			threshold_usd: config.interest_collection_threshold_usd,
			price_cache,
		}))?;

		Ok(())
	}

	/// Repays the loan after collecting any pending interest and deducting liquidation fee
	/// from the provided amount. Returns any excess/unused funds.
	fn repay_via_liquidation(
		&mut self,
		provided_amount: AssetAmount,
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
					// TODO: add support for broker fees
					broker_fee: 0,
				});
			}

			provided_amount.saturating_sub(liquidation_fee)
		};

		self.repay_principal(provided_amount_after_fees)
	}

	/// Repays (fully or partially) the loan with `provided_amount` (that was either debited from
	/// the account or received during liquidation). Returns any unused amount. The caller is
	/// responsible for making sure that all pending interest has already been collected (via
	/// [collect_pending_interest]) and that the provided asset is the same as the loan's asset.
	fn repay_principal(&mut self, provided_amount: AssetAmount) -> AssetAmount {
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
			});
		}

		provided_amount.saturating_sub(repayment_amount)
	}

	fn charge_pending_interest_if_above_threshold(
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
pub(super) struct AssetCollateralForLoan {
	loan_id: LoanId,
	loan_asset: Asset,
	collateral_asset: Asset,
	collateral_amount: AssetAmount,
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
		let _ = LoanAccounts::<T>::try_mutate(borrower_id, |loan_account| {
			let loan_account = loan_account.as_mut().expect("Using keys read just above");

			// Some of these may fail due to oracle prices being unavailable, but that's
			// OK and doesn't need any specific error handling (they will simply be re-tried
			// at a later point).
			weight_used.saturating_accrue(T::WeightInfo::derive_ltv());
			loan_account.derive_ltv(&price_cache).and_then(|ltv| {
				let _ =
					loan_account.derive_and_charge_interest(ltv, &price_cache, &mut weight_used);

				let new_ltv = if let Ok(true) = loan_account.process_auto_top_up(
					borrower_id,
					ltv,
					&price_cache,
					&mut weight_used,
				) {
					// A successful topup means we have to re-derive LTV
					weight_used.saturating_accrue(T::WeightInfo::derive_ltv());
					loan_account.derive_ltv(&price_cache)
				} else {
					Ok(ltv)
				};

				// `new_ltv` should always be Ok (otherwise we wouldn't be able to derive LTV the
				// first time), but let's avoid unwrapping as a defensive measure:
				new_ltv.and_then(|new_ltv| {
					loan_account.update_liquidation_status(
						borrower_id,
						new_ltv,
						&price_cache,
						&mut weight_used,
					)
				})
			})
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
		collateral_topup_asset: Option<Asset>,
		extra_collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<LoanId, DispatchError> {
		T::LpRegistrationApi::ensure_has_refund_address_for_asset(&borrower_id, asset)
			.map_err(|_| Error::<T>::NoRefundAddressSet)?;

		let price_cache = OraclePriceCache::<T>::default();

		let config = LendingConfig::<T>::get();
		ensure!(
			amount_to_borrow >=
				price_cache.amount_from_usd_value(asset, config.minimum_loan_amount_usd)?,
			Error::<T>::RemainingAmountBelowMinimum
		);

		let loan_id = NextLoanId::<T>::get();
		NextLoanId::<T>::set(loan_id + 1);

		LoanAccounts::<T>::mutate(&borrower_id, |maybe_account| {
			let account = Self::create_or_update_loan_account(
				borrower_id.clone(),
				maybe_account,
				collateral_topup_asset,
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

			account.expand_loan_inner(loan, amount_to_borrow, extra_collateral, &price_cache)?;

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

			loan_account.expand_loan_inner(
				loan,
				extra_amount_to_borrow,
				extra_collateral,
				&price_cache,
			)?;

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

			let repayment_amount = match repayment_amount {
				RepaymentAmount::Full => loan.owed_principal,
				RepaymentAmount::Exact(amount) => {
					if amount < loan.owed_principal {
						ensure!(
							price_cache.usd_value_of_allow_stale(loan.asset, amount)? >=
								config.minimum_update_loan_amount_usd,
							Error::<T>::AmountBelowMinimum
						);
					}

					amount
				},
			};

			let loan_asset = loan.asset;

			T::Balance::try_debit_account(borrower_id, loan_asset, repayment_amount)?;

			let excess_amount = loan.repay_principal(repayment_amount);

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

			// NOTE: even if we settle the last loan here, we don't remove
			// the account as there must still be collateral in the account (it is not
			// released automatically).

			Ok::<_, DispatchError>(())
		})
	}

	#[transactional]
	fn add_collateral(
		borrower_id: &Self::AccountId,
		collateral_topup_asset: Option<Asset>,
		collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
		ensure_non_zero_collateral::<T>(&collateral)?;

		let price_cache = OraclePriceCache::<T>::default();

		ensure!(
			price_cache.total_usd_value_of_allow_stale(&collateral,)? >=
				LendingConfig::<T>::get().minimum_update_collateral_amount_usd,
			Error::<T>::AmountBelowMinimum
		);

		LoanAccounts::<T>::mutate(borrower_id, |maybe_account| {
			let loan_account = Self::create_or_update_loan_account(
				borrower_id.clone(),
				maybe_account,
				collateral_topup_asset,
			)?;

			loan_account.try_adding_collateral_from_free_balance(collateral)?;

			Ok(())
		})
	}

	#[transactional]
	fn remove_collateral(
		borrower_id: &Self::AccountId,
		collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
		let price_cache = OraclePriceCache::<T>::default();

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
				let total_collateral_usd = price_cache.total_usd_value_of(&collateral)?;
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
				loan_account.derive_ltv(&price_cache)? > chp_config.ltv_thresholds.target.into()
			{
				fail!(Error::<T>::LtvTooHigh);
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

	fn update_collateral_topup_asset(
		borrower_id: &Self::AccountId,
		collateral_topup_asset: Option<Asset>,
	) -> Result<(), DispatchError> {
		LoanAccounts::<T>::try_mutate(borrower_id, |maybe_account| {
			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanAccountNotFound)?;

			if loan_account.collateral_topup_asset != collateral_topup_asset {
				loan_account.collateral_topup_asset = collateral_topup_asset;

				Self::deposit_event(Event::CollateralTopupAssetUpdated {
					borrower_id: borrower_id.clone(),
					collateral_topup_asset,
				});
			}

			Ok(())
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
								log_or_panic!("Unable to find liquidation swap (swap request id: {swap_request_id}) for loan_id: {loan_id})");
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
						Some(loan) => loan.repay_via_liquidation(output_amount, is_voluntary),
						None => {
							// In rare cases it may be possible for the loan to no longer exist if
							// e.g. the principal was fully covered by a prior liquidation swap.
							output_amount
						},
					};

					// Any amount left after repaying the loan is added to the borrower's
					// collateral balance:
					if excess_amount > 0 {
						loan_account.add_new_collateral(
							BTreeMap::from([(liquidation_swap.to_asset, excess_amount)]),
							CollateralAddedActionType::SystemLiquidationExcessAmount,
						);
					}

					// If this swap is the last liquidation swap for the loan, we should
					// "settle" it (even if it hasn't been repaid in full):
					if is_last_liquidation_swap_for_loan {
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
		collateral_topup_asset: Option<Asset>,
	) -> Result<&mut LoanAccount<T>, Error<T>> {
		let account = maybe_account.get_or_insert(LoanAccount::new(borrower_id.clone()));

		if account.collateral_topup_asset != collateral_topup_asset {
			account.collateral_topup_asset = collateral_topup_asset;
			Self::deposit_event(Event::CollateralTopupAssetUpdated {
				borrower_id,
				collateral_topup_asset: account.collateral_topup_asset,
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

fn ensure_non_zero_collateral<T: Config>(
	collateral: &BTreeMap<Asset, AssetAmount>,
) -> Result<(), Error<T>> {
	ensure!(!collateral.is_empty(), Error::<T>::EmptyCollateral);
	ensure!(collateral.values().all(|amount| *amount > 0), Error::<T>::EmptyCollateral);

	Ok(())
}
