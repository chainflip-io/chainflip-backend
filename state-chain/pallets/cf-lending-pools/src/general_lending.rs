use cf_amm_math::{invert_price, relative_price, Price};
use cf_primitives::SwapRequestId;
use cf_traits::{ExpiryBehaviour, LendingSwapType, PriceLimitsAndExpiry};
use frame_support::{
	fail,
	sp_runtime::{FixedPointNumber, FixedU64},
};

use super::*;

#[cfg(test)]
mod general_lending_tests;

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
	primary_collateral_asset: Asset,
	collateral: BTreeMap<Asset, AssetAmount>,
	loans: BTreeMap<LoanId, GeneralLoan<T>>,
	liquidation_status: LiquidationStatus,
}

impl<T: Config> LoanAccount<T> {
	pub fn new(primary_collateral_asset: Asset) -> Self {
		Self {
			primary_collateral_asset,
			collateral: BTreeMap::new(),
			loans: BTreeMap::new(),
			liquidation_status: LiquidationStatus::NoLiquidation,
		}
	}

	pub fn get_collateral(&self) -> &BTreeMap<Asset, AssetAmount> {
		&self.collateral
	}

	/// Computes account's total collateral value in USD, including what's in liquidation swaps.
	pub fn total_collateral_usd_value(&self) -> Result<AssetAmount, Error<T>> {
		let collateral_in_account_usd_value = self
			.collateral
			.iter()
			.map(|(asset, amount)| usd_value_of::<T>(*asset, *amount).ok())
			.try_fold(0u128, |acc, x| acc.checked_add(x?))
			.ok_or(Error::<T>::OraclePriceUnavailable)?;

		match &self.liquidation_status {
			LiquidationStatus::NoLiquidation => Ok(collateral_in_account_usd_value),
			LiquidationStatus::Liquidating { liquidation_swaps, .. } => {
				let mut total_collateral_usd_value_in_swaps = 0;
				// If we are liquidating loans, some of the collateral will be in pending swaps
				for (swap_request_id, LiquidationSwap { from_asset, .. }) in liquidation_swaps {
					if let Some(swap_progress) =
						T::SwapRequestHandler::inspect_swap_request(*swap_request_id)
					{
						total_collateral_usd_value_in_swaps.saturating_accrue(usd_value_of::<T>(
							*from_asset,
							swap_progress.remaining_input_amount,
						)?);
					} else {
						log_or_panic!("Failed to inspect swap request: {swap_request_id}");
					}
				}

				// Note that in order to keep things simple we don't guarantee that all of the
				// all collateral is being liquidated (e.g. it is possible for the user to top
				// up collateral during liquidation in which case we currently don't update the
				// liquidation swaps), but we *do* include any collateral sitting in the account
				// when determining account's collateralisation ratio.
				Ok(total_collateral_usd_value_in_swaps
					.saturating_add(collateral_in_account_usd_value))
			},
		}
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
			return Err(Error::<T>::InsufficientCollateral);
		}

