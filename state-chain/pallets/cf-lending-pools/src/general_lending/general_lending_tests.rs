use crate::mocks::*;
use cf_amm_math::PRICE_FRACTIONAL_BITS;
use cf_chains::evm::U256;
use cf_primitives::SWAP_DELAY_BLOCKS;
use cf_test_utilities::{assert_event_sequence, assert_has_event, assert_matching_event_count};
use cf_traits::{
	lending::ChpSystemApi,
	mocks::{
		balance_api::MockBalance,
		price_feed_api::MockPriceFeedApi,
		swap_request_api::{MockSwapRequest, MockSwapRequestHandler},
	},
	SafeMode, SetSafeMode, SwapExecutionProgress,
};

use super::*;
use frame_support::{assert_err, assert_noop, assert_ok};

const INIT_BLOCK: u64 = 1;

const LENDER: u64 = BOOSTER_1;
const BORROWER: u64 = LP;

const LOAN_ASSET: Asset = Asset::Btc;
const PRINCIPAL: AssetAmount = 1_000_000_000;

const LOAN_ID: LoanId = LoanId(0);

const SWAP_RATE: u128 = 20;

const INIT_POOL_AMOUNT: AssetAmount = PRINCIPAL * 2;

use crate::LENDING_DEFAULT_CONFIG as CONFIG;

/// Takes the full fee and splits it into network fee and the remainder.
fn take_network_fee(full_amount: AssetAmount) -> (AssetAmount, AssetAmount) {
	// To keep things simple in tests we assume we take the same % from origination and liquidation
	// fees
	assert_eq!(
		CONFIG.network_fee_contributions.from_origination_fee,
		CONFIG.network_fee_contributions.from_liquidation_fee
	);

	let network_fee = CONFIG.network_fee_contributions.from_origination_fee * full_amount;

	(network_fee, full_amount - network_fee)
}

fn setup_chp_pool_with_funds(loan_asset: Asset, init_amount: AssetAmount) {
	LendingConfig::<Test>::set(CONFIG);

	assert_ok!(LendingPools::new_lending_pool(loan_asset));

	System::assert_last_event(RuntimeEvent::LendingPools(Event::<Test>::LendingPoolCreated {
		asset: loan_asset,
	}));

	MockBalance::credit_account(&LENDER, loan_asset, init_amount);
	assert_ok!(LendingPools::add_lender_funds(
		RuntimeOrigin::signed(LENDER),
		loan_asset,
		init_amount
	));

	assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
		lender_id: LENDER,
		asset: loan_asset,
		amount: init_amount,
	}));
}

// A helper function to help with updating asset prices (in atomic USD units)
fn set_asset_price_in_usd(asset: Asset, price: u128) {
	MockPriceFeedApi::set_price(asset, Some(U256::from(price) << PRICE_FRACTIONAL_BITS));
}

#[test]
fn lender_basic_adding_and_removing_funds() {
	new_test_ext().execute_with(|| {
		assert_ok!(LendingPools::new_lending_pool(LOAN_ASSET));

		// Test that it is possible to withdraw funds if you are the sole contributor
		MockBalance::credit_account(&LENDER, LOAN_ASSET, INIT_POOL_AMOUNT);
		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(LENDER),
			LOAN_ASSET,
			INIT_POOL_AMOUNT
		));

		assert_eq!(MockBalance::get_balance(&LENDER, LOAN_ASSET), 0);

		// Remove 25% of the funds first:
		assert_ok!(LendingPools::remove_lender_funds(
			RuntimeOrigin::signed(LENDER),
			LOAN_ASSET,
			Some(INIT_POOL_AMOUNT / 4)
		));
		assert_eq!(MockBalance::get_balance(&LENDER, LOAN_ASSET), INIT_POOL_AMOUNT / 4);

		assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LendingFundsRemoved {
			lender_id: LENDER,
			asset: LOAN_ASSET,
			unlocked_amount: INIT_POOL_AMOUNT / 4,
		}));

		// Remove the remaining 75% of the funds (by setting the amount to None):
		assert_ok!(LendingPools::remove_lender_funds(
			RuntimeOrigin::signed(LENDER),
			LOAN_ASSET,
			None
		));
		assert_eq!(MockBalance::get_balance(&LENDER, LOAN_ASSET), INIT_POOL_AMOUNT);

		assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LendingFundsRemoved {
			lender_id: LENDER,
			asset: LOAN_ASSET,
			unlocked_amount: 3 * INIT_POOL_AMOUNT / 4,
		}));
	});
}

/// Derives the total interest amount to be paid by the borrower per interest charge interval.
fn derive_interest_amounts(
	principal: AssetAmount,
	utilisation: Permill,
) -> (AssetAmount, AssetAmount) {
	let base_interest =
		CONFIG.derive_base_interest_rate_per_payment_interval(LOAN_ASSET, utilisation);

	let network_interest = CONFIG.derive_network_interest_rate_per_payment_interval();

	let pool_amount = (ScaledAmountHP::from_asset_amount(principal) * base_interest)
		.into_asset_amount() *
		SWAP_RATE;

	let network_amount = (ScaledAmountHP::from_asset_amount(principal) * network_interest)
		.into_asset_amount() *
		SWAP_RATE;

	// Tests aren't valid if the fees are zero (need to adjust tests parameters if this is hit)
	assert!(pool_amount > 0 && network_amount > 0);

	(pool_amount, network_amount)
}

#[test]
fn basic_general_lending() {
	// We want the amount to be large enough that we can charge interest immediately
	// (rather than waiting for fractional amounts to accumulate).
	const PRINCIPAL: AssetAmount = 2_000_000_000_000;
	const INIT_POOL_AMOUNT: AssetAmount = PRINCIPAL * 2;
	const COLLATERAL_ASSET: Asset = Asset::Eth;

	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV

	let origination_fee = CONFIG.origination_fee(LOAN_ASSET) * PRINCIPAL * SWAP_RATE;

	// Repaying a small portion to make sure we don't hit low LTV penalty:
	const REPAYMENT_AMOUNT: AssetAmount = PRINCIPAL / 10;

	// 50% utilisation is expected:
	let (pool_interest_1, network_interest_1) =
		derive_interest_amounts(PRINCIPAL, Permill::from_percent(50));

	// 45% utilisation is expected:
	let (pool_interest_2, network_interest_2) =
		derive_interest_amounts(PRINCIPAL - REPAYMENT_AMOUNT, Permill::from_percent(45));

	let total_interest =
		pool_interest_1 + network_interest_1 + pool_interest_2 + network_interest_2;

	new_test_ext()
		.execute_with(|| {
			setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

			// Disable fee swaps for this test (so we easily can check all collected fees)
			LendingConfig::<Test>::set(LendingConfiguration {
				fee_swap_threshold_usd: u128::MAX,
				..CONFIG
			});

			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
			set_asset_price_in_usd(COLLATERAL_ASSET, 1);

			MockBalance::credit_account(
				&BORROWER,
				COLLATERAL_ASSET,
				INIT_COLLATERAL + origination_fee,
			);

			System::reset_events();

			assert_eq!(
				LendingPools::new_loan(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					Some(COLLATERAL_ASSET),
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
				),
				Ok(LOAN_ID)
			);

			let (network_fee, pool_fee) = take_network_fee(origination_fee);

			// NOTE: the sequence of events is important here
			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanCreated {
					loan_id: LOAN_ID,
					borrower_id: BORROWER,
					asset: LOAN_ASSET,
					principal_amount: PRINCIPAL,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::OriginationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee: pool_fee_taken,
					network_fee: network_fee_taken,
					broker_fee: 0,
				}) if pool_fee_taken == pool_fee && network_fee_taken == network_fee
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT,
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
				}
			);

			assert_eq!(
				PendingPoolFees::<Test>::get(LOAN_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, pool_fee)])
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
					collateral: BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							id: LOAN_ID,
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL,
							pending_interest: InterestBreakdown::default(),
						}
					)]),
				})
			);
		})
		.then_process_blocks_until_block(
			INIT_BLOCK + CONFIG.interest_payment_interval_blocks as u64,
		)
		// Checking that interest was charged here:
		.then_execute_with(|_| {
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().collateral,
				BTreeMap::from([(
					COLLATERAL_ASSET,
					INIT_COLLATERAL - pool_interest_1 - network_interest_1
				)])
			);

			assert_eq!(
				PendingPoolFees::<Test>::get(LOAN_ASSET),
				BTreeMap::from([(
					COLLATERAL_ASSET,
					take_network_fee(origination_fee).1 + pool_interest_1
				)])
			);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::InterestTaken {
				loan_id: LOAN_ID,
				pool_interest: BTreeMap::from([(COLLATERAL_ASSET, pool_interest_1)]),
				network_interest: BTreeMap::from([(COLLATERAL_ASSET, network_interest_1)]),
				broker_interest: Default::default(),
				low_ltv_penalty: Default::default(),
			}))
		})
		// === REPAYING SOME OF THE LOAN ===
		.then_execute_with(|_| {
			assert_ok!(LendingPools::try_making_repayment(&BORROWER, LOAN_ID, REPAYMENT_AMOUNT));
			assert_eq!(
				MockBalance::get_balance(&BORROWER, LOAN_ASSET),
				PRINCIPAL - REPAYMENT_AMOUNT
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER)
					.unwrap()
					.loans
					.get(&LOAN_ID)
					.unwrap()
					.owed_principal,
				PRINCIPAL - REPAYMENT_AMOUNT
			);
			// Funds have been returned to the pool:
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT,
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL + REPAYMENT_AMOUNT,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
				}
			);

			assert_eq!(
				PendingPoolFees::<Test>::get(LOAN_ASSET),
				BTreeMap::from([(
					COLLATERAL_ASSET,
					take_network_fee(origination_fee).1 + pool_interest_1
				)])
			);
		})
		.then_process_blocks_until_block(
			INIT_BLOCK + 2 * CONFIG.interest_payment_interval_blocks as u64,
		)
		// === Interest is charged the second time ===
		.then_execute_with(|_| {
			// This time we expect a smaller amount due to the partial repayment (which both
			// the principal and the pool's utilisation):
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().collateral,
				BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL - total_interest)])
			);

			assert_eq!(
				PendingPoolFees::<Test>::get(LOAN_ASSET),
				BTreeMap::from([(
					COLLATERAL_ASSET,
					take_network_fee(origination_fee).1 + pool_interest_1 + pool_interest_2
				)])
			);
		})
		.then_execute_with(|_| {
			// Repaying the remainder of the borrowed amount should finalise the loan:
			assert_ok!(LendingPools::try_making_repayment(
				&BORROWER,
				LOAN_ID,
				PRINCIPAL - REPAYMENT_AMOUNT
			));
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), 0);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
				loan_id: LOAN_ID,
				outstanding_principal: 0,
				via_liquidation: false,
			}));

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
					liquidation_status: LiquidationStatus::NoLiquidation,
					// Note that we don't automatically release the collateral:
					collateral: BTreeMap::from([(
						COLLATERAL_ASSET,
						INIT_COLLATERAL - total_interest
					)]),
					loans: Default::default(),
				})
			);
		});
}

