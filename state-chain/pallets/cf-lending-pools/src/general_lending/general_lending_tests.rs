use crate::mocks::*;
use cf_amm_math::PRICE_FRACTIONAL_BITS;
use cf_chains::evm::U256;
use cf_test_utilities::{assert_event_sequence, assert_has_event, assert_matching_event_count};
use cf_traits::{
	lending::LendingSystemApi,
	mocks::{
		balance_api::MockBalance,
		price_feed_api::MockPriceFeedApi,
		swap_request_api::{MockSwapRequest, MockSwapRequestHandler},
	},
	SafeMode, SetSafeMode, SwapExecutionProgress,
};
use cf_utilities::assert_matches;

use super::*;
use frame_support::{assert_err, assert_noop, assert_ok, sp_runtime::bounded_vec};

const INIT_BLOCK: u64 = 1;

const LENDER: u64 = BOOSTER_1;
const BORROWER: u64 = LP;

const LOAN_ASSET: Asset = Asset::Btc;
const COLLATERAL_ASSET: Asset = Asset::Eth;
const PRINCIPAL: AssetAmount = 1_000_000_000;
const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE; // 75% LTV

const LOAN_ID: LoanId = LoanId(0);

const SWAP_RATE: u128 = 20;

const INIT_POOL_AMOUNT: AssetAmount = PRINCIPAL * 2;

use crate::LENDING_DEFAULT_CONFIG as CONFIG;

trait LendingTestRunnerExt {
	fn with_funded_pool(self, init_pool_amount: AssetAmount) -> Self;
	fn with_default_loan(self) -> Self;
	fn with_voluntary_liquidation(self) -> Self;
}

impl<Ctx: Clone> LendingTestRunnerExt for cf_test_utilities::TestExternalities<Test, Ctx> {
	fn with_funded_pool(self, init_pool_amount: AssetAmount) -> Self {
		self.then_execute_with(|ctx| {
			setup_pool_with_funds(LOAN_ASSET, init_pool_amount);
			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
			set_asset_price_in_usd(COLLATERAL_ASSET, 1);

			ctx
		})
	}

	fn with_default_loan(self) -> Self {
		self.then_execute_with(|ctx| {
			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

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

			ctx
		})
	}

	/// This method initiates voluntary liquidation. Only expected to be called immediately after
	/// [with_default_loan].
	fn with_voluntary_liquidation(self) -> Self {
		const LIQUIDATION_SWAP: SwapRequestId = SwapRequestId(0);

		self.then_execute_with_keep_context(|_| {
			assert_ok!(LendingPools::initiate_voluntary_liquidation(RuntimeOrigin::signed(
				BORROWER
			)));

			// The liquidation is expected to start at the next block
		})
		.then_execute_at_next_block(|ctx| {
			assert_has_event::<Test>(RuntimeEvent::LendingPools(
				Event::<Test>::LiquidationInitiated {
					borrower_id: BORROWER,
					liquidation_type: LiquidationType::SoftVoluntary,
					swaps: BTreeMap::from([(LOAN_ID, vec![LIQUIDATION_SWAP])]),
				},
			));

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
				LiquidationStatus::Liquidating {
					liquidation_swaps: BTreeMap::from([(
						LIQUIDATION_SWAP,
						LiquidationSwap {
							loan_id: LOAN_ID,
							from_asset: COLLATERAL_ASSET,
							to_asset: LOAN_ASSET
						}
					)]),
					liquidation_type: LiquidationType::SoftVoluntary
				}
			);

			ctx
		})
	}
}

// This is a workaround for Permill not providing const methods...
const fn portion_of_amount(fee: Permill, principal: AssetAmount) -> AssetAmount {
	principal.checked_mul(fee.deconstruct() as u128).unwrap() / Permill::ACCURACY as u128
}

const ORIGINATION_FEE: AssetAmount = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL);

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

fn setup_pool_with_funds(loan_asset: Asset, init_amount: AssetAmount) {
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

/// Derives interest amounts (pool and network portions) to be paid by the borrower
/// per interest charge interval. Returns (pool amount, network amount).
fn derive_interest_amounts(
	principal: AssetAmount,
	utilisation: Permill,
	payment_interval_blocks: u32,
) -> (AssetAmount, AssetAmount) {
	let base_interest = CONFIG.derive_base_interest_rate_per_payment_interval(
		LOAN_ASSET,
		utilisation,
		payment_interval_blocks,
	);

	let pool_amount =
		(ScaledAmountHP::from_asset_amount(principal) * base_interest).into_asset_amount();

	let network_interest =
		CONFIG.derive_network_interest_rate_per_payment_interval(payment_interval_blocks);

	let network_amount =
		(ScaledAmountHP::from_asset_amount(principal) * network_interest).into_asset_amount();

	// Tests aren't valid if the fees are zero (need to adjust tests parameters if this is hit)
	assert!(pool_amount > 0 && network_amount > 0);

	(pool_amount, network_amount)
}

/// Helper struct for keeping track of accrued interest with high precision
struct Interest {
	pool: ScaledAmountHP,
	network: ScaledAmountHP,
}

impl Interest {
	fn new() -> Self {
		Self { pool: Default::default(), network: Default::default() }
	}

	fn accrue_interest(
		&mut self,
		principal: AssetAmount,
		utilisation: Permill,
		payment_interval_blocks: u32,
	) {
		let base_interest = CONFIG.derive_base_interest_rate_per_payment_interval(
			LOAN_ASSET,
			utilisation,
			payment_interval_blocks,
		);

		self.pool
			.saturating_accrue(ScaledAmountHP::from_asset_amount(principal) * base_interest);

		let network_interest =
			CONFIG.derive_network_interest_rate_per_payment_interval(payment_interval_blocks);

		self.network
			.saturating_accrue(ScaledAmountHP::from_asset_amount(principal) * network_interest);
	}

	// Collects non-fractional interest. Returns (pool amount, network amount).
	fn collect(&mut self) -> (AssetAmount, AssetAmount) {
		(self.pool.take_non_fractional_part(), self.network.take_non_fractional_part())
	}
}

#[test]
fn basic_general_lending() {
	// We want the amount to be large enough that we can charge interest immediately
	// (rather than waiting for fractional amounts to accumulate).
	const PRINCIPAL: AssetAmount = 2_000_000_000_000;
	const INIT_POOL_AMOUNT: AssetAmount = PRINCIPAL * 2;

	const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE; // 75% LTV

	const ORIGINATION_FEE: AssetAmount = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL);

	let (origination_fee_network, origination_fee_pool) = take_network_fee(ORIGINATION_FEE);

	// Repaying a small portion to make sure we don't hit low LTV penalty:
	const REPAYMENT_AMOUNT: AssetAmount = PRINCIPAL / 10;

	let utilisation_1 = Permill::from_rational(
		PRINCIPAL + ORIGINATION_FEE,
		INIT_POOL_AMOUNT + origination_fee_pool,
	);

	let mut interest = Interest::new();

	interest.accrue_interest(
		PRINCIPAL + ORIGINATION_FEE,
		utilisation_1,
		CONFIG.interest_payment_interval_blocks,
	);

	let (pool_interest_1, network_interest_1) = interest.collect();

	let first_interest_payment_block = INIT_BLOCK + CONFIG.interest_payment_interval_blocks as u64;

	let total_owed_after_first_repayment =
		PRINCIPAL + ORIGINATION_FEE - REPAYMENT_AMOUNT + pool_interest_1 + network_interest_1;

	let utilisation_2 = Permill::from_rational(
		total_owed_after_first_repayment,
		INIT_POOL_AMOUNT + origination_fee_pool + pool_interest_1,
	);

	interest.accrue_interest(
		total_owed_after_first_repayment,
		utilisation_2,
		CONFIG.interest_payment_interval_blocks,
	);

	let (pool_interest_2, network_interest_2) = interest.collect();

	let total_owed_after_second_interest_payment = PRINCIPAL + ORIGINATION_FEE - REPAYMENT_AMOUNT +
		pool_interest_1 +
		network_interest_1 +
		pool_interest_2 +
		network_interest_2;

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.then_execute_with(|_| {
			// Disable fee swaps for this test (so we easily can check all collected fees)
			LendingConfig::<Test>::set(LendingConfiguration {
				fee_swap_threshold_usd: u128::MAX,
				..CONFIG
			});

			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

			System::reset_events();

			let collateral = BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]);

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

			// NOTE: we want LoanCreated event to be emitted before any event
			// referencing it (e.g. OriginationFeeTaken)
			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::PrimaryCollateralAssetUpdated{
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LoanCreated {
					loan_id: LOAN_ID,
					borrower_id: BORROWER,
					asset: LOAN_ASSET,
					principal_amount: PRINCIPAL,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
					borrower_id: BORROWER,
					collateral: ref collateral_in_event,
				}) if collateral_in_event == &collateral,
				RuntimeEvent::LendingPools(Event::<Test>::OriginationFeeTaken {
					loan_id: LOAN_ID,
					..
				})
			);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(
				Event::<Test>::OriginationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee: origination_fee_pool,
					network_fee: origination_fee_network,
					broker_fee: 0,
				},
			));

			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					// The pool's value has increased by pool's origination fee
					total_amount: INIT_POOL_AMOUNT + origination_fee_pool,
					// The available amount has been decreased not only by the loan's principal, but
					// also by the network's origination fee (it will be by the borrower repaid at a
					// later point)
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL - origination_fee_network,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
					collateral: BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false,
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							id: LOAN_ID,
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
							last_interest_payment_at: INIT_BLOCK,
							owed_principal: PRINCIPAL + ORIGINATION_FEE,
							pending_interest: InterestBreakdown::default(),
						}
					)]),
				})
			);
		})
		.then_process_blocks_until_block(first_interest_payment_block)
		// Checking that interest was charged here:
		.then_execute_with(|_| {
			let loan = LoanAccounts::<Test>::get(BORROWER)
				.unwrap()
				.loans
				.get(&LOAN_ID)
				.unwrap()
				.clone();

			assert_eq!(loan.last_interest_payment_at, first_interest_payment_block);
			assert_eq!(
				loan.owed_principal,
				PRINCIPAL + ORIGINATION_FEE + pool_interest_1 + network_interest_1
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + origination_fee_pool + pool_interest_1,
					available_amount: INIT_POOL_AMOUNT -
						PRINCIPAL - origination_fee_network -
						network_interest_1,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::InterestTaken {
				loan_id: LOAN_ID,
				pool_interest: pool_interest_1,
				network_interest: network_interest_1,
				broker_interest: 0,
				low_ltv_penalty: 0,
			}))
		})
		// === REPAYING SOME OF THE LOAN ===
		.then_execute_with(|_| {
			assert_ok!(LendingPools::try_making_repayment(
				&BORROWER,
				LOAN_ID,
				RepaymentAmount::Exact(REPAYMENT_AMOUNT)
			));
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
				total_owed_after_first_repayment
			);
			// Funds have been returned to the pool:
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + origination_fee_pool + pool_interest_1,
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL + REPAYMENT_AMOUNT -
						origination_fee_network -
						network_interest_1,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);

			assert_eq!(
				PendingNetworkFees::<Test>::get(LOAN_ASSET),
				origination_fee_network + network_interest_1
			);

			// TODO: check that network got its payment
		})
		.then_process_blocks_until_block(
			INIT_BLOCK + 2 * CONFIG.interest_payment_interval_blocks as u64,
		)
		// === Interest is charged the second time ===
		.then_execute_with(|_| {
			// This time we expect a smaller amount due to the partial repayment (which both
			// the principal and the pool's utilisation):
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER)
					.unwrap()
					.loans
					.get(&LOAN_ID)
					.unwrap()
					.owed_principal,
				total_owed_after_second_interest_payment
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT +
						origination_fee_pool +
						pool_interest_1 + pool_interest_2,
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL + REPAYMENT_AMOUNT -
						origination_fee_network -
						network_interest_1 - network_interest_2,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);

			assert_eq!(
				PendingNetworkFees::<Test>::get(LOAN_ASSET),
				origination_fee_network + network_interest_1 + network_interest_2
			);
		})
		.then_execute_with(|_| {
			// Repaying the remainder of the borrowed amount should finalise the loan:
			MockBalance::credit_account(
				&BORROWER,
				LOAN_ASSET,
				ORIGINATION_FEE +
					pool_interest_1 + pool_interest_2 +
					network_interest_1 +
					network_interest_2,
			);
			assert_ok!(LendingPools::try_making_repayment(
				&BORROWER,
				LOAN_ID,
				RepaymentAmount::Exact(total_owed_after_second_interest_payment)
			));
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), 0);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
				loan_id: LOAN_ID,
				outstanding_principal: 0,
				via_liquidation: false,
			}));

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT +
						origination_fee_pool +
						pool_interest_1 + pool_interest_2,
					available_amount: INIT_POOL_AMOUNT +
						origination_fee_pool +
						pool_interest_1 + pool_interest_2,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false,
					// Note that we don't automatically release the collateral:
					collateral: BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
					loans: Default::default(),
				})
			);
		});
}

