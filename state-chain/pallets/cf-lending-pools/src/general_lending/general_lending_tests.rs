use crate::mocks::*;
use cf_chains::ForeignChain;
use cf_test_utilities::{
	assert_event_sequence, assert_has_event, assert_matching_event_count, assert_no_matching_event,
};
use cf_traits::{
	lending::LendingSystemApi,
	mocks::{
		balance_api::{MockBalance, MockLpRegistration},
		price_feed_api::MockPriceFeedApi,
		swap_request_api::{MockSwapRequest, MockSwapRequestHandler},
	},
	ExpiryBehaviour::NoExpiry,
	SafeMode, SetSafeMode, SwapExecutionProgress,
};
use cf_utilities::assert_matches;

use super::*;
use frame_support::{assert_err, assert_noop, assert_ok, sp_runtime::bounded_vec};

const INIT_BLOCK: u64 = 1;

const LENDER: u64 = BOOSTER_1;
const BORROWER: u64 = LP;

const LOAN_ASSET: Asset = Asset::Btc;
const LOAN_CHAIN: ForeignChain = ForeignChain::Bitcoin;
const COLLATERAL_ASSET: Asset = Asset::Eth;
const PRINCIPAL: AssetAmount = 1_000_000_000;
const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE; // 75% LTV

const LOAN_ID: LoanId = LoanId(0);
const SOFT_SWAP_PRICE_LIMIT: PriceLimitsAndExpiry<u64> = PriceLimitsAndExpiry {
	expiry_behaviour: NoExpiry,
	min_price: Price::zero(),
	max_oracle_price_slippage: Some(50),
};
const HARD_SWAP_PRICE_LIMIT: PriceLimitsAndExpiry<u64> = PriceLimitsAndExpiry {
	expiry_behaviour: NoExpiry,
	min_price: Price::zero(),
	max_oracle_price_slippage: Some(500),
};

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
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
			MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET, 1);
			setup_pool_with_funds(LOAN_ASSET, init_pool_amount);

			ctx
		})
	}

	fn with_default_loan(self) -> Self {
		self.then_execute_with(|ctx| {
			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

			assert_eq!(
				LendingPools::new_loan(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					None,
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

fn disable_whitelist() {
	assert_ok!(LendingPools::update_whitelist(RuntimeOrigin::root(), WhitelistUpdate::SetAllowAll));
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

	disable_whitelist();

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

#[test]
fn lender_basic_adding_and_removing_funds() {
	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		// Test that it is possible to withdraw funds if you are the sole contributor
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
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

			System::reset_events();

			let collateral = BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]);

			assert_eq!(
				LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL, None, collateral.clone(),),
				Ok(LOAN_ID)
			);

			// NOTE: we want LoanCreated event to be emitted before any event
			// referencing it (e.g. OriginationFeeTaken)
			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanCreated {
					loan_id: LOAN_ID,
					borrower_id: BORROWER,
					asset: LOAN_ASSET,
					principal_amount: PRINCIPAL,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
					borrower_id: BORROWER,
					collateral: ref collateral_in_event,
					action_type: CollateralAddedActionType::Manual,
				}) if collateral_in_event == &collateral,
				RuntimeEvent::LendingPools(Event::<Test>::OriginationFeeTaken {
					loan_id: LOAN_ID,
					..
				})
			);
			assert_no_matching_event!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::CollateralTopupAssetUpdated {
					borrower_id: BORROWER,
					collateral_topup_asset: Some(COLLATERAL_ASSET),
				}),
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
					collateral_topup_asset: None,
					collateral: BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false,
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							id: LOAN_ID,
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
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
					collateral_topup_asset: None,
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
fn collateral_auto_topup() {
	const COLLATERAL_TOPUP: AssetAmount = INIT_COLLATERAL / 100;

	// The user deposits this much of collateral asset into their balance at a later point
	const EXTRA_FUNDS: AssetAmount = INIT_COLLATERAL;

	fn get_ltv() -> FixedU64 {
		let price_cache = OraclePriceCache::<Test>::default();
		LoanAccounts::<Test>::get(BORROWER).unwrap().derive_ltv(&price_cache).unwrap()
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
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE * 1_000_000);
			MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET, 1_000_000);
			setup_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

			// Enable auto top-up for this test.
			assert_ok!(Pallet::<Test>::update_pallet_config(
				RuntimeOrigin::root(),
				bounded_vec![PalletConfigUpdate::SetLtvThresholds {
					ltv_thresholds: LtvThresholds {
						topup: Some(Permill::from_percent(85)),
						..CONFIG.ltv_thresholds
					}
				}],
			));

			MockBalance::credit_account(
				&BORROWER,
				COLLATERAL_ASSET,
				INIT_COLLATERAL + COLLATERAL_TOPUP,
			);
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

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
			MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET, 990_000);

			assert_eq!(get_ltv(), FixedU64::from_rational(757_651_515, 1_000_000_000)); // ~76%
		})
		.then_execute_at_next_block(|_| {
			// No change in collateral (no auto top up):
			assert_eq!(get_collateral(), INIT_COLLATERAL);

			// Drop the price further, this time auto-top up should be triggered
			MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET, 870_000);

			assert_eq!(get_ltv(), FixedU64::from_rational(862_155_173, 1_000_000_000)); // ~86%
		})
		.then_execute_at_next_block(|_| {
			// The user only had a small amount in their balance, all of it gets used:
			assert_eq!(get_collateral(), INIT_COLLATERAL + COLLATERAL_TOPUP);
			assert_eq!(get_ltv(), FixedU64::from_rational(853_618_983, 1_000_000_000)); // ~85%
			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);

			// After we give the user more funds, auto-top up should bring CR back to target
			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, EXTRA_FUNDS);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
				borrower_id: BORROWER,
				collateral: BTreeMap::from([(COLLATERAL_ASSET, COLLATERAL_TOPUP)]),
				action_type: CollateralAddedActionType::SystemTopup,
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
		MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
		MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET, 1);
		setup_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);
		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

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
					collateral_topup_asset: Some(COLLATERAL_ASSET),
					collateral: BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							id: LOAN_ID,
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
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

		// Try to borrow more, but this time we don't have enough collateral for an acceptable LTV
		assert_err!(
			LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				EXTRA_PRINCIPAL_2,
				Default::default()
			),
			Error::<Test>::LtvTooHigh
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
					action_type: CollateralAddedActionType::Manual,
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
					collateral_topup_asset: Some(COLLATERAL_ASSET),
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
			MockPriceFeedApi::set_price_usd_fine(ASSET_1, SWAP_RATE);
			MockPriceFeedApi::set_price_usd_fine(ASSET_2, SWAP_RATE);

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
							origin: SwapOrigin::Internal,
							price_limits_and_expiry: None,
							dca_params: None
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
							origin: SwapOrigin::Internal,
							price_limits_and_expiry: None,
							dca_params: None
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
		MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
		MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_1, 1);
		MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_2, 1);
		setup_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

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

		// Can't add more collateral than what's in the user's free balance
		assert_noop!(
			LendingPools::add_collateral(
				RuntimeOrigin::signed(BORROWER),
				None,
				BTreeMap::from([(COLLATERAL_ASSET_1, INIT_COLLATERAL + ORIGINATION_FEE + 1),]),
			),
			DispatchError::Other("Insufficient balance")
		);

		System::reset_events();

		assert_ok!(LendingPools::add_collateral(
			RuntimeOrigin::signed(BORROWER),
			Some(COLLATERAL_ASSET_1),
			collateral.clone(),
		));

		assert_event_sequence!(
			Test,
			RuntimeEvent::LendingPools(Event::<Test>::CollateralTopupAssetUpdated {
				borrower_id: BORROWER,
				collateral_topup_asset: Some(COLLATERAL_ASSET_1),
			}),
			RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
				borrower_id: BORROWER,
				collateral: ref collateral_in_event,
				action_type: CollateralAddedActionType::Manual,
			}) if collateral_in_event == &collateral
		);

		// Adding collateral creates a loan account:
		assert_eq!(
			LoanAccounts::<Test>::get(BORROWER).unwrap(),
			LoanAccount {
				borrower_id: BORROWER,
				collateral_topup_asset: Some(COLLATERAL_ASSET_1),
				collateral: collateral.clone(),
				loans: BTreeMap::default(),
				liquidation_status: LiquidationStatus::NoLiquidation,
				voluntary_liquidation_requested: false,
			}
		);

		// Can't remove more collateral than what's available:
		assert_noop!(
			LendingPools::remove_collateral(
				RuntimeOrigin::signed(BORROWER),
				BTreeMap::from([(COLLATERAL_ASSET_1, INIT_COLLATERAL + 1),]),
			),
			Error::<Test>::InsufficientCollateral
		);

		// Can't remove collateral if oracle prices aren't available:
		{
			MockPriceFeedApi::set_stale(COLLATERAL_ASSET_1, true);
			assert_noop!(
				LendingPools::remove_collateral(
					RuntimeOrigin::signed(BORROWER),
					BTreeMap::from([(COLLATERAL_ASSET_1, INIT_COLLATERAL + 1),]),
				),
				Error::<Test>::OraclePriceUnavailable
			);
			MockPriceFeedApi::set_stale(COLLATERAL_ASSET_1, false);
		}

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
fn cannot_remove_collateral_if_ltv_would_exceed_safe_threshold() {
	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.execute_with(|| {
			assert_noop!(
				LendingPools::remove_collateral(
					RuntimeOrigin::signed(BORROWER),
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL / 2),]),
				),
				Error::<Test>::LtvTooHigh
			);
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
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
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
								loan_id: LOAN_ID,
							}
						}
					},
					broker_fees: Default::default(),
					origin: SwapOrigin::Internal,
					price_limits_and_expiry: Some(SOFT_SWAP_PRICE_LIMIT),
					dca_params: Some(DcaParameters { number_of_chunks: 3, chunk_interval: 1 }),
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
			assert_eq!(loan_account.total_collateral_usd_value(&OraclePriceCache::default()).unwrap(), INIT_COLLATERAL);

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
					collateral_topup_asset: None,
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
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE_2);
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
								loan_id: LOAN_ID,
							},
						}
					},
					broker_fees: Default::default(),
					origin: SwapOrigin::Internal,
					price_limits_and_expiry: Some(HARD_SWAP_PRICE_LIMIT),
					dca_params: Some(DcaParameters { number_of_chunks: 1, chunk_interval: 1 }),
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
					action_type: CollateralAddedActionType::SystemLiquidationExcessAmount {..},
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
					collateral_topup_asset: None,
					collateral: BTreeMap::from([(LOAN_ASSET, excess_principal)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false,
					loans: Default::default(),
				})
			);
		});
}

