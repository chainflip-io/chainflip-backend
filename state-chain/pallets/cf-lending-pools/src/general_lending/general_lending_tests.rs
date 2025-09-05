use crate::mocks::*;
use cf_amm_math::PRICE_FRACTIONAL_BITS;
use cf_chains::evm::U256;
use cf_test_utilities::assert_matching_event_count;
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

	System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
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

		System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::LendingFundsRemoved {
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

		System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::LendingFundsRemoved {
			lender_id: LENDER,
			asset: LOAN_ASSET,
			unlocked_amount: 3 * INIT_POOL_AMOUNT / 4,
		}));
	});
}

#[test]
fn basic_chp_lending() {
	const COLLATERAL_ASSET: Asset = Asset::Eth;

	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV

	let origination_fee = CONFIG.origination_fee * PRINCIPAL * SWAP_RATE;

	let interest_charge_in_eth_1 = {
		// 25% utilisation is expected:
		let interest_charge_in_btc =
			CONFIG.derive_interest_rate_per_charge_interval(Permill::from_percent(50)) * PRINCIPAL;
		interest_charge_in_btc * SWAP_RATE
	};

	let interest_charge_in_eth_2 = {
		// 25% utilisation is expected:
		let interest_charge_in_btc = CONFIG
			.derive_interest_rate_per_charge_interval(Permill::from_percent(25)) *
			PRINCIPAL / 2;
		interest_charge_in_btc * SWAP_RATE
	};

	let total_interest = interest_charge_in_eth_1 + interest_charge_in_eth_2;

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
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
				),
				Ok(LOAN_ID)
			);

			System::assert_last_event(RuntimeEvent::LendingPools(Event::<Test>::LoanCreated {
				loan_id: LOAN_ID,
				borrower_id: BORROWER,
				asset: LOAN_ASSET,
				principal_amount: PRINCIPAL,
				origination_fee,
			}));

			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT,
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					collected_fees: BTreeMap::from([(COLLATERAL_ASSET, origination_fee)]),
				}
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					primary_collateral_asset: COLLATERAL_ASSET,
					collateral: BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL
						}
					)])
				})
			);
		})
		.then_process_blocks_until_block(INIT_BLOCK + INTEREST_PAYMENT_INTERVAL as u64)
		// Checking that interest was charged here:
		.then_execute_with(|_| {
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().collateral,
				BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL - interest_charge_in_eth_1)])
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap().collected_fees,
				BTreeMap::from([(COLLATERAL_ASSET, origination_fee + interest_charge_in_eth_1)])
			);
		})
		// === REPAYING HALF OF THE LOAN ===
		.then_execute_with(|_| {
			assert_ok!(LendingPools::try_making_repayment(&BORROWER, LOAN_ID, PRINCIPAL / 2));
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL / 2);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER)
					.unwrap()
					.loans
					.get(&LOAN_ID)
					.unwrap()
					.owed_principal,
				PRINCIPAL / 2
			);
			// Funds have been returned to the pool:
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT,
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL / 2,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					collected_fees: BTreeMap::from([(
						COLLATERAL_ASSET,
						origination_fee + interest_charge_in_eth_1
					)]),
				}
			);
		})
		.then_process_blocks_until_block(INIT_BLOCK + 2 * INTEREST_PAYMENT_INTERVAL as u64)
		// === Interest is charged the second time ===
		.then_execute_with(|_| {
			// This time we expect a smaller amount due to the partial repayment (which both
			// the principal and the pool's utilisation):
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().collateral,
				BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL - total_interest)])
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap().collected_fees,
				BTreeMap::from([(COLLATERAL_ASSET, origination_fee + total_interest)])
			);
		})
		.then_execute_with(|_| {
			// Repaying the remainder of the borrowed amount should finalise the loan:
			assert_ok!(LendingPools::try_making_repayment(&BORROWER, LOAN_ID, PRINCIPAL / 2));
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), 0);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					primary_collateral_asset: COLLATERAL_ASSET,
					liquidation_status: LiquidationStatus::NoLiquidation,
					// Note that we don't automatically release the collateral:
					collateral: BTreeMap::from([(
						COLLATERAL_ASSET,
						INIT_COLLATERAL - total_interest
					)]),
					loans: Default::default()
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

	let origination_fee = CONFIG.origination_fee * PRINCIPAL * SWAP_RATE;

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

	let origination_fee = CONFIG.origination_fee * PRINCIPAL * SWAP_RATE;

	let origination_fee_2 = CONFIG.origination_fee * EXTRA_PRINCIPAL_1 * SWAP_RATE;

	// NOTE: expecting utilisation to go up as we keep borrowing more
	let origination_fee_3 = CONFIG.origination_fee * EXTRA_PRINCIPAL_2 * SWAP_RATE;

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

		// Should have enough collateral to borrow a litte more on the same loan
		{
			assert_ok!(LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				EXTRA_PRINCIPAL_1,
				Default::default()
			));

			System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::LoanUpdated {
				loan_id: LOAN_ID,
				extra_principal_amount: EXTRA_PRINCIPAL_1,
				origination_fee: origination_fee_2,
			}));

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT,
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL - EXTRA_PRINCIPAL_1,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					collected_fees: BTreeMap::from([(
						COLLATERAL_ASSET,
						origination_fee + origination_fee_2
					)]),
				}
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap(),
				LoanAccount {
					primary_collateral_asset: COLLATERAL_ASSET,
					collateral: BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL + EXTRA_PRINCIPAL_1
						}
					)]),
					liquidation_status: LiquidationStatus::NoLiquidation
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
					// Pool has accrued extra fees:
					collected_fees: BTreeMap::from([(
						COLLATERAL_ASSET,
						origination_fee + origination_fee_2 + origination_fee_3
					)]),
				}
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap(),
				LoanAccount {
					primary_collateral_asset: COLLATERAL_ASSET,
					// Loan's collateral has been increased:
					collateral: BTreeMap::from([(
						COLLATERAL_ASSET,
						INIT_COLLATERAL + EXTRA_COLLATERAL
					)]),
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
							// Loan's owed principal has been increased:
							owed_principal: PRINCIPAL + EXTRA_PRINCIPAL_1 + EXTRA_PRINCIPAL_2
						}
					)]),
					liquidation_status: LiquidationStatus::NoLiquidation
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
	const PRIMARY_COLLATERAL_ASSET: Asset = Asset::Eth;
	const SECONDARY_COLLATERAL_ASSET: Asset = Asset::Usdc;

	const TOTAL_COLLATERAL_REQUIRED: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV

	// A small but non-zero amount is in the primary asset:
	const INIT_COLLATERAL_AMOUNT_PRIMARY: AssetAmount = 100;
	// The remaining amount is in the secondary asset:
	const INIT_COLLATERAL_AMOUNT_SECONDARY: AssetAmount =
		TOTAL_COLLATERAL_REQUIRED - INIT_COLLATERAL_AMOUNT_PRIMARY;

	let interest_charge = {
		let interest_charge_in_loan_asset =
			CONFIG.derive_interest_rate_per_charge_interval(Permill::from_percent(50)) * PRINCIPAL;
		interest_charge_in_loan_asset * SWAP_RATE
	};

	dbg!(interest_charge);

	let origination_fee = CONFIG.origination_fee * PRINCIPAL * SWAP_RATE;

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
		.then_execute_at_block(INIT_BLOCK + INTEREST_PAYMENT_INTERVAL as u64, |_| {
			let secondary_interest_charge =
				interest_charge.saturating_sub(INIT_COLLATERAL_AMOUNT_PRIMARY);

			assert!(secondary_interest_charge > 0);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap().collected_fees,
				BTreeMap::from([
					// All of the primary asset is consumed as interest:
					(PRIMARY_COLLATERAL_ASSET, INIT_COLLATERAL_AMOUNT_PRIMARY + origination_fee),
					// The remainder is charged from the secondary asset:
					(SECONDARY_COLLATERAL_ASSET, secondary_interest_charge)
				])
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().collateral,
				BTreeMap::from([
					(PRIMARY_COLLATERAL_ASSET, 0),
					(
						SECONDARY_COLLATERAL_ASSET,
						INIT_COLLATERAL_AMOUNT_SECONDARY - secondary_interest_charge
					)
				]),
			);
		})
		.then_execute_at_block(INIT_BLOCK + 2 * INTEREST_PAYMENT_INTERVAL as u64, |_| {
			// The second time the fee is collected, it comes entirely from the secondary asset:
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap().collected_fees,
				BTreeMap::from([
					(PRIMARY_COLLATERAL_ASSET, INIT_COLLATERAL_AMOUNT_PRIMARY + origination_fee),
					(
						SECONDARY_COLLATERAL_ASSET,
						2 * interest_charge - INIT_COLLATERAL_AMOUNT_PRIMARY
					)
				])
			);
		});
}