#[test]
fn collateral_auto_topup() {
	const COLLATERAL_ASSET: Asset = Asset::Eth;

	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV
	const COLLATERAL_TOPUP: AssetAmount = INIT_COLLATERAL / 100;

	// The user deposits this much of collateral asset into their balance at a later point
	const EXTRA_FUNDS: AssetAmount = INIT_COLLATERAL;

	let origination_fee = CONFIG.origination_fee(LOAN_ASSET) * PRINCIPAL * SWAP_RATE;

	fn get_ltv() -> FixedU64 {
		LoanAccounts::<Test>::get(BORROWER).unwrap().derive_ltv().unwrap()
	}

	fn get_collateral() -> AssetAmount {
		*LoanAccounts::<Test>::get(BORROWER)
			.unwrap()
			.collateral
			.get(&COLLATERAL_ASSET)
			.unwrap()
	}

	new_test_ext()
		.execute_with(|| {
			setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE * 1_000_000);
			set_asset_price_in_usd(COLLATERAL_ASSET, 1_000_000);

			MockBalance::credit_account(
				&BORROWER,
				COLLATERAL_ASSET,
				INIT_COLLATERAL + origination_fee + COLLATERAL_TOPUP,
			);

			assert_eq!(
				LendingPools::new_loan(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					Some(COLLATERAL_ASSET),
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
				),
				Ok(LOAN_ID)
			);

			assert_eq!(get_ltv(), FixedU64::from_rational(80, 100)); // ~80%

			// The price drops 1%, but that shouldn't trigger a top-up
			// at the next block
			set_asset_price_in_usd(COLLATERAL_ASSET, 990_000);

			assert_eq!(get_ltv(), FixedU64::from_rational(808_080_808, 1_000_000_000)); // ~81%
		})
		.then_execute_at_next_block(|_| {
			// No change in collateral (no auto top up):
			assert_eq!(get_collateral(), INIT_COLLATERAL);

			// Drop the price further, this time auto-top up should be triggered
			set_asset_price_in_usd(COLLATERAL_ASSET, 920_000);

			assert_eq!(get_ltv(), FixedU64::from_rational(869_565_217, 1_000_000_000)); // ~87%
		})
		.then_execute_at_next_block(|_| {
			// The user only had a small amount in their balance, all of it gets used:
			assert_eq!(get_collateral(), INIT_COLLATERAL + COLLATERAL_TOPUP);
			assert_eq!(get_ltv(), FixedU64::from_rational(860_955_661, 1_000_000_000)); // ~86%
			assert_eq!(MockBalance::get_balance(&LENDER, COLLATERAL_ASSET), 0);

			// After we give the user more funds, auto-top up should bring CR back to target
			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, EXTRA_FUNDS);
		})
		.then_execute_at_next_block(|_| {
			// This much happens to be the exact amount needed to bring CR back to target
			const COLLATERAL_TOPUP_2: AssetAmount = 1_923_913_044;

			assert_eq!(get_ltv(), FixedU64::from_rational(80, 100)); // ~80%
			assert_eq!(get_collateral(), INIT_COLLATERAL + COLLATERAL_TOPUP + COLLATERAL_TOPUP_2);
			assert_eq!(
				MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET),
				EXTRA_FUNDS - COLLATERAL_TOPUP_2
			);
		});
}

#[test]
fn basic_loan_aggregation() {
	const COLLATERAL_ASSET: Asset = Asset::Eth;

	const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE; // 75% LTV

	// Should be able to borrow this amount without providing any extra collateral:
	const EXTRA_PRINCIPAL_1: AssetAmount = PRINCIPAL / 100;
	// This larger amount should require extra collateral:
	const EXTRA_PRINCIPAL_2: AssetAmount = PRINCIPAL / 2;
	const EXTRA_COLLATERAL: AssetAmount = INIT_COLLATERAL / 2;

	let origination_fee = CONFIG.origination_fee(LOAN_ASSET) * PRINCIPAL * SWAP_RATE;

	let origination_fee_2 = CONFIG.origination_fee(LOAN_ASSET) * EXTRA_PRINCIPAL_1 * SWAP_RATE;

	// NOTE: expecting utilisation to go up as we keep borrowing more
	let origination_fee_3 = CONFIG.origination_fee(LOAN_ASSET) * EXTRA_PRINCIPAL_2 * SWAP_RATE;

	new_test_ext().execute_with(|| {
		setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

		set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
		set_asset_price_in_usd(COLLATERAL_ASSET, 1);

		MockBalance::credit_account(
			&BORROWER,
			COLLATERAL_ASSET,
			INIT_COLLATERAL + origination_fee + origination_fee_2,
		);

		assert_eq!(
			LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				PRINCIPAL,
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
			),
			Ok(LOAN_ID)
		);

		System::reset_events();

		// Should have enough collateral to borrow a little more on the same loan
		{
			assert_ok!(LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				EXTRA_PRINCIPAL_1,
				Default::default()
			));

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LoanUpdated {
				loan_id: LOAN_ID,
				extra_principal_amount: EXTRA_PRINCIPAL_1,
			}));

			let (network_fee, pool_fee) = take_network_fee(origination_fee_2);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanUpdated {
					loan_id: LOAN_ID,
					extra_principal_amount: EXTRA_PRINCIPAL_1,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::OriginationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee: pool_fee_taken,
					network_fee: network_fee_taken,
					broker_fee: 0,
				}) if pool_fee_taken == pool_fee && network_fee_taken == network_fee
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT,
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL - EXTRA_PRINCIPAL_1,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
				}
			);

			assert_eq!(
				PendingPoolFees::<Test>::get(LOAN_ASSET),
				BTreeMap::from([(
					COLLATERAL_ASSET,
					take_network_fee(origination_fee).1 + take_network_fee(origination_fee_2).1
				)])
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap(),
				LoanAccount {
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
					collateral: BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							id: LOAN_ID,
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL + EXTRA_PRINCIPAL_1,
							pending_interest: Default::default()
						}
					)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
				}
			);

			assert_eq!(
				MockBalance::get_balance(&BORROWER, LOAN_ASSET),
				PRINCIPAL + EXTRA_PRINCIPAL_1
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
		}

		MockBalance::credit_account(
			&BORROWER,
			COLLATERAL_ASSET,
			EXTRA_COLLATERAL + origination_fee_3,
		);

		// Try to borrow more, but this time we don't have enough collateral
		assert_err!(
			LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				EXTRA_PRINCIPAL_2,
				Default::default()
			),
			Error::<Test>::InsufficientCollateral
		);

		// Should succeed when trying again with extra collateral
		{
			assert_ok!(LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				EXTRA_PRINCIPAL_2,
				BTreeMap::from([(COLLATERAL_ASSET, EXTRA_COLLATERAL)])
			));

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT,
					// Pool's available amount has been reduced:
					available_amount: INIT_POOL_AMOUNT -
						PRINCIPAL - EXTRA_PRINCIPAL_1 -
						EXTRA_PRINCIPAL_2,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
				}
			);

			// Pool has accrued extra fees:
			assert_eq!(
				PendingPoolFees::<Test>::get(LOAN_ASSET),
				BTreeMap::from([(
					COLLATERAL_ASSET,
					take_network_fee(origination_fee).1 +
						take_network_fee(origination_fee_2).1 +
						take_network_fee(origination_fee_3).1
				)])
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap(),
				LoanAccount {
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
					// Loan's collateral has been increased:
					collateral: BTreeMap::from([(
						COLLATERAL_ASSET,
						INIT_COLLATERAL + EXTRA_COLLATERAL
					)]),
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							id: LOAN_ID,
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
							// Loan's owed principal has been increased:
							owed_principal: PRINCIPAL + EXTRA_PRINCIPAL_1 + EXTRA_PRINCIPAL_2,
							pending_interest: Default::default()
						}
					)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
				}
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);

			assert_eq!(
				MockBalance::get_balance(&BORROWER, LOAN_ASSET),
				PRINCIPAL + EXTRA_PRINCIPAL_1 + EXTRA_PRINCIPAL_2
			);
		}
	});
}