#[test]
fn soft_liquidation_escalates_to_hard() {
	// This is high enough to trigger soft liquidation
	const NEW_SWAP_RATE: u128 = 24;

	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);
	const LIQUIDATION_SWAP_2: SwapRequestId = SwapRequestId(1);

	// The user get unfavorable swap rate here, which should trigger
	// escalation to hard liquidation:
	const SWAP_1_OUTPUT_AMOUNT: AssetAmount = 45 * PRINCIPAL / 100;
	const SWAP_1_REMAINING_INPUT_AMOUNT: AssetAmount = INIT_COLLATERAL / 2;

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.then_execute_with(|_| {
			// Change oracle price to trigger liquidation
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
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
					liquidation_type: LiquidationType::Soft
				}
			);

			// Process liquidation swap half way through
			MockSwapRequestHandler::<Test>::set_swap_request_progress(
				LIQUIDATION_SWAP_1,
				SwapExecutionProgress {
					remaining_input_amount: SWAP_1_REMAINING_INPUT_AMOUNT,
					accumulated_output_amount: SWAP_1_OUTPUT_AMOUNT,
				},
			);
		})
		.then_execute_at_next_block(|_| {
			// Due to bad swap rate, we escalate into a hard liquidation:
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

			let liquidation_fee_1 = CONFIG.liquidation_fee(LOAN_ASSET) * SWAP_1_OUTPUT_AMOUNT;

			let total_owed_after_swap_1 =
				PRINCIPAL + ORIGINATION_FEE + liquidation_fee_1 - SWAP_1_OUTPUT_AMOUNT;
			let liquidation_fee_2 = CONFIG.liquidation_fee(LOAN_ASSET) * total_owed_after_swap_1;

			// Swap 2 happens to result in exactly the amount we need to repay the loan + fees:
			let swap_2_output_amount = total_owed_after_swap_1 + liquidation_fee_2;

			// The remaining amount is fully executed:
			LendingPools::process_loan_swap_outcome(
				LIQUIDATION_SWAP_2,
				LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
				swap_2_output_amount,
			);

			// The account has been removed (it had no collateral and no loans)
			assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);

			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

			let (origination_fee_network, origination_fee_pool) = take_network_fee(ORIGINATION_FEE);

			let (liquidation_fee_1_network, liquidation_fee_1_pool) =
				take_network_fee(liquidation_fee_1);

			let (liquidation_fee_2_network, liquidation_fee_2_pool) =
				take_network_fee(liquidation_fee_2);

			let total_amount_in_pool = INIT_POOL_AMOUNT +
				origination_fee_pool +
				liquidation_fee_1_pool +
				liquidation_fee_2_pool;

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					// The pool's value has increased by pool's origination fee
					total_amount: total_amount_in_pool,
					// The available amount has been decreased not only by the loan's principal, but
					// also by the network's origination fee (it will be by the borrower repaid at a
					// later point)
					available_amount: total_amount_in_pool,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
					owed_to_network: 0,
				}
			);

			assert_eq!(
				PendingNetworkFees::<Test>::get(LOAN_ASSET),
				origination_fee_network + liquidation_fee_1_network + liquidation_fee_2_network
			);
		});
}

