use cf_amm_math::Price;
use frame_support::sp_runtime::Perbill;

use super::*;

#[cfg(test)]
mod chp_lending_tests;

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ChpLoan<T: Config> {
	loan_id: ChpLoanId,
	asset: Asset,
	created_at_block: BlockNumberFor<T>,
	expiry_block: BlockNumberFor<T>,
	borrower_id: T::AccountId,
	// Collateral amount (excluding anything that's in a swap during liquidation)
	usdc_collateral: AssetAmount,
	fees_collected_usdc: AssetAmount,
	pool_contributions: Vec<ChpPoolContribution>,
	// Interest charged on the principal amount every block
	interest_rate: Perbill,
	// This is used to calculate the interest rate for the loan
	// to make sure that the interest payments in USDC don't fluctuate
	// with asset price movements.
	asset_price_at_creation: Price,
	status: LoanStatus,
}

impl<T: Config> ChpLoan<T> {
	fn total_principal_amount(&self) -> AssetAmount {
		self.pool_contributions.iter().map(|c| c.principal).sum()
	}

	fn overcollateralisation_ratio(&self) -> Permill {
		let principal_in_usdc =
			usdc_equivalent_amount::<T>(self.asset, self.total_principal_amount());

		let collateral_usdc = match self.status {
			LoanStatus::Active => self.usdc_collateral,
			LoanStatus::Finalising => {
				log_or_panic!(
					"Overcollateralisation ratio should not be required during finalisation"
				);
				Default::default()
			},
			LoanStatus::SoftLiquidation { usdc_collateral } =>
				self.usdc_collateral + usdc_collateral,
			LoanStatus::HardLiquidation { usdc_collateral } =>
				self.usdc_collateral + usdc_collateral,
		};

		Permill::from_rational(collateral_usdc.saturating_sub(principal_in_usdc), principal_in_usdc)
	}

	// Distribute funds (in loan's asset) among pools/participants
	// according to their contributions. Reduces the owed principal
	// amount if a principal repayment.
	fn distribute_funds(&mut self, amount: AssetAmount) {
		let principal_total = self.total_principal_amount();

		for ChpPoolContribution { core_pool_id, loan_id, principal } in &mut self.pool_contributions
		{
			let pool_to_receive =
				Perquintill::from_rational(*principal, principal_total).mul_floor(amount);

			CorePools::<T>::mutate(self.asset, core_pool_id, |maybe_pool| {
				if let Some(pool) = maybe_pool.as_mut() {
					for (lender_id, unlocked_amount) in
						pool.make_repayment(*loan_id, pool_to_receive)
					{
						T::Balance::credit_account(&lender_id, self.asset, unlocked_amount);
					}
				} else {
					log_or_panic!("Core pool should exist for {} chp pool", self.asset);
				}
			});

			// We assume that any repayment goes towards reducing the principal amount,
			// unless it is in finalising state where we only distribute fees:
			if self.status != LoanStatus::Finalising {
				principal.saturating_reduce(pool_to_receive);
			}

			// TODO: when we have multiple pools, we should track rounding errors and
			// returning them to a random pool

			// TODO: should we release portion of the collateral if it is now above the required
			// collateralisation ratio?
		}
	}
}

fn usdc_equivalent_amount<T: Config>(asset: Asset, amount: AssetAmount) -> AssetAmount {
	let asset_price = T::PriceApi::get_price(asset);

	cf_amm_math::output_amount_ceil(amount.into(), asset_price).unique_saturated_into()
}

fn usdc_collateral_required<T: Config>(asset: Asset, loan_principal: AssetAmount) -> AssetAmount {
	let config = ChpConfig::<T>::get();

	let usdc_loan_value = usdc_equivalent_amount::<T>(asset, loan_principal);

	usdc_loan_value + config.overcollateralisation_target * usdc_loan_value
}

pub fn swap_for_chp<T: Config>(
	from_asset: Asset,
	amount: AssetAmount,
	to_asset: Asset,
	loan_id: ChpLoanId,
) {
	T::SwapRequestHandler::init_swap_request(
		from_asset,
		amount,
		to_asset,
		SwapRequestType::Regular { output_action: SwapOutputAction::CreditLendingPool { loan_id } },
		Default::default(), // broker fees
		None,               // no refund
		None,               // no dca parameters
		SwapOrigin::Internal,
	);
}

fn initiate_loan_fees_swap<T: Config>(loan: &mut ChpLoan<T>) {
	if loan.status != LoanStatus::Finalising {
		loan.status = LoanStatus::Finalising;
		swap_for_chp::<T>(COLLATERAL_ASSET, loan.fees_collected_usdc, loan.asset, loan.loan_id);
	} else {
		unreachable!();
	}
}

fn initiate_soft_liquidation<T: Config>(loan: &mut ChpLoan<T>) {
	if loan.status == LoanStatus::Active {
		loan.status = LoanStatus::SoftLiquidation { usdc_collateral: loan.usdc_collateral };

		swap_for_chp::<T>(COLLATERAL_ASSET, loan.usdc_collateral, loan.asset, loan.loan_id);

		loan.usdc_collateral = 0;
	} else {
		unreachable!()
	}
}