#[test]
fn interest_special_cases() {
	// We want the amount to be large enough that we can charge interest immediately
	// (rather than waiting for fractional amounts to accumulate).
	const PRINCIPAL: AssetAmount = 2_000_000_000_000;
	const INIT_POOL_AMOUNT: AssetAmount = PRINCIPAL * 2;

	const PRIMARY_COLLATERAL_ASSET: Asset = Asset::Eth;
	const SECONDARY_COLLATERAL_ASSET: Asset = Asset::Usdc;

	const TOTAL_COLLATERAL_REQUIRED: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV

	// A small but non-zero amount is in the primary asset:
	const INIT_COLLATERAL_AMOUNT_PRIMARY: AssetAmount = 1_000_000;
	// The remaining amount is in the secondary asset:
	const INIT_COLLATERAL_AMOUNT_SECONDARY: AssetAmount =
		TOTAL_COLLATERAL_REQUIRED - INIT_COLLATERAL_AMOUNT_PRIMARY;

	let (full_pool_interest_amount, full_network_interest_amount) =
		derive_interest_amounts(PRINCIPAL, Permill::from_percent(50));

	// Primary collateral is expected to cover network fee entirely
	assert!(full_network_interest_amount < INIT_COLLATERAL_AMOUNT_PRIMARY);

	// The remaining primary collateral will be completely consumed by the pool interest
	let pool_interest_amount_primary =
		INIT_COLLATERAL_AMOUNT_PRIMARY - full_network_interest_amount;

	// A portion of pool interest will have to be charged from the secondary collateral
	let pool_interest_amount_secondary = full_pool_interest_amount - pool_interest_amount_primary;

	let origination_fee = CONFIG.origination_fee(LOAN_ASSET) * PRINCIPAL * SWAP_RATE;

	new_test_ext()
		.execute_with(|| {
			setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE * 1_000_000);

			// For simplicity, both collateral assets have the same price
			set_asset_price_in_usd(PRIMARY_COLLATERAL_ASSET, 1_000_000);
			set_asset_price_in_usd(SECONDARY_COLLATERAL_ASSET, 1_000_000);

			MockBalance::credit_account(
				&BORROWER,
				PRIMARY_COLLATERAL_ASSET,
				INIT_COLLATERAL_AMOUNT_PRIMARY + origination_fee,
			);

			MockBalance::credit_account(
				&BORROWER,
				SECONDARY_COLLATERAL_ASSET,
				INIT_COLLATERAL_AMOUNT_SECONDARY,
			);

			assert_eq!(
				LendingPools::new_loan(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					Some(PRIMARY_COLLATERAL_ASSET),
					BTreeMap::from([
						(PRIMARY_COLLATERAL_ASSET, INIT_COLLATERAL_AMOUNT_PRIMARY),
						(SECONDARY_COLLATERAL_ASSET, INIT_COLLATERAL_AMOUNT_SECONDARY)
					])
				),
				Ok(LOAN_ID)
			);
		})
		.then_execute_at_block(INIT_BLOCK + CONFIG.interest_payment_interval_blocks as u64, |_| {
			assert_eq!(
				&PendingPoolFees::<Test>::get(LOAN_ASSET),
				&BTreeMap::from([
					// All of the primary asset is consumed as interest:
					(
						PRIMARY_COLLATERAL_ASSET,
						pool_interest_amount_primary + take_network_fee(origination_fee).1,
					),
					// The remainder is charged from the secondary asset:
					(SECONDARY_COLLATERAL_ASSET, pool_interest_amount_secondary),
				]),
			);

			assert_eq!(
				&LoanAccounts::<Test>::get(BORROWER).unwrap().collateral,
				&BTreeMap::from([
					(PRIMARY_COLLATERAL_ASSET, 0),
					(
						SECONDARY_COLLATERAL_ASSET,
						INIT_COLLATERAL_AMOUNT_SECONDARY - pool_interest_amount_secondary,
					),
				]),
			);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::InterestTaken {
				loan_id: LOAN_ID,
				pool_interest: BTreeMap::from([
					(PRIMARY_COLLATERAL_ASSET, pool_interest_amount_primary),
					(SECONDARY_COLLATERAL_ASSET, pool_interest_amount_secondary),
				]),
				network_interest: BTreeMap::from([(
					PRIMARY_COLLATERAL_ASSET,
					full_network_interest_amount,
				)]),
				broker_interest: Default::default(),
				low_ltv_penalty: Default::default(),
			}));
		})
		.then_execute_at_block(
			INIT_BLOCK + 2 * CONFIG.interest_payment_interval_blocks as u64,
			|_| {
				// The second time the fee is collected, it comes entirely from the secondary asset:
				assert_eq!(
					PendingPoolFees::<Test>::get(LOAN_ASSET),
					BTreeMap::from([
						(
							PRIMARY_COLLATERAL_ASSET,
							pool_interest_amount_primary + take_network_fee(origination_fee).1
						),
						(
							SECONDARY_COLLATERAL_ASSET,
							// Unlike first interest payment, second interest payment is paid
							// entirely in the secondary collateral asset
							pool_interest_amount_secondary + full_pool_interest_amount
						)
					])
				);
			},
		);
}

#[test]
fn swap_collected_network_fees() {
	const ASSET_1: Asset = Asset::Eth;
	const ASSET_2: Asset = Asset::Usdc;

	const AMOUNT_1: AssetAmount = 200_000;
	const AMOUNT_2: AssetAmount = 100_000;

	let fee_swap_block = CONFIG.fee_swap_interval_blocks as u64;

	new_test_ext()
		.execute_with(|| {
			LendingPools::take_network_fee(AMOUNT_1 * 2, ASSET_1, Permill::from_percent(50));
			LendingPools::take_network_fee(AMOUNT_2 * 4, ASSET_2, Permill::from_percent(25));

			assert_eq!(
				PendingNetworkFees::<Test>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(ASSET_1, AMOUNT_1), (ASSET_2, AMOUNT_2)])
			);
		})
		.then_execute_at_block(fee_swap_block, |_| {
			// Network fee swaps should be initiated here
			assert_eq!(
				MockSwapRequestHandler::<Test>::get_swap_requests(),
				BTreeMap::from([
					(
						SwapRequestId(0),
						MockSwapRequest {
							input_asset: ASSET_1,
							output_asset: Asset::Flip,
							input_amount: AMOUNT_1,
							remaining_input_amount: AMOUNT_1,
							accumulated_output_amount: 0,
							swap_type: SwapRequestType::NetworkFee,
							broker_fees: Default::default(),
							origin: SwapOrigin::Internal
						}
					),
					(
						SwapRequestId(1),
						MockSwapRequest {
							input_asset: ASSET_2,
							output_asset: Asset::Flip,
							input_amount: AMOUNT_2,
							remaining_input_amount: AMOUNT_2,
							accumulated_output_amount: 0,
							swap_type: SwapRequestType::NetworkFee,
							broker_fees: Default::default(),
							origin: SwapOrigin::Internal
						}
					)
				])
			);

			assert_eq!(
				PendingNetworkFees::<Test>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(ASSET_1, 0), (ASSET_2, 0)])
			);
		});
}

#[test]
fn swap_collected_pool_fees() {
	const COLLATERAL_ASSET_1: Asset = Asset::Usdc;
	const COLLATERAL_ASSET_2: Asset = Asset::Eth;

	const INIT_FEE_ASSET_1: AssetAmount = 100;
	const INIT_FEE_ASSET_2: AssetAmount = 1;

	const NETWORK_FEE_AMOUNT: AssetAmount = 30;

	let fee_swap_block = CONFIG.fee_swap_interval_blocks as u64;

	const POOL_FEE_SWAP_ID: SwapRequestId = SwapRequestId(0);
	const NETWORK_FEE_SWAP_ID: SwapRequestId = SwapRequestId(1);

	new_test_ext()
		.execute_with(|| {
			setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

			set_asset_price_in_usd(COLLATERAL_ASSET_1, 1_000_000);
			set_asset_price_in_usd(COLLATERAL_ASSET_2, 5_000_000);
			set_asset_price_in_usd(LOAN_ASSET, 10_000_000);

			LendingPools::credit_fees_to_pool(LOAN_ASSET, COLLATERAL_ASSET_1, INIT_FEE_ASSET_1);
			LendingPools::credit_fees_to_pool(LOAN_ASSET, COLLATERAL_ASSET_2, INIT_FEE_ASSET_2);

			LendingPools::take_network_fee(NETWORK_FEE_AMOUNT, COLLATERAL_ASSET_1, Permill::one());

			assert_eq!(
				PendingPoolFees::<Test>::get(LOAN_ASSET),
				BTreeMap::from([
					(COLLATERAL_ASSET_1, INIT_FEE_ASSET_1),
					(COLLATERAL_ASSET_2, INIT_FEE_ASSET_2)
				]),
			);
		})
		.then_execute_at_next_block(|_| {
			assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());
		})
		.then_execute_at_block(fee_swap_block, |_| {
			// Expecting a fee swap from asset 1 but not asset 2:
			assert_eq!(
				MockSwapRequestHandler::<Test>::get_swap_requests(),
				BTreeMap::from([
					(
						POOL_FEE_SWAP_ID,
						MockSwapRequest {
							input_asset: COLLATERAL_ASSET_1,
							output_asset: LOAN_ASSET,
							input_amount: INIT_FEE_ASSET_1,
							remaining_input_amount: INIT_FEE_ASSET_1,
							accumulated_output_amount: 0,
							swap_type: SwapRequestType::Regular {
								output_action: SwapOutputAction::CreditLendingPool {
									swap_type: LendingSwapType::FeeSwap { pool_asset: LOAN_ASSET }
								}
							},
							broker_fees: Default::default(),
							origin: SwapOrigin::Internal
						}
					),
					(
						NETWORK_FEE_SWAP_ID,
						MockSwapRequest {
							input_asset: COLLATERAL_ASSET_1,
							output_asset: Asset::Flip,
							input_amount: NETWORK_FEE_AMOUNT,
							remaining_input_amount: NETWORK_FEE_AMOUNT,
							accumulated_output_amount: 0,
							swap_type: SwapRequestType::NetworkFee,
							broker_fees: Default::default(),
							origin: SwapOrigin::Internal
						}
					)
				])
			);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(
				Event::<Test>::LendingPoolFeeSwapInitiated {
					asset: LOAN_ASSET,
					swap_request_id: POOL_FEE_SWAP_ID,
				},
			));

			assert_has_event::<Test>(RuntimeEvent::LendingPools(
				Event::<Test>::LendingNetworkFeeSwapInitiated {
					swap_request_id: NETWORK_FEE_SWAP_ID,
				},
			));

			assert_eq!(
				PendingPoolFees::<Test>::get(LOAN_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET_1, 0), (COLLATERAL_ASSET_2, INIT_FEE_ASSET_2)]),
			);
		})
		.then_execute_at_block(fee_swap_block + SWAP_DELAY_BLOCKS as u64, |_| {
			const FEE_SWAP_OUTPUT_1: AssetAmount = INIT_FEE_ASSET_1 / 10;

			// Simulate fee swap:
			LendingPools::process_loan_swap_outcome(
				POOL_FEE_SWAP_ID,
				LendingSwapType::FeeSwap { pool_asset: LOAN_ASSET },
				FEE_SWAP_OUTPUT_1,
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + FEE_SWAP_OUTPUT_1,
					available_amount: INIT_POOL_AMOUNT + FEE_SWAP_OUTPUT_1,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
				}
			);

			assert_eq!(
				PendingPoolFees::<Test>::get(LOAN_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET_1, 0), (COLLATERAL_ASSET_2, INIT_FEE_ASSET_2)])
			);
		});
}