#[test]
fn dynamic_interest_payment_interval() {
	// Testing a scenario where we fail to charge interest at the usual block
	// (according to the regular interval) and instead a larger amount of interest will be charged
	// at a later block.

	let get_loan = || {
		LoanAccounts::<Test>::get(BORROWER)
			.unwrap()
			.loans
			.get(&LOAN_ID)
			.unwrap()
			.clone()
	};

	let interest_payment_first_attempt_block =
		INIT_BLOCK + CONFIG.interest_payment_interval_blocks as u64;

	let utilisation = Permill::from_rational(PRINCIPAL + ORIGINATION_FEE, INIT_POOL_AMOUNT);

	// NOTE: amounts calculated using payment interval extended by 1 block
	let (pool_interest, network_interest) = derive_interest_amounts(
		PRINCIPAL,
		utilisation,
		CONFIG.interest_payment_interval_blocks + 1,
	);

	let (network_origination_fee, pool_origination_fee) = take_network_fee(ORIGINATION_FEE);

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.then_execute_with(|_| {
			// Set a low collection threshold so we can more easily check the collected amount
			assert_ok!(Pallet::<Test>::update_pallet_config(
				RuntimeOrigin::root(),
				bounded_vec![PalletConfigUpdate::SetInterestCollectionThresholdUsd(1)],
			));

			// Making oracle price unavailable to cause interest calculations
			// be skipped for now:
			MockPriceFeedApi::set_price(LOAN_ASSET, None);
		})
		.then_process_blocks_until_block(interest_payment_first_attempt_block)
		.then_execute_with(|_| {
			// No interest payment yet:
			assert_eq!(get_loan().last_interest_payment_at, INIT_BLOCK);

			// Making oracle price available will cause interest to be
			// calculated/taken at the next block
			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(
				get_loan().last_interest_payment_at,
				interest_payment_first_attempt_block + 1
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap().total_amount,
				INIT_POOL_AMOUNT + pool_origination_fee + pool_interest
			);

			assert_eq!(
				PendingNetworkFees::<Test>::get(LOAN_ASSET),
				network_origination_fee + network_interest
			);
		});
}

#[test]
fn collateral_auto_topup() {
	const COLLATERAL_TOPUP: AssetAmount = INIT_COLLATERAL / 100;

	// The user deposits this much of collateral asset into their balance at a later point
	const EXTRA_FUNDS: AssetAmount = INIT_COLLATERAL;

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
			setup_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE * 1_000_000);
			set_asset_price_in_usd(COLLATERAL_ASSET, 1_000_000);

			MockBalance::credit_account(
				&BORROWER,
				COLLATERAL_ASSET,
				INIT_COLLATERAL + COLLATERAL_TOPUP,
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

			assert_eq!(get_ltv(), FixedU64::from_rational(750_075, 1_000_000)); // ~75%

			// The price drops 1%, but that shouldn't trigger a top-up
			// at the next block
			set_asset_price_in_usd(COLLATERAL_ASSET, 990_000);

			assert_eq!(get_ltv(), FixedU64::from_rational(757_651_515, 1_000_000_000)); // ~76%
		})
		.then_execute_at_next_block(|_| {
			// No change in collateral (no auto top up):
			assert_eq!(get_collateral(), INIT_COLLATERAL);

			// Drop the price further, this time auto-top up should be triggered
			set_asset_price_in_usd(COLLATERAL_ASSET, 870_000);

			assert_eq!(get_ltv(), FixedU64::from_rational(862_155_173, 1_000_000_000)); // ~86%
		})
		.then_execute_at_next_block(|_| {
			// The user only had a small amount in their balance, all of it gets used:
			assert_eq!(get_collateral(), INIT_COLLATERAL + COLLATERAL_TOPUP);
			assert_eq!(get_ltv(), FixedU64::from_rational(853_618_983, 1_000_000_000)); // ~85%
			assert_eq!(MockBalance::get_balance(&LENDER, COLLATERAL_ASSET), 0);

			// After we give the user more funds, auto-top up should bring CR back to target
			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, EXTRA_FUNDS);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
				borrower_id: BORROWER,
				collateral: BTreeMap::from([(COLLATERAL_ASSET, COLLATERAL_TOPUP)]),
			}));
		})
		.then_execute_at_next_block(|_| {
			let collateral_topup_2 =
				get_collateral().saturating_sub(INIT_COLLATERAL + COLLATERAL_TOPUP);

			assert_ne!(collateral_topup_2, 0);
			assert_eq!(get_ltv(), FixedU64::from_rational(80, 100)); // 80%
			assert_eq!(
				MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET),
				EXTRA_FUNDS - collateral_topup_2
			);
		});
}

#[test]
fn basic_loan_aggregation() {
	// Should be able to borrow this amount without providing any extra collateral:
	const EXTRA_PRINCIPAL_1: AssetAmount = PRINCIPAL / 100;
	// This larger amount should require extra collateral:
	const EXTRA_PRINCIPAL_2: AssetAmount = PRINCIPAL / 2;
	const EXTRA_COLLATERAL: AssetAmount = INIT_COLLATERAL / 2;

	const ORIGINATION_FEE: AssetAmount = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL);

	let (origination_fee_network_1, origination_fee_pool_1) = take_network_fee(ORIGINATION_FEE);

	const ORIGINATION_FEE_2: AssetAmount =
		portion_of_amount(DEFAULT_ORIGINATION_FEE, EXTRA_PRINCIPAL_1);

	let (origination_fee_network_2, origination_fee_pool_2) = take_network_fee(ORIGINATION_FEE_2);

	const ORIGINATION_FEE_3: AssetAmount =
		portion_of_amount(DEFAULT_ORIGINATION_FEE, EXTRA_PRINCIPAL_2);

	let (origination_fee_network_3, origination_fee_pool_3) = take_network_fee(ORIGINATION_FEE_3);

	new_test_ext().execute_with(|| {
		setup_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

		set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
		set_asset_price_in_usd(COLLATERAL_ASSET, 1);

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

		let collateral = BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]);

		assert_eq!(
			LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				PRINCIPAL,
				Some(COLLATERAL_ASSET),
				collateral.clone()
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

			// NOTE: no CollateralAdded event since we are not adding any yet
			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanUpdated {
					loan_id: LOAN_ID,
					extra_principal_amount: EXTRA_PRINCIPAL_1,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::OriginationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee,
					network_fee,
					broker_fee: 0,
				}) if pool_fee == origination_fee_pool_2 && network_fee == origination_fee_network_2
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT +
						origination_fee_pool_1 +
						origination_fee_pool_2,
					available_amount: INIT_POOL_AMOUNT -
						PRINCIPAL - EXTRA_PRINCIPAL_1 -
						origination_fee_network_1 -
						origination_fee_network_2,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
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
							last_interest_payment_at: INIT_BLOCK,
							owed_principal: PRINCIPAL +
								EXTRA_PRINCIPAL_1 + origination_fee_pool_1 +
								origination_fee_pool_2 + origination_fee_network_1 +
								origination_fee_network_2,
							pending_interest: Default::default()
						}
					)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false,
				}
			);

			assert_eq!(
				MockBalance::get_balance(&BORROWER, LOAN_ASSET),
				PRINCIPAL + EXTRA_PRINCIPAL_1
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
		}

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, EXTRA_COLLATERAL);

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
			System::reset_events();

			let extra_collateral = BTreeMap::from([(COLLATERAL_ASSET, EXTRA_COLLATERAL)]);

			assert_ok!(LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				EXTRA_PRINCIPAL_2,
				extra_collateral.clone()
			));

			let (network_fee, pool_fee) = take_network_fee(ORIGINATION_FEE_3);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanUpdated {
					loan_id: LOAN_ID,
					extra_principal_amount: EXTRA_PRINCIPAL_2,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
					borrower_id: BORROWER,
					ref collateral,
				}) if collateral == &extra_collateral,
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
					total_amount: INIT_POOL_AMOUNT +
						origination_fee_pool_1 +
						origination_fee_pool_2 +
						origination_fee_pool_3,
					available_amount: INIT_POOL_AMOUNT -
						PRINCIPAL - EXTRA_PRINCIPAL_1 -
						EXTRA_PRINCIPAL_2 - origination_fee_network_1 -
						origination_fee_network_2 -
						origination_fee_network_3,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
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
							last_interest_payment_at: INIT_BLOCK,
							// Loan's owed principal has been increased:
							owed_principal: PRINCIPAL +
								EXTRA_PRINCIPAL_1 + EXTRA_PRINCIPAL_2 +
								origination_fee_pool_1 + origination_fee_pool_2 +
								origination_fee_pool_3 + origination_fee_network_1 +
								origination_fee_network_2 +
								origination_fee_network_3,
							pending_interest: Default::default()
						}
					)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false,
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
fn swap_collected_network_fees() {
	const ASSET_1: Asset = Asset::Eth;
	const ASSET_2: Asset = Asset::Usdc;

	// NOTE: these need to be large enough to exceed fee swap threshold
	const AMOUNT_1: AssetAmount = 2_000_000;
	const AMOUNT_2: AssetAmount = 1_000_000;

	let fee_swap_block = CONFIG.fee_swap_interval_blocks as u64;

	new_test_ext()
		.execute_with(|| {
			LendingPools::credit_fees_to_network(ASSET_1, AMOUNT_1);
			LendingPools::credit_fees_to_network(ASSET_2, AMOUNT_2);

			// Network fee collection requires oracle prices available:
			set_asset_price_in_usd(ASSET_1, SWAP_RATE);
			set_asset_price_in_usd(ASSET_2, SWAP_RATE);

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

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LendingNetworkFeeSwapInitiated {
					swap_request_id: SwapRequestId(0)
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LendingNetworkFeeSwapInitiated {
					swap_request_id: SwapRequestId(1)
				})
			);

			assert_eq!(
				PendingNetworkFees::<Test>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(ASSET_1, 0), (ASSET_2, 0)])
			);
		});
}

