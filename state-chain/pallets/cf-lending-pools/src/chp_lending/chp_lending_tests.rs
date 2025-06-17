use crate::mocks::*;
use cf_amm_math::PRICE_FRACTIONAL_BITS;
use cf_chains::evm::U256;
use cf_traits::{
	lending::ChpSystemApi,
	mocks::{
		balance_api::MockBalance,
		swap_request_api::{MockSwapRequest, MockSwapRequestHandler},
	},
};
use frame_support::assert_ok;

const INIT_BLOCK: u64 = 1;
const MAX_LOAN_DURATION: u32 = 30;

const CLEARING_FEE_BASE: Permill = Permill::from_parts(100);
const CLEARING_FEE_UTILISATION_FACTOR: Permill = Permill::from_parts(100);

const INTEREST_BASE: Permill = Permill::from_parts(10);
const INTEREST_UTILISATION_FACTOR: Permill = Permill::from_parts(10);

const LENDER: u64 = BOOSTER_1;

const CORE_POOL_ID: CorePoolId = CorePoolId(0);

const ASSET: Asset = Asset::Btc;
const PRINCIPAL: AssetAmount = 1_000_000;
// This much is required to create the loan
const INIT_COLLATERAL: AssetAmount = (PRINCIPAL + PRINCIPAL / 5) * SWAP_RATE; // 20% overcollateralisation

const EXPECTED_INTEREST: Permill = Permill::from_parts(15); // 50% utilisation expected
const EXPECTED_CLEARING_FEE: Permill = Permill::from_parts(150); // 50% utilisation expected

const LOAN_ID: ChpLoanId = ChpLoanId(0);

// ASSET's price in USDC
const SWAP_RATE: u128 = 2;

const INIT_POOL_AMOUNT: AssetAmount = PRINCIPAL * 2;

use cf_traits::mocks::MockOraclePriceApi;

use super::*;

fn setup_chp_pool_with_funds() {
	ChpConfig::<Test>::set(ChpConfiguration {
		clearing_fee_base: CLEARING_FEE_BASE,
		clearing_fee_utilisation_factor: CLEARING_FEE_UTILISATION_FACTOR,
		interest_base: INTEREST_BASE,
		interest_utilisation_factor: INTEREST_UTILISATION_FACTOR,
		overcollateralisation_target: Permill::from_percent(20),
		overcollateralisation_topup_threshold: Permill::from_percent(15),
		overcollateralisation_soft_threshold: Permill::from_percent(10),
		overcollateralisation_hard_threshold: Permill::from_percent(5),
		max_loan_duration: MAX_LOAN_DURATION,
	});

	assert_ok!(LendingPools::new_chp_pool(ASSET));

	let price = U256::from(SWAP_RATE) << PRICE_FRACTIONAL_BITS;

	MockOraclePriceApi::set_price(ASSET, price);

	System::assert_last_event(RuntimeEvent::LendingPools(Event::<Test>::ChpPoolCreated {
		asset: ASSET,
	}));

	// Depositing double the loan amount to make utilisation after loan 50%:
	MockBalance::credit_account(&LENDER, ASSET, INIT_POOL_AMOUNT);
	assert_ok!(LendingPools::add_chp_funds(RuntimeOrigin::signed(LENDER), ASSET, INIT_POOL_AMOUNT));

	System::assert_has_event(RuntimeEvent::LendingPools(Event::<Test>::ChpFundsAdded {
		lender_id: LENDER,
		asset: ASSET,
		amount: INIT_POOL_AMOUNT,
	}));
}