#[test]
fn adding_and_removing_collateral() {
	const COLLATERAL_ASSET: Asset = Asset::Eth;
	const COLLATERAL_ASSET_2: Asset = Asset::Btc;

	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV
	const INIT_COLLATERAL_AMOUNT_2: AssetAmount = 1000;

	let origination_fee = CONFIG.origination_fee(LOAN_ASSET) * PRINCIPAL * SWAP_RATE;

	new_test_ext().execute_with(|| {
		setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);
		set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
		set_asset_price_in_usd(COLLATERAL_ASSET, 1);
		set_asset_price_in_usd(COLLATERAL_ASSET_2, 1);

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL + origination_fee);
		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET_2, INIT_COLLATERAL_AMOUNT_2);

		let collateral = BTreeMap::from([
			(COLLATERAL_ASSET, INIT_COLLATERAL),
			(COLLATERAL_ASSET_2, INIT_COLLATERAL_AMOUNT_2),
		]);

		assert_ok!(LendingPools::add_collateral(
			RuntimeOrigin::signed(BORROWER),
			Some(COLLATERAL_ASSET),
			collateral.clone(),
		));

		assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
			borrower_id: BORROWER,
			collateral: collateral.clone(),
			primary_collateral_asset: COLLATERAL_ASSET,
		}));

		// Adding collateral creates a loan account:
		assert_eq!(
			LoanAccounts::<Test>::get(BORROWER).unwrap(),
			LoanAccount {
				borrower_id: BORROWER,
				primary_collateral_asset: COLLATERAL_ASSET,
				collateral: collateral.clone(),
				loans: BTreeMap::default(),
				liquidation_status: LiquidationStatus::NoLiquidation,
			}
		);

		assert_ok!(LendingPools::remove_collateral(
			RuntimeOrigin::signed(BORROWER),
			Some(COLLATERAL_ASSET),
			BTreeMap::from([
				(COLLATERAL_ASSET, INIT_COLLATERAL),
				(COLLATERAL_ASSET_2, INIT_COLLATERAL_AMOUNT_2),
			]),
		));

		assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::CollateralRemoved {
			borrower_id: BORROWER,
			collateral,
			primary_collateral_asset: COLLATERAL_ASSET,
		}));

		// Account is removed if all of its collateral is removed:
		assert!(LoanAccounts::<Test>::get(BORROWER).is_none());
	});
}

#[test]
fn basic_liquidation() {
	// Summary: creates a loan and drops oracle price to trigger soft liquidation. The liquidation
	// swap is executed half way through, which should be sufficient to bring CR back to healthy and
	// abort the liquidation. Then we drop the price again to trigger hard liquidation, which will
	// be fully executed repaying the principal in full and closing the loan.

	const COLLATERAL_ASSET: Asset = Asset::Eth;

	// This should trigger soft liquidation
	const NEW_SWAP_RATE: u128 = 23;

	// This should trigger second (hard) liquidation
	const SWAP_RATE_2: u128 = 26;

	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV
	let origination_fee = CONFIG.origination_fee(LOAN_ASSET) * PRINCIPAL * SWAP_RATE;

	// How much collateral will be swapped during liquidation:
	const EXECUTED_COLLATERAL: AssetAmount = 2 * INIT_COLLATERAL / 5;
	// How much of principal asset is bought during first liquidation:
	const SWAPPED_PRINCIPAL: AssetAmount = EXECUTED_COLLATERAL / NEW_SWAP_RATE;

	let liquidation_fee_1 = CONFIG.liquidation_fee(LOAN_ASSET) * SWAPPED_PRINCIPAL;

	// This much will be repaid via first liquidation (everything swapped minus liquidation fee)
	let repaid_amount_1 = SWAPPED_PRINCIPAL - liquidation_fee_1;

	// How much of principal asset is bought during second liquidation:
	const SWAPPED_PRINCIPAL_2: AssetAmount = (INIT_COLLATERAL - EXECUTED_COLLATERAL) / SWAP_RATE_2;
	let liquidation_fee_2 = CONFIG.liquidation_fee(LOAN_ASSET) * SWAPPED_PRINCIPAL_2;

	// This much will be repaid via second liquidation (full principal after first repayment)
	let repaid_amount_2 = PRINCIPAL - repaid_amount_1;

	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);
	const LIQUIDATION_SWAP_2: SwapRequestId = SwapRequestId(1);

	new_test_ext()
		.execute_with(|| {
			// === CREATE A LOAN ===
			setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);
			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
			set_asset_price_in_usd(COLLATERAL_ASSET, 1);

			MockBalance::credit_account(
				&BORROWER,
				COLLATERAL_ASSET,
				INIT_COLLATERAL + origination_fee,
			);

			assert_eq!(
				LendingPools::new_loan(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					Some(COLLATERAL_ASSET),
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
				),
				Ok(LOAN_ID)
			);

			// Drop oracle price to trigger liquidation
			set_asset_price_in_usd(LOAN_ASSET, NEW_SWAP_RATE);
		})
		.then_execute_at_next_block(|_| {
			// Expecting a liquidation swap here:
			let liquidation_swap = MockSwapRequestHandler::<Test>::get_swap_requests()
				.get(&LIQUIDATION_SWAP_1)
				.expect("No swap request found")
				.clone();

			assert_eq!(
				liquidation_swap,
				MockSwapRequest {
					input_asset: COLLATERAL_ASSET,
					output_asset: LOAN_ASSET,
					input_amount: INIT_COLLATERAL,
					remaining_input_amount: INIT_COLLATERAL,
					accumulated_output_amount: 0,
					swap_type: SwapRequestType::Regular {
						output_action: SwapOutputAction::CreditLendingPool {
							swap_type: LendingSwapType::Liquidation {
								borrower_id: BORROWER,
								loan_id: LOAN_ID
							}
						}
					},
					broker_fees: Default::default(),
					origin: SwapOrigin::Internal
				}
			);

			let loan_account = LoanAccounts::<Test>::get(BORROWER).unwrap();

			assert_eq!(
				loan_account.liquidation_status,
				LiquidationStatus::Liquidating {
					liquidation_swaps: BTreeMap::from([(
						LIQUIDATION_SWAP_1,
						LiquidationSwap {
							loan_id: LOAN_ID,
							from_asset: COLLATERAL_ASSET,
							to_asset: LOAN_ASSET
						}
					)]),
					is_hard: false
				}
			);

			// All collateral should now be in liquidation swaps
			assert_eq!(loan_account.collateral, Default::default());

			// Despite collateral having been moved to the swapping pallet, we
			// can still calculate its value:
			assert_eq!(loan_account.total_collateral_usd_value().unwrap(), INIT_COLLATERAL);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(
				Event::<Test>::LiquidationInitiated {
					borrower_id: BORROWER,
					swaps: BTreeMap::from([(LOAN_ID, vec![LIQUIDATION_SWAP_1])]),
					is_hard: false,
				},
			));

			// "Simulate" partial execution of the swap:
			MockSwapRequestHandler::<Test>::set_swap_request_progress(
				LIQUIDATION_SWAP_1,
				SwapExecutionProgress {
					remaining_input_amount: INIT_COLLATERAL - EXECUTED_COLLATERAL,
					accumulated_output_amount: SWAPPED_PRINCIPAL,
				},
			);
		})
		.then_execute_at_next_block(|_| {
			// The loan should be "healthy" again:
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
					liquidation_status: LiquidationStatus::NoLiquidation,
					// Note that we don't automatically release the collateral:
					collateral: BTreeMap::from([(
						COLLATERAL_ASSET,
						INIT_COLLATERAL - EXECUTED_COLLATERAL
					)]),
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							id: LOAN_ID,
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL - (SWAPPED_PRINCIPAL - liquidation_fee_1),
							pending_interest: Default::default()
						}
					)]),
				})
			);

			// Liquidation Swap must have been aborted:
			assert!(!MockSwapRequestHandler::<Test>::get_swap_requests()
				.contains_key(&LIQUIDATION_SWAP_1));

			let (liquidation_fee_network, liquidation_fee_pool) =
				take_network_fee(liquidation_fee_1);

			// Part of the principal has been repaid via liquidation:
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + liquidation_fee_pool,
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL +
						(SWAPPED_PRINCIPAL - liquidation_fee_network),
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
				}
			);

			assert_eq!(
				PendingPoolFees::<Test>::get(LOAN_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, take_network_fee(origination_fee).1)])
			);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount,
				}) if amount == repaid_amount_1,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee,
					network_fee,
					broker_fee
				}) if pool_fee == liquidation_fee_pool && network_fee == liquidation_fee_network && broker_fee == 0
			);

			// Drop oracle price again to trigger liquidation:
			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE_2);
		})
		.then_execute_at_next_block(|_| {
			// Expecting a liquidation swap here:
			let liquidation_swap = MockSwapRequestHandler::<Test>::get_swap_requests()
				.get(&LIQUIDATION_SWAP_2)
				.expect("No swap request found")
				.clone();

			assert_eq!(
				liquidation_swap,
				MockSwapRequest {
					input_asset: COLLATERAL_ASSET,
					output_asset: LOAN_ASSET,
					input_amount: INIT_COLLATERAL - EXECUTED_COLLATERAL,
					remaining_input_amount: INIT_COLLATERAL - EXECUTED_COLLATERAL,
					accumulated_output_amount: 0,
					swap_type: SwapRequestType::Regular {
						output_action: SwapOutputAction::CreditLendingPool {
							swap_type: LendingSwapType::Liquidation {
								borrower_id: BORROWER,
								loan_id: LOAN_ID
							},
						}
					},
					broker_fees: Default::default(),
					origin: SwapOrigin::Internal
				}
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
				LiquidationStatus::Liquidating {
					liquidation_swaps: BTreeMap::from([(
						LIQUIDATION_SWAP_2,
						LiquidationSwap {
							loan_id: LOAN_ID,
							from_asset: COLLATERAL_ASSET,
							to_asset: LOAN_ASSET
						}
					)]),
					is_hard: true
				}
			);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(
				Event::<Test>::LiquidationInitiated {
					borrower_id: BORROWER,
					swaps: BTreeMap::from([(LOAN_ID, vec![LIQUIDATION_SWAP_2])]),
					is_hard: true,
				},
			));
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

			System::reset_events();

			LendingPools::process_loan_swap_outcome(
				LIQUIDATION_SWAP_2,
				LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
				SWAPPED_PRINCIPAL_2,
			);

			let (liquidation_fee_network_2, liquidation_fee_pool_2) =
				take_network_fee(liquidation_fee_2);


			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount,
				}) if amount == repaid_amount_2,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee,
					network_fee,
					broker_fee
				}) if pool_fee == liquidation_fee_pool_2 && network_fee == liquidation_fee_network_2 && broker_fee == 0,
				// The loan should now be settled:
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal: 0,
					via_liquidation: true,
				})
			);

			// This excess principal asset amount will be credited to the borrower's account
			let excess_principal = SWAPPED_PRINCIPAL_2 - repaid_amount_2 - liquidation_fee_2;

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT +
						take_network_fee(liquidation_fee_1).1 +
						take_network_fee(liquidation_fee_2).1,
					available_amount: INIT_POOL_AMOUNT +
						take_network_fee(liquidation_fee_1).1 +
						take_network_fee(liquidation_fee_2).1,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
				}
			);

			assert_eq!(
				PendingPoolFees::<Test>::get(LOAN_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, take_network_fee(origination_fee).1)])
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
					collateral: BTreeMap::from([(LOAN_ASSET, excess_principal)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					loans: Default::default(),
				})
			);
		});
}