#[test]
fn loans_in_liquidation_pay_interest() {
	const NEW_SWAP_RATE: u128 = SWAP_RATE * 2;
	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.then_execute_with(|_| {
			// Set a low collection threshold so we can more easily check the collected amount
			assert_ok!(Pallet::<Test>::update_pallet_config(
				RuntimeOrigin::root(),
				bounded_vec![PalletConfigUpdate::SetInterestCollectionThresholdUsd(1)],
			));
			// Change oracle price to trigger liquidation
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
		})
		.then_execute_at_next_block(|_| {
			// Make sure that the loan is being liquidated
			assert_matches!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
				LiquidationStatus::Liquidating { .. }
			);
		})
		.then_process_blocks_until_block(
			INIT_BLOCK + CONFIG.interest_payment_interval_blocks as u64,
		)
		.then_execute_with(|_| {
			let (pool_interest, network_interest) = {
				let mut interest = Interest::new();

				let (_origination_fee_network, origination_fee_pool) =
					take_network_fee(ORIGINATION_FEE);

				let utilisation = Permill::from_rational(
					PRINCIPAL + ORIGINATION_FEE,
					INIT_POOL_AMOUNT + origination_fee_pool,
				);

				interest.accrue_interest(
					PRINCIPAL + ORIGINATION_FEE,
					utilisation,
					CONFIG.interest_payment_interval_blocks,
				);
				interest.collect()
			};

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
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
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
					collateral_topup_asset: None,
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
	// isn't enough to cover the total loan amount.

	const RECOVERED_PRINCIPAL: AssetAmount = 3 * PRINCIPAL / 4;

	const NEW_SWAP_RATE: u128 = SWAP_RATE * 2;

	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);

	let (origination_fee_network, origination_fee_pool) = take_network_fee(ORIGINATION_FEE);

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).with_default_loan()
		.execute_with(|| {
			// Change oracle price to trigger liquidation
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
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
	// liquidated and the recovered principal isn't enough to cover the total loan amount. However,
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
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

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
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
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

mod multi_asset_collateral_liquidation {

	use super::*;

	fn get_loan_account() -> LoanAccount<Test> {
		LoanAccounts::<Test>::get(BORROWER).unwrap()
	}

	fn add_second_asset_collateral() {
		// Add collateral in a different asset to trigger multiple liquidation liquidation
		// swap
		MockPriceFeedApi::set_price_usd_fine(OTHER_COLLATERAL_ASSET, 1);

		MockBalance::credit_account(&BORROWER, OTHER_COLLATERAL_ASSET, INIT_COLLATERAL);

		assert_ok!(LendingPools::add_collateral(
			RuntimeOrigin::signed(BORROWER),
			None,
			BTreeMap::from([(OTHER_COLLATERAL_ASSET, OTHER_COLLATERAL_ASSET_AMOUNT)]),
		));
	}

	const OTHER_COLLATERAL_ASSET: Asset = Asset::Usdc;
	const OTHER_COLLATERAL_ASSET_AMOUNT: AssetAmount = INIT_COLLATERAL / 10;

	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);
	const LIQUIDATION_SWAP_2: SwapRequestId = SwapRequestId(1);

	/// Two liquidation swaps: swap 1 executes partially, but does not repay the loan yet;
	/// swap 2 executes fully, which aborts liquidation as together with swap 1 we have enough
	/// funds to fully repay the loan.
	#[test]
	fn one_liquidation_swap_completes_the_other_aborted_due_to_low_ltv() {
		// Swap 1 will be a partial swap
		const SWAP_1_REMAINING_INPUT: AssetAmount = INIT_COLLATERAL / 10;
		const SWAP_1_OUTPUT_AMOUNT: AssetAmount = 82 * PRINCIPAL / 100;

		const TOTAL_OWED: AssetAmount = PRINCIPAL + ORIGINATION_FEE;

		// Swap 2 will result in this much excess amount in loan asset (after taking
		// the output from swap 1 into account).
		const EXCESS_AMOUNT: AssetAmount = PRINCIPAL / 100;

		// Swap 2 will be a full swap
		const SWAP_2_OUTPUT_AMOUNT: AssetAmount = TOTAL_OWED - SWAP_1_OUTPUT_AMOUNT + EXCESS_AMOUNT;

		// This should trigger soft liquidation
		const NEW_SWAP_RATE: u128 = 27;

		let liquidation_fee_1 = CONFIG.liquidation_fee(LOAN_ASSET) * SWAP_2_OUTPUT_AMOUNT;
		let liquidation_fee_2 = CONFIG.liquidation_fee(LOAN_ASSET) *
			(TOTAL_OWED + liquidation_fee_1 - SWAP_2_OUTPUT_AMOUNT);

		let total_liquidation_fee = liquidation_fee_1 + liquidation_fee_2;

		// The intention is to have some amount left after liquidation fees to be added to
		// collateral:
		assert!(EXCESS_AMOUNT > total_liquidation_fee);

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.with_default_loan()
			.execute_with(|| {
				add_second_asset_collateral();
				// Change oracle price to trigger liquidation
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
			})
			.then_execute_at_next_block(|_| {
				assert_eq!(
					get_loan_account().liquidation_status,
					LiquidationStatus::Liquidating {
						liquidation_swaps: BTreeMap::from([
							(
								LIQUIDATION_SWAP_1,
								LiquidationSwap {
									loan_id: LOAN_ID,
									from_asset: COLLATERAL_ASSET,
									to_asset: LOAN_ASSET
								}
							),
							(
								LIQUIDATION_SWAP_2,
								LiquidationSwap {
									loan_id: LOAN_ID,
									from_asset: OTHER_COLLATERAL_ASSET,
									to_asset: LOAN_ASSET
								}
							)
						]),
						liquidation_type: LiquidationType::Soft
					}
				);

				// Swap 1 gets executed partially
				MockSwapRequestHandler::<Test>::set_swap_request_progress(
					LIQUIDATION_SWAP_1,
					SwapExecutionProgress {
						remaining_input_amount: SWAP_1_REMAINING_INPUT,
						accumulated_output_amount: SWAP_1_OUTPUT_AMOUNT,
					},
				);
			})
			.then_execute_at_next_block(|_| {
				// We are still liquidating:
				assert_matches!(
					get_loan_account().liquidation_status,
					LiquidationStatus::Liquidating { .. }
				);

				// Swap 2 executes fully and repays the loan in full
				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP_2,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
					SWAP_2_OUTPUT_AMOUNT,
				);

				// One liquidation swap still remains:
				assert_eq!(
					get_loan_account().liquidation_status,
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
			})
			.then_execute_at_next_block(|_| {
				// The remaining liquidation swap should be aborted here:
				assert_eq!(get_loan_account().liquidation_status, LiquidationStatus::NoLiquidation);

				// The loan has been settled:
				assert_eq!(get_loan_account().loans, Default::default());

				// Collateral from both swaps should be returned to the collateral balance:
				assert_eq!(
					get_loan_account().collateral,
					BTreeMap::from([
						(LOAN_ASSET, EXCESS_AMOUNT - total_liquidation_fee),
						(COLLATERAL_ASSET, SWAP_1_REMAINING_INPUT)
					])
				);
			});
	}

	/// Two liquidation swaps: swap 1 executes fully and covers the loan in full; by the time
	/// swap 2 is aborted the loan is already repaid (though not settled), and all its funds
	/// (remaining input + collected output) to the users collateral. Only then does the loan
	/// get settled.
	#[test]
	fn one_liquidation_swap_completely_covers_the_loan_the_other_aborts() {
		const TOTAL_OWED: AssetAmount = PRINCIPAL + ORIGINATION_FEE;

		let liquidation_fee = CONFIG.liquidation_fee(LOAN_ASSET) * TOTAL_OWED;

		const EXCESS_AMOUNT: AssetAmount = PRINCIPAL / 100;

		// This should trigger soft liquidation
		const NEW_SWAP_RATE: u128 = 27;

		// Swap 1 will be executed fully with some excess amount of the total owed principal
		let swap_1_output_amount = TOTAL_OWED + EXCESS_AMOUNT + liquidation_fee;

		// Swap 2 will only be executed half way through
		const SWAP_2_REMAINING_INPUT_AMOUNT: AssetAmount = OTHER_COLLATERAL_ASSET_AMOUNT / 2;
		const SWAP_2_OUTPUT_AMOUNT: AssetAmount = (OTHER_COLLATERAL_ASSET_AMOUNT / 2) / SWAP_RATE;

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.with_default_loan()
			.execute_with(|| {
				add_second_asset_collateral();
				// Change oracle price to trigger liquidation
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
			})
			.then_execute_at_next_block(|_| {
				// Swap 2 gets executed partially
				MockSwapRequestHandler::<Test>::set_swap_request_progress(
					LIQUIDATION_SWAP_2,
					SwapExecutionProgress {
						remaining_input_amount: SWAP_2_REMAINING_INPUT_AMOUNT,
						accumulated_output_amount: SWAP_2_OUTPUT_AMOUNT,
					},
				);
			})
			.then_execute_at_next_block(|_| {
				// We are still liquidating:
				assert_matches!(
					get_loan_account().liquidation_status,
					LiquidationStatus::Liquidating { .. }
				);

				// Swap 1 executes fully and immediately repays the loan in full
				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP_1,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
					swap_1_output_amount,
				);

				// One liquidation swap still remains:
				assert_eq!(
					get_loan_account().liquidation_status,
					LiquidationStatus::Liquidating {
						liquidation_swaps: BTreeMap::from([(
							LIQUIDATION_SWAP_2,
							LiquidationSwap {
								loan_id: LOAN_ID,
								from_asset: OTHER_COLLATERAL_ASSET,
								to_asset: LOAN_ASSET
							}
						)]),
						liquidation_type: LiquidationType::Soft
					}
				);
			})
			.then_execute_at_next_block(|_| {
				// The remaining liquidation swap should be aborted here:
				assert_eq!(get_loan_account().liquidation_status, LiquidationStatus::NoLiquidation);

				// The loan has been settled:
				assert_eq!(get_loan_account().loans, Default::default());

				// Collateral from both swaps should be returned to the collateral balance:
				assert_eq!(
					get_loan_account().collateral,
					BTreeMap::from([
						(LOAN_ASSET, EXCESS_AMOUNT + SWAP_2_OUTPUT_AMOUNT),
						(OTHER_COLLATERAL_ASSET, SWAP_2_REMAINING_INPUT_AMOUNT)
					])
				);
			});
	}

	#[test]
	fn liquidation_with_multiple_loans() {
		const COLLATERAL_ASSET_2: Asset = Asset::Sol;
		const LOAN_ASSET_2: Asset = Asset::Usdc;
		const INIT_COLLATERAL_2: AssetAmount = INIT_COLLATERAL / 5;
		const PRINCIPAL_2: AssetAmount = PRINCIPAL / 5;
		const LOAN_ID_2: LoanId = LoanId(1);

		const ORIGINATION_FEE_2: AssetAmount =
			portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL_2);

		// This should trigger soft liquidation
		const NEW_SWAP_RATE: u128 = 26;

		const LIQUIDATION_SWAP_3: SwapRequestId = SwapRequestId(2);
		const LIQUIDATION_SWAP_4: SwapRequestId = SwapRequestId(3);

		const SWAP_OUTPUT_LOAN_1_SWAP_1: AssetAmount = 7 * PRINCIPAL / 10;
		const SWAP_OUTPUT_LOAN_1_SWAP_3: AssetAmount = 5 * PRINCIPAL / 10;

		const REMAINING_INPUT_SWAP_2: AssetAmount = INIT_COLLATERAL / 5;
		const REMAINING_INPUT_SWAP_4: AssetAmount = INIT_COLLATERAL_2 / 4;
		const SWAP_OUTPUT_LOAN_2_SWAP_2: AssetAmount = PRINCIPAL_2 / 5;
		const SWAP_OUTPUT_LOAN_2_SWAP_4: AssetAmount = PRINCIPAL_2 / 2;

		new_test_ext()
			.execute_with(|| {
				// Setup pools with funds

				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
				setup_pool_with_funds(LOAN_ASSET, PRINCIPAL * 2);

				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET_2, SWAP_RATE);
				setup_pool_with_funds(LOAN_ASSET_2, PRINCIPAL * 2);

				MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET, 1);
				MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_2, 1);
			})
			.then_execute_with(|_| {
				// Fund borrower account
				MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);
				MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET_2, INIT_COLLATERAL_2);
				MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
				MockLpRegistration::register_refund_address(BORROWER, ForeignChain::Ethereum);

				// Create loans
				assert_eq!(
					LendingPools::new_loan(
						BORROWER,
						LOAN_ASSET,
						PRINCIPAL,
						None,
						BTreeMap::from([
							(COLLATERAL_ASSET, INIT_COLLATERAL),
							(COLLATERAL_ASSET_2, INIT_COLLATERAL_2)
						]),
					),
					Ok(LOAN_ID)
				);

				assert_eq!(
					LendingPools::new_loan(
						BORROWER,
						LOAN_ASSET_2,
						PRINCIPAL_2,
						None,
						Default::default(),
					),
					Ok(LOAN_ID_2)
				);
			})
			.then_execute_with(|_| {
				// Change oracle price to trigger liquidation
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
			})
			.then_execute_at_next_block(|_| {
				// Liquidation swaps are ongoing:
				assert_eq!(
					get_loan_account().liquidation_status,
					LiquidationStatus::Liquidating {
						liquidation_swaps: BTreeMap::from([
							(
								LIQUIDATION_SWAP_1,
								LiquidationSwap {
									loan_id: LOAN_ID,
									from_asset: COLLATERAL_ASSET,
									to_asset: LOAN_ASSET
								}
							),
							(
								LIQUIDATION_SWAP_2,
								LiquidationSwap {
									loan_id: LOAN_ID_2,
									from_asset: COLLATERAL_ASSET,
									to_asset: LOAN_ASSET_2
								}
							),
							(
								LIQUIDATION_SWAP_3,
								LiquidationSwap {
									loan_id: LOAN_ID,
									from_asset: COLLATERAL_ASSET_2,
									to_asset: LOAN_ASSET
								}
							),
							(
								LIQUIDATION_SWAP_4,
								LiquidationSwap {
									loan_id: LOAN_ID_2,
									from_asset: COLLATERAL_ASSET_2,
									to_asset: LOAN_ASSET_2
								}
							)
						]),
						liquidation_type: LiquidationType::Soft
					}
				);

				assert_eq!(get_loan_account().collateral, Default::default());
			})
			.then_execute_with(|_| {
				// Swaps for loan 1 are executed all the way through and fully repay the loan:
				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP_1,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
					SWAP_OUTPUT_LOAN_1_SWAP_1,
				);

				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP_3,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
					SWAP_OUTPUT_LOAN_1_SWAP_3,
				);

				assert_eq!(
					get_loan_account().loans,
					BTreeMap::from([(
						LOAN_ID_2,
						GeneralLoan {
							id: LOAN_ID_2,
							asset: LOAN_ASSET_2,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL_2 + ORIGINATION_FEE_2,
							pending_interest: Default::default()
						}
					)])
				);

				// This covers the full amount repaid by the first swap
				let liquidation_fee_1 =
					CONFIG.liquidation_fee(LOAN_ASSET) * SWAP_OUTPUT_LOAN_1_SWAP_1;

				// This covers the remainder (which now also includes liquidation_fee_1)
				let liquidation_fee_2 = CONFIG.liquidation_fee(LOAN_ASSET) *
					(PRINCIPAL + ORIGINATION_FEE + liquidation_fee_1 - SWAP_OUTPUT_LOAN_1_SWAP_1);

				let excess_loan_asset_amount = SWAP_OUTPUT_LOAN_1_SWAP_1 +
					SWAP_OUTPUT_LOAN_1_SWAP_3 -
					(PRINCIPAL + ORIGINATION_FEE + liquidation_fee_1 + liquidation_fee_2);

				assert_eq!(
					get_loan_account().collateral,
					BTreeMap::from([(LOAN_ASSET, excess_loan_asset_amount)])
				);

				excess_loan_asset_amount
			})
			.then_execute_with_keep_context(|_| {
				// Swaps for loan 2 will only be executed partially, but it will be enough to abort
				// them

				MockSwapRequestHandler::<Test>::set_swap_request_progress(
					LIQUIDATION_SWAP_2,
					SwapExecutionProgress {
						remaining_input_amount: REMAINING_INPUT_SWAP_2,
						accumulated_output_amount: SWAP_OUTPUT_LOAN_2_SWAP_2,
					},
				);

				MockSwapRequestHandler::<Test>::set_swap_request_progress(
					LIQUIDATION_SWAP_4,
					SwapExecutionProgress {
						remaining_input_amount: REMAINING_INPUT_SWAP_4,
						accumulated_output_amount: SWAP_OUTPUT_LOAN_2_SWAP_4,
					},
				);
			})
			.then_execute_at_next_block(|excess_loan_asset_amount| {
				assert_eq!(get_loan_account().liquidation_status, LiquidationStatus::NoLiquidation);

				let liquidation_fee = CONFIG.liquidation_fee(LOAN_ASSET) *
					(SWAP_OUTPUT_LOAN_2_SWAP_2 + SWAP_OUTPUT_LOAN_2_SWAP_4);

				assert_eq!(
					get_loan_account().loans,
					BTreeMap::from([(
						LOAN_ID_2,
						GeneralLoan {
							id: LOAN_ID_2,
							asset: LOAN_ASSET_2,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL_2 + ORIGINATION_FEE_2 + liquidation_fee -
								SWAP_OUTPUT_LOAN_2_SWAP_2 -
								SWAP_OUTPUT_LOAN_2_SWAP_4,
							pending_interest: Default::default()
						}
					)])
				);

				assert_eq!(
					get_loan_account().collateral,
					BTreeMap::from([
						(LOAN_ASSET, excess_loan_asset_amount),
						(COLLATERAL_ASSET, REMAINING_INPUT_SWAP_2),
						(COLLATERAL_ASSET_2, REMAINING_INPUT_SWAP_4)
					])
				);
			});
	}
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
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
			MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET, 1);
			setup_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);

			LendingConfig::<Test>::set(config.clone());

			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

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
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE * 2);
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
fn updating_collateral_topup_asset() {
	const COLLATERAL_TOPUP_ASSET: Asset = Asset::Btc;

	assert_ne!(COLLATERAL_ASSET, COLLATERAL_TOPUP_ASSET);

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		// Must have LP role:
		assert_noop!(
			LendingPools::update_collateral_topup_asset(
				RuntimeOrigin::signed(NON_LP),
				Some(COLLATERAL_TOPUP_ASSET)
			),
			DispatchError::BadOrigin
		);

		// Must already have a loan account:
		assert_noop!(
			LendingPools::update_collateral_topup_asset(
				RuntimeOrigin::signed(BORROWER),
				Some(COLLATERAL_TOPUP_ASSET)
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

		assert_ok!(LendingPools::update_collateral_topup_asset(
			RuntimeOrigin::signed(BORROWER),
			Some(COLLATERAL_TOPUP_ASSET)
		));

		assert_has_event::<Test>(RuntimeEvent::LendingPools(
			Event::<Test>::CollateralTopupAssetUpdated {
				borrower_id: BORROWER,
				collateral_topup_asset: Some(COLLATERAL_TOPUP_ASSET),
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
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

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

			// Note that we expose `network_interest_1` in the event despite it not technically
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
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE * 2);
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

	let swap_request = |input_amount, chunks, is_hard_liquidation| MockSwapRequest {
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
		price_limits_and_expiry: Some(if is_hard_liquidation {
			HARD_SWAP_PRICE_LIMIT
		} else {
			SOFT_SWAP_PRICE_LIMIT
		}),
		dca_params: Some(DcaParameters { number_of_chunks: chunks, chunk_interval: 1 }),
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
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);

			assert_eq!(
				get_account().derive_ltv(&OraclePriceCache::default()).unwrap(),
				ltv_at_liquidation
			);
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
				BTreeMap::from([(LIQUIDATION_SWAP_1, swap_request(INIT_COLLATERAL, 1, true))])
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
			assert!(
				get_account().derive_ltv(&OraclePriceCache::default()).unwrap() <
					ltv_at_liquidation
			);

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
				get_account().derive_ltv(&OraclePriceCache::default()).unwrap() <
					CONFIG.ltv_thresholds.hard_liquidation.into()
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
				BTreeMap::from([(LIQUIDATION_SWAP_2, swap_request(INPUT_AMOUNT, 5, false))])
			);

			assert_eq!(get_account().collateral, BTreeMap::default());

			// Adding collateral once more should result in a transition from
			// soft liquidation to a healthy loan:
			fund_account_and_add_collateral(EXTRA_COLLATERAL_3);

			assert!(
				get_account().derive_ltv(&OraclePriceCache::default()).unwrap() <
					CONFIG.ltv_thresholds.target.into()
			);

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
					collateral_topup_asset: None,
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

/// Loan is fully repaid at the same time as liquidation swap is
/// fully processed. Expecting the loan to be correctly settled (without
/// liquidation fees) and the liquidation swap output should be added
/// to the user's collateral.
#[test]
fn full_loan_repayment_followed_by_full_liquidation() {
	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);
	const NEW_SWAP_RATE: u128 = SWAP_RATE * 2;

	const SWAP_OUTPUT_AMOUNT: AssetAmount = 11 * PRINCIPAL / 10;

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.execute_with(|| {
			// Force liquidation
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

			MockBalance::credit_account(&BORROWER, LOAN_ASSET, ORIGINATION_FEE);

			assert_ok!(LendingPools::try_making_repayment(
				&BORROWER,
				LOAN_ID,
				RepaymentAmount::Full
			));

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), 0);

			// The loan has been fully repaid, but we still have a liquidation swap:
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap(),
				LoanAccount {
					borrower_id: BORROWER,
					collateral_topup_asset: None,
					collateral: Default::default(),
					loans: Default::default(),
					liquidation_status: LiquidationStatus::Liquidating {
						liquidation_swaps: BTreeMap::from([(
							LIQUIDATION_SWAP_1,
							LiquidationSwap {
								loan_id: LOAN_ID,
								from_asset: COLLATERAL_ASSET,
								to_asset: LOAN_ASSET
							}
						)]),
						liquidation_type: LiquidationType::Hard
					},
					voluntary_liquidation_requested: false
				}
			);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
				loan_id: LOAN_ID,
				outstanding_principal: 0,
				via_liquidation: false,
			}));

			// No liquidation fee taken:
			assert_no_matching_event!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken { .. })
			);

			System::reset_events();

			// Swap completes before we get a chance to abort it:
			LendingPools::process_loan_swap_outcome(
				LIQUIDATION_SWAP_1,
				LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
				SWAP_OUTPUT_AMOUNT,
			);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
					borrower_id: BORROWER,
					reason: LiquidationCompletionReason::FullySwapped,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
					borrower_id: BORROWER,
					action_type: CollateralAddedActionType::SystemLiquidationExcessAmount { .. },
					..
				})
			);

			// We should not emit the event the second time:
			assert_no_matching_event!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled { .. })
			);

			// No liquidation fee taken from the swap output (thus no event):
			assert_no_matching_event!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken { .. })
			);

			// All of swap output amount goes towards user's collateral:
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap(),
				LoanAccount {
					borrower_id: BORROWER,
					collateral_topup_asset: None,
					collateral: BTreeMap::from([(LOAN_ASSET, SWAP_OUTPUT_AMOUNT)]),
					loans: Default::default(),
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false
				}
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), 0);
			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
		});
}