#[test]
fn adding_and_removing_collateral() {
	const COLLATERAL_ASSET_1: Asset = COLLATERAL_ASSET;
	const COLLATERAL_ASSET_2: Asset = Asset::Usdc;

	const INIT_COLLATERAL: AssetAmount = (5 * PRINCIPAL / 4) * SWAP_RATE; // 80% LTV
	const INIT_COLLATERAL_AMOUNT_2: AssetAmount = 1000;

	new_test_ext().execute_with(|| {
		setup_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);
		set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
		set_asset_price_in_usd(COLLATERAL_ASSET_1, 1);
		set_asset_price_in_usd(COLLATERAL_ASSET_2, 1);

		MockBalance::credit_account(
			&BORROWER,
			COLLATERAL_ASSET_1,
			INIT_COLLATERAL + ORIGINATION_FEE,
		);
		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET_2, INIT_COLLATERAL_AMOUNT_2);

		let collateral = BTreeMap::from([
			(COLLATERAL_ASSET_1, INIT_COLLATERAL),
			(COLLATERAL_ASSET_2, INIT_COLLATERAL_AMOUNT_2),
		]);

		System::reset_events();

		assert_ok!(LendingPools::add_collateral(
			RuntimeOrigin::signed(BORROWER),
			Some(COLLATERAL_ASSET_1),
			collateral.clone(),
		));

		assert_event_sequence!(
			Test,
			RuntimeEvent::LendingPools(Event::<Test>::PrimaryCollateralAssetUpdated{
				borrower_id: BORROWER,
				primary_collateral_asset: COLLATERAL_ASSET_1,
			}),
			RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
				borrower_id: BORROWER,
				collateral: ref collateral_in_event,
			}) if collateral_in_event == &collateral
		);

		// Adding collateral creates a loan account:
		assert_eq!(
			LoanAccounts::<Test>::get(BORROWER).unwrap(),
			LoanAccount {
				borrower_id: BORROWER,
				primary_collateral_asset: COLLATERAL_ASSET_1,
				collateral: collateral.clone(),
				loans: BTreeMap::default(),
				liquidation_status: LiquidationStatus::NoLiquidation,
				voluntary_liquidation_requested: false,
			}
		);

		assert_ok!(LendingPools::remove_collateral(
			RuntimeOrigin::signed(BORROWER),
			BTreeMap::from([
				(COLLATERAL_ASSET_1, INIT_COLLATERAL),
				(COLLATERAL_ASSET_2, INIT_COLLATERAL_AMOUNT_2),
			]),
		));

		assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::CollateralRemoved {
			borrower_id: BORROWER,
			collateral,
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

	// This should trigger soft liquidation
	const NEW_SWAP_RATE: u128 = 25;

	let (origination_fee_network, origination_fee_pool) = take_network_fee(ORIGINATION_FEE);

	// This should trigger second (hard) liquidation
	const SWAP_RATE_2: u128 = 29;

	// How much collateral will be swapped during liquidation:
	const EXECUTED_COLLATERAL: AssetAmount = 3 * INIT_COLLATERAL / 5;
	// How much of principal asset is bought during first liquidation:
	const SWAPPED_PRINCIPAL: AssetAmount = EXECUTED_COLLATERAL / NEW_SWAP_RATE;

	let liquidation_fee_1 = CONFIG.liquidation_fee(LOAN_ASSET) * SWAPPED_PRINCIPAL;
	let (liquidation_fee_network_1, liquidation_fee_pool_1) = take_network_fee(liquidation_fee_1);

	// This much will be repaid via first liquidation (everything swapped minus liquidation fee)
	let repaid_amount_1 = SWAPPED_PRINCIPAL - liquidation_fee_1;

	// How much of principal asset is bought during second liquidation:
	const SWAPPED_PRINCIPAL_2: AssetAmount = (INIT_COLLATERAL - EXECUTED_COLLATERAL) / SWAP_RATE_2;

	// This much will be repaid via second liquidation (full principal after first repayment)
	let repaid_amount_2 = PRINCIPAL + ORIGINATION_FEE - repaid_amount_1;

	let liquidation_fee_2 = CONFIG.liquidation_fee(LOAN_ASSET) * repaid_amount_2;
	let (liquidation_fee_network_2, liquidation_fee_pool_2) = take_network_fee(liquidation_fee_2);

	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);
	const LIQUIDATION_SWAP_2: SwapRequestId = SwapRequestId(1);

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.execute_with(|| {
			// Change oracle price to trigger liquidation
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
					liquidation_type: LiquidationType::Soft
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
					liquidation_type: LiquidationType::Soft,
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
					voluntary_liquidation_requested: false,
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
							last_interest_payment_at: INIT_BLOCK,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL + ORIGINATION_FEE -
								repaid_amount_1,
							pending_interest: Default::default()
						}
					)]),
				})
			);

			// Liquidation Swap must have been aborted:
			assert!(!MockSwapRequestHandler::<Test>::get_swap_requests()
				.contains_key(&LIQUIDATION_SWAP_1));

			// Part of the principal has been repaid via liquidation:
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + origination_fee_pool + liquidation_fee_pool_1,
					// Note that liquidation fee is available immediately since is paid from the
					// liquidation's output:
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL - origination_fee_network +
						repaid_amount_1 + liquidation_fee_pool_1,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
					borrower_id: BORROWER,
					reason: LiquidationCompletionReason::LtvChange,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee,
					network_fee,
					broker_fee
				}) if pool_fee == liquidation_fee_pool_1 && network_fee == liquidation_fee_network_1 && broker_fee == 0,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount,
				}) if amount == repaid_amount_1,
			);

			// No CollateralAdded event when returning existing collateral:
			assert_matching_event_count!(Test, RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded{..}) => 0);


			// Change oracle price again to trigger liquidation:
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
					liquidation_type: LiquidationType::Hard
				}
			);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(
				Event::<Test>::LiquidationInitiated {
					borrower_id: BORROWER,
					swaps: BTreeMap::from([(LOAN_ID, vec![LIQUIDATION_SWAP_2])]),
					liquidation_type: LiquidationType::Hard,
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

			// This excess principal asset amount will be credited to the borrower's collateral
			let excess_principal = SWAPPED_PRINCIPAL_2 - repaid_amount_2 - liquidation_fee_2;

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
					borrower_id: BORROWER,
					reason: LiquidationCompletionReason::FullySwapped,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee,
					network_fee,
					broker_fee
				}) if pool_fee == liquidation_fee_pool_2 && network_fee == liquidation_fee_network_2 && broker_fee == 0,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount,
				}) if amount == repaid_amount_2,
				RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
					borrower_id: BORROWER,
					ref collateral,
				}) if collateral == &BTreeMap::from([(LOAN_ASSET, excess_principal)]),
				// The loan should now be settled:
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal: 0,
					via_liquidation: true,
				}),
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

			// The pool is expected to get all of its original funds back plus all the fees
			// (interest is not collected in this test)
			let expected_total_amount = INIT_POOL_AMOUNT + origination_fee_pool +
				liquidation_fee_pool_1 +
				liquidation_fee_pool_2;
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: expected_total_amount,
					// All of the funds should be available:
					available_amount: expected_total_amount,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);

			assert_eq!(
				PendingNetworkFees::<Test>::get(LOAN_ASSET),
				origination_fee_network + liquidation_fee_network_1 + liquidation_fee_network_2
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
					collateral: BTreeMap::from([(LOAN_ASSET, excess_principal)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false,
					loans: Default::default(),
				})
			);
		});
}

#[test]
fn liquidation_fully_repays_loan_when_aborted() {
	// Test a (likely rare) scenario where liquidation is aborted (due to reaching
	// acceptable LTV), but the collateral already swapped is enough to fully cover
	// a loan.

	const NEW_SWAP_RATE: u128 = SWAP_RATE * 2;

	// The amount "recovered" during liquidation is larger than the total
	// owed principal:
	const RECOVERED_LOAN_ASSET: AssetAmount = PRINCIPAL + PRINCIPAL / 50;
	const REMAINING_COLLATERAL: AssetAmount = INIT_COLLATERAL / 10;

	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.then_execute_with(|_| {
			// Change oracle price to trigger liquidation
			set_asset_price_in_usd(LOAN_ASSET, NEW_SWAP_RATE);
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
				LiquidationStatus::Liquidating {
					liquidation_swaps: BTreeMap::from([(
						LIQUIDATION_SWAP_1,
						LiquidationSwap {
							loan_id: LOAN_ID,
							from_asset: COLLATERAL_ASSET,
							to_asset: LOAN_ASSET
						}
					)]),
					liquidation_type: LiquidationType::Hard
				}
			);

			// Simulate partial liquidation: it is not yet complete, but already produced enough of
			// the loan asset to fully repay the loan. Liquidation swap should be aborted at the
			// next block.
			MockSwapRequestHandler::<Test>::set_swap_request_progress(
				LIQUIDATION_SWAP_1,
				SwapExecutionProgress {
					remaining_input_amount: REMAINING_COLLATERAL,
					accumulated_output_amount: RECOVERED_LOAN_ASSET,
				},
			);
		})
		.then_execute_at_next_block(|_| {
			// Liquidation fee is computed on the total amount to repay:
			let liquidation_fee =
				CONFIG.liquidation_fee(LOAN_ASSET) * (PRINCIPAL + ORIGINATION_FEE);

			// The loan has been repaid, the remaining collateral amount and the excess loan asset
			// amount are credited to the collateral balance:
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false,
					collateral: BTreeMap::from([
						(COLLATERAL_ASSET, REMAINING_COLLATERAL),
						(
							LOAN_ASSET,
							RECOVERED_LOAN_ASSET - PRINCIPAL - ORIGINATION_FEE - liquidation_fee
						)
					]),
					loans: Default::default(),
				})
			);

			// Making sure account's free balance is unaffected:
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);
			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
		});
}

#[test]
fn liquidation_with_outstanding_principal() {
	// Test a scenario where a loan is liquidated and the recovered principal
	// isn't enought to cover the total loan amount.

	const RECOVERED_PRINCIPAL: AssetAmount = 3 * PRINCIPAL / 4;

	const NEW_SWAP_RATE: u128 = SWAP_RATE * 2;

	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);

	let (origination_fee_network, origination_fee_pool) = take_network_fee(ORIGINATION_FEE);

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).with_default_loan()
		.execute_with(|| {
			// Change oracle price to trigger liquidation
			set_asset_price_in_usd(LOAN_ASSET, NEW_SWAP_RATE);
		})
		.then_execute_at_next_block(|_| {
			assert!(MockSwapRequestHandler::<Test>::get_swap_requests()
				.contains_key(&LIQUIDATION_SWAP_1));

			System::reset_events();

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + origination_fee_pool,
					available_amount: INIT_POOL_AMOUNT - PRINCIPAL - origination_fee_network,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);

			LendingPools::process_loan_swap_outcome(
				LIQUIDATION_SWAP_1,
				LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
				RECOVERED_PRINCIPAL,
			);


			let liquidation_fee = CONFIG.liquidation_fee(LOAN_ASSET) * RECOVERED_PRINCIPAL;
			let (liquidation_fee_network, liquidation_fee_pool) =
				take_network_fee(liquidation_fee);

			let repaid_principal = RECOVERED_PRINCIPAL - liquidation_fee;
			let expected_outstanding_principal = PRINCIPAL + ORIGINATION_FEE - repaid_principal;

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
					borrower_id: BORROWER,
					reason: LiquidationCompletionReason::FullySwapped
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee,
					network_fee,
					broker_fee
				}) if pool_fee == liquidation_fee_pool && network_fee == liquidation_fee_network && broker_fee == 0,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount
				}) if amount == RECOVERED_PRINCIPAL - liquidation_fee,
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal,
					via_liquidation: true,
				}) if outstanding_principal == expected_outstanding_principal
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);
			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);

			// The pool has lost outstanding principal (but has accrued origination and liquidation fees):
			let new_total_amount = INIT_POOL_AMOUNT + origination_fee_pool + liquidation_fee_pool - expected_outstanding_principal;

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: new_total_amount,
					available_amount: new_total_amount,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);

			// The account has no loans and no collateral, so it should have been removed:
			assert!(!LoanAccounts::<Test>::contains_key(BORROWER));
		});
}