#[test]
fn swap_collected_fees() {
	const COLLATERAL_ASSET_1: Asset = Asset::Usdc;
	const COLLATERAL_ASSET_2: Asset = Asset::Eth;

	const INIT_FEE_ASSET_1: AssetAmount = 100;
	const INIT_FEE_ASSET_2: AssetAmount = 1;

	const FEE_SWAP_BLOCK: u64 = CONFIG.fee_swap_interval_blocks as u64;

	const FEE_SWAP_ID: SwapRequestId = SwapRequestId(0);

	new_test_ext()
		.execute_with(|| {
			setup_chp_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

			set_asset_price_in_usd(COLLATERAL_ASSET_1, 1_000_000);
			set_asset_price_in_usd(COLLATERAL_ASSET_2, 5_000_000);
			set_asset_price_in_usd(LOAN_ASSET, 10_000_000);

			LendingPools::accrue_fees(LOAN_ASSET, COLLATERAL_ASSET_1, INIT_FEE_ASSET_1);
			LendingPools::accrue_fees(LOAN_ASSET, COLLATERAL_ASSET_2, INIT_FEE_ASSET_2);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap().collected_fees,
				BTreeMap::from([
					(COLLATERAL_ASSET_1, INIT_FEE_ASSET_1),
					(COLLATERAL_ASSET_2, INIT_FEE_ASSET_2)
				]),
			);
		})
		.then_execute_at_next_block(|_| {
			assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());
		})
		.then_execute_at_block(FEE_SWAP_BLOCK, |_| {
			// Expecting a fee swap from asset 1 but not asset 2:
			assert_eq!(
				MockSwapRequestHandler::<Test>::get_swap_requests(),
				BTreeMap::from([(
					FEE_SWAP_ID,
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
				)])
			);

			System::assert_has_event(RuntimeEvent::LendingPools(
				Event::<Test>::LendingFeeCollectionInitiated {
					asset: LOAN_ASSET,
					swap_request_id: FEE_SWAP_ID,
				},
			));

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap().collected_fees,
				BTreeMap::from([(COLLATERAL_ASSET_1, 0), (COLLATERAL_ASSET_2, INIT_FEE_ASSET_2)]),
			);
		})
		.then_execute_at_block(FEE_SWAP_BLOCK, |_| {
			const FEE_SWAP_OUTPUT_1: AssetAmount = INIT_FEE_ASSET_1 / 10;

			// Simulate fee swap:
			LendingPools::process_loan_swap_outcome(
				FEE_SWAP_ID,
				LendingSwapType::FeeSwap { pool_asset: LOAN_ASSET },
				FEE_SWAP_OUTPUT_1,
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + FEE_SWAP_OUTPUT_1,
					available_amount: INIT_POOL_AMOUNT + FEE_SWAP_OUTPUT_1,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					collected_fees: BTreeMap::from([
						(COLLATERAL_ASSET_1, 0),
						(COLLATERAL_ASSET_2, INIT_FEE_ASSET_2)
					]),
				}
			);
		});
}