/// Loan is fully repaid while liquidation swaps have been partially
/// executed. Expecting the loan to be correctly settled and the liquidation
/// swap output and unspent input should be added to the user's collateral.
/// The way liquidation is implemented (liquidation only repays the loan after
/// it is aborted/completed), we won't charge liquidation fee in this
/// scenario, which should be OK.
#[test]
fn full_loan_repayment_during_partial_liquidation() {
	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);
	const NEW_SWAP_RATE: u128 = SWAP_RATE * 2;

	const EXECUTED_COLLATERAL: AssetAmount = INIT_COLLATERAL / 10;
	const SWAP_OUTPUT_AMOUNT: AssetAmount = PRINCIPAL / 10;

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.execute_with(|| {
			// Force liquidation
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
		})
		.then_execute_at_next_block(|_| {
			assert_matches!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
				LiquidationStatus::Liquidating { liquidation_type: LiquidationType::Hard, .. }
			);

			MockSwapRequestHandler::<Test>::set_swap_request_progress(
				LIQUIDATION_SWAP_1,
				SwapExecutionProgress {
					remaining_input_amount: INIT_COLLATERAL - EXECUTED_COLLATERAL,
					accumulated_output_amount: SWAP_OUTPUT_AMOUNT,
				},
			);
		})
		.then_execute_at_next_block(|_| {
			// Should still be liquidating
			assert_matches!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
				LiquidationStatus::Liquidating { liquidation_type: LiquidationType::Hard, .. }
			);

			// The user fully repays the loan while liquidation swap is being executed:
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);
			MockBalance::credit_account(&BORROWER, LOAN_ASSET, ORIGINATION_FEE);

			assert_ok!(LendingPools::try_making_repayment(
				&BORROWER,
				LOAN_ID,
				RepaymentAmount::Full
			));

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
				loan_id: LOAN_ID,
				outstanding_principal: 0,
				via_liquidation: false,
			}));

			// No liquidation fee taken:
			assert_no_matching_event!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken { .. })
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), 0);

			// The loan has been fully repaid, but we still have a liquidation swap:
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap(),
				LoanAccount {
					borrower_id: BORROWER,
					collateral_topup_asset: None,
					collateral: Default::default(),
					loans: Default::default(),
					liquidation_status: LiquidationStatus::Liquidating {
						liquidation_swaps: BTreeMap::from([(
							LIQUIDATION_SWAP_1,
							LiquidationSwap {
								loan_id: LOAN_ID,
								from_asset: COLLATERAL_ASSET,
								to_asset: LOAN_ASSET
							}
						)]),
						liquidation_type: LiquidationType::Hard
					},
					voluntary_liquidation_requested: false
				}
			);
		})
		.then_execute_at_next_block(|_| {
			// Liquidation swap should have been aborted at the beginning of this block with all
			// funds returned as collateral:
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap(),
				LoanAccount {
					borrower_id: BORROWER,
					collateral_topup_asset: None,
					collateral: BTreeMap::from([
						(COLLATERAL_ASSET, INIT_COLLATERAL - EXECUTED_COLLATERAL),
						(LOAN_ASSET, SWAP_OUTPUT_AMOUNT)
					]),
					loans: Default::default(),
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
				RuntimeEvent::LendingPools(Event::<Test>::CollateralAdded {
					borrower_id: BORROWER,
					action_type: CollateralAddedActionType::SystemLiquidationExcessAmount { .. },
					..
				})
			);

			// We should not emit the event the second time:
			assert_no_matching_event!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled { .. })
			);

			// No liquidation fee taken from the swap output (thus no event):
			assert_no_matching_event!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken { .. })
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), 0);
			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
		});
}