pub fn process_interest_for_loan<T: Config>(
	current_block: BlockNumberFor<T>,
	loan: &mut ChpLoan<T>,
) -> Weight {
	if current_block.saturating_sub(loan.created_at_block) % INTEREST_PAYMENT_INTERVAL.into() ==
		0u32.into()
	{
		for ChpPoolContribution { principal, .. } in &loan.pool_contributions {
			let interest_amount_in_loan_asset =
				(loan.interest_rate.int_mul(INTEREST_PAYMENT_INTERVAL)) * *principal;

			// NOTE: we use asset price at the time the loan was created to make sure
			// that interest payments are consistent/predictable.
			let usdc_interest_amount = cf_amm_math::output_amount_ceil(
				interest_amount_in_loan_asset.into(),
				loan.asset_price_at_creation,
			)
			.unique_saturated_into();

			loan.usdc_collateral.saturating_reduce(usdc_interest_amount);
			loan.fees_collected_usdc.saturating_accrue(usdc_interest_amount);
		}
	}

	Weight::zero()
}

// Sweeping but it is a no-op if it fails for whatever reason
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

pub fn process_collateral_topup<T: Config>(
	loan: &mut ChpLoan<T>,
	config: &ChpConfiguration,
) -> Weight {
	if loan.overcollateralisation_ratio() < config.overcollateralisation_topup_threshold {
		let collateral_required =
			usdc_collateral_required::<T>(loan.asset, loan.total_principal_amount());

		let topup_amount_required = collateral_required.saturating_sub(loan.usdc_collateral);

		try_sweep::<T>(&loan.borrower_id);

		let topup_amount = core::cmp::min(
			T::Balance::get_balance(&loan.borrower_id, COLLATERAL_ASSET),
			topup_amount_required,
		);

		if topup_amount > 0 {
			if T::Balance::try_debit_account(&loan.borrower_id, COLLATERAL_ASSET, topup_amount)
				.is_ok()
			{
				loan.usdc_collateral += topup_amount;
			} else {
				log_or_panic!("Unable to debit after checking balance");
			}
		}
	}

	Weight::zero()
}

pub fn chp_upkeep<T: Config>(current_block: BlockNumberFor<T>) -> Weight {
	let config = ChpConfig::<T>::get();

	for (asset, loan_id) in ChpLoans::<T>::iter_keys() {
		let _ = ChpLoans::<T>::try_mutate(asset, loan_id, |loan| {
			let loan = loan.as_mut().expect("keys read directly from storage just above");

			if loan.status == LoanStatus::Active {
				process_interest_for_loan::<T>(current_block, loan);
				process_collateral_topup::<T>(loan, &config);

				// Checking expiry
				if loan.expiry_block <= current_block {
					initiate_soft_liquidation::<T>(loan);
					return Ok::<_, ()>(());
				}
			}

			match loan.status {
				LoanStatus::Active => {
					let overcollateralisation_ratio = loan.overcollateralisation_ratio();

					if overcollateralisation_ratio < config.overcollateralisation_hard_threshold {
						// TODO: same as with soft liquidation, but with a different price setting
						unimplemented!();
					} else if overcollateralisation_ratio <
						config.overcollateralisation_soft_threshold
					{
						initiate_soft_liquidation::<T>(loan);
					}
				},
				LoanStatus::SoftLiquidation { .. } => {
					let overcollateralisation_ratio = loan.overcollateralisation_ratio();

					if overcollateralisation_ratio < config.overcollateralisation_hard_threshold {
						// TODO: cancel soft liquidation swap and initiate a hard liquidaiton one
						unimplemented!();
					}
				},
				LoanStatus::Finalising | LoanStatus::HardLiquidation { .. } => {
					// Nothing to do
				},
			}

			Ok::<_, ()>(())
		});
	}

	Weight::zero() // TODO: benchmark
}

impl<T: Config> ChpLendingApi for Pallet<T> {
	type AccountId = T::AccountId;