#[test]
fn basic_chp_lending() {
	let clearing_fee_btc = EXPECTED_CLEARING_FEE * PRINCIPAL;
	let clearing_fee_usdc = clearing_fee_btc * SWAP_RATE;

	let init_lp_balance_usdc = INIT_COLLATERAL + clearing_fee_usdc; // just enough to create the loan

	let interest_charge_btc_1 = EXPECTED_INTEREST * PRINCIPAL;
	let interest_charge_usdc_1 = interest_charge_btc_1 * SWAP_RATE;

	let interest_charge_btc_2 = EXPECTED_INTEREST * PRINCIPAL / 2;
	let interest_charge_usdc_2 = interest_charge_btc_2 * SWAP_RATE;

	let total_fees_btc = clearing_fee_btc + interest_charge_btc_1 + interest_charge_btc_2;
	let total_fees_usdc = total_fees_btc * SWAP_RATE;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			setup_chp_pool_with_funds();

			MockBalance::credit_account(&LP, COLLATERAL_ASSET, init_lp_balance_usdc);
			assert_eq!(LendingPools::new_chp_loan(LP, ASSET, PRINCIPAL), Ok(LOAN_ID));

			System::assert_last_event(RuntimeEvent::LendingPools(Event::<Test>::ChpLoanCreated {
				loan_id: LOAN_ID,
				borrower_id: LP,
				asset: ASSET,
				amount: PRINCIPAL,
			}));

			assert_eq!(MockBalance::get_balance(&LP, COLLATERAL_ASSET), 0);
			assert_eq!(MockBalance::get_balance(&LP, ASSET), PRINCIPAL);

			assert_eq!(
				ChpLoans::<Test>::get(ASSET, LOAN_ID).unwrap(),
				ChpLoan {
					loan_id: LOAN_ID,
					asset: ASSET,
					created_at_block: INIT_BLOCK,
					expiry_block: INIT_BLOCK + u64::from(MAX_LOAN_DURATION),
					status: LoanStatus::Active,
					borrower_id: LP,
					usdc_collateral: INIT_COLLATERAL,
					fees_collected_usdc: clearing_fee_usdc,
					pool_contributions: vec![ChpPoolContribution {
						core_pool_id: CORE_POOL_ID,
						loan_id: LoanId(0),
						principal: PRINCIPAL,
					}],
					interest_rate: EXPECTED_INTEREST
				}
			);
		})
		.then_process_blocks_until_block(INIT_BLOCK + INTEREST_PAYMENT_INTERVAL as u64)
		.then_execute_with(|_| {
			// Checking that interest was charged here:
			let loan = ChpLoans::<Test>::get(ASSET, LOAN_ID).unwrap();

			assert_eq!(loan.usdc_collateral, INIT_COLLATERAL - interest_charge_usdc_1);
			assert_eq!(loan.fees_collected_usdc, clearing_fee_usdc + interest_charge_usdc_1);
		})
		.then_execute_with(|_| {
			assert_ok!(LendingPools::make_repayment(LOAN_ID, ASSET, PRINCIPAL / 2));
			assert_eq!(MockBalance::get_balance(&LP, ASSET), PRINCIPAL / 2);

			let loan = ChpLoans::<Test>::get(ASSET, LOAN_ID).unwrap();

			// Collateral amount hasn't changed. TODO: consider returning a portion of it?
			assert_eq!(loan.usdc_collateral, INIT_COLLATERAL - interest_charge_usdc_1);

			assert_eq!(loan.total_principal_amount(), PRINCIPAL / 2);
		})
		.then_process_blocks_until_block(INIT_BLOCK + 2 * INTEREST_PAYMENT_INTERVAL as u64)
		.then_execute_with(|_| {
			// Checking that interest was charged again, this time a smaller amount
			// due to the partial repayment:

			let loan = ChpLoans::<Test>::get(ASSET, LOAN_ID).unwrap();
			assert_eq!(
				loan.usdc_collateral,
				INIT_COLLATERAL - interest_charge_usdc_1 - interest_charge_usdc_2
			);
			assert_eq!(loan.fees_collected_usdc, total_fees_usdc);
		})
		.then_execute_with(|_| {
			// Repaying the remainder of the borrowed amount should finalise the loan:
			assert_ok!(LendingPools::make_repayment(LOAN_ID, ASSET, PRINCIPAL / 2));

			let loan = ChpLoans::<Test>::get(ASSET, LOAN_ID).unwrap();
			assert_eq!(loan.status, LoanStatus::Finalising);

			// LP gets their collateral back:
			assert_eq!(
				MockBalance::get_balance(&LP, COLLATERAL_ASSET),
				init_lp_balance_usdc - total_fees_usdc
			);
			assert_eq!(MockBalance::get_balance(&LP, ASSET), 0);

			// There should now be a swap converting fees from USDC to ASSET:
			assert_eq!(
				MockSwapRequestHandler::<Test>::get_swap_requests(),
				vec![MockSwapRequest {
					input_asset: COLLATERAL_ASSET,
					output_asset: ASSET,
					input_amount: total_fees_usdc,
					swap_type: SwapRequestType::Regular {
						output_action: SwapOutputAction::CreditLendingPool { loan_id: LOAN_ID }
					},
					broker_fees: Default::default(),
					origin: SwapOrigin::Internal
				}]
			);
		})
		.then_execute_with(|_| {
			use cf_traits::lending::ChpSystemApi;
			// A swapping pallet is expected to call this once it has swapped the fees
			// into the loan asset, but here we have to trigger it manually:
			Pallet::<Test>::process_loan_swap_outcome(LOAN_ID, ASSET, total_fees_btc);

			System::assert_last_event(RuntimeEvent::LendingPools(Event::<Test>::ChpLoanSettled {
				loan_id: LOAN_ID,
			}));

			// The above should finalise the CHP loan and add swapped fees to the pools:
			assert_eq!(ChpLoans::<Test>::iter().count(), 0);

			let core_pool = CorePools::<Test>::get(ASSET, CORE_POOL_ID).unwrap();

			assert!(core_pool.pending_loans.is_empty());
			assert_eq!(
				core_pool.available_amount,
				ScaledAmount::from_asset_amount(INIT_POOL_AMOUNT + total_fees_btc)
			);
		});
}