mod voluntary_liquidation {

	use super::*;

	fn mock_liquidation_swap(input_amount: AssetAmount, chunks: u32) -> MockSwapRequest {
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
			price_limits_and_expiry: Some(SOFT_SWAP_PRICE_LIMIT),
			dca_params: Some(DcaParameters { number_of_chunks: chunks, chunk_interval: 1 }),
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
				// Simulate partial execution of the liquidation swap. This should be
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
						collateral_topup_asset: None,
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
						action_type: CollateralAddedActionType::SystemLiquidationExcessAmount { .. },
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

				// Simulate partial execution of the liquidation swap.
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
						collateral_topup_asset: None,
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
								owed_principal: PRINCIPAL + ORIGINATION_FEE - SWAPPED_PRINCIPAL,
								created_at_block: INIT_BLOCK,
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

		// Third liquidation will result in this much extra principal (after repaying
		// the loan in full).
		const SWAPPED_PRINCIPAL_EXTRA: AssetAmount = PRINCIPAL / 50;

		let get_account = || LoanAccounts::<Test>::get(BORROWER).unwrap();

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.with_default_loan()
			.with_voluntary_liquidation()
			.then_execute_with(|_| {
				// Simulate partial execution of the liquidation swap. This won't be enough
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
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
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
							owed_principal: PRINCIPAL + ORIGINATION_FEE - SWAPPED_PRINCIPAL_1,
							created_at_block: INIT_BLOCK,
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
						mock_liquidation_swap(INIT_COLLATERAL - SWAPPED_COLLATERAL_1, 3)
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

				// Simulate partial execution of the liquidation swap. This won't be enough
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
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
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
							owed_principal: owed_after_liquidation_2,
							created_at_block: INIT_BLOCK,
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
							INIT_COLLATERAL - SWAPPED_COLLATERAL_1 - SWAPPED_COLLATERAL_2,
							3
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
						collateral_topup_asset: None,
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
						action_type: CollateralAddedActionType::SystemLiquidationExcessAmount { .. },
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

		new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
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
		const OTHER_ASSET: Asset = Asset::Eth;
		assert_ne!(OTHER_ASSET, LOAN_ASSET);

		let try_to_withdraw = || {
			LendingPools::remove_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				Some(INIT_POOL_AMOUNT / 2),
			)
		};

		new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
			MockPriceFeedApi::set_price_usd_fine(OTHER_ASSET, SWAP_RATE);
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

		new_test_ext().with_funded_pool(2 * INIT_POOL_AMOUNT).execute_with(|| {
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

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

		new_test_ext().with_funded_pool(10 * PRINCIPAL).execute_with(|| {
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
			MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_1, 1);
			MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_2, 1);

			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

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

		new_test_ext().with_funded_pool(10 * PRINCIPAL).execute_with(|| {
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
			MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_1, 1);
			MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_2, 1);

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

	#[test]
	fn safe_mode_for_liquidations() {
		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.with_default_loan()
			.execute_with(|| {
				// Disable liquidations
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					liquidations_enabled: false,
					..PalletSafeMode::code_green()
				});

				// Voluntary liquidation should be disabled
				assert_noop!(
					LendingPools::initiate_voluntary_liquidation(RuntimeOrigin::signed(BORROWER)),
					Error::<Test>::LiquidationsDisabled
				);

				// Now set the price to trigger normal liquidation
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, 20 * SWAP_RATE)
			})
			.then_execute_at_next_block(|_| {
				// Forced liquidation should also be disabled
				assert_matching_event_count!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationInitiated { .. }) => 0
				);

				// Check the liquidation status hasn't changed
				assert_eq!(
					LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
					LiquidationStatus::NoLiquidation
				);

				// Turn the liquidations back
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode::code_green());
			})
			.then_execute_at_next_block(|_| {
				// Now the forced liquidation should proceeded as normal
				assert_matching_event_count!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationInitiated { .. }) => 1
				);
				assert_eq!(
					LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
					LiquidationStatus::Liquidating {
						liquidation_swaps: BTreeMap::from([(
							SwapRequestId(0),
							LiquidationSwap {
								loan_id: LOAN_ID,
								from_asset: COLLATERAL_ASSET,
								to_asset: LOAN_ASSET
							}
						)]),
						liquidation_type: LiquidationType::Hard
					}
				);
			});
	}
}