#[test]
fn liquidation_with_outstanding_principal() {
	// Test a scenario where a loan is liquidated and the recovered principal
	// isn't enought to cover the total loan amount.

	const COLLATERAL_ASSET: Asset = Asset::Eth;
	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV

	const RECOVERED_PRINCIPAL: AssetAmount = 3 * PRINCIPAL / 4;

	let origination_fee = CONFIG.origination_fee(LOAN_ASSET) * PRINCIPAL * SWAP_RATE;
	let collateral = BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]);

	const NEW_SWAP_RATE: u128 = SWAP_RATE * 2;

	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);

	new_test_ext()
		.execute_with(|| {
			setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);
			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
			set_asset_price_in_usd(COLLATERAL_ASSET, 1);

			MockBalance::credit_account(
				&BORROWER,
				COLLATERAL_ASSET,
				INIT_COLLATERAL + origination_fee,
			);

			assert_eq!(
				LendingPools::new_loan(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					Some(COLLATERAL_ASSET),
					collateral.clone(),
				),
				Ok(LOAN_ID)
			);

			// Drop oracle price to trigger liquidation
			set_asset_price_in_usd(LOAN_ASSET, NEW_SWAP_RATE);
		})
		.then_execute_at_next_block(|_| {
			assert!(MockSwapRequestHandler::<Test>::get_swap_requests()
				.contains_key(&LIQUIDATION_SWAP_1));

			System::reset_events();

			LendingPools::process_loan_swap_outcome(
				LIQUIDATION_SWAP_1,
				LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
				RECOVERED_PRINCIPAL,
			);

			let liquidation_fee = CONFIG.liquidation_fee(LOAN_ASSET) * RECOVERED_PRINCIPAL;
			let (liquidation_fee_network, liquidation_fee_pool) =
				take_network_fee(liquidation_fee);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount
				}) if amount == RECOVERED_PRINCIPAL - liquidation_fee,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee,
					network_fee,
					broker_fee
				}) if pool_fee == liquidation_fee_pool && network_fee == liquidation_fee_network && broker_fee == 0,
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal,
					via_liquidation: true,
				}) if outstanding_principal == PRINCIPAL - RECOVERED_PRINCIPAL + liquidation_fee
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);
			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);

			// The account has no loans and no collateral, so it should have been removed:
			assert!(!LoanAccounts::<Test>::contains_key(BORROWER));
		});
}

#[test]
fn small_interest_amounts_accumulate() {
	const PRINCIPAL: AssetAmount = 10_000_000;
	const INIT_POOL_AMOUNT: AssetAmount = PRINCIPAL * 10;
	const COLLATERAL_ASSET: Asset = Asset::Eth;
	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV

	let config = LendingConfiguration {
		network_fee_contributions: NetworkFeeContributions {
			// Set a higher network interest so it is closer to the pool interest,
			// which means both interest amounts will reach the threshold at the same time
			extra_interest: Permill::from_percent(3),
			..CONFIG.network_fee_contributions
		},
		// Set a low interest collection threshold so we can test both being able to collect
		// fractional interest and taking interest when reaching the threshold without too
		// many iterations:
		interest_collection_threshold_usd: 1,
		..CONFIG
	};

	let origination_fee = config.origination_fee(LOAN_ASSET) * PRINCIPAL * SWAP_RATE;

	let pool_interest = config
		.derive_base_interest_rate_per_payment_interval(LOAN_ASSET, Permill::from_percent(10));

	let network_interest = config.derive_network_interest_rate_per_payment_interval();

	// Expected fees in pool's asset
	let pool_amount = ScaledAmountHP::from_asset_amount(PRINCIPAL) * pool_interest;
	let network_amount = ScaledAmountHP::from_asset_amount(PRINCIPAL) * network_interest;

	// Making sure the fees are non-zero fractions below 1:
	assert_eq!(pool_amount.into_asset_amount(), 0);
	assert_eq!(network_amount.into_asset_amount(), 0);
	assert!(pool_amount.as_raw() > 0);
	assert!(network_amount.as_raw() > 0);

	new_test_ext()
		.execute_with(|| {
			setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

			LendingConfig::<Test>::set(config);

			set_asset_price_in_usd(LOAN_ASSET, 1000 * SWAP_RATE);
			set_asset_price_in_usd(COLLATERAL_ASSET, 1000);

			MockBalance::credit_account(
				&BORROWER,
				COLLATERAL_ASSET,
				INIT_COLLATERAL + origination_fee,
			);

			assert_eq!(
				LendingPools::new_loan(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					Some(COLLATERAL_ASSET),
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
				),
				Ok(LOAN_ID)
			);
		})
		.then_process_blocks_until_block(
			INIT_BLOCK + CONFIG.interest_payment_interval_blocks as u64,
		)
		// Interest should be recorded here, but not taken yet (it is too small)
		.then_execute_with(|_| {
			let account = LoanAccounts::<Test>::get(BORROWER).unwrap();
			assert_eq!(account.collateral, BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]));

			assert_eq!(
				account.loans[&LOAN_ID].pending_interest,
				InterestBreakdown {
					network: network_amount,
					pool: pool_amount,
					broker: Default::default(),
					low_ltv_penalty: Default::default()
				}
			);
		})
		.then_process_blocks_until_block(
			INIT_BLOCK + 2 * CONFIG.interest_payment_interval_blocks as u64,
		)
		.then_execute_with(|_| {
			let account = LoanAccounts::<Test>::get(BORROWER).unwrap();

			let mut pool_amount_total = pool_amount.saturating_add(pool_amount);
			let mut network_amount_total = network_amount.saturating_add(network_amount);

			let pool_amount_taken = pool_amount_total.take_whole_amount();
			let network_amount_taken = network_amount_total.take_whole_amount();

			// Over two interest payment periods both amounts are expected to become non-zero
			assert!(pool_amount_taken > 0);
			assert!(network_amount_taken > 0);

			assert_eq!(
				account.collateral,
				BTreeMap::from([(
					COLLATERAL_ASSET,
					INIT_COLLATERAL - (pool_amount_taken + network_amount_taken) * SWAP_RATE
				)])
			);

			assert_eq!(
				account.loans[&LOAN_ID].pending_interest,
				InterestBreakdown {
					network: network_amount_total,
					pool: pool_amount_total,
					broker: Default::default(),
					low_ltv_penalty: Default::default()
				}
			);
		});
}