	#[transactional]
	fn new_chp_loan(
		borrower: T::AccountId,
		asset: Asset,
		amount_to_borrow: AssetAmount,
	) -> Result<ChpLoanId, DispatchError> {
		ensure!(T::SafeMode::get().chp_loans_enabled, Error::<T>::ChpLoansDisabled);

		let loan_id = NextChpLoanId::<T>::get();
		NextChpLoanId::<T>::set(loan_id + 1);

		let chp_config = ChpConfig::<T>::get();

		let usdc_collateral_amount = usdc_collateral_required::<T>(asset, amount_to_borrow);

		let chp_pool = ChpPools::<T>::get(asset).ok_or(Error::<T>::PoolDoesNotExist)?;

		let mut pool_contributions = vec![];

		let core_pool_id = chp_pool.core_pool_id;

		let utilisation = CorePools::<T>::try_mutate(asset, core_pool_id, |core_pool| {
			let core_pool = core_pool.as_mut().ok_or(Error::<T>::PoolDoesNotExist)?;

			let utilisation = {
				let liquidity_in_loans: AssetAmount = ChpLoans::<T>::iter_prefix(asset)
					.map(|(_loan_id, chp_loan)| {
						chp_loan
							.pool_contributions
							.iter()
							.filter_map(|c| {
								if c.core_pool_id == core_pool_id {
									Some(c.principal)
								} else {
									None
								}
							})
							.sum::<AssetAmount>()
					})
					.sum();

				let total_liquidity = core_pool.get_available_amount() + liquidity_in_loans;

				Permill::from_rational(liquidity_in_loans + amount_to_borrow, total_liquidity)
			};

			let core_loan_id = core_pool
				.new_loan(amount_to_borrow, LoanUsage::ChpLoan(loan_id))
				.map_err(|_| Error::<T>::InsufficientLiquidity)?;

			pool_contributions.push(ChpPoolContribution {
				core_pool_id,
				loan_id: core_loan_id,
				principal: amount_to_borrow,
			});

			Ok::<_, DispatchError>(utilisation)
		})?;

		let clearing_fee_amount = usdc_equivalent_amount::<T>(
			asset,
			chp_config.derive_clearing_fee(utilisation) * amount_to_borrow,
		);

		T::Balance::try_debit_account(
			&borrower,
			COLLATERAL_ASSET,
			usdc_collateral_amount + clearing_fee_amount,
		)?;

		T::Balance::credit_account(&borrower, asset, amount_to_borrow);

		let created_at_block = frame_system::Pallet::<T>::current_block_number();

		Self::deposit_event(Event::ChpLoanCreated {
			loan_id,
			borrower_id: borrower.clone(),
			asset,
			amount: amount_to_borrow,
		});

		let loan = ChpLoan {
			loan_id,
			asset,
			created_at_block,
			expiry_block: created_at_block + chp_config.max_loan_duration.into(),
			status: LoanStatus::Active,
			borrower_id: borrower,
			usdc_collateral: usdc_collateral_amount,
			fees_collected_usdc: clearing_fee_amount,
			pool_contributions,
			interest_rate: chp_config.derive_interest_rate(utilisation),
			asset_price_at_creation: T::PriceApi::get_price(asset),
		};

		ChpLoans::<T>::insert(asset, loan_id, loan);

		Ok(loan_id)
	}

	fn try_making_repayment(
		loan_id: ChpLoanId,
		asset: Asset,
		amount: AssetAmount,
	) -> Result<(), DispatchError> {
		ChpLoans::<T>::try_mutate(asset, loan_id, |loan| {
			let loan = loan.as_mut().ok_or(Error::<T>::ChpLoanDoesNotExist)?;

			let principal_total = loan.total_principal_amount();

			let amount = core::cmp::min(amount, principal_total);

			T::Balance::try_debit_account(&loan.borrower_id, asset, amount)?;

			loan.distribute_funds(amount);

			if amount == principal_total {
				T::Balance::credit_account(
					&loan.borrower_id,
					COLLATERAL_ASSET,
					loan.usdc_collateral,
				);

				initiate_loan_fees_swap::<T>(loan);
			}

			Ok::<_, DispatchError>(())
		})
	}
}

impl<T: Config> cf_traits::lending::ChpSystemApi for Pallet<T> {
	fn process_loan_swap_outcome(loan_id: ChpLoanId, asset: Asset, output_amount: AssetAmount) {
		ChpLoans::<T>::mutate_exists(asset, loan_id, |maybe_loan| {
			let Some(loan) = maybe_loan else {
				log_or_panic!("Loan does not exist: {loan_id}");
				return;
			};

			match loan.status {
				LoanStatus::Active => {
					log_or_panic!("Swaps for loans in active state are unexpected");
				},
				LoanStatus::SoftLiquidation { .. } => {
					// Use swapped asset to repay the loan:
					let amount_to_repay =
						core::cmp::min(output_amount, loan.total_principal_amount());
					loan.distribute_funds(amount_to_repay);

					// Any amount left after repaying the loan should be returned to the
					// borrower:
					T::Balance::credit_account(
						&loan.borrower_id,
						asset,
						output_amount.saturating_sub(amount_to_repay),
					);

					// Enter finalising state to swap the fees:
					initiate_loan_fees_swap::<T>(loan);
				},
				LoanStatus::HardLiquidation { .. } => {
					// Hard liquidation not supported yet
					unimplemented!()
				},
				LoanStatus::Finalising => {
					loan.distribute_funds(output_amount);

					for ChpPoolContribution { core_pool_id, loan_id, .. } in
						&loan.pool_contributions
					{
						CorePools::<T>::mutate(asset, core_pool_id, |maybe_pool| {
							if let Some(pool) = maybe_pool.as_mut() {
								pool.finalise_loan(*loan_id);
							} else {
								log_or_panic!("Core pool {core_pool_id} does not exist");
							}
						});
					}

					Self::deposit_event(Event::ChpLoanSettled { loan_id });

					// The loan has been finalised so we finally remove it:
					*maybe_loan = None;
				},
			}
		});
	}
}