mod whitelisting {

	use super::*;

	const WHITELISTED_USER: u64 = LP;
	const NON_WHITELISTED_USER: u64 = OTHER_LP;

	fn setup_accounts() {
		for account in [WHITELISTED_USER, NON_WHITELISTED_USER] {
			MockBalance::credit_account(&account, LOAN_ASSET, INIT_POOL_AMOUNT);
			MockBalance::credit_account(&account, COLLATERAL_ASSET, INIT_COLLATERAL);
			MockLpRegistration::register_refund_address(account, LOAN_CHAIN);
		}

		assert_ok!(LendingPools::update_whitelist(
			RuntimeOrigin::root(),
			WhitelistUpdate::SetAllowedAccounts(BTreeSet::from([WHITELISTED_USER]))
		));
	}

	#[test]
	fn adding_lender_funds() {
		new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
			setup_accounts();

			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(WHITELISTED_USER),
				LOAN_ASSET,
				INIT_POOL_AMOUNT
			));

			assert_noop!(
				LendingPools::add_lender_funds(
					RuntimeOrigin::signed(NON_WHITELISTED_USER),
					LOAN_ASSET,
					INIT_POOL_AMOUNT
				),
				Error::<Test>::AccountNotWhitelisted
			);
		});
	}

	#[test]
	fn adding_collateral() {
		new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
			setup_accounts();

			assert_ok!(LendingPools::add_collateral(
				RuntimeOrigin::signed(WHITELISTED_USER),
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
			));

			assert_noop!(
				LendingPools::add_collateral(
					RuntimeOrigin::signed(NON_WHITELISTED_USER),
					Some(COLLATERAL_ASSET),
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
				),
				Error::<Test>::AccountNotWhitelisted
			);
		});
	}

	#[test]
	fn request_loan() {
		new_test_ext().with_funded_pool(2 * INIT_POOL_AMOUNT).execute_with(|| {
			setup_accounts();

			assert_ok!(LendingPools::request_loan(
				RuntimeOrigin::signed(WHITELISTED_USER),
				LOAN_ASSET,
				PRINCIPAL,
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
			));

			assert_noop!(
				LendingPools::request_loan(
					RuntimeOrigin::signed(NON_WHITELISTED_USER),
					LOAN_ASSET,
					PRINCIPAL,
					Some(COLLATERAL_ASSET),
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)]),
				),
				Error::<Test>::AccountNotWhitelisted
			);
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
		collateral_topup_asset: Some(Asset::Eth),
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
		voluntary_liquidation_requested: false,
	};

	new_test_ext().execute_with(|| {
		MockPriceFeedApi::set_price_usd_fine(Asset::Eth, 4_000);
		MockPriceFeedApi::set_price_usd_fine(Asset::Btc, 100_000);
		MockPriceFeedApi::set_price_usd_fine(Asset::Sol, 200);
		MockPriceFeedApi::set_price_usd_fine(Asset::Usdc, 1);

		let collateral = loan_account
			.prepare_collateral_for_liquidation(&OraclePriceCache::default())
			.unwrap();
		assert_ok!(loan_account.init_liquidation_swaps(
			&BORROWER,
			collateral,
			LiquidationType::Soft,
			&OraclePriceCache::default(),
		));

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
					price_limits_and_expiry: Some(SOFT_SWAP_PRICE_LIMIT),
					dca_params: Some(DcaParameters { number_of_chunks: 1, chunk_interval: 1 }),
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

#[test]
fn can_add_but_not_remove_collateral_with_stale_price() {
	const COLLATERAL_ASSET_1: Asset = Asset::Eth;
	const COLLATERAL_ASSET_2: Asset = Asset::Btc;

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_1, 1);
		MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_2, 1);

		// Set one of the collateral asset prices to be stale
		MockPriceFeedApi::set_stale(COLLATERAL_ASSET_1, false);
		MockPriceFeedApi::set_stale(COLLATERAL_ASSET_2, true);

		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET_1, INIT_COLLATERAL);
		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET_2, INIT_COLLATERAL);

		// Should still be able to add both collateral assets
		assert_ok!(LendingPools::add_collateral(
			RuntimeOrigin::signed(BORROWER),
			Some(COLLATERAL_ASSET_1),
			BTreeMap::from([
				(COLLATERAL_ASSET_1, INIT_COLLATERAL),
				(COLLATERAL_ASSET_2, INIT_COLLATERAL)
			]),
		));

		// But should not be able to remove any collateral
		assert_noop!(
			LendingPools::remove_collateral(
				RuntimeOrigin::signed(BORROWER),
				BTreeMap::from([(COLLATERAL_ASSET_2, INIT_COLLATERAL / 2)]),
			),
			Error::<Test>::OraclePriceUnavailable
		);
	});
}