#[test]
fn liquidation_with_outstanding_principal_and_owed_network_fees() {
	// Same as in `liquidation_with_outstanding_principal`, we test a scenario where a loan is
	// liquidated and the recovered principal isn't enought to cover the total loan amount. However,
	// in this test utilisation is 100% and we want to check that the pool owing some fees to the
	// network does not break anything when writing off debt.

	const PRINCIPAL: AssetAmount = INIT_POOL_AMOUNT;
	const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE; // 75% LTV

	const RECOVERED_PRINCIPAL: AssetAmount = 3 * PRINCIPAL / 4;

	const NEW_SWAP_RATE: u128 = SWAP_RATE * 2;

	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);

	const ORIGINATION_FEE: AssetAmount = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL);
	let (origination_fee_network, origination_fee_pool) = take_network_fee(ORIGINATION_FEE);

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT)
		.execute_with(|| {

			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

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

			// Update oracle price to trigger liquidation
			set_asset_price_in_usd(LOAN_ASSET, NEW_SWAP_RATE);
		})
		.then_execute_at_next_block(|_| {
			assert!(MockSwapRequestHandler::<Test>::get_swap_requests()
				.contains_key(&LIQUIDATION_SWAP_1));

			System::reset_events();

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + origination_fee_pool,
					available_amount: 0,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: origination_fee_network,
				}
			);

			LendingPools::process_loan_swap_outcome(
				LIQUIDATION_SWAP_1,
				LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
				RECOVERED_PRINCIPAL,
			);

			let liquidation_fee = CONFIG.liquidation_fee(LOAN_ASSET) * RECOVERED_PRINCIPAL;
			let (liquidation_fee_network, liquidation_fee_pool) =
				take_network_fee(liquidation_fee);

			let repaid_principal = RECOVERED_PRINCIPAL - liquidation_fee;
			let expected_outstanding_principal = PRINCIPAL + ORIGINATION_FEE - repaid_principal;

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
					borrower_id: BORROWER,
					reason: LiquidationCompletionReason::FullySwapped,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken {
					loan_id: LOAN_ID,
					pool_fee,
					network_fee,
					broker_fee
				}) if pool_fee == liquidation_fee_pool && network_fee == liquidation_fee_network && broker_fee == 0,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount
				}) if amount == RECOVERED_PRINCIPAL - liquidation_fee,
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal,
					via_liquidation: true,
				}) if outstanding_principal == expected_outstanding_principal
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);
			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);

			// The pool has lost outstanding principal (but has accrued origination and liquidation fees):
			let new_total_amount = INIT_POOL_AMOUNT + origination_fee_pool + liquidation_fee_pool - expected_outstanding_principal;

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: new_total_amount,
					// Network hasn't collected the fees, but the funds for that are available:
					available_amount: new_total_amount + origination_fee_network,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: origination_fee_network,
				}
			);

			// The account has no loans and no collateral, so it should have been removed:
			assert!(!LoanAccounts::<Test>::contains_key(BORROWER));
		});
}

#[test]
fn small_interest_amounts_accumulate() {
	const PRINCIPAL: AssetAmount = 10_000_000;
	const INIT_POOL_AMOUNT: AssetAmount = PRINCIPAL * 10;
	const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE;

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

	// Expecting regular intervals:
	let interest_payment_interval = config.interest_payment_interval_blocks;

	const ORIGINATION_FEE: AssetAmount = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL);

	let (origination_fee_network, origination_fee_pool) = take_network_fee(ORIGINATION_FEE);

	let utilisation = Permill::from_rational(
		PRINCIPAL + ORIGINATION_FEE,
		INIT_POOL_AMOUNT + origination_fee_pool,
	);

	let pool_interest = config.derive_base_interest_rate_per_payment_interval(
		LOAN_ASSET,
		utilisation,
		interest_payment_interval,
	);

	let network_interest =
		config.derive_network_interest_rate_per_payment_interval(interest_payment_interval);

	// Expected fees in pool's asset
	let pool_amount =
		ScaledAmountHP::from_asset_amount(PRINCIPAL + ORIGINATION_FEE) * pool_interest;
	let network_amount =
		ScaledAmountHP::from_asset_amount(PRINCIPAL + ORIGINATION_FEE) * network_interest;

	// Making sure the fees are non-zero fractions below 1:
	assert_eq!(pool_amount.into_asset_amount(), 0);
	assert_eq!(network_amount.into_asset_amount(), 0);
	assert!(pool_amount.as_raw() > 0);
	assert!(network_amount.as_raw() > 0);

	new_test_ext()
		.execute_with(|| {
			setup_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

			LendingConfig::<Test>::set(config.clone());

			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
			set_asset_price_in_usd(COLLATERAL_ASSET, 1);

			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

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
		.then_process_blocks_until_block(INIT_BLOCK + interest_payment_interval as u64)
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
		.then_process_blocks_until_block(INIT_BLOCK + 2 * interest_payment_interval as u64)
		.then_execute_with(|_| {
			let account = LoanAccounts::<Test>::get(BORROWER).unwrap();

			let mut pool_amount_total = pool_amount.saturating_add(pool_amount);
			let mut network_amount_total = network_amount.saturating_add(network_amount);

			let pool_amount_taken = pool_amount_total.take_non_fractional_part();
			let network_amount_taken = network_amount_total.take_non_fractional_part();

			// Over two interest payment periods both amounts are expected to become non-zero
			assert!(pool_amount_taken > 0);
			assert!(network_amount_taken > 0);

			let pool = GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap();

			assert_eq!(
				pool.total_amount,
				INIT_POOL_AMOUNT + origination_fee_pool + pool_amount_taken
			);

			assert_eq!(
				PendingNetworkFees::<Test>::get(LOAN_ASSET),
				origination_fee_network + network_amount_taken
			);

			let loan = &account.loans[&LOAN_ID];

			assert_eq!(
				loan.pending_interest,
				InterestBreakdown {
					network: network_amount_total,
					pool: pool_amount_total,
					broker: Default::default(),
					low_ltv_penalty: Default::default()
				}
			);

			assert_eq!(
				loan.owed_principal,
				PRINCIPAL + ORIGINATION_FEE + network_amount_taken + pool_amount_taken
			);
		});
}

#[test]
fn reconciling_interest_before_settling_loan() {
	// This test makes sure that we collect any pending interest before settling a loan.

	let interest_payment_interval = CONFIG.interest_payment_interval_blocks;

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.then_process_blocks_until_block(INIT_BLOCK + interest_payment_interval as u64)
		.then_execute_with(|_| {
			// Pending interest has been recorded, but not yet taken accounted for in the loans
			// principal
			const TOTAL_AMOUNT_OWED: AssetAmount = PRINCIPAL + ORIGINATION_FEE;

			let account = LoanAccounts::<Test>::get(BORROWER).unwrap();
			assert_eq!(account.loans[&LOAN_ID].owed_principal, TOTAL_AMOUNT_OWED);

			let (_, origination_fee_pool) = take_network_fee(ORIGINATION_FEE);

			let pool_interest_scaled = {
				let utilisation = Permill::from_rational(
					TOTAL_AMOUNT_OWED,
					INIT_POOL_AMOUNT + origination_fee_pool,
				);

				let pool_interest_rate = CONFIG.derive_base_interest_rate_per_payment_interval(
					LOAN_ASSET,
					utilisation,
					interest_payment_interval,
				);

				ScaledAmountHP::from_asset_amount(TOTAL_AMOUNT_OWED) * pool_interest_rate
			};

			let network_interest_scaled = {
				let network_interest_rate = CONFIG
					.derive_network_interest_rate_per_payment_interval(interest_payment_interval);
				ScaledAmountHP::from_asset_amount(TOTAL_AMOUNT_OWED) * network_interest_rate
			};

			assert_eq!(
				account.loans[&LOAN_ID].pending_interest,
				InterestBreakdown {
					network: network_interest_scaled,
					pool: pool_interest_scaled,
					broker: Default::default(),
					low_ltv_penalty: Default::default()
				}
			);

			// Repaying the loan should result in collection of all pending interest
			let pool_interest = pool_interest_scaled.into_asset_amount();
			let network_interest = network_interest_scaled.into_asset_amount();

			let total_amount_to_repay = TOTAL_AMOUNT_OWED + pool_interest + network_interest;

			// The test is only effective if we have non-fractional fees to collect
			assert!(pool_interest > 0 && network_interest > 0, "interest amounts must be non-zero");

			MockBalance::credit_account(&BORROWER, LOAN_ASSET, total_amount_to_repay);
			assert_ok!(LendingPools::try_making_repayment(
				&BORROWER,
				LOAN_ID,
				RepaymentAmount::Exact(total_amount_to_repay)
			));

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::InterestTaken { .. }),
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount,
				}) if amount == total_amount_to_repay,
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled { .. })
			);

			// Checking the actual values separately to avoid using the awkward matching syntax:
			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::InterestTaken {
				loan_id: LOAN_ID,
				pool_interest,
				network_interest,
				broker_interest: 0,
				low_ltv_penalty: 0,
			}));
		});
}

#[test]
fn making_loan_repayment() {
	const FIRST_REPAYMENT: AssetAmount = PRINCIPAL / 4;

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.execute_with(|| {

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

			// Make a partial repayment:
			assert_ok!(Pallet::<Test>::make_repayment(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				RepaymentAmount::Exact(FIRST_REPAYMENT)
			));

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().loans[&LOAN_ID].owed_principal,
				PRINCIPAL - FIRST_REPAYMENT + ORIGINATION_FEE
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

			MockBalance::credit_account(&BORROWER, LOAN_ASSET, ORIGINATION_FEE);

			// Repay the remaining principal:
			assert_ok!(Pallet::<Test>::make_repayment(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				RepaymentAmount::Full
			));

			assert_eq!(LoanAccounts::<Test>::get(BORROWER).unwrap().loans, Default::default());
			// Note that collateral isn't automatically released upon repayment:
			assert_eq!(LoanAccounts::<Test>::get(BORROWER).unwrap().collateral, BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]));
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), 0);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount,
				}) if amount == PRINCIPAL - FIRST_REPAYMENT + ORIGINATION_FEE,
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal: 0,
					via_liquidation: false,
				})
			);

	});
}

#[test]
fn repaying_more_than_necessary() {
	// Testing that if the user repays more than the total owed amount,
	// the excess amount will go back to their free balance.

	const EXTRA_AMOUNT: AssetAmount = PRINCIPAL / 10;

	let (_origination_fee_network, origination_fee_pool) = take_network_fee(ORIGINATION_FEE);

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.then_execute_at_next_block(|_| {
			MockBalance::credit_account(&BORROWER, LOAN_ASSET, EXTRA_AMOUNT);

			assert_ok!(Pallet::<Test>::make_repayment(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				RepaymentAmount::Exact(PRINCIPAL + EXTRA_AMOUNT)
			));

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount,
				}) if amount == PRINCIPAL + ORIGINATION_FEE,
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal: 0,
					via_liquidation: false,
				})
			);

			// Excess amount is returned:
			assert_eq!(
				MockBalance::get_balance(&BORROWER, LOAN_ASSET),
				EXTRA_AMOUNT - ORIGINATION_FEE
			);
			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);

			assert_eq!(LoanAccounts::<Test>::get(BORROWER).unwrap().loans, Default::default());

			// Check that excess amount isn't erroneously added to collateral or the pool:
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().collateral,
				BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + origination_fee_pool,
					available_amount: INIT_POOL_AMOUNT + origination_fee_pool,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);
		});
}

#[test]
fn borrowing_disallowed_during_liquidation() {
	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.execute_with(|| {
			// Force liquidation
			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE * 2);
		})
		.then_execute_at_next_block(|_| {
			assert_matches!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
				LiquidationStatus::Liquidating { .. }
			);

			assert_noop!(
				LendingPools::new_loan(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					Some(COLLATERAL_ASSET),
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
				),
				Error::<Test>::LiquidationInProgress
			);

			assert_noop!(
				<LendingPools as LendingApi>::expand_loan(
					BORROWER,
					LOAN_ID,
					PRINCIPAL,
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
				),
				Error::<Test>::LiquidationInProgress
			);
		});
}

#[test]
fn updating_primary_collateral_asset() {
	const NEW_PRIMARY_ASSEET: Asset = Asset::Btc;

	assert_ne!(COLLATERAL_ASSET, NEW_PRIMARY_ASSEET);

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		// Must have LP role:
		assert_noop!(
			LendingPools::update_primary_collateral_asset(
				RuntimeOrigin::signed(NON_LP),
				NEW_PRIMARY_ASSEET
			),
			DispatchError::BadOrigin
		);

		// Must alreaady have a loan account:
		assert_noop!(
			LendingPools::update_primary_collateral_asset(
				RuntimeOrigin::signed(BORROWER),
				NEW_PRIMARY_ASSEET
			),
			Error::<Test>::LoanAccountNotFound
		);

		// Should succeed after adding collateral (which implicitly creates an account):
		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

		assert_ok!(LendingPools::add_collateral(
			RuntimeOrigin::signed(BORROWER),
			Some(COLLATERAL_ASSET),
			BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
		));

		assert_ok!(LendingPools::update_primary_collateral_asset(
			RuntimeOrigin::signed(BORROWER),
			NEW_PRIMARY_ASSEET
		));

		assert_has_event::<Test>(RuntimeEvent::LendingPools(
			Event::<Test>::PrimaryCollateralAssetUpdated {
				borrower_id: BORROWER,
				primary_collateral_asset: NEW_PRIMARY_ASSEET,
			},
		));
	});
}