#[test]
fn making_loan_repayment() {
	const COLLATERAL_ASSET: Asset = Asset::Eth;
	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV

	let origination_fee = CONFIG.origination_fee(LOAN_ASSET) * PRINCIPAL * SWAP_RATE;

	let collateral = BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]);

	const FIRST_REPAYMENT: AssetAmount = PRINCIPAL / 4;

	new_test_ext().execute_with(|| {
		setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

		set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE * 1_000_000);
		set_asset_price_in_usd(COLLATERAL_ASSET, 1_000_000);

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL + origination_fee);

		assert_eq!(
			LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				PRINCIPAL,
				Some(COLLATERAL_ASSET),
				collateral.clone(),
			),
			Ok(LOAN_ID)
		);

		assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

		// Make a partial repayment:
		assert_ok!(Pallet::<Test>::make_repayment(
			RuntimeOrigin::signed(BORROWER),
			LOAN_ID,
			FIRST_REPAYMENT
		));

		assert_eq!(
			LoanAccounts::<Test>::get(BORROWER).unwrap().loans[&LOAN_ID].owed_principal,
			PRINCIPAL - FIRST_REPAYMENT
		);
		assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL - FIRST_REPAYMENT);

		assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
			loan_id: LOAN_ID,
			amount: FIRST_REPAYMENT,
		}));

		// No liquidation fees taken:
		assert_matching_event_count!(Test, RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken{..}) => 0);

		// Should not see this event yet:
		assert_matching_event_count!(
			Test,
			RuntimeEvent::LendingPools(Event::<Test>::LoanSettled { .. }) => 0
		);

		System::reset_events();

		// Repay the remaining principal:
		assert_ok!(Pallet::<Test>::make_repayment(
			RuntimeOrigin::signed(BORROWER),
			LOAN_ID,
			PRINCIPAL - FIRST_REPAYMENT
		));

		assert_eq!(LoanAccounts::<Test>::get(BORROWER).unwrap().loans, Default::default());
		// Note that collateral isn't automatically released upon repayment:
		assert_eq!(LoanAccounts::<Test>::get(BORROWER).unwrap().collateral, collateral);
		assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), 0);

		assert_event_sequence!(
			Test,
			RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
				loan_id: LOAN_ID,
				amount: amount_in_event,
			}) if amount_in_event == PRINCIPAL - FIRST_REPAYMENT,
			RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
				loan_id: LOAN_ID,
				outstanding_principal: 0,
				via_liquidation: false,
			})
		);

	});
}

mod safe_mode {

	use super::*;

	#[test]
	fn safe_mode_for_adding_lender_funds() {
		let try_to_add_funds = || {
			LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				INIT_POOL_AMOUNT,
			)
		};

		new_test_ext().execute_with(|| {
			assert_ok!(LendingPools::new_lending_pool(LOAN_ASSET));

			MockBalance::credit_account(&LENDER, LOAN_ASSET, 10 * INIT_POOL_AMOUNT);

			// Adding lender funds is disbled for all assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					add_lender_funds_enabled: BTreeSet::default(),
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_add_funds(), Error::<Test>::AddLenderFundsDisabled);
			}

			// Adding lender funds is enabled, but not for the requested asset:
			{
				const OTHER_ASSET: Asset = Asset::Eth;
				assert_ne!(OTHER_ASSET, LOAN_ASSET);
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					add_lender_funds_enabled: BTreeSet::from([OTHER_ASSET]),
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_add_funds(), Error::<Test>::AddLenderFundsDisabled);
			}

			// Adding lender funds is enabled for the requested asset:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					add_lender_funds_enabled: BTreeSet::from([LOAN_ASSET]),
					..PalletSafeMode::code_green()
				});
				assert_ok!(try_to_add_funds());
			}

			// Adding lender funds is fully enabled (code green):
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode::code_green());
				assert_ok!(try_to_add_funds());
			}
		});
	}

	#[test]
	fn safe_mode_for_removing_lender_funds() {
		let try_to_withdraw = || {
			LendingPools::remove_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				Some(INIT_POOL_AMOUNT / 2),
			)
		};

		new_test_ext().execute_with(|| {
			assert_ok!(LendingPools::new_lending_pool(LOAN_ASSET));

			MockBalance::credit_account(&LENDER, LOAN_ASSET, INIT_POOL_AMOUNT);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				INIT_POOL_AMOUNT
			));

			// Withdrawing is disbled for all assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					withdraw_lender_funds_enabled: BTreeSet::default(),
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_withdraw(), Error::<Test>::RemoveLenderFundsDisabled);
			}

			// Withdrawing is enabled, but not for the requested asset:
			{
				const OTHER_ASSET: Asset = Asset::Eth;
				assert_ne!(OTHER_ASSET, LOAN_ASSET);
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					withdraw_lender_funds_enabled: BTreeSet::from([OTHER_ASSET]),
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_withdraw(), Error::<Test>::RemoveLenderFundsDisabled);
			}

			// Withdrawing is enabled for the requested asset:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					withdraw_lender_funds_enabled: BTreeSet::from([LOAN_ASSET]),
					..PalletSafeMode::code_green()
				});
				assert_ok!(try_to_withdraw());
			}

			// Withdrawing is fully enabled (code green):
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode::code_green());
				assert_ok!(try_to_withdraw());
			}
		});
	}

	#[test]
	fn safe_mode_for_creating_chp_loan() {
		const COLLATERAL_ASSET: Asset = Asset::Eth;
		const INIT_COLLATERAL: AssetAmount = 2 * PRINCIPAL * SWAP_RATE;

		let try_to_borrow = || {
			LendingPools::new_loan(
				LP,
				LOAN_ASSET,
				PRINCIPAL,
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
			)
		};

		new_test_ext().execute_with(|| {
			assert_ok!(LendingPools::new_lending_pool(LOAN_ASSET));

			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
			set_asset_price_in_usd(COLLATERAL_ASSET, 1);

			MockBalance::credit_account(&LENDER, LOAN_ASSET, INIT_POOL_AMOUNT);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				INIT_POOL_AMOUNT
			));

			MockBalance::credit_account(&LP, COLLATERAL_ASSET, 10 * INIT_COLLATERAL);

			// Borrowing is completely disabled:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					borrowing_enabled: BTreeSet::default(),
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_borrow(), Error::<Test>::LoanCreationDisabled);
			}

			// Borrowing is enabled but, not for the asset that we requested:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					borrowing_enabled: BTreeSet::from([COLLATERAL_ASSET]),
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_borrow(), Error::<Test>::LoanCreationDisabled);
			}

			{
				// Should be able to borrow once we enable for the requested asset :
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					borrowing_enabled: BTreeSet::from([LOAN_ASSET]),
					..PalletSafeMode::code_green()
				});
				assert_ok!(try_to_borrow());
			}

			{
				// Should be able to borrow in code green:
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode::code_green());
				assert_ok!(try_to_borrow());
			}
		});
	}

	#[test]
	fn safe_mode_for_adding_collateral() {
		const COLLATERAL_ASSET_1: Asset = Asset::Eth;
		const COLLATERAL_ASSET_2: Asset = Asset::Usdc;
		const INIT_COLLATERAL: AssetAmount = 2 * PRINCIPAL * SWAP_RATE;

		let try_adding_collateral = || {
			LendingPools::add_collateral(
				RuntimeOrigin::signed(BORROWER),
				Some(COLLATERAL_ASSET_1),
				BTreeMap::from([
					(COLLATERAL_ASSET_1, INIT_COLLATERAL),
					(COLLATERAL_ASSET_2, INIT_COLLATERAL),
				]),
			)
		};

		let try_adding_collateral_via_new_loan = || {
			LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				PRINCIPAL,
				Some(COLLATERAL_ASSET_1),
				BTreeMap::from([
					(COLLATERAL_ASSET_1, INIT_COLLATERAL),
					(COLLATERAL_ASSET_2, INIT_COLLATERAL),
				]),
			)
		};

		let try_adding_collateral_via_loan_update = || {
			LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				PRINCIPAL,
				BTreeMap::from([
					(COLLATERAL_ASSET_1, INIT_COLLATERAL),
					(COLLATERAL_ASSET_2, INIT_COLLATERAL),
				]),
			)
		};

		new_test_ext().execute_with(|| {
			assert_ok!(LendingPools::new_lending_pool(LOAN_ASSET));

			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
			set_asset_price_in_usd(COLLATERAL_ASSET_1, 1);
			set_asset_price_in_usd(COLLATERAL_ASSET_2, 1);

			MockBalance::credit_account(&LENDER, LOAN_ASSET, 10 * PRINCIPAL);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				10 * PRINCIPAL
			));

			MockBalance::credit_account(&LP, COLLATERAL_ASSET_1, 10 * INIT_COLLATERAL);
			MockBalance::credit_account(&LP, COLLATERAL_ASSET_2, 10 * INIT_COLLATERAL);

			// Create a loan so we can test adding collateral when updating it
			assert_eq!(
				LendingPools::new_loan(
					LP,
					LOAN_ASSET,
					PRINCIPAL,
					Some(COLLATERAL_ASSET_1),
					BTreeMap::from([(COLLATERAL_ASSET_1, INIT_COLLATERAL)]),
				),
				Ok(LOAN_ID)
			);

			// Adding collateral is disabled for all assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					add_collateral_enabled: BTreeSet::default(),
					..PalletSafeMode::code_green()
				});
				assert_noop!(try_adding_collateral(), Error::<Test>::AddingCollateralDisabled);
				assert_noop!(
					try_adding_collateral_via_new_loan(),
					Error::<Test>::AddingCollateralDisabled
				);
				assert_noop!(
					try_adding_collateral_via_loan_update(),
					Error::<Test>::AddingCollateralDisabled
				);
			}

			// Adding collateral is disabled for at least one of the requested assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					add_collateral_enabled: BTreeSet::from([COLLATERAL_ASSET_1]),
					..PalletSafeMode::code_green()
				});
				assert_noop!(try_adding_collateral(), Error::<Test>::AddingCollateralDisabled);
				assert_noop!(
					try_adding_collateral_via_new_loan(),
					Error::<Test>::AddingCollateralDisabled
				);
				assert_noop!(
					try_adding_collateral_via_loan_update(),
					Error::<Test>::AddingCollateralDisabled
				);
			}

			// Adding collateral is enabled for all requested assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					add_collateral_enabled: BTreeSet::from([
						COLLATERAL_ASSET_1,
						COLLATERAL_ASSET_2,
					]),
					..PalletSafeMode::code_green()
				});
				assert_ok!(try_adding_collateral());
				assert_ok!(try_adding_collateral_via_new_loan());
				assert_ok!(try_adding_collateral_via_loan_update());
			}

			// Adding collateral is enabled for all assets (code green):
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode::code_green());
				assert_ok!(try_adding_collateral());
				assert_ok!(try_adding_collateral_via_new_loan());
				assert_ok!(try_adding_collateral_via_loan_update());
			}
		});
	}

	#[test]
	fn safe_mode_for_removing_collateral() {
		const COLLATERAL_ASSET_1: Asset = Asset::Eth;
		const COLLATERAL_ASSET_2: Asset = Asset::Usdc;
		const COLLATERAL_AMOUNT: AssetAmount = 2 * PRINCIPAL * SWAP_RATE;

		let try_removing_collateral = || {
			LendingPools::remove_collateral(
				RuntimeOrigin::signed(BORROWER),
				Some(COLLATERAL_ASSET_1),
				BTreeMap::from([
					(COLLATERAL_ASSET_1, COLLATERAL_AMOUNT),
					(COLLATERAL_ASSET_2, COLLATERAL_AMOUNT),
				]),
			)
		};

		new_test_ext().execute_with(|| {
			assert_ok!(LendingPools::new_lending_pool(LOAN_ASSET));

			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
			set_asset_price_in_usd(COLLATERAL_ASSET_1, 1);
			set_asset_price_in_usd(COLLATERAL_ASSET_2, 1);

			MockBalance::credit_account(&LENDER, LOAN_ASSET, 10 * PRINCIPAL);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				10 * PRINCIPAL
			));

			MockBalance::credit_account(&LP, COLLATERAL_ASSET_1, 10 * COLLATERAL_AMOUNT);
			MockBalance::credit_account(&LP, COLLATERAL_ASSET_2, 10 * COLLATERAL_AMOUNT);

			// Add collateral so we can test removing it:
			assert_ok!(LendingPools::add_collateral(
				RuntimeOrigin::signed(BORROWER),
				Some(COLLATERAL_ASSET_1),
				BTreeMap::from([
					(COLLATERAL_ASSET_1, 10 * COLLATERAL_AMOUNT),
					(COLLATERAL_ASSET_2, 10 * COLLATERAL_AMOUNT),
				]),
			));

			// Removing collateral is disabled for all assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					remove_collateral_enabled: BTreeSet::default(),
					..PalletSafeMode::code_green()
				});
				assert_noop!(try_removing_collateral(), Error::<Test>::RemovingCollateralDisabled);
			}

			// Removing collateral is disabled for at least one of the requested assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					remove_collateral_enabled: BTreeSet::from([COLLATERAL_ASSET_1]),
					..PalletSafeMode::code_green()
				});
				assert_noop!(try_removing_collateral(), Error::<Test>::RemovingCollateralDisabled);
			}

			// Removing collateral is enabled for all requested assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					remove_collateral_enabled: BTreeSet::from([
						COLLATERAL_ASSET_1,
						COLLATERAL_ASSET_2,
					]),
					..PalletSafeMode::code_green()
				});
				assert_ok!(try_removing_collateral());
			}

			// Removing collateral is enabled for all assets (code green):
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode::code_green());
				assert_ok!(try_removing_collateral());
			}
		});
	}
}