#[test]
fn can_repay_but_not_expand_or_create_a_loan_with_stale_price() {
	const COLLATERAL_ASSET_1: Asset = Asset::Eth;
	const COLLATERAL_ASSET_2: Asset = Asset::Sol;
	const UNRELATED_COLLATERAL_ASSET: Asset = Asset::SolUsdc;

	new_test_ext().with_funded_pool(2 * INIT_POOL_AMOUNT).execute_with(|| {
		MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
		MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_1, 1);

		// Start with good non-stale prices
		MockPriceFeedApi::set_stale(LOAN_ASSET, false);
		MockPriceFeedApi::set_stale(COLLATERAL_ASSET_1, false);
		MockPriceFeedApi::set_stale(COLLATERAL_ASSET_2, false);

		// Except for this collateral asset, just to check having an unrelated stale price
		// doesn't interfere with things.
		MockPriceFeedApi::set_stale(UNRELATED_COLLATERAL_ASSET, true);

		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET_1, INIT_COLLATERAL * 2);

		// Create a loan while the price is fresh
		assert_ok!(LendingPools::new_loan(
			BORROWER,
			LOAN_ASSET,
			PRINCIPAL,
			Some(COLLATERAL_ASSET_1),
			BTreeMap::from([(COLLATERAL_ASSET_1, INIT_COLLATERAL)]),
		));

		// Set the price to be stale
		MockPriceFeedApi::set_stale(COLLATERAL_ASSET_1, true);

		// Should still be able to repay the loan
		assert_ok!(LendingPools::make_repayment(
			RuntimeOrigin::signed(BORROWER),
			LoanId(0),
			RepaymentAmount::Exact(PRINCIPAL / 2),
		));

		// But should not be able to expand the loan
		assert_noop!(
			LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LoanId(0),
				PRINCIPAL / 2,
				BTreeMap::from([]),
			),
			Error::<Test>::OraclePriceUnavailable
		);

		// Or create a new loan
		assert_noop!(
			LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				PRINCIPAL,
				Some(COLLATERAL_ASSET_1),
				BTreeMap::from([(COLLATERAL_ASSET_1, INIT_COLLATERAL)]),
			),
			Error::<Test>::OraclePriceUnavailable
		);

		// Because we have a loan open with a stale collateral price, we also cannot create a loan,
		// even if the price for the new collateral asset and loan asset are fresh.
		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET_2, INIT_COLLATERAL);
		assert_noop!(
			LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				PRINCIPAL,
				Some(COLLATERAL_ASSET_2),
				BTreeMap::from([(COLLATERAL_ASSET_2, INIT_COLLATERAL)]),
			),
			Error::<Test>::OraclePriceUnavailable
		);
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
		const INIT_POOL_AMOUNT_2: AssetAmount = INIT_POOL_AMOUNT * 2;

		const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE;

		const ORIGINATION_FEE: AssetAmount = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL);

		const LOAN_ASSET_2: Asset = Asset::Sol;
		const LOAN_CHAIN_2: ForeignChain = ForeignChain::Solana;
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
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
				MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET, 1);

				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET_2, SWAP_RATE);
				MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_2, 1);

				setup_pool_with_funds(LOAN_ASSET, INIT_POOL_AMOUNT);
				setup_pool_with_funds(LOAN_ASSET_2, INIT_POOL_AMOUNT_2);

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

				MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
				MockLpRegistration::register_refund_address(BORROWER_2, LOAN_CHAIN_2);

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
						collateral_topup_asset: Some(COLLATERAL_ASSET),
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
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET_2, NEW_SWAP_RATE);
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

				let mut interest_loan_1 = Interest::new();

				let (origination_fee_network_1, origination_fee_pool_1) =
					take_network_fee(ORIGINATION_FEE);

				// Interest for loan 1
				{
					let utilisation = Permill::from_rational(
						PRINCIPAL + ORIGINATION_FEE,
						INIT_POOL_AMOUNT + origination_fee_pool_1,
					);

					interest_loan_1.accrue_interest(
						PRINCIPAL + ORIGINATION_FEE,
						utilisation,
						CONFIG.interest_payment_interval_blocks,
					);
				}

				// Interest for loan 2 (in liquidation)
				let mut interest_loan_2 = Interest::new();
				{
					let (_origination_fee_network_2, origination_fee_pool_2) =
						take_network_fee(ORIGINATION_FEE_2);

					let utilisation = Permill::from_rational(
						PRINCIPAL_2 + ORIGINATION_FEE_2,
						INIT_POOL_AMOUNT_2 + origination_fee_pool_2,
					);

					interest_loan_2.accrue_interest(
						PRINCIPAL_2 + ORIGINATION_FEE_2,
						utilisation,
						CONFIG.interest_payment_interval_blocks,
					);
				}

				let (pool_interest_1, network_interest_1) = interest_loan_1.collect();

				let (pool_interest_2, network_interest_2) = interest_loan_2.collect();

				// Both accounts should be returned since we don't specify any:
				assert_eq!(
					super::rpc::get_loan_accounts::<Test>(None),
					vec![
						RpcLoanAccount {
							account: BORROWER_2,
							collateral_topup_asset: Some(COLLATERAL_ASSET_2),
							ltv_ratio: Some(FixedU64::from_rational(1_173_483_514, 1_000_000_000)),
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
								principal_amount: PRINCIPAL_2 +
									ORIGINATION_FEE_2 + pool_interest_2 +
									network_interest_2 - ACCUMULATED_OUTPUT_AMOUNT,
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
							collateral_topup_asset: Some(COLLATERAL_ASSET),
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
									ORIGINATION_FEE + pool_interest_1 +
									network_interest_1,
							}],
							liquidation_status: None
						},
					]
				);

				let utilisation_after_interest_pool_1 = Permill::from_rational(
					PRINCIPAL + ORIGINATION_FEE + pool_interest_1 + network_interest_1,
					INIT_POOL_AMOUNT + origination_fee_pool_1 + pool_interest_1,
				);

				assert_eq!(
					super::rpc::get_lending_pools::<Test>(Some(LOAN_ASSET)),
					vec![RpcLendingPool {
						asset: LOAN_ASSET,
						total_amount: INIT_POOL_AMOUNT + origination_fee_pool_1 + pool_interest_1,
						available_amount: INIT_POOL_AMOUNT -
							PRINCIPAL - origination_fee_network_1 -
							network_interest_1,
						owed_to_network: 0,
						utilisation_rate: utilisation_after_interest_pool_1,
						current_interest_rate: Permill::from_parts(53_335), // 5.33%
						config: CONFIG.get_config_for_asset(LOAN_ASSET).clone(),
					}]
				)
			});
	}
}