#[test]
fn network_fees_under_full_utilisation() {
	// Here we test we correctly record how much is owed to the network
	// so that if utilisation is 100% and it is not possible for the
	// network to take earnings from the pool, we can still collect
	// the full owed amount when some pool funds become available.

	// The loan will request all available funds
	const PRINCIPAL: AssetAmount = INIT_POOL_AMOUNT;
	const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE; // 75% LTV

	// Additional funds that will be added to the pool later:
	const EXTRA_POOL_AMOUNT: AssetAmount = INIT_POOL_AMOUNT / 2;

	const ORIGINATION_FEE: AssetAmount = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL);

	let (origination_fee_network, origination_fee_pool) = take_network_fee(ORIGINATION_FEE);

	let mut interest = Interest::new();

	let utilisation_1 = Permill::from_rational(
		PRINCIPAL + ORIGINATION_FEE,
		INIT_POOL_AMOUNT + origination_fee_pool,
	);

	// Confirming that utilisation is 100% (in fact it is technically slightly higher due network
	// fee depth, but it will be clamped at 100%):
	assert_eq!(utilisation_1, Permill::from_percent(100));

	// First interest payment:
	interest.accrue_interest(
		PRINCIPAL + ORIGINATION_FEE,
		utilisation_1,
		CONFIG.interest_payment_interval_blocks,
	);

	let (pool_interest_1, network_interest_1) = interest.collect();

	let utilisation_2 = Permill::from_rational(
		PRINCIPAL + ORIGINATION_FEE + pool_interest_1 + network_interest_1,
		INIT_POOL_AMOUNT + EXTRA_POOL_AMOUNT + origination_fee_pool + pool_interest_1,
	);

	// Second interest payment:
	interest.accrue_interest(
		PRINCIPAL + ORIGINATION_FEE + pool_interest_1 + network_interest_1,
		utilisation_2,
		CONFIG.interest_payment_interval_blocks,
	);

	let (pool_interest_2, network_interest_2) = interest.collect();

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.execute_with(|| {
			// Set a low collection threshold so we can more easily check the collected amount
			assert_ok!(Pallet::<Test>::update_pallet_config(
				RuntimeOrigin::root(),
				bounded_vec![PalletConfigUpdate::SetInterestCollectionThresholdUsd(1)],
			));

			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

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

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + origination_fee_pool,
					available_amount: 0,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: origination_fee_network,
				}
			);
		})
		.then_process_blocks_until_block(
			INIT_BLOCK + CONFIG.interest_payment_interval_blocks as u64,
		)
		.then_execute_with(|_| {
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + origination_fee_pool + pool_interest_1,
					available_amount: 0,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: origination_fee_network + network_interest_1,
				}
			);

			// LP Adds additional funds, they will (partially) be used to repay the network
			// upon next interest collection
			MockBalance::credit_account(&LENDER, LOAN_ASSET, EXTRA_POOL_AMOUNT);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				EXTRA_POOL_AMOUNT
			));

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT +
						EXTRA_POOL_AMOUNT + origination_fee_pool +
						pool_interest_1,
					available_amount: EXTRA_POOL_AMOUNT,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: origination_fee_network + network_interest_1,
				}
			);

			// Note that we expose `network_interest_1` in the event despite it not techincally
			// accessible to the network yet
			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::InterestTaken {
				loan_id: LOAN_ID,
				pool_interest: pool_interest_1,
				network_interest: network_interest_1,
				broker_interest: 0,
				low_ltv_penalty: 0,
			}));
		})
		.then_process_blocks_until_block(
			INIT_BLOCK + 2 * CONFIG.interest_payment_interval_blocks as u64,
		)
		.then_execute_with(|_| {
			// Expecting the second interest charge + fees payed to the network to be repaid in
			// full.
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT +
						EXTRA_POOL_AMOUNT + origination_fee_pool +
						pool_interest_1 + pool_interest_2,
					available_amount: EXTRA_POOL_AMOUNT -
						origination_fee_network -
						network_interest_1 - network_interest_2,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::InterestTaken {
				loan_id: LOAN_ID,
				pool_interest: pool_interest_2,
				network_interest: network_interest_2,
				broker_interest: 0,
				low_ltv_penalty: 0,
			}));
		});
}

#[test]
fn removing_collateral_disallowed_during_liquidation() {
	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.execute_with(|| {
			// Force liquidation
			set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE * 2);
		})
		.then_execute_at_next_block(|_| {
			assert_matches!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
				LiquidationStatus::Liquidating { .. }
			);

			assert_noop!(
				<LendingPools as LendingApi>::remove_collateral(
					&BORROWER,
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
				),
				Error::<Test>::LiquidationInProgress
			);
		});
}

#[test]
fn adding_collateral_during_liquidation() {
	const EXTRA_COLLATERAL: AssetAmount = INIT_COLLATERAL / 10;
	const EXTRA_COLLATERAL_2: AssetAmount = 6 * INIT_COLLATERAL / 10;
	const EXTRA_COLLATERAL_3: AssetAmount = INIT_COLLATERAL / 2;

	const NEW_SWAP_RATE: u128 = SWAP_RATE * 2;

	let ltv_at_liquidation =
		FixedU64::from_rational((PRINCIPAL + ORIGINATION_FEE) * NEW_SWAP_RATE, INIT_COLLATERAL);

	let get_account = || LoanAccounts::<Test>::get(BORROWER).unwrap();

	let fund_account_and_add_collateral = |amount| {
		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, amount);

		assert_ok!(<LendingPools as LendingApi>::add_collateral(
			&BORROWER,
			None,
			BTreeMap::from([(COLLATERAL_ASSET, amount)]),
		));
	};

	let swap_request = |input_amount| MockSwapRequest {
		input_asset: COLLATERAL_ASSET,
		output_asset: LOAN_ASSET,
		input_amount,
		remaining_input_amount: input_amount,
		accumulated_output_amount: 0,
		swap_type: SwapRequestType::Regular {
			output_action: SwapOutputAction::CreditLendingPool {
				swap_type: LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
			},
		},
		broker_fees: Default::default(),
		origin: SwapOrigin::Internal,
	};

	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);
	const LIQUIDATION_SWAP_2: SwapRequestId = SwapRequestId(1);

	// Some small amount of collateral will be swapped during hard liquidation to increase test
	// coverage:
	const SWAPPED_COLLATERAL_1: AssetAmount = INIT_COLLATERAL / 1000;
	const RECOVERED_PRINCIPAL_1: AssetAmount = SWAPPED_COLLATERAL_1 / NEW_SWAP_RATE;

	// Ditto for soft liquidation:
	const SWAPPED_COLLATERAL_2: AssetAmount = INIT_COLLATERAL / 2000;
	const RECOVERED_PRINCIPAL_2: AssetAmount = SWAPPED_COLLATERAL_2 / NEW_SWAP_RATE;

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.execute_with(|| {
			// Disable liquidation fee as it is not the focus of this test:
			assert_ok!(Pallet::<Test>::update_pallet_config(
				RuntimeOrigin::root(),
				bounded_vec![PalletConfigUpdate::SetLendingPoolConfiguration {
					asset: None,
					config: Some(LendingPoolConfiguration {
						liquidation_fee: Permill::zero(),
						..CONFIG.default_pool_config
					}),
				}],
			));

			// Force liquidation
			set_asset_price_in_usd(LOAN_ASSET, NEW_SWAP_RATE);

			assert_eq!(get_account().derive_ltv().unwrap(), ltv_at_liquidation);
		})
		.then_execute_at_next_block(|_| {
			assert_matches!(
				get_account().liquidation_status,
				LiquidationStatus::Liquidating { liquidation_type: LiquidationType::Hard, .. }
			);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationInitiated {
					borrower_id: BORROWER,
					ref swaps,
					liquidation_type: LiquidationType::Hard,
				}) if swaps == &BTreeMap::from([(LOAN_ID, vec![LIQUIDATION_SWAP_1])])
			);

			assert_eq!(
				MockSwapRequestHandler::<Test>::get_swap_requests(),
				BTreeMap::from([(LIQUIDATION_SWAP_1, swap_request(INIT_COLLATERAL))])
			);

			// Adding a small amount will improve LTV, but not enough to change liquidation
			// status.
			fund_account_and_add_collateral(EXTRA_COLLATERAL);

			// We don't bother restarting liquidation swaps to incorporate the extra collateral,
			// instead the collateral is simply added to the collateral balance:
			assert_eq!(
				get_account().collateral,
				BTreeMap::from([(COLLATERAL_ASSET, EXTRA_COLLATERAL)])
			);

			// The extra collateral does reduce LTV however:
			assert!(get_account().derive_ltv().unwrap() < ltv_at_liquidation);

			// Simulate partial liquidation:
			MockSwapRequestHandler::<Test>::set_swap_request_progress(
				LIQUIDATION_SWAP_1,
				SwapExecutionProgress {
					remaining_input_amount: INIT_COLLATERAL - SWAPPED_COLLATERAL_1,
					accumulated_output_amount: RECOVERED_PRINCIPAL_1,
				},
			);
		})
		.then_execute_at_next_block(|_| {
			assert_matches!(
				get_account().liquidation_status,
				LiquidationStatus::Liquidating { liquidation_type: LiquidationType::Hard, .. }
			);

			// Adding more collateral is expected to result in a transition from hard
			// liquidation to soft liquidation.
			fund_account_and_add_collateral(EXTRA_COLLATERAL_2);

			assert!(
				get_account().derive_ltv().unwrap() < CONFIG.ltv_thresholds.hard_liquidation.into()
			);
		})
		.then_execute_at_next_block(|_| {
			assert_matches!(
				get_account().liquidation_status,
				LiquidationStatus::Liquidating { liquidation_type: LiquidationType::Soft, .. }
			);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
					borrower_id: BORROWER,
					reason: LiquidationCompletionReason::LtvChange,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount: RECOVERED_PRINCIPAL_1
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationInitiated {
					borrower_id: BORROWER,
					ref swaps,
					liquidation_type: LiquidationType::Soft,
				}) if swaps == &BTreeMap::from([(LOAN_ID, vec![LIQUIDATION_SWAP_2])])
			);

			// All added collateral less what has been swapped in the first swap
			const INPUT_AMOUNT: AssetAmount =
				INIT_COLLATERAL + EXTRA_COLLATERAL + EXTRA_COLLATERAL_2 - SWAPPED_COLLATERAL_1;

			// This time the extra collateral does get included in the swap:
			assert_eq!(
				MockSwapRequestHandler::<Test>::get_swap_requests(),
				BTreeMap::from([(LIQUIDATION_SWAP_2, swap_request(INPUT_AMOUNT))])
			);

			assert_eq!(get_account().collateral, BTreeMap::default());

			// Adding collateral once more should result in a transition from
			// soft liquidation to a healthy loan:
			fund_account_and_add_collateral(EXTRA_COLLATERAL_3);

			assert!(get_account().derive_ltv().unwrap() < CONFIG.ltv_thresholds.target.into());

			// Simulate partial liquidation:
			MockSwapRequestHandler::<Test>::set_swap_request_progress(
				LIQUIDATION_SWAP_2,
				SwapExecutionProgress {
					remaining_input_amount: INPUT_AMOUNT - SWAPPED_COLLATERAL_2,
					accumulated_output_amount: RECOVERED_PRINCIPAL_2,
				},
			);
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(
				get_account(),
				LoanAccount {
					borrower_id: BORROWER,
					primary_collateral_asset: COLLATERAL_ASSET,
					collateral: BTreeMap::from([(
						COLLATERAL_ASSET,
						INIT_COLLATERAL +
							EXTRA_COLLATERAL + EXTRA_COLLATERAL_2 +
							EXTRA_COLLATERAL_3 - SWAPPED_COLLATERAL_1 -
							SWAPPED_COLLATERAL_2
					)]),
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							id: LOAN_ID,
							asset: LOAN_ASSET,
							last_interest_payment_at: INIT_BLOCK,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL + ORIGINATION_FEE -
								RECOVERED_PRINCIPAL_1 - RECOVERED_PRINCIPAL_2,
							pending_interest: InterestBreakdown {
								network: 0.into(),
								pool: 0.into(),
								broker: 0.into(),
								low_ltv_penalty: 0.into()
							}
						}
					)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false
				}
			);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
					borrower_id: BORROWER,
					reason: LiquidationCompletionReason::LtvChange,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount: RECOVERED_PRINCIPAL_2
				}),
			);
		});
}