#[test]
fn adding_and_removing_collateral() {
	const COLLATERAL_ASSET: Asset = Asset::Eth;
	const COLLATERAL_ASSET_2: Asset = Asset::Btc;

	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV
	const INIT_COLLATERAL_AMOUNT_2: AssetAmount = 1000;

	let origination_fee = CONFIG.origination_fee * PRINCIPAL * SWAP_RATE;

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

		System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
			borrower_id: BORROWER,
			collateral: collateral.clone(),
			primary_collateral_asset: COLLATERAL_ASSET,
		}));

		// Adding collateral creates a loan account:
		assert_eq!(
			LoanAccounts::<Test>::get(BORROWER).unwrap(),
			LoanAccount {
				primary_collateral_asset: COLLATERAL_ASSET,
				collateral: collateral.clone(),
				loans: BTreeMap::default(),
				liquidation_status: LiquidationStatus::NoLiquidation
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

		System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::CollateralRemoved {
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
	// swap is executed half way through, which should be sufficient to bring CR back to healty and
	// abort the liquidation. Then we drop the price again to trigger hard liquidation, which will
	// be fully executed repaying the principal in full and closing the loan.

	const COLLATERAL_ASSET: Asset = Asset::Eth;

	// This should trigger soft liquidation
	const NEW_SWAP_RATE: u128 = 23;

	// This should trigger second (hard) liquidation
	const SWAP_RATE_2: u128 = 26;

	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV
	let origination_fee = CONFIG.origination_fee * PRINCIPAL * SWAP_RATE;

	// How much collateral will be swapped during liquidation:
	const EXECUTED_COLLATERAL: AssetAmount = 2 * INIT_COLLATERAL / 5;
	// How much of principal asset is bought during first liquidation:
	const SWAPPED_PRINCIPAL: AssetAmount = EXECUTED_COLLATERAL / NEW_SWAP_RATE;
	// How much of principal asset is bought during second liquidation:
	const SWAPPED_PRINCIPAL_2: AssetAmount = (INIT_COLLATERAL - EXECUTED_COLLATERAL) / SWAP_RATE_2;

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
			let (liquidation_swap_id, liquidation_swap) =
				MockSwapRequestHandler::<Test>::get_swap_requests()
					.into_iter()
					.next()
					.expect("No swap request found");

			assert_eq!(liquidation_swap_id, LIQUIDATION_SWAP_1);

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

			System::assert_has_event(RuntimeEvent::LendingPools(
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
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL - SWAPPED_PRINCIPAL
						}
					)])
				})
			);

			// Liquidation Swap must have been aborted:
			assert!(MockSwapRequestHandler::<Test>::get_swap_requests().is_empty());

			// Part of the principal has been repaid via liquidation:
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT,
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL + SWAPPED_PRINCIPAL,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					collected_fees: BTreeMap::from([(COLLATERAL_ASSET, origination_fee)]),
				}
			);

			// Drop oracle price again to trigger liquidation:
			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE_2);
		})
		.then_execute_at_next_block(|_| {
			// Expecting a liquidation swap here:
			let (_, liquidation_swap) = MockSwapRequestHandler::<Test>::get_swap_requests()
				.into_iter()
				.next()
				.expect("No swap request found");

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

			System::assert_has_event(RuntimeEvent::LendingPools(
				Event::<Test>::LiquidationInitiated {
					borrower_id: BORROWER,
					swaps: BTreeMap::from([(LOAN_ID, vec![LIQUIDATION_SWAP_2])]),
					is_hard: true,
				},
			));
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

			LendingPools::process_loan_swap_outcome(
				LIQUIDATION_SWAP_2,
				LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
				SWAPPED_PRINCIPAL_2,
			);

			// The should now be settled:
			System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
				loan_id: LOAN_ID,
				total_fees: Default::default(),
			}));

			// This remaining principal will be credited to the borrower's account
			const REMAINING_PRINCIPAL: u128 = SWAPPED_PRINCIPAL_2 - (PRINCIPAL - SWAPPED_PRINCIPAL);

			assert_eq!(
				MockBalance::get_balance(&BORROWER, LOAN_ASSET),
				PRINCIPAL + REMAINING_PRINCIPAL
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT,
					available_amount: INIT_POOL_AMOUNT,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					// TODO: liquidation fee should be charged as well!
					collected_fees: BTreeMap::from([(COLLATERAL_ASSET, origination_fee)]),
				}
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					primary_collateral_asset: COLLATERAL_ASSET,
					collateral: Default::default(),
					liquidation_status: LiquidationStatus::NoLiquidation,
					loans: Default::default()
				})
			);
		});
}