#[test]
fn loan_minimum_is_enforced() {
	const MIN_LOAN_AMOUNT_USD: AssetAmount = 1_000;
	const MIN_LOAN_AMOUNT_ASSET: AssetAmount = MIN_LOAN_AMOUNT_USD / SWAP_RATE;
	const COLLATERAL_AMOUNT: AssetAmount = MIN_LOAN_AMOUNT_ASSET * SWAP_RATE * 2;

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		// Set the minimum loan amount
		LendingConfig::<Test>::set(LendingConfiguration {
			minimum_loan_amount_usd: MIN_LOAN_AMOUNT_USD,
			minimum_update_loan_amount_usd: 0,
			..LendingConfigDefault::get()
		});

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, COLLATERAL_AMOUNT);
		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

		// Should not be able to create a loan below the minimum amount
		assert_noop!(
			LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				MIN_LOAN_AMOUNT_ASSET - 1,
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, COLLATERAL_AMOUNT)])
			),
			Error::<Test>::AmountBelowMinimum
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
			Error::<Test>::RemainingAmountBelowMinimum,
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
fn supply_minimum_is_enforced() {
	const MIN_SUPPLY_AMOUNT_USD: AssetAmount = 1_000_000;

	// Min amount that can be supplied in pool's asset
	const MIN_SUPPLY_AMOUNT: AssetAmount = MIN_SUPPLY_AMOUNT_USD / SWAP_RATE;

	new_test_ext().execute_with(|| {
		// Set the minimum supply amount
		LendingConfig::<Test>::set(LendingConfiguration {
			minimum_supply_amount_usd: MIN_SUPPLY_AMOUNT_USD,
			..LendingConfigDefault::get()
		});

		MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);

		disable_whitelist();

		assert_ok!(LendingPools::new_lending_pool(LOAN_ASSET));

		MockBalance::credit_account(&LENDER, LOAN_ASSET, 2 * MIN_SUPPLY_AMOUNT);

		// Can't supply below minimum
		assert_noop!(
			LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				MIN_SUPPLY_AMOUNT / 2
			),
			Error::<Test>::AmountBelowMinimum
		);

		// Can supply the minimum amount
		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(LENDER),
			LOAN_ASSET,
			MIN_SUPPLY_AMOUNT
		));

		// Add some more to test removing funds
		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(LENDER),
			LOAN_ASSET,
			MIN_SUPPLY_AMOUNT
		));

		// Can't leave less than the minimum in the pool
		assert_noop!(
			LendingPools::remove_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				Some(3 * MIN_SUPPLY_AMOUNT / 2)
			),
			Error::<Test>::RemainingAmountBelowMinimum
		);

		// Can remove funds partially (leaves more than the min in the pool):
		assert_ok!(LendingPools::remove_lender_funds(
			RuntimeOrigin::signed(LENDER),
			LOAN_ASSET,
			Some(MIN_SUPPLY_AMOUNT / 2)
		));

		// Can remove all funds:
		assert_ok!(LendingPools::remove_lender_funds(
			RuntimeOrigin::signed(LENDER),
			LOAN_ASSET,
			None
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
		// Set the minimum loan amount
		LendingConfig::<Test>::set(LendingConfiguration {
			minimum_loan_amount_usd: MIN_LOAN_AMOUNT_USD,
			minimum_update_loan_amount_usd: MIN_UPDATE_USD,
			..LendingConfigDefault::get()
		});

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, COLLATERAL_AMOUNT);
		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

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

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET, SWAP_RATE);
		MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_2, 1);
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

#[test]
fn must_have_refund_address_for_loan_asset() {
	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
		MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET, 1);

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

		// Should not be able to create a loan without a refund address set
		assert_noop!(
			LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				PRINCIPAL,
				Some(COLLATERAL_ASSET),
				BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
			),
			Error::<Test>::NoRefundAddressSet
		);

		// Set refund address and try again
		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
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
	});
}

#[test]
fn can_handle_liquidation_with_zero_collateral() {
	const LOAN_ID: LoanId = LoanId(0);
	const BORROWER: u64 = 1;

	// Create an account with a loan but no collateral. This should be an unreachable state, but we
	// want to make sure that it is handled gracefully.
	let loan_account = LoanAccount::<Test> {
		borrower_id: BORROWER,
		collateral_topup_asset: None,
		collateral: BTreeMap::new(),
		loans: BTreeMap::from([(
			LOAN_ID,
			GeneralLoan {
				id: LOAN_ID,
				asset: Asset::Btc,
				created_at_block: 0,
				owed_principal: 20,
				pending_interest: Default::default(),
			},
		)]),
		liquidation_status: LiquidationStatus::NoLiquidation,
		voluntary_liquidation_requested: false,
	};

	new_test_ext()
		.execute_with(|| {
			MockPriceFeedApi::set_price_usd_fine(Asset::Btc, 1);

			LoanAccounts::<Test>::insert(BORROWER, loan_account.clone());
		})
		// Running the next block will run upkeep and trigger liquidation. We just want to make sure
		// this does not panic.
		.then_process_next_block()
		.then_execute_with(|_| {
			// We expect to find a the loan account in liquidation but with no swaps associated with
			// it. This means it is stuck like this until we fix it. This is acceptable behavior
			// because this state should be unreachable.
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
				LiquidationStatus::Liquidating {
					liquidation_type: LiquidationType::Hard,
					liquidation_swaps: BTreeMap::new()
				}
			)
		});
}

#[test]
fn same_asset_loan() {
	const EXPANDED_PRINCIPAL: AssetAmount = PRINCIPAL / 2;
	const SWAP_DEFICIT: AssetAmount = 1_000;
	const SWAP_OUTPUT_AMOUNT: AssetAmount = INIT_COLLATERAL + PRINCIPAL - SWAP_DEFICIT;

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.execute_with(|| {
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, 1);
			MockBalance::credit_account(&BORROWER, LOAN_ASSET, INIT_COLLATERAL);
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

			// Should be able to create a loan where the loan asset is the same as the collateral
			// asset
			assert_eq!(
				LendingPools::new_loan(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					Some(LOAN_ASSET),
					BTreeMap::from([(LOAN_ASSET, INIT_COLLATERAL)])
				),
				Ok(LOAN_ID)
			);

			// And expand using the loan asset we just got to make sure it works.
			assert_ok!(LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				EXPANDED_PRINCIPAL,
				BTreeMap::from([(LOAN_ASSET, PRINCIPAL)])
			));

			// Now start a liquidation
			assert_ok!(LendingPools::initiate_voluntary_liquidation(RuntimeOrigin::signed(
				BORROWER
			)));
		})
		.then_process_next_block()
		.then_execute_with(|_| {
			// Check that the liquidation is happening
			assert_eq!(
				MockSwapRequestHandler::<Test>::get_swap_requests(),
				BTreeMap::from([(
					SwapRequestId(0),
					MockSwapRequest {
						input_asset: LOAN_ASSET,
						output_asset: LOAN_ASSET,
						input_amount: INIT_COLLATERAL + PRINCIPAL,
						remaining_input_amount: INIT_COLLATERAL + PRINCIPAL,
						accumulated_output_amount: 0,
						swap_type: SwapRequestType::Regular {
							output_action: SwapOutputAction::CreditLendingPool {
								swap_type: LendingSwapType::Liquidation {
									borrower_id: BORROWER,
									loan_id: LOAN_ID
								},
							},
						},
						broker_fees: Default::default(),
						origin: SwapOrigin::Internal,
						price_limits_and_expiry: Some(SOFT_SWAP_PRICE_LIMIT),
						dca_params: Some(DcaParameters { number_of_chunks: 3, chunk_interval: 1 }),
					}
				)])
			);

			// Finish the swap
			LendingPools::process_loan_swap_outcome(
				SwapRequestId(0),
				LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
				SWAP_OUTPUT_AMOUNT,
			);

			// Check that the loan has been repaid and account has the correct amount left
			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
				loan_id: LOAN_ID,
				outstanding_principal: 0,
				via_liquidation: true,
			}));
			let account = LoanAccounts::<Test>::get(BORROWER).unwrap();
			assert!(account.loans.is_empty());
			let origination_fee =
				portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL + EXPANDED_PRINCIPAL);
			let expected_collateral_left =
				SWAP_OUTPUT_AMOUNT - (PRINCIPAL + EXPANDED_PRINCIPAL + origination_fee);
			assert_eq!(*account.collateral.get(&LOAN_ASSET).unwrap(), expected_collateral_left);
		});
}
