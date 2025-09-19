use cf_amm_math::{invert_price, relative_price, Price};
use cf_primitives::{DcaParameters, SwapRequestId};
use cf_traits::{ExpiryBehaviour, LendingSwapType, PriceLimitsAndExpiry};
use frame_support::{
	fail,
	sp_runtime::{FixedPointNumber, FixedU64, PerThing},
};

use super::*;

#[cfg(test)]
mod general_lending_tests;

pub enum LoanRepaymentOutcome {
	// In case of full repayment, we may have some excess amount left
	// over which the caller of `repay_loan` will need to allocate somewhere
	// (likely return to the borrower).
	FullyRepaid { excess_amount: AssetAmount },
	PartiallyRepaid,
}

/// Helps to link swap id in liquidation status to loan id
#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct LiquidationSwap {
	loan_id: LoanId,
	from_asset: Asset,
	to_asset: Asset,
}

/// Whether the account's collateral is being liquidated (and if so, stores ids of liquidation
/// swaps)
#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum LiquidationStatus {
	NoLiquidation,
	Liquidating { liquidation_swaps: BTreeMap<SwapRequestId, LiquidationSwap>, is_hard: bool },
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

	/// Returns the account's collateral including any amounts that are in liquidation swaps.
	pub fn get_total_collateral(&self) -> BTreeMap<Asset, AssetAmount> {
		// Note that in order to keep things simple we don't guarantee that all of the
		// all collateral is being liquidated (e.g. it is possible for the user to top
		// up collateral during liquidation in which case we currently don't update the
		// liquidation swaps), but we *do* include any collateral sitting in the account
		// when determining account's collateralisation ratio.
		//
		// Start with the collateral sitting in the account:
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
		borrower_id: &T::AccountId,
		collateral: &BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
		for (asset, amount) in collateral {
			ensure!(
				T::SafeMode::get().add_collateral_enabled.contains(asset),
				Error::<T>::AddingCollateralDisabled
			);
			T::Balance::try_debit_account(borrower_id, *asset, *amount)?;
			self.collateral.entry(*asset).or_default().saturating_accrue(*amount);
		}

		Ok(())
	}

	/// Computes account's total collateral value in USD, including what's in liquidation swaps.
	pub fn total_collateral_usd_value(&self) -> Result<AssetAmount, Error<T>> {
		self.get_total_collateral()
			.iter()
			.map(|(asset, amount)| usd_value_of::<T>(*asset, *amount))
			.try_fold(0u128, |acc, x| Ok(acc.saturating_add(x?)))
	}

	pub fn update_liquidation_status(&mut self, borrower_id: &T::AccountId) {
		let config = LendingConfig::<T>::get();

		let Ok(ltv) = self.derive_ltv() else {
			// Don't change liquidation status if we can't determine the
			// collateralisation ratio
			return;
		};

		// Every time we transition from a liquidating state we abort all liquidation swaps
		// and repay any swapped into principal. If the next state is "NoLiquidation", the
		// collateral is returned into the loan account; if it is "Liquidating", the collateral
		// is used in the new liquidation swaps.
		// TODO (next): split this into two parts to make the borrower checker happy?
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
					// Transition from hard liquidation to active:
					let swaps = core::mem::take(liquidation_swaps);
					let collateral = self.abort_liquidation_swaps(&swaps);
					self.return_collateral(collateral);
				} else if ltv < config.ltv_thresholds.hard_liquidation_abort {
					// Transition from hard liquidation to soft liquidation:
					let swaps = core::mem::take(liquidation_swaps);
					let collateral = self.abort_liquidation_swaps(&swaps);
					self.init_liquidation_swaps(borrower_id, collateral, false);
				}
			},
			LiquidationStatus::Liquidating { liquidation_swaps, .. } => {
				if ltv > config.ltv_thresholds.hard_liquidation {
					// Transition from soft liquidation to hard liquidation:
					let swaps = core::mem::take(liquidation_swaps);
					let collateral = self.abort_liquidation_swaps(&swaps);
					self.init_liquidation_swaps(borrower_id, collateral, true);
				} else if ltv < config.ltv_thresholds.soft_liquidation_abort {
					// Transition from soft liquidation to active:
					let swaps = core::mem::take(liquidation_swaps);
					let collateral = self.abort_liquidation_swaps(&swaps);
					self.return_collateral(collateral);
				}
			},
		}
	}

	// Abort all provided liquidation swaps, repays any already swapped principal assets and
	// returns remaining collateral assets alongside the corresponding loan information.
	fn abort_liquidation_swaps(
		&mut self,
		liquidation_swaps: &BTreeMap<SwapRequestId, LiquidationSwap>,
	) -> Vec<AssetCollateralForLoan> {
		let mut collateral_collected = vec![];

		// It should be rare, but not impossible that a partial liquidation fully repays
		// the loan. We delay settling them until the end of this function to make sure that
		// all liquidations fees are correctly paid.
		let mut fully_repaid_loans = vec![];

		for (swap_request_id, LiquidationSwap { loan_id, from_asset, to_asset }) in
			liquidation_swaps
		{
			if let Some(swap_progress) = T::SwapRequestHandler::abort_swap_request(*swap_request_id)
			{
				let excess_amount = match self.repay_principal(
					*loan_id,
					*to_asset,
					swap_progress.accumulated_output_amount,
					true, /* liquidation */
				) {
					Ok(LoanRepaymentOutcome::FullyRepaid { excess_amount }) => {
						fully_repaid_loans.push(loan_id);
						excess_amount
					},
					Ok(LoanRepaymentOutcome::PartiallyRepaid) => 0,
					Err(_) => {
						// On failure the full amount is unspent
						swap_progress.accumulated_output_amount
					},
				};

				if excess_amount > 0 {
					T::Balance::credit_account(&self.borrower_id, *to_asset, excess_amount);
				}

				collateral_collected.push(AssetCollateralForLoan {
					loan_id: *loan_id,
					loan_asset: *to_asset,
					collateral_asset: *from_asset,
					collateral_amount: swap_progress.remaining_input_amount,
				});
			} else {
				log_or_panic!("Failed to abort swap request: {swap_request_id}");
			}
		}

		for loan_id in fully_repaid_loans {
			self.settle_loan(*loan_id, true /* via liquidation */);
		}

		collateral_collected
	}

	/// Computes the total amount owed in account's loans in USD adjusting for the amount that will
	/// be repaid by the collateral that has already been swapped into the loan asset.
	pub fn total_owed_usd_value(&self) -> Result<AssetAmount, Error<T>> {
		let total_owed = self
			.loans
			.values()
			.map(|loan| loan.owed_principal_usd_value().ok())
			.try_fold(0u128, |acc, x| acc.checked_add(x?))
			.ok_or(Error::<T>::OraclePriceUnavailable)?;

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

	pub fn charge_interest(&mut self) -> Result<(), Error<T>> {
		let config = LendingConfig::<T>::get();

		if self.liquidation_status != LiquidationStatus::NoLiquidation {
			// For simplicity, we don't charge interest during liquidations
			// (the account will already incur a liquidation fee)
			return Ok(())
		}

		for (loan_id, loan) in self.loans.iter_mut() {
			if frame_system::Pallet::<T>::block_number().saturating_sub(loan.created_at_block) %
				config.interest_payment_interval_blocks.into() ==
				0u32.into()
			{
				let interest_rate_per_payment_interval = {
					let utilisation = GeneralLendingPools::<T>::get(loan.asset)
						.map(|pool| pool.get_utilisation())
						.unwrap_or_default();

					config.derive_interest_rate_per_payment_interval(loan.asset, utilisation)
				};

				let mut remaining_interest_amount_in_loan_asset =
					interest_rate_per_payment_interval * loan.owed_principal;

				// Interest is charged from the primary collateral asset first. If it fails to cover
				// the interest, we use the remaining assets:
				let collateral_asset_order = [self.primary_collateral_asset]
					.into_iter()
					.chain(
						self.collateral
							.keys()
							.copied()
							.filter(|asset| *asset != self.primary_collateral_asset),
					)
					.collect::<Vec<_>>(); // collecting to make borrow checker happy

				let mut interest_amounts = BTreeMap::new();

				for collateral_asset in collateral_asset_order {
					// Determine how much should be charged from the given collateral asset
					let interest_required_in_collateral_asset = equivalent_amount::<T>(
						loan.asset,
						collateral_asset,
						remaining_interest_amount_in_loan_asset,
					)?;

					if let Some(available_collateral_amount) =
						self.collateral.get_mut(&collateral_asset)
					{
						// Don't charge more than what's available
						let amount_charged = core::cmp::min(
							interest_required_in_collateral_asset,
							*available_collateral_amount,
						);

						available_collateral_amount.saturating_reduce(amount_charged);

						// Reduce the remaining interest amount to pay in loan asset's terms
						{
							let amount_charged_in_loan_asset =
								if amount_charged == interest_required_in_collateral_asset {
									remaining_interest_amount_in_loan_asset
								} else {
									equivalent_amount(collateral_asset, loan.asset, amount_charged)?
								};

							remaining_interest_amount_in_loan_asset
								.saturating_reduce(amount_charged_in_loan_asset);
						}

						loan.fees_paid
							.entry(collateral_asset)
							.or_default()
							.saturating_accrue(amount_charged);

						interest_amounts.insert(collateral_asset, amount_charged);

						// TODO: emit network fee in any event?
						let remaining_fees = Pallet::<T>::take_network_fee(
							amount_charged,
							collateral_asset,
							config.network_fee_contributions.from_interest,
						);

						Pallet::<T>::accrue_fees(loan.asset, collateral_asset, remaining_fees);

						if remaining_interest_amount_in_loan_asset == 0 {
							break;
						}
					}
				}

				Pallet::<T>::deposit_event(Event::InterestTaken {
					loan_id: *loan_id,
					amounts: interest_amounts,
				});
			}
		}

		Ok(())
	}

	/// Checks if a top up is required and if so, performs it
	pub fn process_auto_top_up(&mut self, borrower_id: &T::AccountId) -> Result<(), Error<T>> {
		// Auto top up is currently only possible from the primary collateral asset
		let config = LendingConfig::<T>::get();

		if self.derive_ltv()? <= config.ltv_thresholds.topup {
			return Ok(())
		}

		let top_up_required_in_usd = {
			let loan_value_in_usd = self.total_owed_usd_value()?;
			let collateral_required_in_usd = config
				.ltv_thresholds
				.target
				.reciprocal()
				.map(|ltv_inverted| ltv_inverted.saturating_mul_int(loan_value_in_usd))
				// This effectively disables auto top up if the ltv target erroneously set to 0:
				.unwrap_or(0);

			collateral_required_in_usd.saturating_sub(self.total_collateral_usd_value()?)
		};

		let top_up_required_in_collateral_asset =
			amount_from_usd_value::<T>(self.primary_collateral_asset, top_up_required_in_usd)?;

		try_sweep::<T>(borrower_id);

		// Don't attempt to charge more than what's available:
		let top_up_amount = core::cmp::min(
			T::Balance::get_balance(borrower_id, self.primary_collateral_asset),
			top_up_required_in_collateral_asset,
		);

		if top_up_amount > 0 {
			if T::Balance::try_debit_account(
				borrower_id,
				self.primary_collateral_asset,
				top_up_amount,
			)
			.is_ok()
			{
				self.collateral
					.entry(self.primary_collateral_asset)
					.or_default()
					.saturating_accrue(top_up_amount);
			} else {
				log_or_panic!("Unable to debit after checking balance");
			}
		}

		Ok(())
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
			let distribution = distribute_proportionally(
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

	/// Return collateral from aborted liquidation swaps (to be called when no further liquidation
	/// is required).
	fn return_collateral(&mut self, liquidation_swaps_collected: Vec<AssetCollateralForLoan>) {
		for AssetCollateralForLoan { collateral_asset, collateral_amount, .. } in
			liquidation_swaps_collected
		{
			self.collateral
				.entry(collateral_asset)
				.or_default()
				.saturating_accrue(collateral_amount);
		}

		self.liquidation_status = LiquidationStatus::NoLiquidation;
	}

	fn settle_loan(&mut self, loan_id: LoanId, via_liquidation: bool) {
		if let Some(loan) = self.loans.remove(&loan_id) {
			Pallet::<T>::deposit_event(Event::LoanSettled {
				loan_id,
				total_fees: loan.fees_paid,
				via_liquidation,
			});
		}
	}

	/// Repays (fully or partially) the loan with `provided_amount` (that was either debited from
	/// the account or received during liquidation). Returns any unused amount. If the loan does not
	/// exist, returns `None`.
	#[transactional]
	fn repay_principal(
		&mut self,
		loan_id: LoanId,
		provided_asset: Asset,
		provided_amount: AssetAmount,
		should_charge_liquidation_fee: bool,
	) -> Result<LoanRepaymentOutcome, DispatchError> {
		let config = LendingConfig::<T>::get();

		let Some(loan) = self.loans.get_mut(&loan_id) else {
			// In rare cases it may be possible for the loan to no longer exist if
			// e.g. the principal was fully covered by a prior liquidation swap.
			fail!(Error::<T>::LoanNotFound);
		};

		if loan.asset != provided_asset {
			log_or_panic!(
				"Unexpected asset {} provided to repay loan {loan_id}, expected {}",
				provided_asset,
				loan.asset
			);
			fail!(Error::<T>::InternalInvariantViolation);
		}

		let (provided_amount_after_fees, liquidation_fee) = if should_charge_liquidation_fee {
			let liquidation_fee =
				config.get_config_for_asset(loan.asset).liquidation_fee * provided_amount;
			let after_fees = provided_amount.saturating_sub(liquidation_fee);

			(after_fees, liquidation_fee)
		} else {
			(provided_amount, 0)
		};

		// Making sure the user doesn't pay more than the total principal:
		let repayment_amount = core::cmp::min(provided_amount_after_fees, loan.owed_principal);

		loan.repay_funds(repayment_amount);

		let liquidation_fees = if liquidation_fee > 0 {
			let remaining_fee = Pallet::<T>::take_network_fee(
				liquidation_fee,
				loan.asset,
				config.network_fee_contributions.from_liquidation_fee,
			);

			loan.fees_paid.entry(loan.asset).or_default().saturating_accrue(liquidation_fee);

			Pallet::<T>::accrue_fees(loan.asset, loan.asset, remaining_fee);

			BTreeMap::from([(loan.asset, liquidation_fee)])
		} else {
			Default::default()
		};

		Pallet::<T>::deposit_event(Event::LoanRepaid {
			loan_id,
			amount: repayment_amount,
			liquidation_fees,
		});

		if loan.owed_principal == 0 {
			// NOTE: in some cases we may want to delay settling/removing the loan (e.g. there may
			// be pending liquidation swaps to process), so we let the caller settle it instead
			// of doing it here.
			Ok(LoanRepaymentOutcome::FullyRepaid {
				excess_amount: provided_amount_after_fees.saturating_sub(repayment_amount),
			})
		} else {
			Ok(LoanRepaymentOutcome::PartiallyRepaid)
		}
	}
}

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct GeneralLoan<T: Config> {
	pub asset: Asset,
	pub created_at_block: BlockNumberFor<T>,
	pub owed_principal: AssetAmount,
	/// Total fees paid to the pool throughout the lifetime of the loan
	/// (these are to be used for informational purposes only)
	pub fees_paid: BTreeMap<Asset, AssetAmount>,
}

impl<T: Config> GeneralLoan<T> {
	fn owed_principal_usd_value(&self) -> Result<AssetAmount, Error<T>> {
		usd_value_of::<T>(self.asset, self.owed_principal)
	}

	/// Repays previously borrowed funds to the pool in pool's asset, reducing the owed principal
	/// amount.
	fn repay_funds(&mut self, amount: AssetAmount) {
		GeneralLendingPools::<T>::mutate(self.asset, |maybe_pool| {
			if let Some(pool) = maybe_pool.as_mut() {
				pool.receive_repayment(amount);
			} else {
				log_or_panic!("CHP Pool must exist for asset {}", self.asset);
			}
		});

		self.owed_principal.saturating_reduce(amount);
	}
}

fn get_price<T: Config>(asset: Asset) -> Result<Price, Error<T>> {
	Ok(T::PriceApi::get_price(asset).ok_or(Error::<T>::OraclePriceUnavailable)?.price)
}

/// Uses oracle prices to calculate the amount of `asset_2` that's equivalent in USD value to
/// `amount` of `asset_1`
fn equivalent_amount<T: Config>(
	asset_1: Asset,
	asset_2: Asset,
	amount: AssetAmount,
) -> Result<AssetAmount, Error<T>> {
	let asset_1_price = get_price::<T>(asset_1)?;
	let asset_2_price = get_price::<T>(asset_2)?;

	// how much of asset 2 you get per asset 1
	let price = relative_price(asset_1_price, asset_2_price);

	Ok(cf_amm_math::output_amount_ceil(amount.into(), price).unique_saturated_into())
}

/// Uses oracle prices to calculate the USD value of the given asset amount
fn usd_value_of<T: Config>(asset: Asset, amount: AssetAmount) -> Result<AssetAmount, Error<T>> {
	let price_in_usd = get_price::<T>(asset)?;
	Ok(cf_amm_math::output_amount_ceil(amount.into(), price_in_usd).unique_saturated_into())
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
					(total_amount_usd / LendingConfig::<T>::get().liquidation_swap_chunk_size_usd)
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
		// Fee swaps are expected to be small so we won't bother splitting them into chunks
		LendingSwapType::FeeSwap { .. } => None,
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

			// Not being able to charge interest or top up collateral is OK (most likely due to
			// stale oracle)
			let _ = loan_account.charge_interest();
			let _ = loan_account.process_auto_top_up(borrower_id);
			loan_account.update_liquidation_status(borrower_id);
		});
	}

	// Swap fees in every asset every FEE_CHECK_INTERVAL blocks, but only if they exceed
	// FEE_SWAP_THRESHOLD_USD in value
	if current_block % config.fee_swap_interval_blocks.into() == 0u32.into() {
		for pool_asset in PendingPoolFees::<T>::iter_keys().collect::<Vec<_>>() {
			PendingPoolFees::<T>::mutate(pool_asset, |pending_fees| {
				for (collateral_asset, fee_amount) in pending_fees {
					let Ok(fee_usd_value) = usd_value_of::<T>(*collateral_asset, *fee_amount)
					else {
						// Don't swap yet if we can't determine asset's price
						continue;
					};

					if fee_usd_value >= config.fee_swap_threshold_usd {
						let fees_to_swap = core::mem::take(fee_amount);
						let swap_request_id = initiate_swap::<T>(
							*collateral_asset,
							fees_to_swap,
							pool_asset,
							LendingSwapType::FeeSwap { pool_asset },
							config.fee_swap_max_oracle_slippage,
						);

						Pallet::<T>::deposit_event(Event::LendingPoolFeeCollectionInitiated {
							asset: pool_asset,
							swap_request_id,
						});
					}
				}
			});
		}

		// Additionally swapp all network fee contributions from fees:
		for asset in PendingNetworkFees::<T>::iter_keys().collect::<Vec<_>>() {
			PendingNetworkFees::<T>::mutate(asset, |fee_amount| {
				// NOTE: if asset is FLIP, we shouldn't need to swap, but it should still work,
				// and it seems easiest to not write a special case (esp if we only support
				// boost for BTC)
				if *fee_amount > 0 {
					let swap_request_id =
						T::SwapRequestHandler::init_network_fee_swap_request(asset, *fee_amount);

					Pallet::<T>::deposit_event(Event::LendingNetworkFeeCollectionInitiated {
						swap_request_id,
					});
				}
				*fee_amount = 0;
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
		ensure!(
			T::SafeMode::get().borrowing_enabled.contains(&asset),
			Error::<T>::LoanCreationDisabled
		);

		let config = LendingConfig::<T>::get();

		let loan_id = NextLoanId::<T>::get();
		NextLoanId::<T>::set(loan_id + 1);

		LoanAccounts::<T>::mutate(&borrower_id, |maybe_account| {
			let account = Self::create_or_update_loan_account(
				borrower_id.clone(),
				maybe_account,
				primary_collateral_asset,
			)?;

			let primary_collateral_asset = account.primary_collateral_asset;

			ensure!(
				extra_collateral.contains_key(&primary_collateral_asset),
				Error::<T>::InvalidLoanParameters
			);

			account.primary_collateral_asset = primary_collateral_asset;

			account.try_adding_collateral_from_free_balance(&borrower_id, &extra_collateral)?;

			GeneralLendingPools::<T>::try_mutate(asset, |pool| {
				let pool = pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

				pool.provide_funds_for_loan(amount_to_borrow).map_err(Error::<T>::from)?;

				Ok::<_, DispatchError>(())
			})?;

			let mut loan = GeneralLoan {
				asset,
				created_at_block: frame_system::Pallet::<T>::current_block_number(),
				owed_principal: amount_to_borrow,
				fees_paid: BTreeMap::new(),
			};

			let origination_fee = Self::charge_origination_fee(
				&borrower_id,
				&mut loan,
				primary_collateral_asset,
				amount_to_borrow,
			)?;

			account.loans.insert(loan_id, loan);

			ensure!(
				account.derive_ltv()? <= config.ltv_thresholds.target,
				Error::<T>::InsufficientCollateral
			);

			T::Balance::credit_account(&borrower_id, asset, amount_to_borrow);

			Self::deposit_event(Event::LoanCreated {
				loan_id,
				borrower_id: borrower_id.clone(),
				asset,
				principal_amount: amount_to_borrow,
				origination_fee,
			});

			Ok::<_, DispatchError>(())
		})?;

		Ok(loan_id)
	}

	/// Borrows `extra_amount_to_borrow` by expanding `loan_id`. Adds any extra collateral to the
	/// account (which may be required to cover the new total owed amount).
	#[transactional]
	fn expand_loan(
		borrower_id: Self::AccountId,
		loan_id: LoanId,
		extra_amount_to_borrow: AssetAmount,
		extra_collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
		let config = LendingConfig::<T>::get();

		LoanAccounts::<T>::mutate(&borrower_id, |maybe_account| {
			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanNotFound)?;

			{
				let loan = loan_account.loans.get_mut(&loan_id).ok_or(Error::<T>::LoanNotFound)?;

				ensure!(
					T::SafeMode::get().borrowing_enabled.contains(&loan.asset),
					Error::<T>::LoanCreationDisabled
				);

				loan.owed_principal.saturating_accrue(extra_amount_to_borrow);
			}

			loan_account
				.try_adding_collateral_from_free_balance(&borrower_id, &extra_collateral)?;

			if loan_account.derive_ltv()? > config.ltv_thresholds.target {
				return Err(Error::<T>::InsufficientCollateral.into());
			}

			// NOTE: have to get a new reference to the loan again to satisfy the borrow checker
			let loan = loan_account.loans.get_mut(&loan_id).expect("checked above");

			let primary_collateral_asset = loan_account.primary_collateral_asset;
			let loan_asset = loan.asset;

			GeneralLendingPools::<T>::try_mutate(loan_asset, |pool| {
				let pool = pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

				pool.provide_funds_for_loan(extra_amount_to_borrow).map_err(Error::<T>::from)?;

				Ok::<_, DispatchError>(())
			})?;

			let origination_fee = Self::charge_origination_fee(
				&borrower_id,
				loan,
				primary_collateral_asset,
				extra_amount_to_borrow,
			)?;

			Self::deposit_event(Event::LoanUpdated {
				loan_id,
				extra_principal_amount: extra_amount_to_borrow,
				origination_fee,
			});

			T::Balance::credit_account(&borrower_id, loan_asset, extra_amount_to_borrow);

			Ok::<_, DispatchError>(())
		})?;

		Ok(())
	}

	/// Repays (fully or partially) a loan.
	#[transactional]
	fn try_making_repayment(
		borrower_id: &T::AccountId,
		loan_id: LoanId,
		repayment_amount: AssetAmount,
	) -> Result<(), DispatchError> {
		LoanAccounts::<T>::mutate(borrower_id, |maybe_account| {
			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanNotFound)?;

			let loan_asset =
				loan_account.loans.get_mut(&loan_id).ok_or(Error::<T>::LoanNotFound)?.asset;

			T::Balance::try_debit_account(borrower_id, loan_asset, repayment_amount)?;

			if let LoanRepaymentOutcome::FullyRepaid { excess_amount } = loan_account
				.repay_principal(
					loan_id,
					loan_asset,
					repayment_amount,
					false, /* no liquidation fee */
				)? {
				loan_account.settle_loan(loan_id, false /* via liquidation */);

				T::Balance::credit_account(borrower_id, loan_asset, excess_amount);
			}

			Ok::<_, DispatchError>(())
		})
	}

	#[transactional]
	fn add_collateral(
		borrower_id: &Self::AccountId,
		primary_collateral_asset: Option<Asset>,
		collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
		LoanAccounts::<T>::mutate(borrower_id, |maybe_account| {
			let loan_account = Self::create_or_update_loan_account(
				borrower_id.clone(),
				maybe_account,
				primary_collateral_asset,
			)?;

			if let Some(primary_collateral_asset) = primary_collateral_asset {
				loan_account.primary_collateral_asset = primary_collateral_asset;
			}

			loan_account.try_adding_collateral_from_free_balance(borrower_id, &collateral)?;

			Self::deposit_event(Event::CollateralAdded {
				borrower_id: borrower_id.clone(),
				collateral,
				primary_collateral_asset: loan_account.primary_collateral_asset,
			});

			Ok::<_, DispatchError>(())
		})
	}

	#[transactional]
	fn remove_collateral(
		borrower_id: &Self::AccountId,
		primary_collateral_asset: Option<Asset>,
		collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
		LoanAccounts::<T>::mutate(borrower_id, |maybe_account| {
			let chp_config = LendingConfig::<T>::get();

			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanAccountNotFound)?;

			if let Some(primary_collateral_asset) = primary_collateral_asset {
				loan_account.primary_collateral_asset = primary_collateral_asset;
			}

			for (asset, amount) in &collateral {
				ensure!(
					T::SafeMode::get().remove_collateral_enabled.contains(asset),
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
				loan_account.derive_ltv()? > chp_config.ltv_thresholds.target
			{
				fail!(Error::<T>::InsufficientCollateral);
			}

			Self::deposit_event(Event::CollateralRemoved {
				borrower_id: borrower_id.clone(),
				collateral,
				primary_collateral_asset: loan_account.primary_collateral_asset,
			});

			if loan_account.collateral.is_empty() && loan_account.loans.is_empty() {
				*maybe_account = None;
			}

			Ok(())
		})
	}
}

impl<T: Config> cf_traits::lending::ChpSystemApi for Pallet<T> {
	type AccountId = T::AccountId;

	fn process_loan_swap_outcome(
		swap_request_id: SwapRequestId,
		swap_type: LendingSwapType<Self::AccountId>,
		output_amount: AssetAmount,
	) {
		match swap_type {
			LendingSwapType::Liquidation { borrower_id, loan_id } => {
				LoanAccounts::<T>::mutate(&borrower_id, |maybe_account| {
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

					let remaining_amount = match loan_account.repay_principal(
						loan_id,
						liquidation_swap.to_asset,
						output_amount,
						true, /* liquidation */
					) {
						Ok(LoanRepaymentOutcome::FullyRepaid { excess_amount }) => {
							// NOTE: we don't need to worry about settling the loan just yet
							// as there may be more liquidation swaps to process for the loan.
							excess_amount
						},
						Ok(LoanRepaymentOutcome::PartiallyRepaid) => 0,
						Err(_) => {
							// On failure, the full amount is considered unspent
							output_amount
						},
					};

					// Any amount left after repaying the loan should be returned to the
					// borrower:
					T::Balance::credit_account(
						&borrower_id,
						liquidation_swap.to_asset,
						remaining_amount,
					);

					// If this swap is the last liquidation swap for the loan, we should
					// "settle" it (even if it hasn't been repaid in full):
					if is_last_liquidation_swap {
						loan_account.settle_loan(loan_id, true /* via liquidation */);
					}
				});
			},
			LendingSwapType::FeeSwap { pool_asset } => {
				GeneralLendingPools::<T>::mutate(pool_asset, |pool| {
					let Some(pool) = pool.as_mut() else {
						log_or_panic!("Pool must exist for {pool_asset}");
						return;
					};

					pool.receive_fees(output_amount);
				});
			},
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Pays fee to the pool in *any* asset. If the asset doesn't match the pool's native
	/// asset, the amount will be combined with other pending fees awaiting a swap into the
	/// native asset.
	fn accrue_fees(loan_asset: Asset, fee_asset: Asset, fee_amount: AssetAmount) {
		if loan_asset == fee_asset {
			GeneralLendingPools::<T>::mutate(loan_asset, |maybe_pool| {
				if let Some(pool) = maybe_pool.as_mut() {
					pool.receive_fees(fee_amount);
				} else {
					log_or_panic!("Lending Pool must exist for asset {}", loan_asset);
				}
			});
		} else {
			PendingPoolFees::<T>::mutate(loan_asset, |pending_fees| {
				pending_fees.entry(fee_asset).or_insert(0).saturating_accrue(fee_amount);
			});
		}
	}

	/// Takes a portion from the full fee and sends it to where network fees go, returning the rest.
	fn take_network_fee(
		full_fee_amount: AssetAmount,
		fee_asset: Asset,
		network_fee_contribution: Percent,
	) -> AssetAmount {
		let network_fee_amount = network_fee_contribution * full_fee_amount;

		PendingNetworkFees::<T>::mutate(fee_asset, |pending_amount| {
			pending_amount.saturating_accrue(network_fee_amount);
		});

		full_fee_amount.saturating_sub(network_fee_amount)
	}

	fn create_or_update_loan_account(
		borrower_id: T::AccountId,
		maybe_account: &mut Option<LoanAccount<T>>,
		primary_collateral_asset: Option<Asset>,
	) -> Result<&mut LoanAccount<T>, Error<T>> {
		let account = match maybe_account {
			Some(account) => {
				// If the user provides primary collateral asset, we update it:
				if let Some(asset) = primary_collateral_asset {
					account.primary_collateral_asset = asset;
				}
				account
			},
			None => {
				let primary_collateral_asset =
					primary_collateral_asset.ok_or(Error::<T>::InvalidLoanParameters)?;

				let account = LoanAccount::new(borrower_id, primary_collateral_asset);
				maybe_account.insert(account)
			},
		};

		Ok(account)
	}

	/// Derives the required origination fee, charges it from the borrower's account,
	/// and sends it to the pool.
	fn charge_origination_fee(
		borrower_id: &T::AccountId,
		loan: &mut GeneralLoan<T>,
		primary_collateral_asset: Asset,
		principal: AssetAmount,
	) -> Result<AssetAmount, DispatchError> {
		let config = LendingConfig::<T>::get();

		let origination_fee_amount = equivalent_amount::<T>(
			loan.asset,
			primary_collateral_asset,
			config.origination_fee(loan.asset) * principal,
		)?;

		loan.fees_paid
			.entry(primary_collateral_asset)
			.or_default()
			.saturating_accrue(origination_fee_amount);

		T::Balance::try_debit_account(
			borrower_id,
			primary_collateral_asset,
			origination_fee_amount,
		)?;

		let remaining_fee = Self::take_network_fee(
			origination_fee_amount,
			primary_collateral_asset,
			config.network_fee_contributions.from_origination_fee,
		);

		Self::accrue_fees(loan.asset, primary_collateral_asset, remaining_fee);

		Ok(origination_fee_amount)
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
		pub total_fees: Vec<AssetAndAmount<Amount>>,
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
					total_fees: loan
						.fees_paid
						.into_iter()
						.map(|(asset, amount)| AssetAndAmount { asset, amount })
						.collect(),
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
	/// Borrowers aren't allowed to add more collateral if their ltv would drop below this
	/// threshold.
	pub minimum: FixedU64,
	/// Borrowers aren't allowed to borrow more (or withdraw collateral) if their Loan-to-value
	/// ratio (principal/collateral) would exceed this threshold.
	pub target: FixedU64,
	/// Reaching this threshold will trigger a top-up of the collateral
	pub topup: FixedU64,
	/// Reaching this threshold will trigger soft liquidation account's loans
	pub soft_liquidation: FixedU64,
	/// If a loan that's being liquidated reaches this threshold, it will be considered
	/// "healthy" again and the liquidation will be aborted. This is meant to be slightly
	/// lower than the soft threshold to avoid frequent oscillations between liquidating and
	/// not liquidating.
	pub soft_liquidation_abort: FixedU64,
	/// Reaching this threshold will trigger hard liquidation of the loan
	pub hard_liquidation: FixedU64,
	/// Same as overcollateralisation_soft_liquidation_abort_threshold, but for
	/// transitioning from hard to soft liquidation
	pub hard_liquidation_abort: FixedU64,
}

impl LtvThresholds {
	pub fn validate(&self) -> DispatchResult {
		ensure!(
			self.minimum <= self.target &&
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
	pub from_interest: Percent,
	pub from_origination_fee: Percent,
	pub from_liquidation_fee: Percent,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
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
	/// Determines how frequently (in blocks) we collect interest payments from loans.
	pub interest_payment_interval_blocks: u32,
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
				interest_at_zero_utilisation,
				interest_at_junction_utilisation,
				Permill::zero(),
				junction_utilisation,
				utilisation,
			)
		} else {
			interpolate_linear_segment(
				interest_at_junction_utilisation,
				interest_at_max_utilisation,
				junction_utilisation,
				Permill::one(),
				utilisation,
			)
		}
	}

	/// Computes the interest rate to be paid each payment interval. Uses Perbill
	/// as the value is likely to be a very small fraction due to the interval being short.
	fn derive_interest_rate_per_payment_interval(
		&self,
		asset: Asset,
		utilisation: Permill,
	) -> Perbill {
		use cf_primitives::BLOCKS_IN_YEAR;

		let interest_rate = self.derive_interest_rate_per_year(asset, utilisation);

		Perbill::from_parts(
			(interest_rate.deconstruct() * (Perbill::ACCURACY / Permill::ACCURACY)) /
				(BLOCKS_IN_YEAR / self.interest_payment_interval_blocks),
		)
	}

	pub fn origination_fee(&self, asset: Asset) -> Permill {
		self.get_config_for_asset(asset).origination_fee
	}

	pub fn liquidation_fee(&self, asset: Asset) -> Permill {
		self.get_config_for_asset(asset).liquidation_fee
	}
}

/// Computes interest rate at utilisation `u` given a linear segment defined by interest values `i0`
/// and `i1` at utilisation `u0` and `u1`, respectively. The code assumes u0 <= u <= u1, i1 >= i0
/// and u0 != u1.
fn interpolate_linear_segment(
	i0: Permill,
	i1: Permill,
	u0: Permill,
	u1: Permill,
	u: Permill,
) -> Permill {
	if u0 > u || u > u1 || i0 > i1 || u0 == u1 {
		log_or_panic!("Invalid interest curve parameters");
		return Permill::zero();
	}

	// Converting everything to u64 to get access to more operations
	let i0 = i0.deconstruct() as u64;
	let i1 = i1.deconstruct() as u64;
	let u0 = u0.deconstruct() as u64;
	let u1 = u1.deconstruct() as u64;
	let u = u.deconstruct() as u64;

	// Slope coefficient:
	let slope = ((i1 - i0) * Permill::ACCURACY as u64) / (u1 - u0);

	let result = i0 + slope * (u - u0) / Permill::ACCURACY as u64;

	u32::try_from(result).map(Permill::from_parts).unwrap_or(Permill::one())
}