#[test]
fn stop_lending() {
	new_test_ext().execute_with(|| {
		setup_chp_pool_with_funds();

		let clearing_fee = EXPECTED_CLEARING_FEE * PRINCIPAL * SWAP_RATE;
		let init_lp_balance = INIT_COLLATERAL + clearing_fee;

		const SWAPPED_FEES: AssetAmount = 15;

		MockBalance::credit_account(&LP, COLLATERAL_ASSET, init_lp_balance);
		assert_eq!(LendingPools::new_chp_loan(LP, ASSET, PRINCIPAL), Ok(LOAN_ID));

		assert_eq!(MockBalance::get_balance(&LENDER, ASSET), 0);
		assert_eq!(MockBalance::get_balance(&LENDER, COLLATERAL_ASSET), 0);

		assert_ok!(LendingPools::stop_chp_lending(RuntimeOrigin::signed(LENDER), ASSET));

		// Some amount is release immediately, but some is still locked in a loan:
		System::assert_last_event(RuntimeEvent::LendingPools(Event::<Test>::StoppedChpLending {
			lender_id: LENDER,
			asset: ASSET,
			unlocked_amount: PRINCIPAL,
			pending_loans: BTreeSet::from_iter([LOAN_ID]),
		}));

		assert_eq!(MockBalance::get_balance(&LENDER, ASSET), PRINCIPAL);
		assert_eq!(MockBalance::get_balance(&LENDER, COLLATERAL_ASSET), 0);

		// Once the loan is repaid, the lender should get the remaining amount:
		assert_ok!(LendingPools::make_repayment(LOAN_ID, ASSET, PRINCIPAL));
		assert_eq!(MockBalance::get_balance(&LENDER, ASSET), PRINCIPAL * 2);
		assert_eq!(MockBalance::get_balance(&LENDER, COLLATERAL_ASSET), 0);
		assert_eq!(ChpLoans::<Test>::get(ASSET, LOAN_ID).unwrap().status, LoanStatus::Finalising);

		// Fee swap should finalise the loan crediting swapeed fees to the lender (even though
		// they already stopped lending):
		Pallet::<Test>::process_loan_swap_outcome(LOAN_ID, ASSET, SWAPPED_FEES);
		assert!(ChpLoans::<Test>::get(ASSET, LOAN_ID).is_none());
		assert_eq!(MockBalance::get_balance(&LENDER, ASSET), PRINCIPAL * 2 + SWAPPED_FEES);
		assert_eq!(MockBalance::get_balance(&LENDER, COLLATERAL_ASSET), 0);
	});
}