#[test]
fn making_loan_repayment() {
	const COLLATERAL_ASSET: Asset = Asset::Eth;
	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV

	let origination_fee = CONFIG.origination_fee * PRINCIPAL * SWAP_RATE;

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

		System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
			loan_id: LOAN_ID,
			amount: FIRST_REPAYMENT,
			liquidation_fees: Default::default(),
		}));

		// Should not see this event yet:
		assert_matching_event_count!(
			Test,
			RuntimeEvent::LendingPools(Event::<Test>::LoanSettled { .. }) => 0
		);

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

		System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
			loan_id: LOAN_ID,
			amount: PRINCIPAL - FIRST_REPAYMENT,
			liquidation_fees: Default::default(),
		}));

		System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
			loan_id: LOAN_ID,
			total_fees: Default::default(),
		}));
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
		distribute_proportionally(100u128, BTreeMap::from([(1, 999)])),
		BTreeMap::from([(1, 100)])
	);

	// Distributes proportionally:
	assert_eq!(
		distribute_proportionally(1000u128, BTreeMap::from([(1, 33), (2, 50), (3, 17)])),
		BTreeMap::from([(1, 330), (2, 500), (3, 170)])
	);

	// Handles rounding errors in a reasonable way:
	assert_eq!(
		distribute_proportionally(1000u128, BTreeMap::from([(1, 100), (2, 100), (3, 100)])),
		BTreeMap::from([(1, 333), (2, 333), (3, 334)])
	);

	// Some extreme cases:
	assert_eq!(
		distribute_proportionally::<u32, _>(1000u128, BTreeMap::from([])),
		BTreeMap::from([])
	);

	assert_eq!(
		distribute_proportionally::<u32, _>(0u128, BTreeMap::from([(1, 100)])),
		BTreeMap::from([(1, 0)])
	);

	assert_eq!(
		distribute_proportionally::<u32, _>(1000u128, BTreeMap::from([(1, 0), (2, 100)])),
		BTreeMap::from([(1, 0), (2, 1000)])
	);

	assert_eq!(
		distribute_proportionally::<u32, _>(u128::MAX, BTreeMap::from([(1, 100), (2, 100)])),
		BTreeMap::from([(1, u128::MAX / 2), (2, u128::MAX / 2 + 1)])
	);

	assert_eq!(
		distribute_proportionally::<u32, _>(
			1000u128,
			BTreeMap::from([(1, u128::MAX), (2, u128::MAX)])
		),
		BTreeMap::from([(1, 0), (2, 1000)])
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
		primary_collateral_asset: Asset::Eth,
		collateral: BTreeMap::from([(Asset::Eth, 500), (Asset::Usdc, 1_000_000)]),
		loans: BTreeMap::from([
			(LOAN_1, GeneralLoan { asset: Asset::Btc, created_at_block: 0, owed_principal: 20 }),
			(LOAN_2, GeneralLoan { asset: Asset::Sol, created_at_block: 0, owed_principal: 2000 }),
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

		System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::LiquidationInitiated {
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
		const COLLATERAL_ASSET: Asset = Asset::Eth;
		const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV

		let origination_fee = CONFIG.origination_fee * PRINCIPAL * SWAP_RATE;

		const LOAN_ASSET_2: Asset = Asset::Usdc;
		const PRINCIPAL_2: AssetAmount = PRINCIPAL * 2;
		const COLLATERAL_ASSET_2: Asset = Asset::Sol;
		const INIT_COLLATERAL_2: AssetAmount = INIT_COLLATERAL * 2;
		const BORROWER_2: u64 = OTHER_LP;
		const LOAN_ID_2: LoanId = LoanId(1);

		let origination_fee_2 = CONFIG.origination_fee * PRINCIPAL_2 * SWAP_RATE;

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
							total_fees: Default::default()
						}],
						liquidation_status: None
					}]
				);

				// Trigger liquidation of one of the accounts:
				set_asset_price_in_usd(LOAN_ASSET_2, 5 * SWAP_RATE / 4);
			})
			.then_process_blocks_until_block(INIT_BLOCK + INTEREST_PAYMENT_INTERVAL as u64)
			.then_execute_with(|_| {
				// Interest amount happens to be this much, the exact amount is not important
				// in this particular test:
				const INTEREST_AMOUNT: AssetAmount = 2660;

				// Both accounts should be returned since we don't specify any:
				assert_eq!(
					super::rpc::get_loan_accounts::<Test>(None),
					vec![
						RpcLoanAccount {
							account: BORROWER_2,
							primary_collateral_asset: COLLATERAL_ASSET_2,
							ltv_ratio: Some(FixedU64::from_rational(1, 1)),
							// NOTE: all of the collateral is in liquidation swaps. Should we
							// include that here too? If so, do we need to include the amount
							// of loan asset recovered so far through liquidation swaps?
							collateral: Default::default(),
							loans: vec![RpcLoan {
								loan_id: LOAN_ID_2,
								asset: LOAN_ASSET_2,
								created_at: INIT_BLOCK as u32,
								principal_amount: PRINCIPAL_2,
								total_fees: Default::default()
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
							ltv_ratio: Some(FixedU64::from_rational(800_000_085, 1_000_000_000)),
							collateral: vec![AssetAndAmount {
								asset: COLLATERAL_ASSET,
								amount: INIT_COLLATERAL - INTEREST_AMOUNT
							}],
							loans: vec![RpcLoan {
								loan_id: LOAN_ID,
								asset: LOAN_ASSET,
								created_at: INIT_BLOCK as u32,
								principal_amount: PRINCIPAL,
								total_fees: Default::default()
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
						utilisation_rate: 5000, // 50%
						interest_rate: 700      // 7%
					}]
				)
			});
	}
}