#[test]
fn distribute_proportionally_test() {
	// A single party should get everything:
	assert_eq!(
		distribute_proportionally(100u128, BTreeMap::from([(1, 999)]).iter().map(|(k, v)| (k, *v))),
		BTreeMap::from([(&1, 100)])
	);

	// Distributes proportionally:
	assert_eq!(
		distribute_proportionally(
			1000u128,
			BTreeMap::from([(1, 33), (2, 50), (3, 17)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, 330), (&2, 500), (&3, 170)])
	);

	// Handles rounding errors in a reasonable way:
	assert_eq!(
		distribute_proportionally(
			1000u128,
			BTreeMap::from([(1, 100), (2, 100), (3, 100)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, 333), (&2, 333), (&3, 334)])
	);

	// Some extreme cases:
	assert_eq!(
		distribute_proportionally::<u32, _, _>(
			1000u128,
			BTreeMap::<u32, u128>::from([]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([])
	);

	assert_eq!(
		distribute_proportionally::<u32, _, _>(
			0u128,
			BTreeMap::from([(1, 100)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, 0)])
	);

	assert_eq!(
		distribute_proportionally::<u32, _, _>(
			1000u128,
			BTreeMap::from([(1, 0), (2, 100)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, 0), (&2, 1000)])
	);

	assert_eq!(
		distribute_proportionally::<u32, _, _>(
			u128::MAX,
			BTreeMap::from([(1, 100), (2, 100)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, u128::MAX / 2), (&2, u128::MAX / 2 + 1)])
	);

	assert_eq!(
		distribute_proportionally::<u32, _, _>(
			1000u128,
			BTreeMap::from([(1, u128::MAX), (2, u128::MAX)]).iter().map(|(k, v)| (k, *v))
		),
		BTreeMap::from([(&1, 0), (&2, 1000)])
	);
}

#[test]
fn init_liquidation_swaps_test() {
	// Test that we handle multi-asset collateral + multi-asset loans correctly in case of
	// liquidation: each collateral asset should be split proportionally to the value of every
	// loan (the expected ratio is 1:5 in this case)

	const LOAN_1: LoanId = LoanId(0);
	const LOAN_2: LoanId = LoanId(1);

	const SWAP_1: SwapRequestId = SwapRequestId(0);
	const SWAP_2: SwapRequestId = SwapRequestId(1);
	const SWAP_3: SwapRequestId = SwapRequestId(2);
	const SWAP_4: SwapRequestId = SwapRequestId(3);

	const BORROWER: u64 = 1;

	let mut loan_account = LoanAccount::<Test> {
		borrower_id: BORROWER,
		primary_collateral_asset: Asset::Eth,
		collateral: BTreeMap::from([(Asset::Eth, 500), (Asset::Usdc, 1_000_000)]),
		loans: BTreeMap::from([
			(
				LOAN_1,
				GeneralLoan {
					id: LOAN_ID,
					asset: Asset::Btc,
					created_at_block: 0,
					owed_principal: 20,
					pending_interest: Default::default(),
				},
			),
			(
				LOAN_2,
				GeneralLoan {
					id: LOAN_ID,
					asset: Asset::Sol,
					created_at_block: 0,
					owed_principal: 2000,
					pending_interest: Default::default(),
				},
			),
		]),
		liquidation_status: LiquidationStatus::NoLiquidation,
	};

	new_test_ext().execute_with(|| {
		set_asset_price_in_usd(Asset::Eth, 4_000);
		set_asset_price_in_usd(Asset::Btc, 100_000);
		set_asset_price_in_usd(Asset::Sol, 200);
		set_asset_price_in_usd(Asset::Usdc, 1);

		let collateral = loan_account.prepare_collateral_for_liquidation().unwrap();
		loan_account.init_liquidation_swaps(&BORROWER, collateral, false);

		let expected_swaps = [
			(SWAP_1, LOAN_1, Asset::Eth, Asset::Btc, 417),
			(SWAP_2, LOAN_2, Asset::Eth, Asset::Sol, 83),
			(SWAP_3, LOAN_1, Asset::Usdc, Asset::Btc, 833334),
			(SWAP_4, LOAN_2, Asset::Usdc, Asset::Sol, 166666),
		]
		.into_iter()
		.map(|(swap_req_id, loan_id, input_asset, output_asset, input_amount)| {
			(
				swap_req_id,
				MockSwapRequest {
					input_asset,
					output_asset,
					input_amount,
					remaining_input_amount: input_amount,
					accumulated_output_amount: 0,
					swap_type: SwapRequestType::Regular {
						output_action: SwapOutputAction::CreditLendingPool {
							swap_type: LendingSwapType::Liquidation {
								borrower_id: BORROWER,
								loan_id,
							},
						},
					},
					broker_fees: Default::default(),
					origin: SwapOrigin::Internal,
				},
			)
		})
		.collect();

		assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LiquidationInitiated {
			borrower_id: BORROWER,
			swaps: BTreeMap::from([(LOAN_1, vec![SWAP_1, SWAP_3]), (LOAN_2, vec![SWAP_2, SWAP_4])]),
			is_hard: false,
		}));

		assert_eq!(MockSwapRequestHandler::<Test>::get_swap_requests(), expected_swaps);

		assert_eq!(
			loan_account.liquidation_status,
			LiquidationStatus::Liquidating {
				is_hard: false,
				liquidation_swaps: BTreeMap::from([
					(
						SWAP_1,
						LiquidationSwap {
							loan_id: LOAN_1,
							from_asset: Asset::Eth,
							to_asset: Asset::Btc
						}
					),
					(
						SWAP_2,
						LiquidationSwap {
							loan_id: LOAN_2,
							from_asset: Asset::Eth,
							to_asset: Asset::Sol
						}
					),
					(
						SWAP_3,
						LiquidationSwap {
							loan_id: LOAN_1,
							from_asset: Asset::Usdc,
							to_asset: Asset::Btc
						}
					),
					(
						SWAP_4,
						LiquidationSwap {
							loan_id: LOAN_2,
							from_asset: Asset::Usdc,
							to_asset: Asset::Sol
						}
					),
				])
			}
		)
	});
}

mod rpcs {

	use cf_primitives::AssetAndAmount;
	use rpc::{RpcLiquidationStatus, RpcLiquidationSwap, RpcLoan};

	use super::*;