#[test]
fn soft_liquidation() {
	use cf_chains::evm::U256;
	use cf_primitives::PRICE_FRACTIONAL_BITS;

	// 10% price increase -> 10% more collateral
	const EXTRA_COLLATERAL: AssetAmount = INIT_COLLATERAL / 10;

	let default_price = U256::from(SWAP_RATE) << PRICE_FRACTIONAL_BITS;

	let clearing_fee = EXPECTED_CLEARING_FEE * PRINCIPAL * SWAP_RATE;
	let init_lp_balance = INIT_COLLATERAL + clearing_fee + EXTRA_COLLATERAL;

	new_test_ext()
		.execute_with(|| {
			assert_eq!(System::block_number(), INIT_BLOCK);

			setup_chp_pool_with_funds();

			MockBalance::credit_account(&LP, COLLATERAL_ASSET, init_lp_balance);
			assert_eq!(LendingPools::new_chp_loan(LP, ASSET, PRINCIPAL), Ok(LOAN_ID));

			assert_eq!(MockBalance::get_balance(&LP, COLLATERAL_ASSET), EXTRA_COLLATERAL);
		})
		.then_execute_with(|_| {
			// After some time the asset's price goes up by 10% and the loan becomes
			// undercollateralised:
			let new_price = (default_price / 10) + default_price;

			MockOraclePriceApi::set_price(ASSET, new_price);
		})
		.then_execute_at_next_block(|_| {
			// Here we expect auto top up due to reaching soft liquidation threshold
			assert_eq!(MockBalance::get_balance(&LP, COLLATERAL_ASSET), 0);

			let loan = ChpLoans::<Test>::get(ASSET, LOAN_ID).unwrap();
			assert_eq!(loan.usdc_collateral, INIT_COLLATERAL + EXTRA_COLLATERAL);
			assert_eq!(loan.status, LoanStatus::Active);
		})
		.then_execute_with(|_| {
			// Price increases again and this time the user doesn't have enough funds
			// to top up the collateral, so soft liquidation will be initiated
			let new_price = (default_price / 4) + default_price;

			MockOraclePriceApi::set_price(ASSET, new_price);

			// CONTINUE HERE
		})
		.then_execute_at_next_block(|_| {
			// The borrower has no collateral left -> should trigger soft liquidation
			let loan = ChpLoans::<Test>::get(ASSET, LOAN_ID).unwrap();
			assert_eq!(loan.usdc_collateral, 0);
			assert_eq!(
				loan.status,
				LoanStatus::SoftLiquidation { usdc_collateral: INIT_COLLATERAL + EXTRA_COLLATERAL }
			);

			// A Swap should have been initiated:
			assert_eq!(
				MockSwapRequestHandler::<Test>::get_swap_requests(),
				vec![MockSwapRequest {
					input_asset: COLLATERAL_ASSET,
					output_asset: ASSET,
					input_amount: INIT_COLLATERAL + EXTRA_COLLATERAL,
					swap_type: SwapRequestType::Regular {
						output_action: SwapOutputAction::CreditLendingPool { loan_id: LOAN_ID }
					},
					broker_fees: Default::default(),
					origin: SwapOrigin::Internal
				}]
			);
		})
		.then_execute_at_next_block(|_| {
			const SWAP_OUTPUT_EXTRA: AssetAmount = 10_000;

			const COLLATERAL_SWAP_OUTPUT: AssetAmount = PRINCIPAL + SWAP_OUTPUT_EXTRA;

			let core_pool = CorePools::<Test>::get(ASSET, CORE_POOL_ID).unwrap();
			assert_eq!(
				core_pool.available_amount,
				ScaledAmount::from_asset_amount(INIT_POOL_AMOUNT - PRINCIPAL)
			);

			// This will be called by the swapping pallet once the liquidation swap is done:
			Pallet::<Test>::process_loan_swap_outcome(LOAN_ID, ASSET, COLLATERAL_SWAP_OUTPUT);

			let core_pool = CorePools::<Test>::get(ASSET, CORE_POOL_ID).unwrap();
			assert_eq!(
				core_pool.available_amount,
				ScaledAmount::from_asset_amount(INIT_POOL_AMOUNT)
			);

			// The borrower should be credited the remaining swapped amount:
			assert_eq!(MockBalance::get_balance(&LP, COLLATERAL_ASSET), 0);
			assert_eq!(MockBalance::get_balance(&LP, ASSET), PRINCIPAL + SWAP_OUTPUT_EXTRA);

			let loan = ChpLoans::<Test>::get(ASSET, LOAN_ID).unwrap();
			assert_eq!(loan.status, LoanStatus::Finalising);
		})
		.then_execute_at_next_block(|_| {
			let swapped_fees = clearing_fee * SWAP_RATE;

			// This will be called by the swapping pallet once the fee swap is done:
			Pallet::<Test>::process_loan_swap_outcome(LOAN_ID, ASSET, swapped_fees);

			// The above should finalise the CHP loan and add swapped fees to the pools:
			assert_eq!(ChpLoans::<Test>::iter().count(), 0);

			let core_pool = CorePools::<Test>::get(ASSET, CORE_POOL_ID).unwrap();

			assert!(core_pool.pending_loans.is_empty());
			assert_eq!(
				core_pool.available_amount,
				ScaledAmount::from_asset_amount(INIT_POOL_AMOUNT + swapped_fees)
			);
		});
}