		Ok(FixedU64::from_rational(principal, collateral))
	}

	pub fn charge_interest(&mut self) -> Result<(), Error<T>> {
		if self.liquidation_status != LiquidationStatus::NoLiquidation {
			// For simplicity, we don't charge interest during liquidations
			// (the account will already incur a liquidation fee)
			return Ok(())
		}

		for loan in self.loans.values() {
			if frame_system::Pallet::<T>::block_number().saturating_sub(loan.created_at_block) %
				INTEREST_PAYMENT_INTERVAL.into() ==
				0u32.into()
			{
				let interest_rate_per_payment_interval = {
					let utilisation = GeneralLendingPools::<T>::get(loan.asset)
						.map(|pool| pool.get_utilisation())
						.unwrap_or_default();

					LendingConfig::<T>::get().derive_interest_rate_per_charge_interval(utilisation)
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

						// Determine how much we actually charged in loan asset's terms
						let amount_charged_in_loan_asset =
							if amount_charged == interest_required_in_collateral_asset {
								remaining_interest_amount_in_loan_asset
							} else {
								equivalent_amount(collateral_asset, loan.asset, amount_charged)?
							};

						remaining_interest_amount_in_loan_asset
							.saturating_reduce(amount_charged_in_loan_asset);

						Pallet::<T>::accrue_fees(loan.asset, collateral_asset, amount_charged);

						if remaining_interest_amount_in_loan_asset == 0 {
							break;
						}
					}
				}
			}
		}

		Ok(())
	}

	/// Checks if a top up is required and if so, performs it
	pub fn process_auto_top_up(&mut self, borrower_id: &T::AccountId) -> Result<(), Error<T>> {
		// Auto top up is currently only possible from the primary collateral asset
		let config = LendingConfig::<T>::get();

		if self.derive_ltv()? <= config.ltv_topup_threshold {
			return Ok(())
		}

		let top_up_required_in_usd = {
			let loan_value_in_usd = self.total_owed_usd_value()?;
			let collateral_required_in_usd = config
				.ltv_target_threshold
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
			let distribution =
				distribute_proportionally(collateral_amount, principal_amounts_usd.iter().cloned());

			for ((loan_id, loan_asset), collateral_amount) in distribution {
				prepared_collateral.push(AssetCollateralForLoan {
					loan_id,
					loan_asset,
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
		let mut liquidation_swaps = BTreeMap::new();

		for AssetCollateralForLoan { loan_id, loan_asset, collateral_asset, collateral_amount } in
			collateral
		{
			let from_asset = collateral_asset;
			let to_asset = loan_asset;

			let max_slippage = if is_hard {
				HARD_LIQUIDATION_MAX_ORACLE_SLIPPAGE
			} else {
				SOFT_LIQUIDATION_MAX_ORACLE_SLIPPAGE
			};

			let swap_request_id = initiate_swap::<T>(
				from_asset,
				collateral_amount,
				to_asset,
				LendingSwapType::Liquidation { borrower_id: borrower_id.clone(), loan_id },
				max_slippage,
			);

			liquidation_swaps
				.insert(swap_request_id, LiquidationSwap { loan_id, from_asset, to_asset });
		}

		Pallet::<T>::deposit_event(Event::LiquidationInitiated {
			borrower_id: borrower_id.clone(),
			swap_request_ids: liquidation_swaps.keys().cloned().collect(),
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

	fn settle_loan(&mut self, loan_id: LoanId) {
		if let Some(_loan) = self.loans.remove(&loan_id) {
			// TODO: record all collected fees/interest and provide the total here:
			Pallet::<T>::deposit_event(Event::LoanSettled {
				loan_id,
				total_fees: Default::default(),
			});
		}
	}

	/// Repays (fully or partially) the loan with `provided_amount` (that was either debited from
	/// the account or received during liquidation). Returns any unused amount. If the loan does not
	/// exist, returns `None`.
	fn repay_principal(
		&mut self,
		loan_id: LoanId,
		provided_amount: AssetAmount,
		liquidation_fees: Option<AssetAmount>,
	) -> Option<AssetAmount> {
		let Some(loan) = self.loans.get_mut(&loan_id) else {
			// In rare cases it may be possible for the loan to no longer exist if
			// e.g. the principal was fully covered by a prior liquidation swap.
			return None;
		};

		// Making sure the user doesn't pay more than the total principal:
		let repayment_amount = core::cmp::min(provided_amount, loan.owed_principal);

		loan.pay_to_pool(repayment_amount, true /* is principal */);

		let liquidation_fees = match liquidation_fees {
			Some(fees) => BTreeMap::from([(loan.asset, fees)]),
			None => Default::default(),
		};

		Pallet::<T>::deposit_event(Event::LoanRepaid {
			loan_id,
			amount: repayment_amount,
			liquidation_fees,
		});

		if loan.owed_principal == 0 {
			self.settle_loan(loan_id);
		}

		Some(provided_amount.saturating_sub(repayment_amount))
	}
}

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct GeneralLoan<T: Config> {
	pub asset: Asset,
	pub created_at_block: BlockNumberFor<T>,
	pub owed_principal: AssetAmount,
}

impl<T: Config> GeneralLoan<T> {
	fn owed_principal_usd_value(&self) -> Result<AssetAmount, Error<T>> {
		usd_value_of::<T>(self.asset, self.owed_principal)
	}

	/// Pays loan asset to the pool. Reduces the owed principal
	/// amount if a principal repayment (rather than a fee)
	fn pay_to_pool(&mut self, amount: AssetAmount, is_principal: bool) {
		GeneralLendingPools::<T>::mutate(self.asset, |maybe_pool| {
			if let Some(pool) = maybe_pool.as_mut() {
				pool.accept_payment(amount, is_principal);
			} else {
				log_or_panic!("CHP Pool must exist for asset {}", self.asset);
			}
		});

		if is_principal {
			self.owed_principal.saturating_reduce(amount);
		}
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
		None, // no dca parameters
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

// Abort all provided liquidation swaps, repays any already swapped principal assets and
// returns remaining collateral assets alongside the corresponding loan information.
fn abort_liquidation_swaps<T: Config>(
	loans: &mut BTreeMap<LoanId, GeneralLoan<T>>,
	liquidation_swaps: &BTreeMap<SwapRequestId, LiquidationSwap>,
) -> Vec<AssetCollateralForLoan> {
	let mut collateral_collected = Vec::new();

	for (swap_request_id, LiquidationSwap { loan_id, from_asset, to_asset }) in liquidation_swaps {
		if let Some(swap_progress) = T::SwapRequestHandler::abort_swap_request(*swap_request_id) {
			if let Some(loan) = loans.get_mut(loan_id) {
				if loan.asset == *to_asset {
					loan.pay_to_pool(swap_progress.accumulated_output_amount, true);
				} else {
					log_or_panic!(
						"Unexpected asset {} in liquidation swap, expected {}",
						to_asset,
						loan.asset
					);
				}
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

	collateral_collected
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

			let Ok(ltv) = loan_account.derive_ltv() else {
				// Don't change liquidation status if we can't determine the
				// collateralisation ratio
				return;
			};

			// Every time we transition from a liquidating state we abort all liquidation swaps
			// and repay any swapped into principal. If the next state is "NoLiquidation", the
			// collateral is returned into the loan account; if it is "Liquidating", the collateral
			// is used in the new liquidation swaps.
			match &loan_account.liquidation_status {
				LiquidationStatus::NoLiquidation =>
					if ltv > config.ltv_hard_threshold {
						if let Ok(collateral) = loan_account.prepare_collateral_for_liquidation() {
							loan_account.init_liquidation_swaps(borrower_id, collateral, true);
						}
					} else if ltv > config.ltv_soft_threshold {
						if let Ok(collateral) = loan_account.prepare_collateral_for_liquidation() {
							loan_account.init_liquidation_swaps(borrower_id, collateral, false);
						}
					},
				LiquidationStatus::Liquidating { liquidation_swaps, is_hard } if *is_hard => {
					if ltv < config.ltv_soft_liquidation_abort_threshold {
						// Transition from hard liquidation to active:
						let collateral =
							abort_liquidation_swaps(&mut loan_account.loans, liquidation_swaps);
						loan_account.return_collateral(collateral);
					} else if ltv < config.ltv_hard_liquidation_abort_threshold {
						// Transition from hard liquidation to soft liquidation:
						let collateral =
							abort_liquidation_swaps(&mut loan_account.loans, liquidation_swaps);
						loan_account.init_liquidation_swaps(borrower_id, collateral, false);
					}
				},
				LiquidationStatus::Liquidating { liquidation_swaps, .. } => {
					if ltv > config.ltv_hard_threshold {
						// Transition from soft liquidation to hard liquidation:
						let collateral =
							abort_liquidation_swaps(&mut loan_account.loans, liquidation_swaps);
						loan_account.init_liquidation_swaps(borrower_id, collateral, true);
					} else if ltv < config.ltv_soft_liquidation_abort_threshold {
						// Transition from soft liquidation to active:
						let collateral =
							abort_liquidation_swaps(&mut loan_account.loans, liquidation_swaps);
						loan_account.return_collateral(collateral);
					}
				},
			}
		});
	}

	// Swap fees in every asset every FEE_CHECK_INTERVAL blocks, but only if they exceed
	// FEE_SWAP_THRESHOLD_USD in value
	if current_block % config.fee_swap_interval_blocks.into() == 0u32.into() {
		for pool_asset in GeneralLendingPools::<T>::iter_keys().collect::<Vec<_>>() {
			GeneralLendingPools::<T>::mutate(pool_asset, |maybe_pool| {
				let pool: &mut LendingPool<T> =
					maybe_pool.as_mut().expect("Iterating over keys obtained a line above");

				for (collateral_asset, fee_amount) in &mut pool.collected_fees {
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
							FEE_SWAP_MAX_ORACLE_SLIPPAGE,
						);

						Pallet::<T>::deposit_event(Event::LendingFeeCollectionInitiated {
							asset: pool_asset,
							swap_request_id,
						});
					}
				}
			})
		}
	}

	Weight::zero()
}

impl<T: Config> LendingApi for Pallet<T> {
	type AccountId = T::AccountId;

	/// Create a new loan (assigning a new loan id) provided that the account's existing collateral
	/// plus any `extra_collateral` is sufficient. Will update the primary collateral asser if
	/// provided.
	#[transactional]
	fn new_loan(
		borrower_id: T::AccountId,
		asset: Asset,
		amount_to_borrow: AssetAmount,
		primary_collateral_asset: Option<Asset>,
		extra_collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<LoanId, DispatchError> {
		ensure!(T::SafeMode::get().borrowing_enabled, Error::<T>::LoanCreationDisabled);

		let chp_config = LendingConfig::<T>::get();

		let loan_id = NextLoanId::<T>::get();
		NextLoanId::<T>::set(loan_id + 1);

		LoanAccounts::<T>::mutate(&borrower_id, |maybe_account| {
			let account =
				Self::create_loan_account_if_empty(maybe_account, primary_collateral_asset)?;

			let primary_collateral_asset = account.primary_collateral_asset;

			ensure!(
				extra_collateral.contains_key(&primary_collateral_asset),
				Error::<T>::InvalidLoanParameters
			);

			let loan = GeneralLoan {
				asset,
				created_at_block: frame_system::Pallet::<T>::current_block_number(),
				owed_principal: amount_to_borrow,
			};

			for (asset, amount) in &extra_collateral {
				T::Balance::try_debit_account(&borrower_id, *asset, *amount)?;
			}

			account.primary_collateral_asset = primary_collateral_asset;

			for (asset, amount) in &extra_collateral {
				account.collateral.entry(*asset).or_default().saturating_accrue(*amount);
			}

			account.loans.insert(loan_id, loan);

			if account.derive_ltv()? > chp_config.ltv_target_threshold {
				fail!(Error::<T>::InsufficientCollateral);
			}

			GeneralLendingPools::<T>::try_mutate(asset, |pool| {
				let pool = pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

				pool.borrow_funds(amount_to_borrow)?;

				Ok::<_, DispatchError>(())
			})?;

			let origination_fee_amount = equivalent_amount::<T>(
				asset,
				primary_collateral_asset,
				chp_config.origination_fee * amount_to_borrow,
			)?;

			T::Balance::try_debit_account(
				&borrower_id,
				primary_collateral_asset,
				origination_fee_amount,
			)?;

			T::Balance::credit_account(&borrower_id, asset, amount_to_borrow);

			Self::deposit_event(Event::LoanCreated {
				loan_id,
				borrower_id: borrower_id.clone(),
				asset,
				principal_amount: amount_to_borrow,
				origination_fee: origination_fee_amount,
			});

			Self::accrue_fees(asset, primary_collateral_asset, origination_fee_amount);

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
		ensure!(T::SafeMode::get().borrowing_enabled, Error::<T>::LoanCreationDisabled);

		let chp_config = LendingConfig::<T>::get();

		LoanAccounts::<T>::mutate(&borrower_id, |maybe_account| {
			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanNotFound)?;

			for (asset, amount) in &extra_collateral {
				T::Balance::try_debit_account(&borrower_id, *asset, *amount)?;
				loan_account.collateral.entry(*asset).or_default().saturating_accrue(*amount);
			}

			{
				let loan = loan_account.loans.get_mut(&loan_id).ok_or(Error::<T>::LoanNotFound)?;
				loan.owed_principal.saturating_accrue(extra_amount_to_borrow);
			}

			if loan_account.derive_ltv()? > chp_config.ltv_target_threshold {
				return Err(Error::<T>::InsufficientCollateral.into());
			}

			// NOTE: have to get a new reference to the loan again to satisfy the borrow checker
			let loan = loan_account.loans.get_mut(&loan_id).expect("checked above");

			let primary_collateral_asset = loan_account.primary_collateral_asset;
			let loan_asset = loan.asset;

			GeneralLendingPools::<T>::try_mutate(loan_asset, |pool| {
				let pool = pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

				pool.borrow_funds(extra_amount_to_borrow)?;

				Ok::<_, DispatchError>(())
			})?;

			let origination_fee_amount = equivalent_amount::<T>(
				loan_asset,
				primary_collateral_asset,
				chp_config.origination_fee * extra_amount_to_borrow,
			)?;

			T::Balance::try_debit_account(
				&borrower_id,
				primary_collateral_asset,
				origination_fee_amount,
			)?;

			Self::deposit_event(Event::LoanUpdated {
				loan_id,
				extra_principal_amount: extra_amount_to_borrow,
				origination_fee: origination_fee_amount,
			});

			Self::accrue_fees(loan_asset, primary_collateral_asset, origination_fee_amount);

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

			let remaining_amount = loan_account
				.repay_principal(loan_id, repayment_amount, None)
				.ok_or(Error::<T>::LoanNotFound)?;

			T::Balance::try_debit_account(
				borrower_id,
				loan_asset,
				repayment_amount.saturating_sub(remaining_amount),
			)?;

			Ok::<_, DispatchError>(())
		})?;

		Ok(())
	}

	#[transactional]
	fn add_collateral(
		borrower_id: &Self::AccountId,
		primary_collateral_asset: Option<Asset>,
		collateral: BTreeMap<Asset, AssetAmount>,
	) -> Result<(), DispatchError> {
		ensure!(T::SafeMode::get().adding_collateral_enabled, Error::<T>::AddingCollateralDisabled);

		LoanAccounts::<T>::mutate(borrower_id, |maybe_account| {
			let loan_account =
				Self::create_loan_account_if_empty(maybe_account, primary_collateral_asset)?;

			if let Some(primary_collateral_asset) = primary_collateral_asset {
				loan_account.primary_collateral_asset = primary_collateral_asset;
			}

			for (asset, amount) in &collateral {
				T::Balance::try_debit_account(borrower_id, *asset, *amount)?;

				loan_account.collateral.entry(*asset).or_insert(0).saturating_accrue(*amount);
			}

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
		ensure!(
			T::SafeMode::get().removing_collateral_enabled,
			Error::<T>::RemovingCollateralDisabled
		);

		LoanAccounts::<T>::mutate(borrower_id, |maybe_account| {
			let chp_config = LendingConfig::<T>::get();

			let loan_account = maybe_account.as_mut().ok_or(Error::<T>::LoanAccountNotFound)?;

			if let Some(primary_collateral_asset) = primary_collateral_asset {
				loan_account.primary_collateral_asset = primary_collateral_asset;
			}

			for (asset, amount) in &collateral {
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
				loan_account.derive_ltv()? > chp_config.ltv_target_threshold
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

					// TODO: compute this
					let liquidation_fee = 0;

					let remaining_amount = match loan_account.repay_principal(
						loan_id,
						output_amount,
						Some(liquidation_fee),
					) {
						Some(remaining_amount) => remaining_amount,
						// Note: not being able to find the loan is not considered
						// an error here because this can happen if one of the prior liquidations
						// happened to repay it in full (in which case the funds should go to the
						// borrower).
						None => output_amount,
					};

					let mut should_settle_loan = false;

					// Updating liquidation status:
					match &mut loan_account.liquidation_status {
						LiquidationStatus::NoLiquidation => {
							log_or_panic!("Unexpected liquidation (swap request id: {swap_request_id}, loan_id: {loan_id})");
						},
						LiquidationStatus::Liquidating { liquidation_swaps, .. } => {
							if let Some(swap) = liquidation_swaps.remove(&swap_request_id) {
								// Any amount left after repaying the loan should be returned to the
								// borrower:
								T::Balance::credit_account(
									&borrower_id,
									swap.to_asset,
									remaining_amount,
								);
							}

							// If there are no more liquidation swaps for the loan, we should
							// "settle" it (even if it hasn't been repaid in full):
							if liquidation_swaps
								.values()
								.filter(|swap| swap.loan_id == loan_id)
								.count() == 0
							{
								should_settle_loan = true;
							}

							if liquidation_swaps.is_empty() {
								loan_account.liquidation_status = LiquidationStatus::NoLiquidation;
							}
						},
					};

					if should_settle_loan {
						loan_account.settle_loan(loan_id);
					}
				});
			},
			LendingSwapType::FeeSwap { pool_asset } => {
				GeneralLendingPools::<T>::mutate(pool_asset, |pool| {
					let Some(pool) = pool.as_mut() else {
						log_or_panic!("Pool must exist for {pool_asset}");
						return;
					};

					pool.accept_payment(output_amount, false);
				});
			},
		}
	}
}

impl<T: Config> Pallet<T> {
	fn accrue_fees(loan_asset: Asset, fee_asset: Asset, fee_amount: AssetAmount) {
		GeneralLendingPools::<T>::mutate(loan_asset, |pool| {
			if let Some(pool) = pool {
				pool.collected_fees.entry(fee_asset).or_insert(0).saturating_accrue(fee_amount);
			} else {
				log_or_panic!("CHP pool must exist for {loan_asset}");
			}
		});
	}

	fn create_loan_account_if_empty(
		maybe_account: &mut Option<LoanAccount<T>>,
		primary_collateral_asset: Option<Asset>,
	) -> Result<&mut LoanAccount<T>, Error<T>> {
		let account = match maybe_account {
			Some(account) => account,
			None => {
				let primary_collateral_asset =
					primary_collateral_asset.ok_or(Error::<T>::InvalidLoanParameters)?;

				let account = LoanAccount::new(primary_collateral_asset);
				maybe_account.insert(account)
			},
		};

		Ok(account)
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
		pub utilisation_rate: BasisPoints,
		pub interest_rate: BasisPoints,
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
		lender_id: T::AccountId,
		loan_account: LoanAccount<T>,
	) -> RpcLoanAccount<T::AccountId, AssetAmount> {
		RpcLoanAccount {
			account: lender_id,
			primary_collateral_asset: loan_account.primary_collateral_asset,
			ltv_ratio: loan_account.derive_ltv().ok(),
			collateral: loan_account
				.collateral
				.into_iter()
				.map(|(asset, amount)| AssetAndAmount { asset, amount })
				.collect(),
			loans: loan_account
				.loans
				.into_iter()
				.map(|(loan_id, loan)| {
					RpcLoan {
						loan_id,
						asset: loan.asset,
						created_at: loan.created_at_block.unique_saturated_into(),
						principal_amount: loan.owed_principal,
						// TODO: store historical fees on loans and expose them here
						total_fees: Default::default(),
					}
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
		lender_id: Option<T::AccountId>,
	) -> Vec<RpcLoanAccount<T::AccountId, AssetAmount>> {
		if let Some(lender_id) = lender_id {
			LoanAccounts::<T>::get(&lender_id)
				.into_iter()
				.map(|loan_account| build_rpc_loan_account(lender_id.clone(), loan_account))
				.collect()
		} else {
			LoanAccounts::<T>::iter()
				.map(|(lender_id, loan_account)| {
					build_rpc_loan_account(lender_id.clone(), loan_account)
				})
				.collect()
		}
	}

	fn build_rpc_lending_pool<T: Config>(
		asset: Asset,
		pool: &LendingPool<T>,
	) -> RpcLendingPool<AssetAmount> {
		let config = LendingConfig::<T>::get();

		let utilisation = pool.get_utilisation();

		let interest_rate = config.derive_interest_rate_per_year(utilisation);

		RpcLendingPool {
			asset,
			total_amount: pool.total_amount,
			available_amount: pool.available_amount,
			utilisation_rate: (utilisation.deconstruct() / 100) as u16,
			interest_rate: (interest_rate.deconstruct() / 100_000) as u16,
		}
	}

	pub fn get_lending_pools<T: Config>(asset: Option<Asset>) -> Vec<RpcLendingPool<AssetAmount>> {
		if let Some(asset) = asset {
			GeneralLendingPools::<T>::get(asset)
				.iter()
				.map(|pool| build_rpc_lending_pool(asset, pool))
				.collect()
		} else {
			GeneralLendingPools::<T>::iter()
				.map(|(asset, pool)| build_rpc_lending_pool(asset, &pool))
				.collect()
		}
	}
}