mod voluntary_liquidation {

	use super::*;

	fn mock_liquidation_swap(input_amount: AssetAmount) -> MockSwapRequest {
		MockSwapRequest {
			input_asset: COLLATERAL_ASSET,
			output_asset: LOAN_ASSET,
			input_amount,
			remaining_input_amount: input_amount,
			accumulated_output_amount: 0,
			swap_type: SwapRequestType::Regular {
				output_action: SwapOutputAction::CreditLendingPool {
					swap_type: LendingSwapType::Liquidation {
						borrower_id: BORROWER,
						loan_id: LOAN_ID,
					},
				},
			},
			broker_fees: Default::default(),
			origin: SwapOrigin::Internal,
		}
	}

	#[test]
	fn voluntary_liquidation_happy_path() {
		const LIQUIDATION_SWAP: SwapRequestId = SwapRequestId(0);

		const SWAPPED_COLLATERAL: AssetAmount = 4 * INIT_COLLATERAL / 5;
		const SWAPPED_PRINCIPAL: AssetAmount = SWAPPED_COLLATERAL / SWAP_RATE;

		const TOTAL_TO_REPAY: AssetAmount = PRINCIPAL + ORIGINATION_FEE;

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.with_default_loan()
			.with_voluntary_liquidation()
			.then_execute_with(|_| {
				// Simulate partial execution of the liquidaiton swap. This should be
				// sufficient to fully repay the loan.
				MockSwapRequestHandler::<Test>::set_swap_request_progress(
					LIQUIDATION_SWAP,
					SwapExecutionProgress {
						remaining_input_amount: INIT_COLLATERAL - SWAPPED_COLLATERAL,
						accumulated_output_amount: SWAPPED_PRINCIPAL,
					},
				);
			})
			.then_execute_at_next_block(|_| {
				const EXCESS_PRINCIPAL: AssetAmount = SWAPPED_PRINCIPAL - TOTAL_TO_REPAY;

				assert_eq!(
					LoanAccounts::<Test>::get(BORROWER).unwrap(),
					LoanAccount {
						borrower_id: BORROWER,
						primary_collateral_asset: COLLATERAL_ASSET,
						collateral: BTreeMap::from([
							(COLLATERAL_ASSET, INIT_COLLATERAL - SWAPPED_COLLATERAL),
							(LOAN_ASSET, EXCESS_PRINCIPAL)
						]),
						loans: Default::default(),
						liquidation_status: LiquidationStatus::NoLiquidation,
						// The flag has been reset:
						voluntary_liquidation_requested: false
					}
				);

				assert_event_sequence!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
						borrower_id: BORROWER,
						reason: LiquidationCompletionReason::FullySwapped,
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
						loan_id: LOAN_ID,
						amount: TOTAL_TO_REPAY,
					}),
					RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
						borrower_id: BORROWER,
						ref collateral,
					}) if collateral == &BTreeMap::from([(LOAN_ASSET, EXCESS_PRINCIPAL)]),
					RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
						loan_id: LOAN_ID,
						outstanding_principal: 0,
						via_liquidation: true,
					}),
				);

				// No liquidation fee should be taken (thus no event):
				assert_matching_event_count!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken { .. }) => 0
				);
			});
	}

	#[test]
	fn voluntary_liquidation_stopped_manually() {
		const LIQUIDATION_SWAP: SwapRequestId = SwapRequestId(0);

		const SWAPPED_COLLATERAL: AssetAmount = INIT_COLLATERAL / 5;
		const SWAPPED_PRINCIPAL: AssetAmount = SWAPPED_COLLATERAL / SWAP_RATE;

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.with_default_loan()
			.with_voluntary_liquidation()
			.then_execute_with(|_| {
				System::reset_events();

				// Simulate partial execution of the liquidaiton swap.
				MockSwapRequestHandler::<Test>::set_swap_request_progress(
					LIQUIDATION_SWAP,
					SwapExecutionProgress {
						remaining_input_amount: INIT_COLLATERAL - SWAPPED_COLLATERAL,
						accumulated_output_amount: SWAPPED_PRINCIPAL,
					},
				);

				// The user manually aborts the liquidation:
				assert_ok!(LendingPools::stop_voluntary_liquidation(RuntimeOrigin::signed(
					BORROWER
				)));
			})
			.then_execute_at_next_block(|_| {
				assert_eq!(
					LoanAccounts::<Test>::get(BORROWER).unwrap(),
					LoanAccount {
						borrower_id: BORROWER,
						primary_collateral_asset: COLLATERAL_ASSET,
						// Part of collateral was used in liquidation:
						collateral: BTreeMap::from([(
							COLLATERAL_ASSET,
							INIT_COLLATERAL - SWAPPED_COLLATERAL
						),]),
						// The loan is partially repaid:
						loans: BTreeMap::from([(
							LOAN_ID,
							GeneralLoan {
								id: LOAN_ID,
								asset: LOAN_ASSET,
								last_interest_payment_at: INIT_BLOCK,
								created_at_block: INIT_BLOCK,
								owed_principal: PRINCIPAL + ORIGINATION_FEE - SWAPPED_PRINCIPAL,
								pending_interest: Default::default()
							}
						)]),
						liquidation_status: LiquidationStatus::NoLiquidation,
						// The flag has been reset:
						voluntary_liquidation_requested: false
					}
				);

				assert_event_sequence!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
						borrower_id: BORROWER,
						reason: LiquidationCompletionReason::ManualAbort,
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
						loan_id: LOAN_ID,
						amount: SWAPPED_PRINCIPAL,
					}),
				);

				// No liquidation fee should be taken (thus no event):
				assert_matching_event_count!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken { .. }) => 0
				);
			});
	}

	#[test]
	fn voluntary_liquidation_with_escalation_and_deescalation() {
		const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);
		const LIQUIDATION_SWAP_2: SwapRequestId = SwapRequestId(1);
		const LIQUIDATION_SWAP_3: SwapRequestId = SwapRequestId(2);

		const SWAPPED_COLLATERAL_1: AssetAmount = INIT_COLLATERAL / 10;
		const SWAPPED_PRINCIPAL_1: AssetAmount = SWAPPED_COLLATERAL_1 / SWAP_RATE;

		const NEW_SWAP_RATE: u128 = 13 * SWAP_RATE / 10;

		const SWAPPED_COLLATERAL_2: AssetAmount = INIT_COLLATERAL / 10;
		const SWAPPED_PRINCIPAL_2: AssetAmount = SWAPPED_COLLATERAL_2 / NEW_SWAP_RATE;

		let liquidation_fee = CONFIG.liquidation_fee(LOAN_ASSET) * SWAPPED_PRINCIPAL_2;

		let owed_after_liquidation_2 =
			PRINCIPAL + ORIGINATION_FEE - SWAPPED_PRINCIPAL_1 - SWAPPED_PRINCIPAL_2 +
				liquidation_fee;

		// Thrid liquidation will result in this much extra principal (after repaying
		// the loan in full).
		const SWAPPED_PRINCIPAL_EXTRA: AssetAmount = PRINCIPAL / 50;

		let get_account = || LoanAccounts::<Test>::get(BORROWER).unwrap();

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.with_default_loan()
			.with_voluntary_liquidation()
			.then_execute_with(|_| {
				// Simulate partial execution of the liquidaiton swap. This won't be enough
				// to repay the loan yet.
				MockSwapRequestHandler::<Test>::set_swap_request_progress(
					LIQUIDATION_SWAP_1,
					SwapExecutionProgress {
						remaining_input_amount: INIT_COLLATERAL - SWAPPED_COLLATERAL_1,
						accumulated_output_amount: SWAPPED_PRINCIPAL_1,
					},
				);

				// Oracle price change leads to high LTV triggering escalation to soft
				// (forced) liquidation:
				set_asset_price_in_usd(LOAN_ASSET, NEW_SWAP_RATE);
			})
			.then_execute_at_next_block(|_| {
				// Loan has been partially repaid:
				assert_eq!(
					get_account().loans,
					BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							id: LOAN_ID,
							asset: LOAN_ASSET,
							last_interest_payment_at: INIT_BLOCK,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL + ORIGINATION_FEE - SWAPPED_PRINCIPAL_1,
							pending_interest: Default::default()
						}
					)])
				);

				// Transitioned to soft (forced) liquidation via a new swap:
				assert_eq!(
					get_account().liquidation_status,
					LiquidationStatus::Liquidating {
						liquidation_swaps: BTreeMap::from([(
							LIQUIDATION_SWAP_2,
							LiquidationSwap {
								loan_id: LOAN_ID,
								from_asset: COLLATERAL_ASSET,
								to_asset: LOAN_ASSET
							}
						)]),
						liquidation_type: LiquidationType::Soft
					}
				);

				// The flag is still set:
				assert!(get_account().voluntary_liquidation_requested);

				// All of collateral is still in liquidation:
				assert_eq!(get_account().collateral, Default::default());

				assert_eq!(
					MockSwapRequestHandler::<Test>::get_swap_requests(),
					BTreeMap::from([(
						LIQUIDATION_SWAP_2,
						mock_liquidation_swap(INIT_COLLATERAL - SWAPPED_COLLATERAL_1)
					)])
				);

				assert_event_sequence!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
						borrower_id: BORROWER,
						reason: LiquidationCompletionReason::LtvChange,
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
						loan_id: LOAN_ID,
						amount: SWAPPED_PRINCIPAL_1,
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationInitiated {
						borrower_id: BORROWER,
						liquidation_type: LiquidationType::Soft,
						ref swaps
					}) if swaps == &BTreeMap::from([(LOAN_ID, vec![LIQUIDATION_SWAP_2])]),
				);

				// No liquidation fee should be taken (thus no event):
				assert_matching_event_count!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken { .. }) => 0
				);

				// Simulate partial execution of the liquidaiton swap. This won't be enough
				// to repay the loan yet.
				MockSwapRequestHandler::<Test>::set_swap_request_progress(
					LIQUIDATION_SWAP_2,
					SwapExecutionProgress {
						remaining_input_amount: INIT_COLLATERAL -
							SWAPPED_COLLATERAL_1 - SWAPPED_COLLATERAL_2,
						accumulated_output_amount: SWAPPED_PRINCIPAL_2,
					},
				);

				// Updating the price again to trigger "recovery" into voluntary liquidation
				set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
			})
			.then_execute_at_next_block(|_| {
				// Loan has been partially repaid again:
				assert_eq!(
					get_account().loans,
					BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							id: LOAN_ID,
							asset: LOAN_ASSET,
							last_interest_payment_at: INIT_BLOCK,
							created_at_block: INIT_BLOCK,
							owed_principal: owed_after_liquidation_2,
							pending_interest: Default::default()
						}
					)])
				);

				// The flag is still set:
				assert!(get_account().voluntary_liquidation_requested);

				// All of collateral is still in liquidation:
				assert_eq!(get_account().collateral, Default::default());

				// Transitioned back to voluntary liquidation via a new swap:
				assert_eq!(
					get_account().liquidation_status,
					LiquidationStatus::Liquidating {
						liquidation_swaps: BTreeMap::from([(
							LIQUIDATION_SWAP_3,
							LiquidationSwap {
								loan_id: LOAN_ID,
								from_asset: COLLATERAL_ASSET,
								to_asset: LOAN_ASSET
							}
						)]),
						liquidation_type: LiquidationType::SoftVoluntary
					}
				);

				assert_eq!(
					MockSwapRequestHandler::<Test>::get_swap_requests(),
					BTreeMap::from([(
						LIQUIDATION_SWAP_3,
						mock_liquidation_swap(
							INIT_COLLATERAL - SWAPPED_COLLATERAL_1 - SWAPPED_COLLATERAL_2
						)
					)])
				);

				assert_event_sequence!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
						borrower_id: BORROWER,
						reason: LiquidationCompletionReason::LtvChange,
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken { .. }),
					RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
						loan_id: LOAN_ID,
						amount,
					}) if amount == SWAPPED_PRINCIPAL_2 - liquidation_fee,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationInitiated {
						borrower_id: BORROWER,
						liquidation_type: LiquidationType::SoftVoluntary,
						ref swaps
					}) if swaps == &BTreeMap::from([(LOAN_ID, vec![LIQUIDATION_SWAP_3])]),
				);
			})
			.then_execute_at_next_block(|_| {
				// Now the liquidation swap will be executed in full

				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP_3,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
					owed_after_liquidation_2 + SWAPPED_PRINCIPAL_EXTRA,
				);

				assert_eq!(
					LoanAccounts::<Test>::get(BORROWER).unwrap(),
					LoanAccount {
						borrower_id: BORROWER,
						primary_collateral_asset: COLLATERAL_ASSET,
						collateral: BTreeMap::from([(LOAN_ASSET, SWAPPED_PRINCIPAL_EXTRA)]),
						loans: Default::default(),
						liquidation_status: LiquidationStatus::NoLiquidation,
						// The flag has been reset:
						voluntary_liquidation_requested: false
					}
				);

				assert_event_sequence!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
						borrower_id: BORROWER,
						reason: LiquidationCompletionReason::FullySwapped,
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
						loan_id: LOAN_ID,
						amount,
					}) if amount == owed_after_liquidation_2,
					RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
						borrower_id: BORROWER,
						ref collateral,
					}) if collateral == &BTreeMap::from([(LOAN_ASSET, SWAPPED_PRINCIPAL_EXTRA)]),
					RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
						loan_id: LOAN_ID,
						outstanding_principal: 0,
						via_liquidation: true,
					}),
				);

				// No liquidation fee should be taken (thus no event):
				assert_matching_event_count!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken { .. }) => 0
				);
			});
	}

	#[test]
	fn voluntary_liquidation_requires_loans() {
		new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).then_execute_with(|_| {
			// Add collateral to ensure the account	exists:
			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

			assert_ok!(LendingPools::add_collateral(
				RuntimeOrigin::signed(BORROWER),
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL),])
			));

			assert_noop!(
				LendingPools::initiate_voluntary_liquidation(RuntimeOrigin::signed(BORROWER)),
				Error::<Test>::AccountHasNoLoans
			);
		});
	}
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

			// Adding lender funds is disabled for all assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					add_lender_funds: SafeModeSet::Red,
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_add_funds(), Error::<Test>::AddLenderFundsDisabled);
			}

			// Adding lender funds is enabled, but not for the requested asset:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					add_lender_funds: SafeModeSet::Amber(BTreeSet::from([LOAN_ASSET])),
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_add_funds(), Error::<Test>::AddLenderFundsDisabled);
			}

			// Adding lender funds is enabled for the requested asset:
			{
				const OTHER_ASSET: Asset = Asset::Eth;
				assert_ne!(OTHER_ASSET, LOAN_ASSET);
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					add_lender_funds: SafeModeSet::Amber(BTreeSet::from([OTHER_ASSET])),
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

			// Withdrawing is disabled for all assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					withdraw_lender_funds: SafeModeSet::Red,
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_withdraw(), Error::<Test>::RemoveLenderFundsDisabled);
			}

			// Withdrawing is enabled, but not for the requested asset:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					withdraw_lender_funds: SafeModeSet::Amber(BTreeSet::from([LOAN_ASSET])),
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_withdraw(), Error::<Test>::RemoveLenderFundsDisabled);
			}

			// Withdrawing is enabled for the requested asset:
			{
				const OTHER_ASSET: Asset = Asset::Eth;
				assert_ne!(OTHER_ASSET, LOAN_ASSET);
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					withdraw_lender_funds: SafeModeSet::Amber(BTreeSet::from([OTHER_ASSET])),
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
	fn safe_mode_for_creating_loan() {
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

			MockBalance::credit_account(&LENDER, LOAN_ASSET, 2 * INIT_POOL_AMOUNT);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				2 * INIT_POOL_AMOUNT
			));

			MockBalance::credit_account(&LP, COLLATERAL_ASSET, 10 * INIT_COLLATERAL);

			// Borrowing is completely disabled:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					borrowing: SafeModeSet::Red,
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_borrow(), Error::<Test>::LoanCreationDisabled);
			}

			// Borrowing is enabled but, not for the asset that we requested:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					borrowing: SafeModeSet::Amber(BTreeSet::from([LOAN_ASSET])),
					..PalletSafeMode::code_green()
				});

				assert_noop!(try_to_borrow(), Error::<Test>::LoanCreationDisabled);
			}

			{
				// Should be able to borrow once we enable for the requested asset :
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					borrowing: SafeModeSet::Amber(BTreeSet::from([COLLATERAL_ASSET])),
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
		const COLLATERAL_ASSET_1: Asset = COLLATERAL_ASSET;
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
					add_collateral: SafeModeSet::Red,
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

			// Adding collateral is disabled for one of the requested assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					add_collateral: SafeModeSet::Amber(BTreeSet::from([COLLATERAL_ASSET_1])),
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
		const COLLATERAL_ASSET_1: Asset = COLLATERAL_ASSET;
		const COLLATERAL_ASSET_2: Asset = Asset::Usdc;
		const COLLATERAL_AMOUNT: AssetAmount = 2 * PRINCIPAL * SWAP_RATE;

		let try_removing_collateral = || {
			LendingPools::remove_collateral(
				RuntimeOrigin::signed(BORROWER),
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
					remove_collateral: SafeModeSet::Red,
					..PalletSafeMode::code_green()
				});
				assert_noop!(try_removing_collateral(), Error::<Test>::RemovingCollateralDisabled);
			}

			// Removing collateral is disabled for one of the requested assets:
			{
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					remove_collateral: SafeModeSet::Amber(BTreeSet::from([COLLATERAL_ASSET_1])),
					..PalletSafeMode::code_green()
				});
				assert_noop!(try_removing_collateral(), Error::<Test>::RemovingCollateralDisabled);
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
					last_interest_payment_at: 0,
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
					last_interest_payment_at: 0,
					owed_principal: 2000,
					pending_interest: Default::default(),
				},
			),
		]),
		liquidation_status: LiquidationStatus::NoLiquidation,
		voluntary_liquidation_requested: false,
	};

	new_test_ext().execute_with(|| {
		set_asset_price_in_usd(Asset::Eth, 4_000);
		set_asset_price_in_usd(Asset::Btc, 100_000);
		set_asset_price_in_usd(Asset::Sol, 200);
		set_asset_price_in_usd(Asset::Usdc, 1);

		let collateral = loan_account.prepare_collateral_for_liquidation().unwrap();
		loan_account.init_liquidation_swaps(&BORROWER, collateral, LiquidationType::Soft);

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
			liquidation_type: LiquidationType::Soft,
		}));

		assert_eq!(MockSwapRequestHandler::<Test>::get_swap_requests(), expected_swaps);

		assert_eq!(
			loan_account.liquidation_status,
			LiquidationStatus::Liquidating {
				liquidation_type: LiquidationType::Soft,
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

		const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE;

		const ORIGINATION_FEE: AssetAmount = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL);

		const LOAN_ASSET_2: Asset = Asset::Sol;
		const PRINCIPAL_2: AssetAmount = PRINCIPAL * 2;
		const COLLATERAL_ASSET_2: Asset = Asset::Usdc;
		const INIT_COLLATERAL_2: AssetAmount = INIT_COLLATERAL * 2;

		/// This much of borrower 2's collateral will be executed during liquidation
		/// at the time of calling RPC.
		const EXECUTED_COLLATERAL_2: AssetAmount = INIT_COLLATERAL_2 / 4;

		const BORROWER_2: u64 = OTHER_LP;
		const LOAN_ID_2: LoanId = LoanId(1);

		const ORIGINATION_FEE_2: AssetAmount =
			portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL_2);

		/// Price of COLLATERAL_ASSET_2 will be increased to this much to trigger liquidation
		/// of borrower 2's collateral.
		const NEW_SWAP_RATE: u128 = 3 * SWAP_RATE / 2;

		new_test_ext()
			.execute_with(|| {
				setup_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);
				setup_pool_with_funds(LOAN_ASSET_2, INIT_POOL_AMOUNT * 2);

				set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
				set_asset_price_in_usd(COLLATERAL_ASSET, 1);

				set_asset_price_in_usd(LOAN_ASSET_2, SWAP_RATE);
				set_asset_price_in_usd(COLLATERAL_ASSET_2, 1);

				MockBalance::credit_account(
					&BORROWER,
					COLLATERAL_ASSET,
					INIT_COLLATERAL + ORIGINATION_FEE,
				);

				MockBalance::credit_account(
					&BORROWER_2,
					COLLATERAL_ASSET_2,
					INIT_COLLATERAL_2 + ORIGINATION_FEE_2,
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
						ltv_ratio: Some(FixedU64::from_rational(750_075, 1_000_000)),
						collateral: vec![AssetAndAmount {
							asset: COLLATERAL_ASSET,
							amount: INIT_COLLATERAL
						}],
						loans: vec![RpcLoan {
							loan_id: LOAN_ID,
							asset: LOAN_ASSET,
							created_at: INIT_BLOCK as u32,
							principal_amount: PRINCIPAL + ORIGINATION_FEE,
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

				let mut interest = Interest::new();

				let (origination_fee_network_1, origination_fee_pool_1) =
					take_network_fee(ORIGINATION_FEE);

				let utilisation = Permill::from_rational(
					PRINCIPAL + ORIGINATION_FEE,
					INIT_POOL_AMOUNT + origination_fee_pool_1,
				);

				// Only the first loan will pay interest
				interest.accrue_interest(
					PRINCIPAL + ORIGINATION_FEE,
					utilisation,
					CONFIG.interest_payment_interval_blocks,
				);

				let (pool_interest, network_interest) = interest.collect();

				let utilisation_after_interest = Permill::from_rational(
					PRINCIPAL + ORIGINATION_FEE + pool_interest + network_interest,
					INIT_POOL_AMOUNT + origination_fee_pool_1 + pool_interest,
				);

				// Both accounts should be returned since we don't specify any:
				assert_eq!(
					super::rpc::get_loan_accounts::<Test>(None),
					vec![
						RpcLoanAccount {
							account: BORROWER_2,
							primary_collateral_asset: COLLATERAL_ASSET_2,
							ltv_ratio: Some(FixedU64::from_rational(1_173_483_333, 1_000_000_000)),
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
								principal_amount: PRINCIPAL_2 + ORIGINATION_FEE_2 -
									ACCUMULATED_OUTPUT_AMOUNT,
							}],
							liquidation_status: Some(RpcLiquidationStatus {
								liquidation_swaps: vec![RpcLiquidationSwap {
									swap_request_id: SwapRequestId(0),
									loan_id: LOAN_ID_2,
								}],
								liquidation_type: LiquidationType::Hard
							})
						},
						RpcLoanAccount {
							account: BORROWER,
							primary_collateral_asset: COLLATERAL_ASSET,
							// LTV slightly increased due to interest payment:
							ltv_ratio: Some(FixedU64::from_rational(750_075_090, 1_000_000_000)),
							collateral: vec![AssetAndAmount {
								asset: COLLATERAL_ASSET,
								amount: INIT_COLLATERAL
							}],
							loans: vec![RpcLoan {
								loan_id: LOAN_ID,
								asset: LOAN_ASSET,
								created_at: INIT_BLOCK as u32,
								principal_amount: PRINCIPAL +
									ORIGINATION_FEE + pool_interest +
									network_interest,
							}],
							liquidation_status: None
						},
					]
				);

				assert_eq!(
					super::rpc::get_lending_pools::<Test>(Some(LOAN_ASSET)),
					vec![RpcLendingPool {
						asset: LOAN_ASSET,
						total_amount: INIT_POOL_AMOUNT + origination_fee_pool_1 + pool_interest,
						available_amount: INIT_POOL_AMOUNT -
							PRINCIPAL - origination_fee_network_1 -
							network_interest,
						utilisation_rate: utilisation_after_interest,
						current_interest_rate: Permill::from_parts(53_335), // 5.33%
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
			Permill::from_percent(0),
			Permill::from_percent(90),
			Permill::from_percent(2),
			Permill::from_percent(8),
			Permill::from_percent(45),
		),
		Permill::from_parts(49_999) // ~5%
	);

	// Linear segment starts at 90% and ends at 100%
	assert_eq!(
		interpolate_linear_segment(
			Permill::from_percent(90),
			Permill::from_percent(100),
			Permill::from_percent(8),
			Permill::from_percent(50),
			Permill::from_percent(95),
		),
		Permill::from_percent(29)
	);

	// Linear segment from 0% to 100% and zero slope
	assert_eq!(
		interpolate_linear_segment(
			Permill::from_percent(0),
			Permill::from_percent(100),
			Permill::from_percent(5),
			Permill::from_percent(5),
			Permill::from_percent(75),
		),
		Permill::from_percent(5)
	);

	// === Some linear segments with a negative slope ===
	assert_eq!(
		interpolate_linear_segment(
			Permill::from_percent(0),
			Permill::from_percent(50),
			Permill::from_percent(50),
			Permill::from_percent(10),
			Permill::from_percent(25),
		),
		Permill::from_percent(30)
	);

	assert_eq!(
		interpolate_linear_segment(
			Permill::from_percent(0),
			Permill::from_percent(50),
			Permill::from_percent(50),
			Permill::from_percent(10),
			Permill::from_percent(0),
		),
		Permill::from_percent(50)
	);

	assert_eq!(
		interpolate_linear_segment(
			Permill::from_percent(0),
			Permill::from_percent(50),
			Permill::from_percent(50),
			Permill::from_percent(10),
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
		CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(0)),
		Permill::from_percent(2)
	);

	assert_eq!(
		CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(45)),
		Permill::from_parts(49_999) // (2% + 8%) / 2 = 5%
	);

	assert_eq!(
		CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(90)),
		Permill::from_percent(8)
	);

	assert_eq!(
		CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(95)),
		Permill::from_percent(29) // (8% + 50%) / 2 = 29%
	);

	assert_eq!(
		CONFIG.derive_interest_rate_per_year(asset, Permill::from_percent(100)),
		Permill::from_percent(50)
	);
}