	#[test]
	fn lending_pools_and_account() {
		// We want the amount to be large enough that we can charge interest immediately
		// (rather than waiting for fractional amounts to accumulate).
		const PRINCIPAL: AssetAmount = 2_000_000_000_000;
		const INIT_POOL_AMOUNT: AssetAmount = PRINCIPAL * 2;

		const COLLATERAL_ASSET: Asset = Asset::Eth;
		const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV

		let origination_fee = CONFIG.origination_fee(LOAN_ASSET) * PRINCIPAL * SWAP_RATE;

		const LOAN_ASSET_2: Asset = Asset::Usdc;
		const PRINCIPAL_2: AssetAmount = PRINCIPAL * 2;
		const COLLATERAL_ASSET_2: Asset = Asset::Sol;
		const INIT_COLLATERAL_2: AssetAmount = INIT_COLLATERAL * 2;

		/// This much of borrower 2's collateral will be executed during liquidation
		/// at the time of calling RPC.
		const EXECUTED_COLLATERAL_2: AssetAmount = INIT_COLLATERAL_2 / 4;

		const BORROWER_2: u64 = OTHER_LP;
		const LOAN_ID_2: LoanId = LoanId(1);

		let origination_fee_2 = CONFIG.origination_fee(LOAN_ASSET) * PRINCIPAL_2 * SWAP_RATE;

		/// Price of COLLATERAL_ASSET_2 will be increased to this much to trigger liquidation
		/// of borrower 2's collateral.
		const NEW_SWAP_RATE: u128 = 5 * SWAP_RATE / 4;

		new_test_ext()
			.execute_with(|| {
				setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);
				setup_chp_pool_with_funds(LOAN_ASSET_2, INIT_POOL_AMOUNT * 2);

				set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
				set_asset_price_in_usd(COLLATERAL_ASSET, 1);

				set_asset_price_in_usd(LOAN_ASSET_2, SWAP_RATE);
				set_asset_price_in_usd(COLLATERAL_ASSET_2, 1);

				MockBalance::credit_account(
					&BORROWER,
					COLLATERAL_ASSET,
					INIT_COLLATERAL + origination_fee,
				);

				MockBalance::credit_account(
					&BORROWER_2,
					COLLATERAL_ASSET_2,
					INIT_COLLATERAL_2 + origination_fee_2,
				);

				assert_eq!(
					LendingPools::new_loan(
						BORROWER,
						LOAN_ASSET,
						PRINCIPAL,
						Some(COLLATERAL_ASSET),
						BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
					),
					Ok(LOAN_ID)
				);

				assert_eq!(
					LendingPools::new_loan(
						BORROWER_2,
						LOAN_ASSET_2,
						PRINCIPAL_2,
						Some(COLLATERAL_ASSET_2),
						BTreeMap::from([(COLLATERAL_ASSET_2, INIT_COLLATERAL_2)])
					),
					Ok(LOAN_ID_2)
				);

				// Should get info only for the specified account:
				assert_eq!(
					super::rpc::get_loan_accounts::<Test>(Some(BORROWER)),
					vec![RpcLoanAccount {
						account: BORROWER,
						primary_collateral_asset: COLLATERAL_ASSET,
						ltv_ratio: Some(FixedU64::from_rational(8, 10)),
						collateral: vec![AssetAndAmount {
							asset: COLLATERAL_ASSET,
							amount: INIT_COLLATERAL
						}],
						loans: vec![RpcLoan {
							loan_id: LOAN_ID,
							asset: LOAN_ASSET,
							created_at: INIT_BLOCK as u32,
							principal_amount: PRINCIPAL,
						}],
						liquidation_status: None
					}]
				);

				// Trigger liquidation of one of the accounts (BORROWER_2):
				set_asset_price_in_usd(LOAN_ASSET_2, NEW_SWAP_RATE);
			})
			.then_process_blocks_until_block(
				INIT_BLOCK + CONFIG.interest_payment_interval_blocks as u64,
			)
			.then_execute_with(|_| {
				// Liquidation swap's execution price will be slightly worse than the oracle price:
				const ACCUMULATED_OUTPUT_AMOUNT: AssetAmount =
					98 * (EXECUTED_COLLATERAL_2 / NEW_SWAP_RATE) / 100;

				// Simulate partial execution of the liquidation swap:
				MockSwapRequestHandler::<Test>::set_swap_request_progress(
					SwapRequestId(0),
					SwapExecutionProgress {
						remaining_input_amount: INIT_COLLATERAL_2 - EXECUTED_COLLATERAL_2,
						accumulated_output_amount: ACCUMULATED_OUTPUT_AMOUNT,
					},
				);

				// Interest amount happens to be this much, the exact amount is not important
				// in this particular test:
				const INTEREST_AMOUNT: AssetAmount = 4816540;

				// Both accounts should be returned since we don't specify any:
				assert_eq!(
					super::rpc::get_loan_accounts::<Test>(None),
					vec![
						RpcLoanAccount {
							account: BORROWER_2,
							primary_collateral_asset: COLLATERAL_ASSET_2,
							ltv_ratio: Some(FixedU64::from_rational(1_006_666_667, 1_000_000_000)),
							// NOTE: all of collateral is in liquidation swaps, but we include
							// any amount that has not been swapped yet:
							collateral: vec![AssetAndAmount {
								asset: COLLATERAL_ASSET_2,
								amount: INIT_COLLATERAL_2 - EXECUTED_COLLATERAL_2,
							}],
							loans: vec![RpcLoan {
								loan_id: LOAN_ID_2,
								asset: LOAN_ASSET_2,
								created_at: INIT_BLOCK as u32,
								// NOTE: we account for the principal asset already swapped in
								// liquidation swaps:
								principal_amount: PRINCIPAL_2 - ACCUMULATED_OUTPUT_AMOUNT,
							}],
							liquidation_status: Some(RpcLiquidationStatus {
								liquidation_swaps: vec![RpcLiquidationSwap {
									swap_request_id: SwapRequestId(0),
									loan_id: LOAN_ID_2,
								}],
								is_hard: true
							})
						},
						RpcLoanAccount {
							account: BORROWER,
							primary_collateral_asset: COLLATERAL_ASSET,
							// LTV slightly increased due to interest payment:
							ltv_ratio: Some(FixedU64::from_rational(800_000_077, 1_000_000_000)),
							collateral: vec![AssetAndAmount {
								asset: COLLATERAL_ASSET,
								amount: INIT_COLLATERAL - INTEREST_AMOUNT
							}],
							loans: vec![RpcLoan {
								loan_id: LOAN_ID,
								asset: LOAN_ASSET,
								created_at: INIT_BLOCK as u32,
								principal_amount: PRINCIPAL,
							}],
							liquidation_status: None
						},
					]
				);

				assert_eq!(
					super::rpc::get_lending_pools::<Test>(Some(LOAN_ASSET)),
					vec![RpcLendingPool {
						asset: LOAN_ASSET,
						total_amount: INIT_POOL_AMOUNT,
						available_amount: INIT_POOL_AMOUNT - PRINCIPAL,
						utilisation_rate: Permill::from_percent(50),
						current_interest_rate: Permill::from_parts(53_333), // 5.33%
						config: CONFIG.get_config_for_asset(LOAN_ASSET).clone(),
					}]
				)
			});
	}
}

#[test]
fn linear_segment_interpolation() {
	// Linear segment starts at 0% and ends at 90%
	assert_eq!(
		interpolate_linear_segment(
			Permill::from_percent(2),
			Permill::from_percent(8),
			Permill::from_percent(0),
			Permill::from_percent(90),
			Permill::from_percent(45),
		),
		Permill::from_parts(49_999) // ~5%
	);

	// Linear segment starts at 90% and ends at 100%
	assert_eq!(
		interpolate_linear_segment(
			Permill::from_percent(8),
			Permill::from_percent(50),
			Permill::from_percent(90),
			Permill::from_percent(100),
			Permill::from_percent(95),
		),
		Permill::from_percent(29)
	);

	// Linear segment from 0% to 100% and zero slope
	assert_eq!(
		interpolate_linear_segment(
			Permill::from_percent(5),
			Permill::from_percent(5),
			Permill::from_percent(0),
			Permill::from_percent(100),
			Permill::from_percent(75),
		),
		Permill::from_percent(5)
	);

	// === Some linear segments with a negative slope ===
	assert_eq!(
		interpolate_linear_segment(
			Permill::from_percent(50),
			Permill::from_percent(10),
			Permill::from_percent(0),
			Permill::from_percent(50),
			Permill::from_percent(25),
		),
		Permill::from_percent(30)
	);

	assert_eq!(
		interpolate_linear_segment(
			Permill::from_percent(50),
			Permill::from_percent(10),
			Permill::from_percent(0),
			Permill::from_percent(50),
			Permill::from_percent(0),
		),
		Permill::from_percent(50)
	);

	assert_eq!(
		interpolate_linear_segment(
			Permill::from_percent(50),
			Permill::from_percent(10),
			Permill::from_percent(0),
			Permill::from_percent(50),
			Permill::from_percent(50),
		),
		Permill::from_percent(10)
	);
}

#[test]
fn interest_rate_curve() {
	// The exact asset is not important for this test
	let asset = Asset::Btc;

	assert_eq!(
		LENDING_DEFAULT_CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(0)),
		Permill::from_percent(2)
	);

	assert_eq!(
		LENDING_DEFAULT_CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(45)),
		Permill::from_parts(49_999) // (2% + 8%) / 2 = 5%
	);

	assert_eq!(
		LENDING_DEFAULT_CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(90)),
		Permill::from_percent(8)
	);

	assert_eq!(
		LENDING_DEFAULT_CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(95)),
		Permill::from_percent(29) // (8% + 50%) / 2 = 29%
	);

	assert_eq!(
		LENDING_DEFAULT_CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(100)),
		Permill::from_percent(50)
	);
}

#[test]
fn derive_extra_interest_from_low_ltv() {
	assert_eq!(
		LENDING_DEFAULT_CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::zero()),
		Permill::from_percent(1)
	);

	assert_eq!(
		LENDING_DEFAULT_CONFIG
			.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(10, 100)),
		Permill::from_parts(8_000) // 0.8%
	);

	assert_eq!(
		LENDING_DEFAULT_CONFIG
			.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(25, 100)),
		Permill::from_parts(5_000) // 0.5%
	);

	// Any value above 50% LTV should result in 0% additional interest:
	assert_eq!(
		LENDING_DEFAULT_CONFIG
			.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(50, 100)),
		Permill::from_percent(0)
	);

	assert_eq!(
		LENDING_DEFAULT_CONFIG
			.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(80, 100)),
		Permill::from_percent(0)
	);

	assert_eq!(
		LENDING_DEFAULT_CONFIG
			.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(120, 100)),
		Permill::from_percent(0)
	);
}