#[test]
fn loan_expiration() {
	const EXPIRY_BLOCK: u64 = INIT_BLOCK + MAX_LOAN_DURATION as u64;

	let interest_charge_usdc = EXPECTED_INTEREST * PRINCIPAL * SWAP_RATE;

	new_test_ext()
		.execute_with(|| {
			setup_chp_pool_with_funds();

			let clearing_fee = EXPECTED_CLEARING_FEE * PRINCIPAL * SWAP_RATE;
			let init_lp_balance = INIT_COLLATERAL + clearing_fee;

			MockBalance::credit_account(&LP, COLLATERAL_ASSET, init_lp_balance);
			assert_eq!(LendingPools::new_chp_loan(LP, ASSET, PRINCIPAL), Ok(LOAN_ID));
		})
		.then_process_blocks_until_block(EXPIRY_BLOCK)
		.then_execute_with(|_| {
			let loan = ChpLoans::<Test>::get(ASSET, LOAN_ID).unwrap();
			assert_eq!(
				loan.status,
				LoanStatus::SoftLiquidation {
					usdc_collateral: INIT_COLLATERAL - interest_charge_usdc * 3
				}
			);
		});
}

mod safe_mode {

	use super::*;
	use cf_traits::{SafeMode, SetSafeMode};
	use frame_support::assert_noop;

	#[test]
	fn safe_mode_for_adding_funds() {
		new_test_ext().execute_with(|| {
			assert_ok!(LendingPools::new_chp_pool(ASSET));

			MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
				add_chp_funds_enabled: false,
				..PalletSafeMode::CODE_GREEN
			});

			MockBalance::credit_account(&LENDER, ASSET, INIT_POOL_AMOUNT);
			assert_noop!(
				LendingPools::add_chp_funds(RuntimeOrigin::signed(LENDER), ASSET, INIT_POOL_AMOUNT),
				Error::<Test>::AddChpFundsDisabled
			);

			MockRuntimeSafeMode::set_safe_mode(PalletSafeMode { ..PalletSafeMode::CODE_GREEN });

			assert_ok!(LendingPools::add_chp_funds(
				RuntimeOrigin::signed(LENDER),
				ASSET,
				INIT_POOL_AMOUNT
			),);
		});
	}

	#[test]
	fn safe_mode_for_removing_funds() {
		new_test_ext().execute_with(|| {
			assert_ok!(LendingPools::new_chp_pool(ASSET));

			MockBalance::credit_account(&LENDER, ASSET, INIT_POOL_AMOUNT);
			assert_ok!(LendingPools::add_chp_funds(
				RuntimeOrigin::signed(LENDER),
				ASSET,
				INIT_POOL_AMOUNT
			));

			MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
				stop_chp_lending_enabled: false,
				..PalletSafeMode::CODE_GREEN
			});

			assert_noop!(
				LendingPools::stop_chp_lending(RuntimeOrigin::signed(LENDER), ASSET),
				Error::<Test>::StopChpLendingDisabled
			);

			MockRuntimeSafeMode::set_safe_mode(PalletSafeMode { ..PalletSafeMode::CODE_GREEN });

			assert_ok!(LendingPools::stop_chp_lending(RuntimeOrigin::signed(LENDER), ASSET));
		});
	}

	#[test]
	fn safe_mode_for_creating_chp_loan() {
		new_test_ext().execute_with(|| {
			assert_ok!(LendingPools::new_chp_pool(ASSET));

			MockBalance::credit_account(&LENDER, ASSET, INIT_POOL_AMOUNT);
			assert_ok!(LendingPools::add_chp_funds(
				RuntimeOrigin::signed(LENDER),
				ASSET,
				INIT_POOL_AMOUNT
			));

			MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
				chp_loans_enabled: false,
				..PalletSafeMode::CODE_GREEN
			});

			MockBalance::credit_account(&LP, COLLATERAL_ASSET, 2 * PRINCIPAL * SWAP_RATE);
			assert_noop!(
				LendingPools::new_chp_loan(LP, ASSET, PRINCIPAL),
				Error::<Test>::ChpLoansDisabled
			);

			MockRuntimeSafeMode::set_safe_mode(PalletSafeMode { ..PalletSafeMode::CODE_GREEN });

			assert_ok!(LendingPools::new_chp_loan(LP, ASSET, PRINCIPAL));
		});
	}
}