#[test]
fn derive_extra_interest_from_low_ltv() {
	assert_eq!(
		CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::zero()),
		Permill::from_percent(1)
	);

	assert_eq!(
		CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(10, 100)),
		Permill::from_parts(8_000) // 0.8%
	);

	assert_eq!(
		CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(25, 100)),
		Permill::from_parts(5_000) // 0.5%
	);

	// Any value above 50% LTV should result in 0% additional interest:
	assert_eq!(
		CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(50, 100)),
		Permill::from_percent(0)
	);

	assert_eq!(
		CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(80, 100)),
		Permill::from_percent(0)
	);

	assert_eq!(
		CONFIG.derive_low_ltv_interest_rate_per_year(FixedU64::from_rational(120, 100)),
		Permill::from_percent(0)
	);
}

#[test]
fn loan_minimum_is_enforced() {
	const MIN_LOAN_AMOUNT_USD: AssetAmount = 1_000;
	const MIN_LOAN_AMOUNT_ASSET: AssetAmount = MIN_LOAN_AMOUNT_USD / SWAP_RATE;
	const COLLATERAL_AMOUNT: AssetAmount = MIN_LOAN_AMOUNT_ASSET * SWAP_RATE * 2;

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
		set_asset_price_in_usd(COLLATERAL_ASSET, 1);

		// Set the minimum loan amount
		LendingConfig::<Test>::set(LendingConfiguration {
			minimum_loan_amount_usd: MIN_LOAN_AMOUNT_USD,
			minimum_update_loan_amount_usd: 0,
			..LendingConfigDefault::get()
		});

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, COLLATERAL_AMOUNT);

		// Should not be able to create a loan below the minimum amount
		assert_noop!(
			LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				MIN_LOAN_AMOUNT_ASSET - 1,
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, COLLATERAL_AMOUNT)])
			),
			Error::<Test>::LoanBelowMinimumAmount
		);

		// A loan equal to or above the minimum amount should be fine
		assert_eq!(
			LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				MIN_LOAN_AMOUNT_ASSET,
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, COLLATERAL_AMOUNT)])
			),
			Ok(LOAN_ID)
		);

		// Now try and repay an amount that would leave the loan below the minimum
		assert_noop!(
			LendingPools::try_making_repayment(&BORROWER, LOAN_ID, RepaymentAmount::Exact(1)),
			Error::<Test>::LoanBelowMinimumAmount,
		);

		// If we expand the loan so a partial repayment would not take it below the minimum,
		assert_eq!(
			LendingPools::expand_loan(RuntimeOrigin::signed(BORROWER), LOAN_ID, 1, BTreeMap::new()),
			Ok(())
		);
		assert_ok!(LendingPools::try_making_repayment(
			&BORROWER,
			LOAN_ID,
			RepaymentAmount::Exact(1)
		));

		// Finally, repay the rest of the loan should be fine, even though 0 is below the minimum
		assert_ok!(LendingPools::try_making_repayment(
			&BORROWER,
			LOAN_ID,
			RepaymentAmount::Exact(MIN_LOAN_AMOUNT_ASSET)
		));
	});
}

#[test]
fn expand_or_repay_loan_minimum_is_enforced() {
	const MIN_LOAN_AMOUNT_USD: AssetAmount = 1_000;
	const MIN_UPDATE_USD: AssetAmount = 500;
	const MIN_UPDATE_AMOUNT_ASSET: AssetAmount = MIN_UPDATE_USD / SWAP_RATE;
	const MIN_LOAN_AMOUNT_ASSET: AssetAmount = MIN_LOAN_AMOUNT_USD / SWAP_RATE;
	const COLLATERAL_ASSET: Asset = Asset::Eth;
	const COLLATERAL_AMOUNT: AssetAmount = MIN_LOAN_AMOUNT_ASSET * SWAP_RATE * 2;

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		set_asset_price_in_usd(LOAN_ASSET, SWAP_RATE);
		set_asset_price_in_usd(COLLATERAL_ASSET, 1);

		// Set the minimum loan amount
		LendingConfig::<Test>::set(LendingConfiguration {
			minimum_loan_amount_usd: MIN_LOAN_AMOUNT_USD,
			minimum_update_loan_amount_usd: MIN_UPDATE_USD,
			..LendingConfigDefault::get()
		});

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, COLLATERAL_AMOUNT);

		// Create a loan, doesn't really matter what amount.
		assert_eq!(
			LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				MIN_LOAN_AMOUNT_ASSET,
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, COLLATERAL_AMOUNT)])
			),
			Ok(LOAN_ID)
		);

		// Should not be able to expand the loan by an amount below the minimum
		assert_noop!(
			LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				MIN_UPDATE_AMOUNT_ASSET - 1,
				BTreeMap::new()
			),
			Error::<Test>::AmountBelowMinimum
		);

		// Expanding by an amount equal to or above the minimum should be fine
		assert_eq!(
			LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				MIN_UPDATE_AMOUNT_ASSET,
				BTreeMap::new()
			),
			Ok(())
		);

		// Should not be able to repay an amount that is below the minimum
		assert_noop!(
			LendingPools::make_repayment(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				RepaymentAmount::Exact(MIN_UPDATE_AMOUNT_ASSET - 1)
			),
			Error::<Test>::AmountBelowMinimum
		);

		// Repaying an amount equal to or above the minimum should be fine
		assert_ok!(LendingPools::make_repayment(
			RuntimeOrigin::signed(BORROWER),
			LOAN_ID,
			RepaymentAmount::Exact(MIN_UPDATE_AMOUNT_ASSET)
		));

		// Set a very high minimum update amount
		LendingConfig::<Test>::set(LendingConfiguration {
			minimum_loan_amount_usd: MIN_LOAN_AMOUNT_USD,
			minimum_update_loan_amount_usd: MIN_LOAN_AMOUNT_ASSET * 1000000,
			..LendingConfigDefault::get()
		});

		// Now make sure we can repay the full amount even though it's below the minimum update
		// amount
		assert_ok!(LendingPools::make_repayment(
			RuntimeOrigin::signed(BORROWER),
			LOAN_ID,
			RepaymentAmount::Exact(MIN_LOAN_AMOUNT_ASSET)
		));
	});
}

#[test]
fn adding_or_removing_collateral_minimum_is_enforced() {
	const MIN_UPDATE_USD: AssetAmount = 6000;
	const MIN_UPDATE_AMOUNT_ASSET: AssetAmount = MIN_UPDATE_USD / SWAP_RATE;
	const EXTRA_COLLATERAL_AMOUNT: AssetAmount = 100;

	const COLLATERAL_ASSET: Asset = Asset::Eth;
	const COLLATERAL_ASSET_2: Asset = Asset::Flip;

	new_test_ext().execute_with(|| {
		set_asset_price_in_usd(COLLATERAL_ASSET, SWAP_RATE);
		set_asset_price_in_usd(COLLATERAL_ASSET_2, 1);
		MockBalance::credit_account(
			&BORROWER,
			COLLATERAL_ASSET,
			MIN_UPDATE_AMOUNT_ASSET + MIN_UPDATE_AMOUNT_ASSET / 2 + EXTRA_COLLATERAL_AMOUNT,
		);
		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET_2, MIN_UPDATE_USD / 2);

		// Set the minimum collateral update amount
		LendingConfig::<Test>::set(LendingConfiguration {
			minimum_update_collateral_amount_usd: MIN_UPDATE_USD,
			..LendingConfigDefault::get()
		});

		// Should not be able to add collateral below the minimum amount
		assert_noop!(
			LendingPools::add_collateral(
				RuntimeOrigin::signed(BORROWER),
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, MIN_UPDATE_AMOUNT_ASSET - 1)])
			),
			Error::<Test>::AmountBelowMinimum
		);

		// Adding an amount equal to or above the minimum amount should be fine
		assert_ok!(LendingPools::add_collateral(
			RuntimeOrigin::signed(BORROWER),
			Some(COLLATERAL_ASSET),
			BTreeMap::from([(COLLATERAL_ASSET, MIN_UPDATE_AMOUNT_ASSET)])
		));

		// As long as the total added is above the minimum, it should be fine
		assert_ok!(LendingPools::add_collateral(
			RuntimeOrigin::signed(BORROWER),
			Some(COLLATERAL_ASSET),
			BTreeMap::from([
				(COLLATERAL_ASSET, MIN_UPDATE_AMOUNT_ASSET / 2 + EXTRA_COLLATERAL_AMOUNT),
				(COLLATERAL_ASSET_2, MIN_UPDATE_USD / 2)
			])
		));

		// Should not be able to remove collateral below the minimum amount
		assert_noop!(
			LendingPools::remove_collateral(
				RuntimeOrigin::signed(BORROWER),
				BTreeMap::from([(COLLATERAL_ASSET, MIN_UPDATE_AMOUNT_ASSET - 1)])
			),
			Error::<Test>::AmountBelowMinimum
		);

		// Removing an amount equal to or above the minimum amount should be fine
		assert_ok!(LendingPools::remove_collateral(
			RuntimeOrigin::signed(BORROWER),
			BTreeMap::from([(COLLATERAL_ASSET, MIN_UPDATE_AMOUNT_ASSET)])
		));

		// Even if its split between multiple assets, as long as the total is above the minimum
		assert_ok!(LendingPools::remove_collateral(
			RuntimeOrigin::signed(BORROWER),
			BTreeMap::from([
				(COLLATERAL_ASSET, MIN_UPDATE_AMOUNT_ASSET / 2),
				(COLLATERAL_ASSET_2, MIN_UPDATE_USD / 2)
			])
		));

		// And if only a small amount is left, it should be fine to remove it all
		let account = LoanAccounts::<Test>::get(BORROWER).unwrap();
		let extra_collateral_amount = account.collateral.get(&COLLATERAL_ASSET).unwrap();
		assert!(
			extra_collateral_amount < &MIN_UPDATE_AMOUNT_ASSET,
			"Left over collateral needs to be below the minimum for test to work."
		);
		assert_ok!(LendingPools::remove_collateral(
			RuntimeOrigin::signed(BORROWER),
			BTreeMap::from([(COLLATERAL_ASSET, *extra_collateral_amount)])
		));
	});
}
