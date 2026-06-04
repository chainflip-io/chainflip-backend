use crate::{general_lending::general_lending_pool::LendingPoolError, mocks::*};
use cf_chains::ForeignChain;
use cf_test_utilities::{
	assert_event_sequence, assert_has_event, assert_has_matching_event,
	assert_matching_event_count, assert_no_matching_event,
};
use cf_traits::{
	lending::LendingSystemApi,
	mocks::{
		account_role_registry::MockAccountRoleRegistry,
		balance_api::{MockBalance, MockLpRegistration},
		network_fee_api::MockNetworkFeeApi,
		price_feed_api::MockPriceFeedApi,
		swap_request_api::{MockSwapRequest, MockSwapRequestHandler},
	},
	AccountRoleRegistry,
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
	fn disable_network_fees(self) -> Self;
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

	fn disable_network_fees(self) -> Self {
		self.then_execute_with(|ctx| {
			assert_ok!(Pallet::<Test>::update_pallet_config(
				RuntimeOrigin::root(),
				bounded_vec![PalletConfigUpdate::SetNetworkFeeContributions {
					contributions: NetworkFeeContributions {
						extra_interest: Default::default(),
						from_origination_fee: Default::default(),
						from_liquidation_fee: Default::default(),
						low_ltv_penalty_max: Default::default()
					}
				}],
			));

			ctx
		})
	}

	fn with_default_loan(self) -> Self {
		self.then_execute_with(|ctx| {
			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

			assert_eq!(
				create_loan_and_supply_collateral(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
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

fn register_as_broker(account: &AccountId) {
	assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_broker(account));
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

fn get_collateral() -> AssetAmount {
	GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
		.unwrap()
		.get_supply_position_for_account(&BORROWER)
		.unwrap_or(0)
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
		action_type: SupplyAddedActionType::Manual,
	}));
}

fn create_loan_and_supply_collateral(
	borrower: u64,
	asset: Asset,
	amount: AssetAmount,
	collateral: BTreeMap<Asset, AssetAmount>,
) -> Result<LoanId, DispatchError> {
	for (collateral_asset, collateral_amount) in &collateral {
		if GeneralLendingPools::<Test>::get(collateral_asset).is_none() {
			LendingPools::new_lending_pool(*collateral_asset)?;
		}
		LendingPools::add_lender_funds(
			RuntimeOrigin::signed(borrower),
			*collateral_asset,
			*collateral_amount,
		)?;
	}
	LendingPools::new_loan(borrower, asset, amount, None)
}

#[test]
fn collateral_reported_for_supply_only_account() {
	// `cf_account_info` reports collateral via `get_total_collateral_for_account`. An account
	// that has supplied funds but never borrowed has no loan account, yet its supply positions
	// must still be reported as collateral.
	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		// LENDER supplied funds via `with_funded_pool` but never took out a loan:
		assert!(LoanAccounts::<Test>::get(LENDER).is_none());

		assert_eq!(
			get_total_collateral_for_account::<Test>(&LENDER),
			BTreeMap::from([(LOAN_ASSET, INIT_POOL_AMOUNT)]),
		);
	});
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
			action_type: SupplyRemovedActionType::Manual,
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
			action_type: SupplyRemovedActionType::Manual,
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

			assert_eq!(
				create_loan_and_supply_collateral(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
				),
				Ok(LOAN_ID)
			);

			// NOTE: we want LoanCreated event to be emitted before any event
			// referencing it (e.g. OriginationFeeTaken)
			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LendingPoolCreated {
					asset: COLLATERAL_ASSET,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
					lender_id: BORROWER,
					asset: COLLATERAL_ASSET,
					amount: INIT_COLLATERAL,
					action_type: SupplyAddedActionType::Manual,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LoanCreated {
					loan_id: LOAN_ID,
					loan_type: LoanType::User(BORROWER),
					asset: LOAN_ASSET,
					principal_amount: PRINCIPAL,
					broker: None,
				}),
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
				}
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER),
				Some(LoanAccount {
					borrower_id: BORROWER,
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
							broker: None,
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
				}
			);

			// Account is removed once the last loan is repaid:
			assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);
		});
}

/// When a loan is created with a broker attached, the broker's per-year fee accrues alongside
/// the pool/network interest and, on each interest payment block, is paid out of the pool's
/// available liquidity directly into the broker's free balance.
#[test]
fn broker_interest_credited_to_broker() {
	use cf_primitives::{Beneficiary, BASIS_POINTS_PER_MILLION};

	// Pick a principal large enough that one interest period charges a non-zero broker fee.
	const PRINCIPAL: AssetAmount = 2_000_000_000_000;
	const INIT_POOL_AMOUNT: AssetAmount = PRINCIPAL * 2;
	const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE; // 75% LTV
	const ORIGINATION_FEE: AssetAmount = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL);

	const BROKER: u64 = 999;
	const BROKER_BPS: u16 = 100; // 1% per year

	// With network fees disabled below, all of the origination fee goes to the pool and the
	// loan's initial owed principal is `PRINCIPAL + ORIGINATION_FEE`. The pool's utilisation at
	// the time of interest accrual is therefore:
	let utilisation =
		Permill::from_rational(PRINCIPAL + ORIGINATION_FEE, INIT_POOL_AMOUNT + ORIGINATION_FEE);

	// Pool interest charged for one payment interval at this utilisation:
	let pool_rate_per_interval = CONFIG.derive_base_interest_rate_per_payment_interval(
		LOAN_ASSET,
		utilisation,
		CONFIG.interest_payment_interval_blocks,
	);
	let expected_pool_interest = (ScaledAmountHP::from_asset_amount(PRINCIPAL + ORIGINATION_FEE) *
		pool_rate_per_interval)
		.take_non_fractional_part();

	// Broker interest computed using the same conversion as the pallet:
	let broker_rate_per_interval = CONFIG.interest_per_year_to_per_payment_interval(
		Permill::from_parts(BROKER_BPS as u32 * BASIS_POINTS_PER_MILLION),
		CONFIG.interest_payment_interval_blocks,
	);
	let expected_broker_interest =
		(ScaledAmountHP::from_asset_amount(PRINCIPAL + ORIGINATION_FEE) * broker_rate_per_interval)
			.take_non_fractional_part();

	// NOTE: intentional use of a hardcoded value: it was computed by hand to make sure it matches
	// our expectation.
	assert_eq!(expected_broker_interest, 38029);

	let first_interest_payment_block = INIT_BLOCK + CONFIG.interest_payment_interval_blocks as u64;

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.disable_network_fees()
		.then_execute_with(|_| {
			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
			register_as_broker(&BROKER);

			assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET));

			assert_ok!(supply_funds::<Test>(
				BORROWER,
				COLLATERAL_ASSET,
				INIT_COLLATERAL,
				SupplyAddedActionType::Manual,
			));

			assert_ok!(LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				PRINCIPAL,
				Some(Beneficiary { account: BROKER, bps: BROKER_BPS }),
			));

			// The loan's `LoanCreated` event records the broker's account id and bps.
			assert_has_matching_event!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanCreated {
					loan_id: LOAN_ID,
					loan_type: LoanType::User(BORROWER),
					broker,
					..
				}) if *broker == Some(Beneficiary { account: BROKER, bps: BROKER_BPS }),
			);

			assert_eq!(MockBalance::get_balance(&BROKER, LOAN_ASSET), 0);
		})
		.then_process_blocks_until_block(first_interest_payment_block)
		.then_execute_with(|_| {
			// The broker has been credited their accrued fee in the loan asset.
			assert_eq!(MockBalance::get_balance(&BROKER, LOAN_ASSET), expected_broker_interest);
			// We didn't credit the wrong asset:
			assert_eq!(MockBalance::get_balance(&BROKER, COLLATERAL_ASSET), 0);

			// Both the pool interest and the broker fee were rolled into the loan's owed
			// principal (so the borrower will repay them back to the pool):
			let loan = LoanAccounts::<Test>::get(BORROWER)
				.unwrap()
				.loans
				.get(&LOAN_ID)
				.unwrap()
				.clone();
			assert_eq!(
				loan.owed_principal,
				PRINCIPAL + ORIGINATION_FEE + expected_pool_interest + expected_broker_interest,
			);

			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::InterestTaken {
				loan_id: LOAN_ID,
				pool_interest: expected_pool_interest,
				network_interest: 0,
				broker_interest: expected_broker_interest,
				low_ltv_penalty: 0,
			}));
		});
}

/// If the pool has no available liquidity at the time of interest collection, the broker
/// fee for that interval is left in `pending_interest.broker` and rolled forward. Once a
/// lender supplies more funds, the next collection round pays the broker the cumulative
/// amount in one go.
#[test]
fn broker_fee_collected_after_pool_replenished() {
	use cf_primitives::{Beneficiary, BASIS_POINTS_PER_MILLION};

	const PRINCIPAL: AssetAmount = 2_000_000_000_000;
	// Pool exactly equals the loan principal so utilisation hits 100% after the loan and the
	// pool has zero available liquidity to pay broker fees on the first interest payment.
	const INIT_POOL_AMOUNT: AssetAmount = PRINCIPAL;
	const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE;
	const ORIGINATION_FEE: AssetAmount = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL);

	const BROKER: u64 = 999;
	const BROKER_BPS: u16 = 100; // 1% per year

	// Plenty of additional liquidity so that the deferred broker fee can be paid in full
	// at the next interest interval.
	const EXTRA_FUNDS: AssetAmount = PRINCIPAL;

	let broker_rate_per_interval = CONFIG.interest_per_year_to_per_payment_interval(
		Permill::from_parts(BROKER_BPS as u32 * BASIS_POINTS_PER_MILLION),
		CONFIG.interest_payment_interval_blocks,
	);

	// At the start of each interval the loan's `owed_principal` is what gets multiplied by
	// the broker rate. After the first interest payment, `owed_principal` is bumped by the
	// pool interest charged at 100% utilisation:
	let pool_rate_at_full_utilisation = CONFIG.derive_base_interest_rate_per_payment_interval(
		LOAN_ASSET,
		Permill::one(),
		CONFIG.interest_payment_interval_blocks,
	);
	let principal_interval_1 = PRINCIPAL + ORIGINATION_FEE;
	let pool_interest_1 = (ScaledAmountHP::from_asset_amount(principal_interval_1) *
		pool_rate_at_full_utilisation)
		.take_non_fractional_part();
	let principal_interval_2 = principal_interval_1 + pool_interest_1;

	// The broker fee for interval 1 is left in pending after a failed collection; the broker fee
	// for interval 2 then accumulates on top of it.
	let broker_fee_1 =
		ScaledAmountHP::from_asset_amount(principal_interval_1) * broker_rate_per_interval;
	let mut combined_pending = broker_fee_1 +
		ScaledAmountHP::from_asset_amount(principal_interval_2) * broker_rate_per_interval;
	let expected_broker_total = combined_pending.take_non_fractional_part();

	let first_interest_payment_block = INIT_BLOCK + CONFIG.interest_payment_interval_blocks as u64;
	let second_interest_payment_block =
		INIT_BLOCK + 2 * CONFIG.interest_payment_interval_blocks as u64;

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.disable_network_fees()
		.then_execute_with(|_| {
			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
			register_as_broker(&BROKER);

			assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET));
			assert_ok!(supply_funds::<Test>(
				BORROWER,
				COLLATERAL_ASSET,
				INIT_COLLATERAL,
				SupplyAddedActionType::Manual,
			));

			assert_ok!(LendingPools::new_loan(
				BORROWER,
				LOAN_ASSET,
				PRINCIPAL,
				Some(Beneficiary { account: BROKER, bps: BROKER_BPS }),
			));

			// Sanity check: the pool is now fully utilised.
			let pool = GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap();
			assert_eq!(pool.available_amount, 0);
		})
		.then_process_blocks_until_block(first_interest_payment_block)
		.then_execute_with(|_| {
			// The broker fee was charged but the pool had nothing to pay it with, so the
			// broker's free balance is still zero and the InterestTaken event reports a
			// zero broker_interest for this interval.
			assert_eq!(MockBalance::get_balance(&BROKER, LOAN_ASSET), 0);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::InterestTaken {
					loan_id: LOAN_ID,
					broker_interest: 0,
					..
				}),
			);

			// Broker fee is still pending:
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER)
					.unwrap()
					.loans
					.get(&LOAN_ID)
					.unwrap()
					.pending_interest
					.broker,
				broker_fee_1
			);
		})
		.then_execute_with(|_| {
			// A lender adds more liquidity to the pool.
			MockBalance::credit_account(&LENDER, LOAN_ASSET, EXTRA_FUNDS);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				EXTRA_FUNDS,
			));
		})
		.then_process_blocks_until_block(second_interest_payment_block)
		.then_execute_with(|_| {
			// The broker now receives the deferred fee from interval 1 plus the freshly
			// accrued fee from interval 2 in a single payout.
			assert_eq!(MockBalance::get_balance(&BROKER, LOAN_ASSET), expected_broker_total);
			assert_has_matching_event!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::InterestTaken {
					loan_id: LOAN_ID,
					broker_interest,
					..
				}) if *broker_interest == expected_broker_total,
			);

			// Broker fees have been collected in full (ignoring fractional part):
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER)
					.unwrap()
					.loans
					.get(&LOAN_ID)
					.unwrap()
					.pending_interest
					.broker
					.into_asset_amount(),
				0
			);
		});
}

mod broker_fees {

	use super::*;
	use cf_primitives::Beneficiary;

	const BROKER: AccountId = 999;

	/// Set up a borrower with collateral, then call `new_loan` with the given broker
	/// beneficiary. Broker registration is left to the caller.
	#[transactional]
	fn try_loan_with_broker(broker: Beneficiary<AccountId>) -> Result<LoanId, DispatchError> {
		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);
		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
		assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET));
		assert_ok!(supply_funds::<Test>(
			BORROWER,
			COLLATERAL_ASSET,
			INIT_COLLATERAL,
			SupplyAddedActionType::Manual,
		));

		LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL, Some(broker))
	}

	/// `new_loan` rejects broker fees above [`MAX_BROKER_FEE_BPS`].
	#[test]
	fn broker_fee_above_cap_is_rejected() {
		use crate::general_lending::MAX_BROKER_FEE_BPS;

		new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).then_execute_with(|_| {
			register_as_broker(&BROKER);
			assert_noop!(
				try_loan_with_broker(Beneficiary { account: BROKER, bps: MAX_BROKER_FEE_BPS + 1 }),
				Error::<Test>::BrokerFeeTooHigh,
			);
		});
	}

	/// `new_loan` rejects a broker fee of zero (callers should pass `None` instead of a
	/// zero-fee broker beneficiary).
	#[test]
	fn zero_broker_fee_is_rejected() {
		new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).then_execute_with(|_| {
			register_as_broker(&BROKER);
			assert_noop!(
				try_loan_with_broker(Beneficiary { account: BROKER, bps: 0 }),
				Error::<Test>::InvalidZeroBrokerFee,
			);
		});
	}

	/// `new_loan` rejects a broker beneficiary whose account is not registered as a Broker.
	#[test]
	fn unknown_broker_is_rejected() {
		new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).then_execute_with(|_| {
			// BROKER is intentionally not registered as a broker account.
			assert_noop!(
				try_loan_with_broker(Beneficiary { account: BROKER, bps: 100 }),
				Error::<Test>::UnknownBroker,
			);
		});
	}
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

		assert_eq!(
			create_loan_and_supply_collateral(
				BORROWER,
				LOAN_ASSET,
				PRINCIPAL,
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
				}
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap(),
				LoanAccount {
					borrower_id: BORROWER,
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
							pending_interest: Default::default(),
							broker: None,
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

		// Try to borrow more, but this time we don't have enough collateral for an acceptable LTV
		assert_err!(
			LendingPools::expand_loan(RuntimeOrigin::signed(BORROWER), LOAN_ID, EXTRA_PRINCIPAL_2,),
			Error::<Test>::LtvTooHigh
		);

		// Should succeed when trying again after supplying extra collateral to the pool
		{
			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, EXTRA_COLLATERAL);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BORROWER),
				COLLATERAL_ASSET,
				EXTRA_COLLATERAL,
			));

			System::reset_events();

			assert_ok!(LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				EXTRA_PRINCIPAL_2,
			));

			let (network_fee, pool_fee) = take_network_fee(ORIGINATION_FEE_3);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanUpdated {
					loan_id: LOAN_ID,
					extra_principal_amount: EXTRA_PRINCIPAL_2,
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
				}
			);

			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap(),
				LoanAccount {
					borrower_id: BORROWER,
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
							pending_interest: Default::default(),
							broker: None,
						}
					)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false,
				}
			);

			// Supplied collateral has been increased:
			assert_eq!(get_collateral(), INIT_COLLATERAL + EXTRA_COLLATERAL);

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

	// The soft liquidation swap pulls just enough collateral to cover the owed principal
	// (in USD) plus the soft-slippage buffer; the rest stays in the supply pool.
	let liquidation_input = required_collateral_with_buffer(
		(PRINCIPAL + ORIGINATION_FEE) * NEW_SWAP_RATE,
		bps_to_permill(CONFIG.soft_liquidation_max_oracle_slippage)
			.saturating_add(CONFIG.liquidation_fee(LOAN_ASSET)),
	);
	let pool_remainder_after_init = INIT_COLLATERAL - liquidation_input;

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

	assert!(liquidation_input >= EXECUTED_COLLATERAL);

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
					input_amount: liquidation_input,
					remaining_input_amount: liquidation_input,
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

			// Only the collateral required to repay the loan (plus the slippage buffer) is
			// removed from the supply pool — the rest stays available to other lenders.
			assert_eq!(
				get_collateral_in_supply_pools::<Test>(&loan_account.borrower_id),
				BTreeMap::from([(COLLATERAL_ASSET, pool_remainder_after_init)])
			);

			// Despite collateral having been moved to the swapping pallet, we
			// can still calculate its value:
			assert_eq!(
				loan_account.total_collateral_usd_value(&OraclePriceCache::default()).unwrap(),
				INIT_COLLATERAL
			);

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
					remaining_input_amount: liquidation_input - EXECUTED_COLLATERAL,
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
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false,
					loans: BTreeMap::from([(
						LOAN_ID,
						GeneralLoan {
							id: LOAN_ID,
							asset: LOAN_ASSET,
							created_at_block: INIT_BLOCK,
							owed_principal: PRINCIPAL + ORIGINATION_FEE - repaid_amount_1,
							pending_interest: Default::default(),
							broker: None,
						}
					)]),
				})
			);

			// Remaining collateral is re-supplied to the pool:
			assert_eq!(get_collateral(), INIT_COLLATERAL - EXECUTED_COLLATERAL);

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
				}) if pool_fee == liquidation_fee_pool_1 &&
					network_fee == liquidation_fee_network_1 &&
					broker_fee == 0,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount,
					action_type: LoanRepaidActionType::Liquidation {
						swap_request_id: LIQUIDATION_SWAP_1
					}
				}) if amount == repaid_amount_1,
			);

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
				}) if pool_fee == liquidation_fee_pool_2 &&
					network_fee == liquidation_fee_network_2 &&
					broker_fee == 0,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					action_type: LoanRepaidActionType::Liquidation {
						swap_request_id: LIQUIDATION_SWAP_2
					},
					amount,
				}) if amount == repaid_amount_2,
				RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
					lender_id: BORROWER,
					asset: LOAN_ASSET,
					amount,
					action_type: SupplyAddedActionType::SystemLiquidationExcessAmount {
						loan_id: LOAN_ID,
						swap_request_id: LIQUIDATION_SWAP_2,
					},
				}) if amount == excess_principal,
				// The loan should now be settled:
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal: 0,
					via_liquidation: true,
				}),
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

			// The pool is expected to get all of its original funds back plus all the fees
			// (interest is not collected in this test), plus the excess principal supplied
			// by the borrower.
			let expected_total_amount = INIT_POOL_AMOUNT +
				origination_fee_pool +
				liquidation_fee_pool_1 +
				liquidation_fee_pool_2 +
				excess_principal;
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: expected_total_amount,
					// All of the funds should be available:
					available_amount: expected_total_amount,
					// Borrower now has a small amount in the loan asset pool:
					lender_shares: BTreeMap::from([
						(LENDER, Perquintill::from_parts(996_405_890_220_689_421)),
						(BORROWER, Perquintill::from_parts(3_594_109_779_310_579)),
					]),
				}
			);

			assert_eq!(
				PendingNetworkFees::<Test>::get(LOAN_ASSET),
				origination_fee_network + liquidation_fee_network_1 + liquidation_fee_network_2
			);

			// Account is removed once the last loan is settled:
			assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);

			// Excess principal is now supplied to the loan asset pool:
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET)
					.unwrap()
					.get_supply_position_for_account(&BORROWER)
					.unwrap(),
				excess_principal
			);
		});
}

#[test]
fn soft_liquidation_escalates_to_hard() {
	// This is high enough to trigger soft liquidation
	const NEW_SWAP_RATE: u128 = 24;

	const LIQUIDATION_SWAP_1: SwapRequestId = SwapRequestId(0);
	const LIQUIDATION_SWAP_2: SwapRequestId = SwapRequestId(1);

	// The user gets an unfavourable swap rate here: only 45% of PRINCIPAL of loan asset is
	// produced by the time most of the swap input has been consumed, leaving us with too
	// little remaining collateral to cover the still-outstanding principal at the new
	// oracle price — this triggers escalation to hard liquidation.
	const SWAP_1_OUTPUT_AMOUNT: AssetAmount = 45 * PRINCIPAL / 100;
	const SWAP_1_REMAINING_INPUT_AMOUNT: AssetAmount = 2 * INIT_COLLATERAL / 5;

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

			// The account is removed as it has no loans and no supplied collateral:
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

			// The loan has been repaid, the account is removed, and the remaining collateral amount
			// and the excess loan asset amount are credited to supply pools:
			assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);

			assert_eq!(
				GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
					.unwrap()
					.get_supply_position_for_account(&BORROWER)
					.unwrap(),
				REMAINING_COLLATERAL
			);
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET)
					.unwrap()
					.get_supply_position_for_account(&BORROWER)
					.unwrap(),
				RECOVERED_LOAN_ASSET - PRINCIPAL - ORIGINATION_FEE - liquidation_fee
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

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
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
				}
			);

			LendingPools::process_loan_swap_outcome(
				LIQUIDATION_SWAP_1,
				LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
				RECOVERED_PRINCIPAL,
			);

			let liquidation_fee = CONFIG.liquidation_fee(LOAN_ASSET) * RECOVERED_PRINCIPAL;
			let (liquidation_fee_network, liquidation_fee_pool) = take_network_fee(liquidation_fee);

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
					amount,
					action_type: LoanRepaidActionType::Liquidation { swap_request_id: LIQUIDATION_SWAP_1 }
				}) if amount == RECOVERED_PRINCIPAL - liquidation_fee,
				RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
					loan_id: LOAN_ID,
					outstanding_principal,
					via_liquidation: true,
				}) if outstanding_principal == expected_outstanding_principal
			);

			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);
			assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);

			// The pool has lost outstanding principal (but has accrued origination and liquidation
			// fees):
			let new_total_amount = INIT_POOL_AMOUNT + origination_fee_pool + liquidation_fee_pool -
				expected_outstanding_principal;

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: new_total_amount,
					available_amount: new_total_amount,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
				}
			);

			// The account is removed as it has no loans and no supplied collateral:
			assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);
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

		if GeneralLendingPools::<Test>::get(OTHER_COLLATERAL_ASSET).is_none() {
			assert_ok!(LendingPools::new_lending_pool(OTHER_COLLATERAL_ASSET));
		}
		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BORROWER),
			OTHER_COLLATERAL_ASSET,
			OTHER_COLLATERAL_ASSET_AMOUNT,
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
		// Swap 1 will be a partial swap. With the "collect just enough" liquidation flow, a
		// portion of the original collateral stays in the supply pool, which lowers the LTV
		// computed during the post-partial-swap upkeep. We pick this remaining-input amount
		// so the LTV stays inside the soft-liquidation band (between soft_abort=88% and
		// hard_liquidation=95%) rather than dropping below 88% and aborting prematurely.
		const SWAP_1_REMAINING_INPUT: AssetAmount = 700_000_000;
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

		// Soft liquidation only collects the owed principal (in USD) plus the slippage
		// buffer, drawn proportionally from each available collateral asset; the rest
		// stays in the supply pools throughout liquidation.
		let liquidation_estimates: BTreeMap<Asset, AssetAmount> =
			compute_per_asset_liquidation_estimates(
				required_collateral_with_buffer(
					TOTAL_OWED * NEW_SWAP_RATE,
					bps_to_permill(CONFIG.soft_liquidation_max_oracle_slippage)
						.saturating_add(CONFIG.liquidation_fee(LOAN_ASSET)),
				),
				&[
					(COLLATERAL_ASSET, INIT_COLLATERAL, INIT_COLLATERAL),
					(
						OTHER_COLLATERAL_ASSET,
						OTHER_COLLATERAL_ASSET_AMOUNT,
						OTHER_COLLATERAL_ASSET_AMOUNT,
					),
				],
			)
			.into_iter()
			.collect();
		let collateral_leftover_at_init =
			INIT_COLLATERAL - liquidation_estimates[&COLLATERAL_ASSET];
		let other_collateral_leftover_at_init =
			OTHER_COLLATERAL_ASSET_AMOUNT - liquidation_estimates[&OTHER_COLLATERAL_ASSET];

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
				// The remaining liquidation swap should be aborted here, the loan has been
				// settled, and the account has been removed:
				assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);

				// Collateral from both swaps should be returned to supply pools alongside
				// the leftovers that were never pulled into the swaps at init.
				assert_eq!(
					GeneralLendingPools::<Test>::get(LOAN_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					EXCESS_AMOUNT - total_liquidation_fee
				);
				assert_eq!(
					GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					SWAP_1_REMAINING_INPUT + collateral_leftover_at_init
				);
				assert_eq!(
					GeneralLendingPools::<Test>::get(OTHER_COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					other_collateral_leftover_at_init
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

		// Soft liquidation only collects the owed principal (in USD) plus the slippage
		// buffer, drawn proportionally from each available collateral asset; the rest
		// stays in the supply pools throughout liquidation.
		let liquidation_estimates: BTreeMap<Asset, AssetAmount> =
			compute_per_asset_liquidation_estimates(
				required_collateral_with_buffer(
					TOTAL_OWED * NEW_SWAP_RATE,
					bps_to_permill(CONFIG.soft_liquidation_max_oracle_slippage)
						.saturating_add(CONFIG.liquidation_fee(LOAN_ASSET)),
				),
				&[
					(COLLATERAL_ASSET, INIT_COLLATERAL, INIT_COLLATERAL),
					(
						OTHER_COLLATERAL_ASSET,
						OTHER_COLLATERAL_ASSET_AMOUNT,
						OTHER_COLLATERAL_ASSET_AMOUNT,
					),
				],
			)
			.into_iter()
			.collect();
		let collateral_leftover_at_init =
			INIT_COLLATERAL - liquidation_estimates[&COLLATERAL_ASSET];
		let other_collateral_leftover_at_init =
			OTHER_COLLATERAL_ASSET_AMOUNT - liquidation_estimates[&OTHER_COLLATERAL_ASSET];

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
				// The remaining liquidation swap should be aborted here, the loan has been
				// settled, and the account has been removed:
				assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);

				// Collateral from both swaps should be returned to supply pools alongside
				// the leftovers that were never pulled into the swaps at init.
				assert_eq!(
					GeneralLendingPools::<Test>::get(LOAN_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					EXCESS_AMOUNT + SWAP_2_OUTPUT_AMOUNT
				);
				assert_eq!(
					GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					collateral_leftover_at_init
				);
				assert_eq!(
					GeneralLendingPools::<Test>::get(OTHER_COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					SWAP_2_REMAINING_INPUT_AMOUNT + other_collateral_leftover_at_init
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

		// Soft liquidation only collects the owed principal (in USD) plus the slippage
		// buffer, drawn proportionally from each available collateral asset; the rest
		// stays in the supply pools throughout liquidation.
		let total_owed_usd = (PRINCIPAL + ORIGINATION_FEE) * NEW_SWAP_RATE +
			(PRINCIPAL_2 + ORIGINATION_FEE_2) * SWAP_RATE;
		let liquidation_estimates: BTreeMap<Asset, AssetAmount> =
			compute_per_asset_liquidation_estimates(
				required_collateral_with_buffer(
					total_owed_usd,
					bps_to_permill(CONFIG.soft_liquidation_max_oracle_slippage).saturating_add(
						CONFIG
							.liquidation_fee(LOAN_ASSET)
							.max(CONFIG.liquidation_fee(LOAN_ASSET_2)),
					),
				),
				&[
					(COLLATERAL_ASSET, INIT_COLLATERAL, INIT_COLLATERAL),
					(COLLATERAL_ASSET_2, INIT_COLLATERAL_2, INIT_COLLATERAL_2),
				],
			)
			.into_iter()
			.collect();
		let collateral_leftover_at_init =
			INIT_COLLATERAL - liquidation_estimates[&COLLATERAL_ASSET];
		let collateral_2_leftover_at_init =
			INIT_COLLATERAL_2 - liquidation_estimates[&COLLATERAL_ASSET_2];

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

				assert_eq!(
					create_loan_and_supply_collateral(
						BORROWER,
						LOAN_ASSET,
						PRINCIPAL,
						BTreeMap::from([
							(COLLATERAL_ASSET, INIT_COLLATERAL),
							(COLLATERAL_ASSET_2, INIT_COLLATERAL_2),
						])
					),
					Ok(LOAN_ID)
				);

				assert_eq!(
					LendingPools::new_loan(BORROWER, LOAN_ASSET_2, PRINCIPAL_2, None),
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

				// Soft liquidation collects only the principal (in USD) plus a 0.5% slippage
				// buffer, drawn from each collateral asset proportionally to its share of the
				// available USD value. The remainder stays in the supply pools.
				assert_eq!(
					get_collateral_in_supply_pools::<Test>(&BORROWER),
					BTreeMap::from([
						(COLLATERAL_ASSET, collateral_leftover_at_init),
						(COLLATERAL_ASSET_2, collateral_2_leftover_at_init),
					])
				);
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
							pending_interest: Default::default(),
							broker: None,
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
					get_collateral_in_supply_pools::<Test>(&BORROWER),
					BTreeMap::from([
						(LOAN_ASSET, excess_loan_asset_amount),
						(COLLATERAL_ASSET, collateral_leftover_at_init),
						(COLLATERAL_ASSET_2, collateral_2_leftover_at_init),
					])
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
							pending_interest: Default::default(),
							broker: None,
						}
					)])
				);

				assert_eq!(
					get_collateral_in_supply_pools::<Test>(&BORROWER),
					BTreeMap::from([
						(LOAN_ASSET, excess_loan_asset_amount),
						(COLLATERAL_ASSET, REMAINING_INPUT_SWAP_2 + collateral_leftover_at_init),
						(
							COLLATERAL_ASSET_2,
							REMAINING_INPUT_SWAP_4 + collateral_2_leftover_at_init
						),
					])
				);
			});
	}

	/// When one loan's only liquidation swap completes with a shortfall while a sibling
	/// loan's swap is still in flight, the shortfall loan must NOT be written off as bad
	/// debt: the sibling swap may finish with excess that becomes available collateral
	/// (via `supply_from_liquidation`) and the next upkeep round can liquidate against
	/// it. Settlement is therefore deferred until no liquidation swaps remain in flight
	/// for the borrower.
	#[test]
	fn loan_not_written_off_while_sibling_liquidation_swap_in_flight() {
		const LOAN_ID_2: LoanId = LoanId(1);
		const PRINCIPAL_2: AssetAmount = PRINCIPAL;

		// Total collateral covers both loans together at the same ~75% LTV the single-loan
		// tests start at.
		const TOTAL_COLLATERAL: AssetAmount = INIT_COLLATERAL * 2;

		// Doubling the loan-asset price pushes total LTV to ~150%, forcing the take-all
		// branch in `compute_per_asset_liquidation_estimates` so all collateral is drained
		// from supply and split between the two loans' liquidation swaps.
		const NEW_SWAP_RATE: u128 = SWAP_RATE * 2;

		// Loan 1's swap returns less than its share of principal (severe slippage).
		const LOAN_1_SWAP_OUTPUT: AssetAmount = PRINCIPAL / 2;

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT * 4)
			.then_execute_with(|_| {
				MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, TOTAL_COLLATERAL);
				MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

				assert_eq!(
					create_loan_and_supply_collateral(
						BORROWER,
						LOAN_ASSET,
						PRINCIPAL,
						BTreeMap::from([(COLLATERAL_ASSET, TOTAL_COLLATERAL)]),
					),
					Ok(LOAN_ID),
				);
				assert_eq!(
					LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL_2, None),
					Ok(LOAN_ID_2),
				);
			})
			.then_execute_with(|_| {
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
			})
			.then_execute_at_next_block(|_| {
				// All collateral has been pulled into liquidation swaps (take_all path):
				let acc = get_loan_account();
				assert!(general_lending::is_zero_collateral(
					&get_collateral_in_supply_pools::<Test>(&acc.borrower_id)
				));
				match &acc.liquidation_status {
					LiquidationStatus::Liquidating { liquidation_swaps, .. } => {
						assert_eq!(liquidation_swaps.len(), 2);
						assert!(liquidation_swaps.values().any(|s| s.loan_id == LOAN_ID));
						assert!(liquidation_swaps.values().any(|s| s.loan_id == LOAN_ID_2));
					},
					_ => panic!("expected Liquidating status"),
				}

				// Loan 1's swap completes first, with a shortfall. Loan 2's swap is still
				// in flight at this point — supply pools are empty.
				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP_1,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
					LOAN_1_SWAP_OUTPUT,
				);

				let acc = get_loan_account();

				// Loan 1 must NOT be written off while loan 2's swap is still in flight,
				// even though no collateral remains in supply pools.
				assert!(acc.loans.contains_key(&LOAN_ID));

				let liquidation_fee_1 = CONFIG.liquidation_fee(LOAN_ASSET) * LOAN_1_SWAP_OUTPUT;
				let l1_repaid = LOAN_1_SWAP_OUTPUT - liquidation_fee_1;
				assert_eq!(
					acc.loans[&LOAN_ID].owed_principal,
					PRINCIPAL + ORIGINATION_FEE - l1_repaid,
				);

				// Liquidation status is still `Liquidating`, with only loan 2's swap left.
				match &acc.liquidation_status {
					LiquidationStatus::Liquidating { liquidation_swaps, .. } => {
						assert_eq!(liquidation_swaps.len(), 1);
						assert!(liquidation_swaps.values().all(|s| s.loan_id == LOAN_ID_2));
					},
					_ => panic!(
						"expected Liquidating status to persist while sibling swap in flight"
					),
				}

				// Loan 2's swap also completes with a shortfall, leaving the borrower with
				// no collateral and no swaps in flight. At this point both loans are
				// unrecoverable and must be written off — otherwise loan 1 would be stuck
				// forever (no collateral means the next upkeep can't initiate new swaps).
				const LOAN_2_SWAP_OUTPUT: AssetAmount = PRINCIPAL_2 / 2;
				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP_2,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID_2 },
					LOAN_2_SWAP_OUTPUT,
				);

				// Both loans written off as bad debt; account removed.
				assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);
			});
	}

	/// Companion to [loan_not_written_off_while_sibling_liquidation_swap_in_flight] where
	/// loan 2's swap completes with *excess* (positive slippage). The deferred loan 1 must
	/// not be written off; loan 2 settles cleanly, the excess lands in the loan-asset
	/// supply pool, and the next upkeep round can attempt to re-liquidate loan 1 against
	/// the freshly available collateral.
	#[test]
	fn deferred_loan_recovers_when_sibling_swap_returns_excess() {
		const LOAN_ID_2: LoanId = LoanId(1);
		const PRINCIPAL_2: AssetAmount = PRINCIPAL;

		const TOTAL_COLLATERAL: AssetAmount = INIT_COLLATERAL * 2;
		const NEW_SWAP_RATE: u128 = SWAP_RATE * 2;

		// Loan 1's swap returns less than its share of principal (severe slippage).
		const LOAN_1_SWAP_OUTPUT: AssetAmount = PRINCIPAL / 2;
		// Loan 2's swap returns enough to fully repay loan 2 and leave a small excess in
		// the loan-asset supply pool — but not enough collateral to bring loan 1's LTV
		// back below the soft-liquidation threshold, so the next upkeep round must
		// re-enter liquidation for loan 1.
		const LOAN_2_SWAP_OUTPUT: AssetAmount = 11 * PRINCIPAL_2 / 10;

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT * 4)
			.then_execute_with(|_| {
				MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, TOTAL_COLLATERAL);
				MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

				assert_eq!(
					create_loan_and_supply_collateral(
						BORROWER,
						LOAN_ASSET,
						PRINCIPAL,
						BTreeMap::from([(COLLATERAL_ASSET, TOTAL_COLLATERAL)]),
					),
					Ok(LOAN_ID),
				);
				assert_eq!(
					LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL_2, None),
					Ok(LOAN_ID_2),
				);
			})
			.then_execute_with(|_| {
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
			})
			.then_execute_at_next_block(|_| {
				// Loan 1's swap completes first with a shortfall: deferred (sibling in flight).
				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP_1,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
					LOAN_1_SWAP_OUTPUT,
				);
				assert!(get_loan_account().loans.contains_key(&LOAN_ID));

				// Loan 2's swap completes with excess.
				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP_2,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID_2 },
					LOAN_2_SWAP_OUTPUT,
				);

				let acc = get_loan_account();

				// Loan 2 fully repaid + settled. Loan 1 still outstanding (no premature
				// write-off because the loan-asset supply pool now holds the excess from
				// loan 2's swap, so `no_collateral_left` was false at the sweep check).
				assert!(!acc.loans.contains_key(&LOAN_ID_2));
				assert!(acc.loans.contains_key(&LOAN_ID));

				let liquidation_fee_1 = CONFIG.liquidation_fee(LOAN_ASSET) * LOAN_1_SWAP_OUTPUT;
				let l1_repaid = LOAN_1_SWAP_OUTPUT - liquidation_fee_1;
				assert_eq!(
					acc.loans[&LOAN_ID].owed_principal,
					PRINCIPAL + ORIGINATION_FEE - l1_repaid,
				);

				// Liquidation finished from the perspective of the swap engine.
				assert_eq!(acc.liquidation_status, LiquidationStatus::NoLiquidation);

				// Loan 2's excess (after the loan-2 liquidation fee) sits in the loan-asset
				// supply pool, available as collateral for the next upkeep round.
				let liquidation_fee_2 =
					CONFIG.liquidation_fee(LOAN_ASSET) * (PRINCIPAL_2 + ORIGINATION_FEE);
				let expected_excess =
					LOAN_2_SWAP_OUTPUT - (PRINCIPAL_2 + ORIGINATION_FEE) - liquidation_fee_2;
				assert_eq!(
					GeneralLendingPools::<Test>::get(LOAN_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					expected_excess,
				);
			})
			.then_execute_at_next_block(|_| {
				// Next upkeep observes loan 1's outstanding principal against the freshly
				// supplied loan-asset collateral (from loan 2's excess), finds LTV still
				// above the soft-liquidation threshold, and re-enters liquidation for
				// loan 1 — proving the deferred loan really does keep making progress
				// rather than sitting unrepayable.
				let acc = get_loan_account();
				assert!(acc.loans.contains_key(&LOAN_ID));
				assert!(!acc.loans.contains_key(&LOAN_ID_2));
				assert_matches!(acc.liquidation_status, LiquidationStatus::Liquidating { .. });
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
				create_loan_and_supply_collateral(
					BORROWER,
					LOAN_ASSET,
					PRINCIPAL,
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
				),
				Ok(LOAN_ID)
			);
		})
		.then_process_blocks_until_block(INIT_BLOCK + interest_payment_interval as u64)
		// Interest should be recorded here, but not taken yet (it is too small)
		.then_execute_with(|_| {
			let account = LoanAccounts::<Test>::get(BORROWER).unwrap();
			assert_eq!(
				get_collateral_in_supply_pools::<Test>(&account.borrower_id),
				BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
			);

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
					action_type: LoanRepaidActionType::Manual
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
				action_type: LoanRepaidActionType::Manual,
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

			// Account is removed once the last loan is repaid; supplied collateral stays in the pool:
			assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);
			assert_eq!(
				GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
					.unwrap()
					.get_supply_position_for_account(&BORROWER)
					.unwrap(),
				INIT_COLLATERAL
			);
			assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), 0);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					action_type: LoanRepaidActionType::Manual,
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
					action_type: LoanRepaidActionType::Manual,
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

			// Account is removed once the last loan is repaid:
			assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);

			// Check that excess amount isn't erroneously added to collateral or the pool:
			assert_eq!(
				GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
					.unwrap()
					.get_supply_position_for_account(&BORROWER)
					.unwrap(),
				INIT_COLLATERAL
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
				LendingPool {
					total_amount: INIT_POOL_AMOUNT + origination_fee_pool,
					available_amount: INIT_POOL_AMOUNT + origination_fee_pool,
					lender_shares: BTreeMap::from([(LENDER, Perquintill::one())]),
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
				LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL, None),
				Error::<Test>::LiquidationInProgress
			);

			assert_noop!(
				<LendingPools as LendingApi>::expand_loan(BORROWER, LOAN_ID, PRINCIPAL,),
				Error::<Test>::LiquidationInProgress
			);
		});
}

#[test]
fn origination_rejected_when_pool_cant_cover_network_fee() {
	// `fund_loan` refuses to originate a loan when the pool's `available_amount` can't
	// cover both the principal and the network portion of the origination fee. Fund the
	// pool with exactly `PRINCIPAL + origination_fee_network`: borrowing `PRINCIPAL` fits
	// exactly, while `PRINCIPAL + 1` does not.
	const PRINCIPAL: AssetAmount = INIT_POOL_AMOUNT;
	const ORIGINATION_FEE: AssetAmount = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL);
	const INIT_COLLATERAL: AssetAmount = (4 * PRINCIPAL / 3) * SWAP_RATE; // 75% LTV

	let (origination_fee_network, _) = take_network_fee(ORIGINATION_FEE);
	let pool_funds = PRINCIPAL + origination_fee_network;

	new_test_ext().with_funded_pool(pool_funds).execute_with(|| {
		// Supply collateral up front so loan attempts only differ in the requested principal.
		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);
		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
		LendingPools::new_lending_pool(COLLATERAL_ASSET).unwrap();
		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BORROWER),
			COLLATERAL_ASSET,
			INIT_COLLATERAL,
		));

		// One unit beyond what the pool's headroom can fund → rejected.
		assert_noop!(
			LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL + 1, None),
			Error::<Test>::InsufficientLiquidity,
		);

		// Borrowing the full PRINCIPAL succeeds: the pool was funded with exactly the
		// network-fee headroom for this principal. After the loan, `available` is zero.
		assert_ok!(LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL, None));
		assert_eq!(GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap().available_amount, 0,);
	});
}

#[test]
fn removing_supplied_funds_disallowed_during_liquidation() {
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
				LendingPools::remove_lender_funds(
					RuntimeOrigin::signed(BORROWER),
					COLLATERAL_ASSET,
					Some(INIT_COLLATERAL),
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

	let fund_account_and_add_supply = |amount| {
		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, amount);

		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BORROWER),
			COLLATERAL_ASSET,
			amount,
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
				RuntimeEvent::LendingPools(Event::<Test>::LendingFundsRemoved {
					lender_id: BORROWER,
					asset: COLLATERAL_ASSET,
					unlocked_amount: INIT_COLLATERAL,
					action_type: SupplyRemovedActionType::SystemLiquidation
				}),
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
			fund_account_and_add_supply(EXTRA_COLLATERAL);

			// We don't bother restarting liquidation swaps to incorporate the extra supply,
			// instead the supply is simply added to the pool:
			assert_eq!(
				get_collateral_in_supply_pools::<Test>(&BORROWER),
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
			fund_account_and_add_supply(EXTRA_COLLATERAL_2);

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

			// At soft liquidation init, only enough collateral is collected to cover the
			// outstanding principal at the new oracle rate plus the soft-slippage buffer.
			// This test disables the liquidation fee, so it doesn't contribute to the buffer.
			let owed_after_swap_1 = PRINCIPAL + ORIGINATION_FEE - RECOVERED_PRINCIPAL_1;
			let liquidation_input = required_collateral_with_buffer(
				owed_after_swap_1 * NEW_SWAP_RATE,
				bps_to_permill(CONFIG.soft_liquidation_max_oracle_slippage),
			);

			// What's left in the pool after the soft liquidation swap is initiated: the
			// total available collateral (everything we've supplied minus what was already
			// swapped in the aborted hard liquidation) less the new swap input.
			let total_available_at_soft_init =
				INIT_COLLATERAL + EXTRA_COLLATERAL + EXTRA_COLLATERAL_2 - SWAPPED_COLLATERAL_1;
			let pool_remainder_after_init = total_available_at_soft_init - liquidation_input;

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
					borrower_id: BORROWER,
					reason: LiquidationCompletionReason::LtvChange,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
					loan_id: LOAN_ID,
					amount: RECOVERED_PRINCIPAL_1,
					action_type: LoanRepaidActionType::Liquidation { swap_request_id: LIQUIDATION_SWAP_1 }
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
					lender_id: BORROWER,
					asset: COLLATERAL_ASSET,
					amount,
					action_type: SupplyAddedActionType::SystemLiquidationUnusedAmount {
						loan_id: LOAN_ID,
						swap_request_id: LIQUIDATION_SWAP_1,
					},
				}) if amount == INIT_COLLATERAL - SWAPPED_COLLATERAL_1,
				RuntimeEvent::LendingPools(Event::<Test>::LendingFundsRemoved {
					lender_id: BORROWER,
					asset: COLLATERAL_ASSET,
					unlocked_amount,
					action_type: SupplyRemovedActionType::SystemLiquidation
				}) if unlocked_amount == liquidation_input,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationInitiated {
					borrower_id: BORROWER,
					ref swaps,
					liquidation_type: LiquidationType::Soft,
				}) if swaps == &BTreeMap::from([(LOAN_ID, vec![LIQUIDATION_SWAP_2])])
			);

			// The newly-initiated swap pulls just the required amount, not all available
			// collateral:
			assert_eq!(
				MockSwapRequestHandler::<Test>::get_swap_requests(),
				BTreeMap::from([(LIQUIDATION_SWAP_2, swap_request(liquidation_input, 5, false))])
			);

			// The unused remainder stays in the supply pool:
			assert_eq!(
				get_collateral_in_supply_pools::<Test>(&BORROWER),
				BTreeMap::from([(COLLATERAL_ASSET, pool_remainder_after_init)])
			);

			// Adding collateral once more should result in a transition from
			// soft liquidation to a healthy loan:
			fund_account_and_add_supply(EXTRA_COLLATERAL_3);

			assert!(
				get_account().derive_ltv(&OraclePriceCache::default()).unwrap() <
					CONFIG.ltv_thresholds.target.into()
			);

			// Simulate partial liquidation:
			MockSwapRequestHandler::<Test>::set_swap_request_progress(
				LIQUIDATION_SWAP_2,
				SwapExecutionProgress {
					remaining_input_amount: liquidation_input - SWAPPED_COLLATERAL_2,
					accumulated_output_amount: RECOVERED_PRINCIPAL_2,
				},
			);
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(
				get_account(),
				LoanAccount {
					borrower_id: BORROWER,
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
							},
							broker: None,
						}
					)]),
					liquidation_status: LiquidationStatus::NoLiquidation,
					voluntary_liquidation_requested: false
				}
			);

			// Collateral returned to supply pools after liquidation abort:
			assert_eq!(
				get_collateral_in_supply_pools::<Test>(&BORROWER),
				BTreeMap::from([(
					COLLATERAL_ASSET,
					INIT_COLLATERAL + EXTRA_COLLATERAL + EXTRA_COLLATERAL_2 + EXTRA_COLLATERAL_3 -
						SWAPPED_COLLATERAL_1 -
						SWAPPED_COLLATERAL_2
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
					amount: RECOVERED_PRINCIPAL_2,
					action_type: LoanRepaidActionType::Liquidation {
						swap_request_id: LIQUIDATION_SWAP_2
					}
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
				RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
					lender_id: BORROWER,
					action_type: SupplyAddedActionType::SystemLiquidationExcessAmount { .. },
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

			// The account is removed as the loan is settled and liquidation is done.
			// All of swap output amount goes towards user's supply:
			assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET)
					.unwrap()
					.get_supply_position_for_account(&BORROWER)
					.unwrap(),
				SWAP_OUTPUT_AMOUNT
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
			// funds returned to supply pools, and the account removed:
			assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);
			assert_eq!(
				GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
					.unwrap()
					.get_supply_position_for_account(&BORROWER)
					.unwrap(),
				INIT_COLLATERAL - EXECUTED_COLLATERAL
			);
			assert_eq!(
				GeneralLendingPools::<Test>::get(LOAN_ASSET)
					.unwrap()
					.get_supply_position_for_account(&BORROWER)
					.unwrap(),
				SWAP_OUTPUT_AMOUNT
			);

			assert_event_sequence!(
				Test,
				RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
					borrower_id: BORROWER,
					reason: LiquidationCompletionReason::LtvChange,
				}),
				RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
					lender_id: BORROWER,
					action_type: SupplyAddedActionType::SystemLiquidationExcessAmount {
						loan_id: LOAN_ID,
						swap_request_id: LIQUIDATION_SWAP_1
					},
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

		// Voluntary liquidation only collects the owed principal (in USD) plus the
		// soft-slippage buffer; the rest stays in the supply pool. Voluntary liquidations
		// don't charge the liquidation fee, so it doesn't contribute to the buffer.
		let liquidation_input = required_collateral_with_buffer(
			(PRINCIPAL + ORIGINATION_FEE) * SWAP_RATE,
			bps_to_permill(CONFIG.soft_liquidation_max_oracle_slippage),
		);
		// Pick a SWAPPED_COLLATERAL that fits within the liquidation input and still
		// produces enough principal to fully repay the loan with some excess.
		let swapped_collateral = liquidation_input - 100_000_000;
		let swapped_principal = swapped_collateral / SWAP_RATE;

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
						remaining_input_amount: liquidation_input - swapped_collateral,
						accumulated_output_amount: swapped_principal,
					},
				);
			})
			.then_execute_at_next_block(|_| {
				let excess_principal = swapped_principal - TOTAL_TO_REPAY;
				let remaining_input = liquidation_input - swapped_collateral;

				// The account is removed once the loan is settled and voluntary liquidation ends.
				// Borrower's collateral pool position = (remainder left in the pool at init) +
				// (remaining-input returned by the aborted swap) = INIT_COLLATERAL - SWAPPED.
				assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);
				assert_eq!(
					GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					INIT_COLLATERAL - swapped_collateral
				);
				assert_eq!(
					GeneralLendingPools::<Test>::get(LOAN_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					excess_principal
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
						action_type: LoanRepaidActionType::Liquidation {
							swap_request_id: LIQUIDATION_SWAP
						}
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
						lender_id: BORROWER,
						action_type: SupplyAddedActionType::SystemLiquidationExcessAmount { .. },
						..
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
						lender_id: BORROWER,
						asset: COLLATERAL_ASSET,
						amount,
						action_type: SupplyAddedActionType::SystemLiquidationUnusedAmount {
							loan_id: LOAN_ID,
							swap_request_id: LIQUIDATION_SWAP,
						},
					}) if amount == remaining_input,
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

		// Voluntary liquidation only collects the owed principal plus the soft-slippage
		// buffer (no liquidation fee on voluntary liquidations), so the swap input is much
		// smaller than INIT_COLLATERAL.
		let liquidation_input = required_collateral_with_buffer(
			(PRINCIPAL + ORIGINATION_FEE) * SWAP_RATE,
			bps_to_permill(CONFIG.soft_liquidation_max_oracle_slippage),
		);

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
						remaining_input_amount: liquidation_input - SWAPPED_COLLATERAL,
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
						// The loan is partially repaid:
						loans: BTreeMap::from([(
							LOAN_ID,
							GeneralLoan {
								id: LOAN_ID,
								asset: LOAN_ASSET,
								owed_principal: PRINCIPAL + ORIGINATION_FEE - SWAPPED_PRINCIPAL,
								created_at_block: INIT_BLOCK,
								pending_interest: Default::default(),
								broker: None,
							}
						)]),
						liquidation_status: LiquidationStatus::NoLiquidation,
						// The flag has been reset:
						voluntary_liquidation_requested: false
					}
				);
				// Part of collateral was returned to supply pool:
				assert_eq!(
					get_collateral_in_supply_pools::<Test>(&BORROWER),
					BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL - SWAPPED_COLLATERAL)])
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
						action_type: LoanRepaidActionType::Liquidation {
							swap_request_id: LIQUIDATION_SWAP
						}
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

		// Each liquidation only collects the owed principal (in USD) plus the soft/voluntary
		// slippage buffer, capped at the available collateral. With ETH at $1, the
		// required-with-slippage value (in fine USD) equals the ETH amount we'd pull in.
		let slippage_bps = CONFIG.soft_liquidation_max_oracle_slippage;
		// Swap 1: voluntary, owed = (PRINCIPAL + ORIGINATION_FEE) BTC at SWAP_RATE.
		// Voluntary liquidations don't charge the liquidation fee.
		let liquidation_input_1 = required_collateral_with_buffer(
			(PRINCIPAL + ORIGINATION_FEE) * SWAP_RATE,
			bps_to_permill(slippage_bps),
		);
		// Swap 2: forced soft after the price spike to NEW_SWAP_RATE, owed has dropped by
		// SWAPPED_PRINCIPAL_1 from the partial swap 1.
		let liquidation_input_2 = required_collateral_with_buffer(
			(PRINCIPAL + ORIGINATION_FEE - SWAPPED_PRINCIPAL_1) * NEW_SWAP_RATE,
			bps_to_permill(slippage_bps).saturating_add(CONFIG.liquidation_fee(LOAN_ASSET)),
		);
		// Swap 3: back to voluntary at SWAP_RATE, owed = owed_after_liquidation_2.
		let liquidation_input_3 = required_collateral_with_buffer(
			owed_after_liquidation_2 * SWAP_RATE,
			bps_to_permill(slippage_bps),
		);

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
						remaining_input_amount: liquidation_input_1 - SWAPPED_COLLATERAL_1,
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
							pending_interest: Default::default(),
							broker: None,
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

				// The new soft swap pulls only the required-with-slippage amount; the rest
				// of the recovered collateral stays in the supply pool.
				let pool_leftover_after_soft_init =
					INIT_COLLATERAL - SWAPPED_COLLATERAL_1 - liquidation_input_2;
				assert_eq!(
					get_collateral_in_supply_pools::<Test>(&BORROWER),
					BTreeMap::from([(COLLATERAL_ASSET, pool_leftover_after_soft_init)])
				);

				assert_eq!(
					MockSwapRequestHandler::<Test>::get_swap_requests(),
					BTreeMap::from([(
						LIQUIDATION_SWAP_2,
						mock_liquidation_swap(liquidation_input_2, 3)
					)])
				);

				let swap_1_remaining = liquidation_input_1 - SWAPPED_COLLATERAL_1;
				assert_event_sequence!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
						borrower_id: BORROWER,
						reason: LiquidationCompletionReason::LtvChange,
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
						loan_id: LOAN_ID,
						amount: SWAPPED_PRINCIPAL_1,
						action_type: LoanRepaidActionType::Liquidation {
							swap_request_id: LIQUIDATION_SWAP_1
						}
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
						lender_id: BORROWER,
						asset: COLLATERAL_ASSET,
						amount,
						action_type: SupplyAddedActionType::SystemLiquidationUnusedAmount {
							loan_id: LOAN_ID,
							swap_request_id: LIQUIDATION_SWAP_1,
						},
					}) if amount == swap_1_remaining,
					RuntimeEvent::LendingPools(Event::<Test>::LendingFundsRemoved {
						lender_id: BORROWER,
						asset: COLLATERAL_ASSET,
						unlocked_amount,
						action_type: SupplyRemovedActionType::SystemLiquidation
					}) if unlocked_amount == liquidation_input_2,
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
						remaining_input_amount: liquidation_input_2 - SWAPPED_COLLATERAL_2,
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
							pending_interest: Default::default(),
							broker: None,
						}
					)])
				);

				// The flag is still set:
				assert!(get_account().voluntary_liquidation_requested);

				// The new voluntary swap pulls only the required-with-slippage amount; the
				// rest of the recovered collateral stays in the supply pool.
				let pool_leftover_after_voluntary_reinit = INIT_COLLATERAL -
					SWAPPED_COLLATERAL_1 -
					SWAPPED_COLLATERAL_2 -
					liquidation_input_3;
				assert_eq!(
					get_collateral_in_supply_pools::<Test>(&BORROWER),
					BTreeMap::from([(COLLATERAL_ASSET, pool_leftover_after_voluntary_reinit)])
				);

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
						mock_liquidation_swap(liquidation_input_3, 2)
					)])
				);

				let swap_2_remaining = liquidation_input_2 - SWAPPED_COLLATERAL_2;
				assert_event_sequence!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
						borrower_id: BORROWER,
						reason: LiquidationCompletionReason::LtvChange,
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken { .. }),
					RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
						loan_id: LOAN_ID,
						action_type: LoanRepaidActionType::Liquidation {
							swap_request_id: LIQUIDATION_SWAP_2
						},
						amount,
					}) if amount == SWAPPED_PRINCIPAL_2 - liquidation_fee,
					RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
						lender_id: BORROWER,
						asset: COLLATERAL_ASSET,
						amount,
						action_type: SupplyAddedActionType::SystemLiquidationUnusedAmount {
							loan_id: LOAN_ID,
							swap_request_id: LIQUIDATION_SWAP_2,
						},
					}) if amount == swap_2_remaining,
					RuntimeEvent::LendingPools(Event::<Test>::LendingFundsRemoved {
						lender_id: BORROWER,
						asset: COLLATERAL_ASSET,
						unlocked_amount,
						action_type: SupplyRemovedActionType::SystemLiquidation
					}) if unlocked_amount == liquidation_input_3,
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

				// The account is removed once the loan is settled and voluntary liquidation ends:
				assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);
				assert_eq!(
					GeneralLendingPools::<Test>::get(LOAN_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					SWAPPED_PRINCIPAL_EXTRA
				);

				assert_event_sequence!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
						borrower_id: BORROWER,
						reason: LiquidationCompletionReason::FullySwapped,
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
						loan_id: LOAN_ID,
						action_type: LoanRepaidActionType::Liquidation { swap_request_id: LIQUIDATION_SWAP_3 },
						amount,
					}) if amount == owed_after_liquidation_2,
					RuntimeEvent::LendingPools(Event::<Test>::LendingFundsAdded {
						lender_id: BORROWER,
						action_type: SupplyAddedActionType::SystemLiquidationExcessAmount { .. },
						..
					}),
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
		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.with_default_loan()
			.execute_with(|| {
				// Fully repay the loan; the account is removed along with the last loan:
				MockBalance::credit_account(&BORROWER, LOAN_ASSET, ORIGINATION_FEE);
				assert_ok!(LendingPools::try_making_repayment(
					&BORROWER,
					LOAN_ID,
					RepaymentAmount::Full
				));
				assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);

				assert_noop!(
					LendingPools::initiate_voluntary_liquidation(RuntimeOrigin::signed(BORROWER)),
					Error::<Test>::LoanAccountNotFound
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

		let try_to_borrow = || LendingPools::new_loan(LP, LOAN_ASSET, PRINCIPAL, None);

		new_test_ext().with_funded_pool(2 * INIT_POOL_AMOUNT).execute_with(|| {
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

			MockBalance::credit_account(&LP, COLLATERAL_ASSET, 10 * INIT_COLLATERAL);

			if GeneralLendingPools::<Test>::get(COLLATERAL_ASSET).is_none() {
				assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET));
			}
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LP),
				COLLATERAL_ASSET,
				10 * INIT_COLLATERAL,
			));

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

	// Regression test for PRO-2860.
	//
	// When `update_liquidation_status` returns `Err(LiquidationsDisabled)` from inside
	// `lending_upkeep`'s `try_mutate_exists` closure, the prior
	// `check_low_ltv_penalty_and_collect_interest` call writes to external storage
	// (lending pool totals, `PendingNetworkFees`). Those writes must roll back together
	// with the borrower-side mutations (which `try_mutate_exists` discards on `Err`),
	// otherwise the pool/network books reflect interest the borrower no longer owes.
	#[test]
	fn upkeep_does_not_commit_fees_when_liquidations_are_disabled() {
		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.with_default_loan()
			.then_execute_with(|_| {
				// Snapshot pool / fee / loan state right after loan creation — once the
				// origination fee has been taken. With the fix, this is exactly what we
				// expect to see after upkeep runs while liquidations are disabled.
				let pool_before = GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap();
				let pending_network_fees_before = PendingNetworkFees::<Test>::get(LOAN_ASSET);
				let loan_account_before = LoanAccounts::<Test>::get(BORROWER).unwrap();

				// Trigger the bug's preconditions:
				//  - liquidations disabled via safe mode,
				//  - collection threshold low enough that any accrued interest gets collected,
				//  - oracle price moved so that LTV exceeds the hard liquidation threshold (forces
				//    `update_liquidation_status` down the `LiquidationsDisabled` path).
				MockRuntimeSafeMode::set_safe_mode(PalletSafeMode {
					liquidations_enabled: false,
					..PalletSafeMode::code_green()
				});
				assert_ok!(Pallet::<Test>::update_pallet_config(
					RuntimeOrigin::root(),
					bounded_vec![PalletConfigUpdate::SetInterestCollectionThresholdUsd(1)],
				));
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE * 20);

				System::reset_events();

				(pool_before, pending_network_fees_before, loan_account_before)
			})
			// Process up to the first interest payment block so that upkeep runs
			// `derive_and_charge_interest` followed by
			// `check_low_ltv_penalty_and_collect_interest` followed by
			// `update_liquidation_status` (which errors).
			.then_process_blocks_until_block(
				INIT_BLOCK + CONFIG.interest_payment_interval_blocks as u64,
			)
			.then_execute_with(|(pool_before, fees_before, loan_account_before)| {
				// Sanity: liquidation must not have started.
				assert_eq!(
					LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
					LiquidationStatus::NoLiquidation
				);
				assert_no_matching_event!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationInitiated { .. })
				);

				// The invariant: because the per-account closure returned `Err`, the
				// upkeep tick for this borrower must be a no-op across all storage.
				assert_eq!(
					GeneralLendingPools::<Test>::get(LOAN_ASSET).unwrap(),
					pool_before,
					"pool accounting moved despite the loan-account mutations being rolled back",
				);
				assert_eq!(
					PendingNetworkFees::<Test>::get(LOAN_ASSET),
					fees_before,
					"network fees were collected without a matching borrower receivable",
				);
				assert_eq!(
					LoanAccounts::<Test>::get(BORROWER).unwrap(),
					loan_account_before,
					"loan account changed but upkeep was supposed to be a no-op",
				);
				assert_no_matching_event!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::InterestTaken { .. })
				);
			});
	}
}

mod liquidation_math_tests {
	use super::*;

	const ETH: Asset = Asset::Eth;
	const BTC: Asset = Asset::Btc;
	const SOL: Asset = Asset::Sol;
	const USDC: Asset = Asset::Usdc;

	fn estimates(
		total_owed_usd: AssetAmount,
		slippage_bps: BasisPoints,
		positions: &[(Asset, AssetAmount, AssetAmount)],
	) -> Vec<(Asset, AssetAmount)> {
		compute_per_asset_liquidation_estimates(
			required_collateral_with_buffer(total_owed_usd, bps_to_permill(slippage_bps)),
			positions,
		)
	}

	#[test]
	fn required_collateral_with_no_buffer_equals_owed() {
		assert_eq!(required_collateral_with_buffer(1_000, Permill::zero()), 1_000);
	}

	#[test]
	fn required_collateral_with_buffer_inflates_with_ceil_rounding() {
		// 1_000 / (1 - 0.005) = 1_005.025... -> 1_006 (ceil).
		assert_eq!(required_collateral_with_buffer(1_000, bps_to_permill(50)), 1_006);
		// 1_000 / (1 - 0.05) = 1_052.63... -> 1_053 (ceil).
		assert_eq!(required_collateral_with_buffer(1_000, bps_to_permill(500)), 1_053);
		// 65 bps combined buffer: 1_000 / (1 - 0.0065) = 1_006.54... -> 1_007 (ceil).
		assert_eq!(required_collateral_with_buffer(1_000, bps_to_permill(65)), 1_007);
	}

	#[test]
	fn required_collateral_saturates_when_buffer_is_total_loss() {
		// A 100% buffer means any swap could return zero, so no finite amount is enough.
		assert_eq!(required_collateral_with_buffer(1_000, Permill::one()), AssetAmount::MAX);
	}

	#[test]
	fn empty_positions_give_no_estimates() {
		assert!(estimates(1_000, 50, &[]).is_empty());
	}

	#[test]
	fn single_asset_with_excess_takes_only_required_with_buffer() {
		// Owed 1_000 USD, slippage 0.5%, available 10_000 USD worth of ETH (10_000 ETH @ $1).
		// Required = ceil(1_000 / 0.995) = 1_006. ETH price is 1, so amount equals USD.
		assert_eq!(estimates(1_000, 50, &[(ETH, 10_000, 10_000)]), vec![(ETH, 1_006)]);
	}

	#[test]
	fn single_asset_with_deficit_takes_everything() {
		// Owed 1_000 USD, slippage 0.5% → required 1_006 USD. Only 800 USD available.
		assert_eq!(estimates(1_000, 50, &[(ETH, 800, 800)]), vec![(ETH, 800)]);
	}

	#[test]
	fn at_required_threshold_takes_all() {
		// Exactly enough collateral (in USD) to cover the slippage-buffered owed amount.
		assert_eq!(estimates(1_000, 50, &[(ETH, 1_006, 1_006)]), vec![(ETH, 1_006)]);
	}

	#[test]
	fn zero_owed_takes_nothing() {
		assert!(estimates(0, 50, &[(ETH, 1_000, 1_000)]).is_empty());
	}

	#[test]
	fn two_assets_with_equal_usd_split_evenly() {
		// Owed 1_000 USD, slippage 0%, available: 100 ETH @ $5 and 50 BTC @ $10 = $500 each.
		// target = 1_000, total_avail = 1_000 → take everything.
		assert_eq!(
			estimates(1_000, 0, &[(ETH, 100, 500), (BTC, 50, 500)]),
			vec![(ETH, 100), (BTC, 50)]
		);
	}

	#[test]
	fn two_assets_drawn_proportionally_to_usd_share() {
		// ETH: 100 units @ $1 = $100 USD. SOL: 200 units @ $5 = $1000 USD. Total = $1100.
		// Owed 550 USD, slippage 0% → target = 550. ETH share = 100/1100 ≈ 9.09%, SOL = 90.91%.
		// ETH take = ceil(100 * 550 / 1100) = 50. SOL take = ceil(200 * 550 / 1100) = 100.
		assert_eq!(
			estimates(550, 0, &[(ETH, 100, 100), (SOL, 200, 1_000)]),
			vec![(ETH, 50), (SOL, 100)]
		);
	}

	#[test]
	fn three_assets_proportional_split_with_slippage_buffer() {
		// Total available: 1_000 ETH ($2_000) + 500 SOL ($500) + 1_000_000 USDC ($1_000_000) =
		// $1_002_500. Owed 500_000 USD, slippage 0.5% → required = ceil(500_000 / 0.995) =
		// 502_513. Per-asset: ETH = ceil(1_000 * 502_513 / 1_002_500) ≈ 502, SOL = ceil(500 *
		// 502_513 / 1_002_500) ≈ 251, USDC = ceil(1_000_000 * 502_513 / 1_002_500) ≈ 501_260.
		assert_eq!(
			estimates(
				500_000,
				50,
				&[(ETH, 1_000, 2_000), (SOL, 500, 500), (USDC, 1_000_000, 1_000_000)],
			),
			vec![(ETH, 502), (SOL, 251), (USDC, 501_260)]
		);
	}

	#[test]
	fn rounds_up_so_collected_usd_meets_target() {
		// ETH and SOL each contribute $50 USD with awkward unit counts that force rounding.
		// Owed 33 USD, slippage 0% → target 33 USD, total avail 100 USD. Each share = 33/2 USD
		// rounded up. ETH: ceil(7 * 33 / 100) = ceil(2.31) = 3. SOL: ceil(13 * 33 / 100) =
		// ceil(4.29) = 5. The sum (8 + leftover from rounding) covers the target.
		assert_eq!(estimates(33, 0, &[(ETH, 7, 50), (SOL, 13, 50)]), vec![(ETH, 3), (SOL, 5)]);
	}

	#[test]
	fn deficit_in_some_assets_still_takes_all_when_total_required_exceeds_available() {
		// Required 100_000 USD with 0% slippage but only $300 + $200 = $500 available → take all.
		assert_eq!(
			estimates(100_000, 0, &[(ETH, 30, 300), (SOL, 100, 200)],),
			vec![(ETH, 30), (SOL, 100)]
		);
	}

	#[test]
	fn extreme_slippage_forces_take_all() {
		// 99.5% slippage means required ≈ 200x owed → almost certainly exceeds available.
		assert_eq!(estimates(100, 9_950, &[(ETH, 50, 50)]), vec![(ETH, 50)]);
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
	fn request_loan() {
		new_test_ext().with_funded_pool(2 * INIT_POOL_AMOUNT).execute_with(|| {
			setup_accounts();

			// Supply collateral for both users:
			if GeneralLendingPools::<Test>::get(COLLATERAL_ASSET).is_none() {
				assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET));
			}
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(WHITELISTED_USER),
				COLLATERAL_ASSET,
				INIT_COLLATERAL,
			));

			assert_ok!(LendingPools::request_loan(
				RuntimeOrigin::signed(WHITELISTED_USER),
				LOAN_ASSET,
				PRINCIPAL,
				None,
			));

			assert_noop!(
				LendingPools::request_loan(
					RuntimeOrigin::signed(NON_WHITELISTED_USER),
					LOAN_ASSET,
					PRINCIPAL,
					None,
				),
				Error::<Test>::AccountNotWhitelisted
			);
		});
	}
}

#[test]
fn network_fee_inflates_liquidation_buffer() {
	// Single-asset collateral + single loan + voluntary liquidation isolates the
	// `worst_case_network_fee_rate` term: the liquidation fee is zeroed for voluntary, and
	// a single position skips the per-asset proportional split. So the collateral pulled
	// must equal `required_collateral_with_buffer(owed_usd, slippage + network_fee)` exactly.
	//
	// Soft slippage (50 bps from default config) + 500 bps network fee = 550 bps buffer.
	// owed_usd = 945 -> required = ceil(945 / (1 - 0.055)) = 1000.

	const BORROWER: u64 = 1;
	const OWED_USD: AssetAmount = 945;

	new_test_ext().execute_with(|| {
		// price_fine = 1 → token amount equals USD value in these tests.
		MockPriceFeedApi::set_price_usd_fine(Asset::Btc, 1);
		MockPriceFeedApi::set_price_usd_fine(Asset::Usdc, 1);

		assert_ok!(LendingPools::new_lending_pool(Asset::Usdc));
		assert_ok!(supply_funds::<Test>(
			BORROWER,
			Asset::Usdc,
			10_000,
			SupplyAddedActionType::Manual,
		));

		MockNetworkFeeApi::set_network_fee_rate(Permill::from_percent(5));

		let mut loan_account = LoanAccount::<Test> {
			borrower_id: BORROWER,
			loans: BTreeMap::from([(
				LoanId(0),
				GeneralLoan {
					id: LoanId(0),
					asset: Asset::Btc,
					created_at_block: 0,
					owed_principal: OWED_USD,
					pending_interest: Default::default(),
					broker: None,
				},
			)]),
			liquidation_status: LiquidationStatus::NoLiquidation,
			voluntary_liquidation_requested: false,
		};

		let collateral = loan_account
			.prepare_collateral_for_liquidation(
				&OraclePriceCache::default(),
				LiquidationType::SoftVoluntary,
			)
			.unwrap();

		assert_eq!(collateral.len(), 1);
		assert_eq!(collateral[0].collateral_amount, 1_000);
	});
}

#[test]
fn init_liquidation_swaps_test() {
	// Test that we handle multi-asset collateral + multi-asset loans correctly in case of
	// liquidation. We collect just enough collateral to cover the outstanding principal plus
	// the (soft) max liquidation slippage, taking from each collateral asset proportionally to
	// its share of available USD value, then split the collected collateral across the loans
	// proportionally to the USD value of each loan (the expected loan ratio is 1:5 in this case).
	//
	// Loan principal: 20 BTC @ 100k = $2,000,000 + 2000 SOL @ 200 = $400,000 → $2,400,000.
	// Soft slippage: 50 bps → required = ceil(2_400_000 * 10_000 / 9_950) = $2_412_061.
	// Collateral: 500 ETH @ 4k = $2_000_000 + 1_000_000 USDC @ 1 = $1_000_000 → $3_000_000.
	// Per-asset (rounded up): ETH = ceil(500 * 2_412_061 / 3_000_000) = 403,
	// USDC = ceil(1_000_000 * 2_412_061 / 3_000_000) = 804_021.
	// Then split per loan in 5:1 (BTC:SOL), distributing rounding remainders deterministically.

	const LOAN_1: LoanId = LoanId(0);
	const LOAN_2: LoanId = LoanId(1);

	const SWAP_1: SwapRequestId = SwapRequestId(0);
	const SWAP_2: SwapRequestId = SwapRequestId(1);
	const SWAP_3: SwapRequestId = SwapRequestId(2);
	const SWAP_4: SwapRequestId = SwapRequestId(3);

	const BORROWER: u64 = 1;

	let mut loan_account = LoanAccount::<Test> {
		borrower_id: BORROWER,
		loans: BTreeMap::from([
			(
				LOAN_1,
				GeneralLoan {
					id: LOAN_ID,
					asset: Asset::Btc,
					created_at_block: 0,
					owed_principal: 20,
					pending_interest: Default::default(),
					broker: None,
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
					broker: None,
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

		// Supply collateral into lending pools (replaces legacy_collateral)
		assert_ok!(LendingPools::new_lending_pool(Asset::Eth));
		assert_ok!(LendingPools::new_lending_pool(Asset::Usdc));
		assert_ok!(supply_funds::<Test>(BORROWER, Asset::Eth, 500, SupplyAddedActionType::Manual));
		assert_ok!(supply_funds::<Test>(
			BORROWER,
			Asset::Usdc,
			1_000_000,
			SupplyAddedActionType::Manual
		));

		let collateral = loan_account
			.prepare_collateral_for_liquidation(&OraclePriceCache::default(), LiquidationType::Soft)
			.unwrap();
		assert_ok!(loan_account.init_liquidation_swaps(
			&BORROWER,
			collateral,
			LiquidationType::Soft,
			&OraclePriceCache::default(),
		));

		let expected_swaps = [
			(SWAP_1, LOAN_1, Asset::Eth, Asset::Btc, 335),
			(SWAP_2, LOAN_2, Asset::Eth, Asset::Sol, 68),
			(SWAP_3, LOAN_1, Asset::Usdc, Asset::Btc, 670_354),
			(SWAP_4, LOAN_2, Asset::Usdc, Asset::Sol, 134_071),
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
fn can_add_but_not_remove_supply_with_stale_price_if_has_loans() {
	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.with_default_loan()
		.execute_with(|| {
			// BORROWER now has a loan in LOAN_ASSET with COLLATERAL_ASSET as collateral (the
			// supply position doubles as collateral in this lending model).
			MockPriceFeedApi::set_stale(COLLATERAL_ASSET, true);

			// Should still be able to add supply with stale price
			MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BORROWER),
				COLLATERAL_ASSET,
				INIT_COLLATERAL,
			));

			// But should not be able to remove supply with stale price: computing the LTV
			// headroom requires fresh oracle prices.
			assert_noop!(
				LendingPools::remove_lender_funds(
					RuntimeOrigin::signed(BORROWER),
					COLLATERAL_ASSET,
					Some(INIT_COLLATERAL / 2),
				),
				Error::<Test>::OraclePriceUnavailable
			);
		});
}

/// Makes sure that stale oracle prices don't affect lenders' ability
/// to remove funds as long as they don't have any loans (no need to
/// compute LTV).
#[test]
fn can_remove_supply_with_stale_price_if_no_loans() {
	const SUPPLY_ASSET: Asset = Asset::Eth;

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		MockPriceFeedApi::set_price_usd_fine(SUPPLY_ASSET, 1);
		MockPriceFeedApi::set_stale(SUPPLY_ASSET, false);

		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
		MockBalance::credit_account(&BORROWER, SUPPLY_ASSET, INIT_COLLATERAL * 2);

		assert_ok!(LendingPools::new_lending_pool(SUPPLY_ASSET));

		// Add supply while price is fresh
		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BORROWER),
			SUPPLY_ASSET,
			INIT_COLLATERAL,
		));

		// Make the price stale
		MockPriceFeedApi::set_stale(SUPPLY_ASSET, true);

		// Should be able to remove supply with stale price if the user has no loans
		assert_ok!(LendingPools::remove_lender_funds(
			RuntimeOrigin::signed(BORROWER),
			SUPPLY_ASSET,
			None,
		));
	});
}

/// Same as above but with a partial-amount withdrawal: stale prices should
/// not block a no-loans lender from removing a specific amount either.
#[test]
fn can_partially_remove_supply_with_stale_price_if_no_loans() {
	const SUPPLY_ASSET: Asset = Asset::Eth;

	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		MockPriceFeedApi::set_price_usd_fine(SUPPLY_ASSET, 1);
		MockPriceFeedApi::set_stale(SUPPLY_ASSET, false);

		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
		MockBalance::credit_account(&BORROWER, SUPPLY_ASSET, INIT_COLLATERAL * 2);

		assert_ok!(LendingPools::new_lending_pool(SUPPLY_ASSET));

		// Add supply while price is fresh
		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BORROWER),
			SUPPLY_ASSET,
			INIT_COLLATERAL,
		));

		// Make the price stale
		MockPriceFeedApi::set_stale(SUPPLY_ASSET, true);

		// Should be able to partially remove supply with stale price if the user has
		// no loans (no LTV check is needed).
		assert_ok!(LendingPools::remove_lender_funds(
			RuntimeOrigin::signed(BORROWER),
			SUPPLY_ASSET,
			Some(INIT_COLLATERAL / 2),
		));
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
		assert_ok!(create_loan_and_supply_collateral(
			BORROWER,
			LOAN_ASSET,
			PRINCIPAL,
			BTreeMap::from([(COLLATERAL_ASSET_1, INIT_COLLATERAL)])
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
			LendingPools::expand_loan(RuntimeOrigin::signed(BORROWER), LoanId(0), PRINCIPAL / 2,),
			Error::<Test>::OraclePriceUnavailable
		);

		// Or create a new loan
		assert_noop!(
			LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL, None),
			Error::<Test>::OraclePriceUnavailable
		);

		// Because we have a loan open with a stale collateral price, we also cannot create a loan,
		// even if the price for the new collateral asset and loan asset are fresh.
		assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET_2));
		assert_ok!(supply_funds::<Test>(
			BORROWER,
			COLLATERAL_ASSET_2,
			INIT_COLLATERAL,
			SupplyAddedActionType::Manual,
		));
		assert_noop!(
			LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL, None),
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
					create_loan_and_supply_collateral(
						BORROWER,
						LOAN_ASSET,
						PRINCIPAL,
						BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
					),
					Ok(LOAN_ID)
				);

				assert_eq!(
					create_loan_and_supply_collateral(
						BORROWER_2,
						LOAN_ASSET_2,
						PRINCIPAL_2,
						BTreeMap::from([(COLLATERAL_ASSET_2, INIT_COLLATERAL_2)])
					),
					Ok(LOAN_ID_2)
				);

				// Should get info only for the specified account:
				assert_eq!(
					super::rpc::get_loan_accounts::<Test>(Some(BORROWER)),
					vec![RpcLoanAccount {
						account: BORROWER,
						ltv_ratio: Some(FixedU64::from_rational(750_075, 1_000_000)),
						collateral: vec![AssetAndAmount {
							asset: COLLATERAL_ASSET,
							amount: INIT_COLLATERAL
						}],
						loans: vec![RpcLoan {
							loan_id: LOAN_ID,
							loan_type: LoanType::User(BORROWER),
							asset: LOAN_ASSET,
							created_at: INIT_BLOCK as u32,
							principal_amount: PRINCIPAL + ORIGINATION_FEE,
							broker: None,
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
							ltv_ratio: Some(FixedU64::from_rational(1_173_483_514, 1_000_000_000)),
							// NOTE: all of collateral is in liquidation swaps, but we include
							// any amount that has not been swapped yet:
							collateral: vec![AssetAndAmount {
								asset: COLLATERAL_ASSET_2,
								amount: INIT_COLLATERAL_2 - EXECUTED_COLLATERAL_2,
							}],
							loans: vec![RpcLoan {
								loan_id: LOAN_ID_2,
								loan_type: LoanType::User(BORROWER_2),
								asset: LOAN_ASSET_2,
								created_at: INIT_BLOCK as u32,
								// NOTE: we account for the principal asset already swapped in
								// liquidation swaps:
								principal_amount: PRINCIPAL_2 +
									ORIGINATION_FEE_2 + pool_interest_2 +
									network_interest_2 - ACCUMULATED_OUTPUT_AMOUNT,
								broker: None,
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
							// LTV slightly increased due to interest payment:
							ltv_ratio: Some(FixedU64::from_rational(750_075_090, 1_000_000_000)),
							collateral: vec![AssetAndAmount {
								asset: COLLATERAL_ASSET,
								amount: INIT_COLLATERAL
							}],
							loans: vec![RpcLoan {
								loan_id: LOAN_ID,
								loan_type: LoanType::User(BORROWER),
								asset: LOAN_ASSET,
								created_at: INIT_BLOCK as u32,
								principal_amount: PRINCIPAL +
									ORIGINATION_FEE + pool_interest_1 +
									network_interest_1,
								broker: None,
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
						utilisation_rate: utilisation_after_interest_pool_1,
						utilisation_cap: Permill::one(),
						current_interest_rate: Permill::from_parts(53_335) +
							CONFIG.network_fee_contributions.extra_interest, // 5.33% + 1%
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
			minimum_supply_amount_usd: 0,
			minimum_update_supply_amount_usd: 0,
			..LendingConfigDefault::get()
		});

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, COLLATERAL_AMOUNT);
		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

		// Supply collateral into lending pool
		if GeneralLendingPools::<Test>::get(COLLATERAL_ASSET).is_none() {
			assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET));
		}
		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BORROWER),
			COLLATERAL_ASSET,
			COLLATERAL_AMOUNT,
		));

		// Should not be able to create a loan below the minimum amount
		assert_noop!(
			LendingPools::new_loan(BORROWER, LOAN_ASSET, MIN_LOAN_AMOUNT_ASSET - 1, None,),
			Error::<Test>::AmountBelowMinimum
		);

		// A loan equal to or above the minimum amount should be fine
		assert_eq!(
			LendingPools::new_loan(BORROWER, LOAN_ASSET, MIN_LOAN_AMOUNT_ASSET, None,),
			Ok(LOAN_ID)
		);

		// Now try and repay an amount that would leave the loan below the minimum
		assert_noop!(
			LendingPools::try_making_repayment(&BORROWER, LOAN_ID, RepaymentAmount::Exact(1)),
			Error::<Test>::RemainingAmountBelowMinimum,
		);

		// If we expand the loan so a partial repayment would not take it below the minimum,
		assert_eq!(LendingPools::expand_loan(RuntimeOrigin::signed(BORROWER), LOAN_ID, 1), Ok(()));
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

mod supply_minimum_is_enforced {
	use super::*;

	const MIN_SUPPLY_AMOUNT_USD: AssetAmount = 1_000_000;
	const MIN_SUPPLY_UPDATE_AMOUNT_USD: AssetAmount = 100_000;

	// Min amount that can be supplied in pool's asset
	const MIN_SUPPLY_AMOUNT: AssetAmount = MIN_SUPPLY_AMOUNT_USD / SWAP_RATE;
	const MIN_SUPPLY_UPDATE_AMOUNT: AssetAmount = MIN_SUPPLY_UPDATE_AMOUNT_USD / SWAP_RATE;

	fn setup() {
		LendingConfig::<Test>::set(LendingConfiguration {
			minimum_supply_amount_usd: MIN_SUPPLY_AMOUNT_USD,
			minimum_update_supply_amount_usd: MIN_SUPPLY_UPDATE_AMOUNT_USD,
			..LendingConfigDefault::get()
		});

		MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);

		disable_whitelist();

		assert_ok!(LendingPools::new_lending_pool(LOAN_ASSET));

		MockBalance::credit_account(&LENDER, LOAN_ASSET, 2 * MIN_SUPPLY_AMOUNT);
	}

	#[test]
	fn initial_add_must_reach_minimum_supply_amount() {
		new_test_ext().execute_with(|| {
			setup();

			// Can't supply below the minimum supply amount:
			assert_noop!(
				LendingPools::add_lender_funds(
					RuntimeOrigin::signed(LENDER),
					LOAN_ASSET,
					MIN_SUPPLY_AMOUNT - 1
				),
				Error::<Test>::AmountBelowMinimum
			);

			// Can supply exactly the minimum amount:
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				MIN_SUPPLY_AMOUNT
			));
		});
	}

	#[test]
	fn subsequent_add_must_meet_minimum_update_amount() {
		new_test_ext().execute_with(|| {
			setup();

			// Bring the lender's supply to the minimum:
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				MIN_SUPPLY_AMOUNT
			));

			// Can't update with an amount smaller than the minimum update:
			assert_noop!(
				LendingPools::add_lender_funds(
					RuntimeOrigin::signed(LENDER),
					LOAN_ASSET,
					MIN_SUPPLY_UPDATE_AMOUNT - 1
				),
				Error::<Test>::AmountBelowMinimum
			);

			// Can update with exactly the minimum update amount:
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				MIN_SUPPLY_UPDATE_AMOUNT
			));
		});
	}

	#[test]
	fn remove_must_leave_at_least_minimum_supply_amount() {
		new_test_ext().execute_with(|| {
			setup();

			// Supply enough so we can remove part of it:
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				MIN_SUPPLY_AMOUNT + MIN_SUPPLY_UPDATE_AMOUNT
			));

			// Can't leave less than the minimum in the pool:
			assert_noop!(
				LendingPools::remove_lender_funds(
					RuntimeOrigin::signed(LENDER),
					LOAN_ASSET,
					Some(MIN_SUPPLY_UPDATE_AMOUNT + 1)
				),
				Error::<Test>::RemainingAmountBelowMinimum
			);

			// Can remove partially when the remaining amount is still at the minimum:
			assert_ok!(LendingPools::remove_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				Some(MIN_SUPPLY_UPDATE_AMOUNT)
			));

			// Can always remove all funds:
			assert_ok!(LendingPools::remove_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				None
			));
		});
	}

	#[test]
	fn can_remove_all_funds_event_even_when_smaller_than_min_update() {
		new_test_ext().execute_with(|| {
			setup();

			// Bring the lender's supply to the minimum:
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				MIN_SUPPLY_AMOUNT
			));

			const NEW_SWAP_RATE: u128 = 1;

			const {
				assert!(
					MIN_SUPPLY_AMOUNT * NEW_SWAP_RATE < MIN_SUPPLY_UPDATE_AMOUNT_USD,
					"test requires total supplied amount to be smaller"
				);
			}

			// Price of the asset goes way down:
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);

			// Can still remove all funds:
			assert_ok!(LendingPools::remove_lender_funds(
				RuntimeOrigin::signed(LENDER),
				LOAN_ASSET,
				None
			));
		});
	}
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
			minimum_supply_amount_usd: 0,
			minimum_update_supply_amount_usd: 0,
			..LendingConfigDefault::get()
		});

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, COLLATERAL_AMOUNT);
		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

		// Create a loan, doesn't really matter what amount.
		assert_eq!(
			create_loan_and_supply_collateral(
				BORROWER,
				LOAN_ASSET,
				MIN_LOAN_AMOUNT_ASSET,
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
			),
			Error::<Test>::AmountBelowMinimum
		);

		// Expanding by an amount equal to or above the minimum should be fine
		assert_eq!(
			LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				MIN_UPDATE_AMOUNT_ASSET,
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
fn must_have_refund_address_for_loan_asset() {
	new_test_ext().with_funded_pool(INIT_POOL_AMOUNT).execute_with(|| {
		MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, SWAP_RATE);
		MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET, 1);

		MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

		// Supply collateral into lending pool
		if GeneralLendingPools::<Test>::get(COLLATERAL_ASSET).is_none() {
			assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET));
		}
		assert_ok!(LendingPools::add_lender_funds(
			RuntimeOrigin::signed(BORROWER),
			COLLATERAL_ASSET,
			INIT_COLLATERAL,
		));

		// Should not be able to create a loan without a refund address set
		assert_noop!(
			LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL, None),
			Error::<Test>::NoRefundAddressSet
		);

		// Set refund address and try again
		MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
		assert_eq!(LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL, None), Ok(LOAN_ID));
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
		loans: BTreeMap::from([(
			LOAN_ID,
			GeneralLoan {
				id: LOAN_ID,
				asset: Asset::Btc,
				created_at_block: 0,
				owed_principal: 20,
				pending_interest: Default::default(),
				broker: None,
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
		// Running the next block will run upkeep and will attempt to trigger liquidation. We just
		// want to make sure this does not panic.
		.then_process_next_block()
		.then_execute_with(|_| {
			// The account is not in liquidation state since we could not initiate any liquidation
			// swaps:
			assert_eq!(
				LoanAccounts::<Test>::get(BORROWER).unwrap().liquidation_status,
				LiquidationStatus::NoLiquidation
			)
		});
}

#[test]
fn same_asset_loan() {
	const EXPANDED_PRINCIPAL: AssetAmount = PRINCIPAL / 2;
	const SWAP_DEFICIT: AssetAmount = 1_000;

	// Voluntary (soft) liquidation only collects what's needed to cover the outstanding
	// principal (PRINCIPAL + EXPANDED_PRINCIPAL + origination fees on each) plus the
	// soft-slippage buffer. LOAN_ASSET price is 1 fine USD, so owed_usd == owed.
	let total_origination_fee = portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL) +
		portion_of_amount(DEFAULT_ORIGINATION_FEE, EXPANDED_PRINCIPAL);
	// Voluntary liquidation doesn't charge the liquidation fee, so only the slippage
	// buffer applies.
	let liquidation_input = required_collateral_with_buffer(
		PRINCIPAL + EXPANDED_PRINCIPAL + total_origination_fee,
		bps_to_permill(CONFIG.soft_liquidation_max_oracle_slippage),
	);

	new_test_ext()
		.with_funded_pool(INIT_POOL_AMOUNT)
		.execute_with(|| {
			MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, 1);
			MockBalance::credit_account(&BORROWER, LOAN_ASSET, INIT_COLLATERAL);
			MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);

			// Should be able to create a loan where the loan asset is the same as the collateral
			// asset
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ASSET,
				INIT_COLLATERAL,
			));
			assert_eq!(LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL, None), Ok(LOAN_ID));

			// Supply additional collateral and expand
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ASSET,
				PRINCIPAL,
			));
			assert_ok!(LendingPools::expand_loan(
				RuntimeOrigin::signed(BORROWER),
				LOAN_ID,
				EXPANDED_PRINCIPAL,
			));

			// Now start a liquidation
			assert_ok!(LendingPools::initiate_voluntary_liquidation(RuntimeOrigin::signed(
				BORROWER
			)));
		})
		.then_process_next_block()
		.then_execute_with(|_| {
			let swap_requests = MockSwapRequestHandler::<Test>::get_swap_requests();
			let swap = swap_requests.get(&SwapRequestId(0)).expect("swap request not found");

			// The borrower's supply position is more than enough to cover the loan, so the
			// liquidation only requests the required-with-slippage amount. With a USD value
			// of ~1.5B fine USD and a soft chunk size of 10B fine USD, this fits in one DCA
			// chunk.
			assert_eq!(
				*swap,
				MockSwapRequest {
					input_asset: LOAN_ASSET,
					output_asset: LOAN_ASSET,
					input_amount: liquidation_input,
					remaining_input_amount: liquidation_input,
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
					dca_params: Some(DcaParameters { number_of_chunks: 1, chunk_interval: 1 }),
				}
			);

			let swap_output_amount = liquidation_input - SWAP_DEFICIT;

			// Finish the swap
			LendingPools::process_loan_swap_outcome(
				SwapRequestId(0),
				LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
				swap_output_amount,
			);

			// Check that the loan has been repaid and account has the correct amount left
			assert_has_event::<Test>(RuntimeEvent::LendingPools(Event::<Test>::LoanSettled {
				loan_id: LOAN_ID,
				outstanding_principal: 0,
				via_liquidation: true,
			}));
			// Loan settled and account removed, with the remaining loan asset supplied back:
			assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);
			let origination_fee =
				portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL + EXPANDED_PRINCIPAL);
			// Borrower's final supply position = (the leftover that was never pulled into
			// the swap) + (the swap excess after repaying loan and origination fee). The
			// leftover equals (initial position) - (input pulled), and the excess equals
			// (swap output) - (loan + origination_fee). This simplifies to:
			//   initial_supply - SWAP_DEFICIT - (loan + origination_fee)
			// plus a small share of the pool's origination-fee credits accrued while the
			// borrower was holding most of the pool's supply.
			let expected_collateral_left_minimum =
				INIT_COLLATERAL + PRINCIPAL -
					SWAP_DEFICIT - (PRINCIPAL + EXPANDED_PRINCIPAL + origination_fee);
			let actual_supply = GeneralLendingPools::<Test>::get(LOAN_ASSET)
				.unwrap()
				.get_supply_position_for_account(&BORROWER)
				.unwrap();
			assert!(actual_supply >= expected_collateral_left_minimum);
			assert!(actual_supply < expected_collateral_left_minimum + INIT_COLLATERAL / 100_000);
		});
}

mod supply_as_collateral {

	use super::*;

	const LIQUIDATION_SWAP: SwapRequestId = SwapRequestId(0);

	fn get_account() -> LoanAccount<Test> {
		LoanAccounts::<Test>::get(BORROWER).unwrap()
	}

	#[test]
	fn basic_lending_and_liquidation() {
		const EXCESS_AMOUNT: AssetAmount = PRINCIPAL / 10;

		const SWAPPED_PRINCIPAL: AssetAmount = PRINCIPAL + ORIGINATION_FEE + EXCESS_AMOUNT;

		// This will trigger soft liquidation (LTV jumps from 75% to 93.75%).
		const NEW_SWAP_RATE: u128 = 25;

		// Soft liquidation only collects the owed principal (in USD) plus the slippage
		// buffer; the rest stays in the supply pool.
		let liquidation_input = required_collateral_with_buffer(
			(PRINCIPAL + ORIGINATION_FEE) * NEW_SWAP_RATE,
			bps_to_permill(CONFIG.soft_liquidation_max_oracle_slippage)
				.saturating_add(CONFIG.liquidation_fee(LOAN_ASSET)),
		);
		let pool_remainder_after_init = INIT_COLLATERAL - liquidation_input;

		let liquidation_fee = CONFIG.liquidation_fee(LOAN_ASSET) * (PRINCIPAL + ORIGINATION_FEE);

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.then_execute_with(|_| {
				// Setup another pool to which the borrower can supply funds.
				assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET));
				MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
			})
			.then_execute_with(|_| {
				MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

				// Instead of providing collateral, the borrower supplies funds to another pool:
				assert_ok!(LendingPools::add_lender_funds(
					RuntimeOrigin::signed(BORROWER),
					COLLATERAL_ASSET,
					INIT_COLLATERAL
				));

				assert_eq!(
					GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER),
					Ok(INIT_COLLATERAL)
				);

				assert_eq!(
					LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL, None),
					Ok(LOAN_ID)
				);

				assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);

				// Change oracle price to trigger liquidation
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
			})
			.then_execute_at_next_block(|_| {
				// Check that liquidation started with supplied funds as collateral
				assert_matches!(
					get_account().liquidation_status,
					LiquidationStatus::Liquidating { .. }
				);

				let liquidation_swap = MockSwapRequestHandler::<Test>::get_swap_requests()
					.get(&LIQUIDATION_SWAP)
					.expect("No swap request found")
					.clone();

				assert_eq!(
					liquidation_swap,
					MockSwapRequest {
						input_asset: COLLATERAL_ASSET,
						output_asset: LOAN_ASSET,
						input_amount: liquidation_input,
						remaining_input_amount: liquidation_input,
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
						origin: SwapOrigin::Internal,
						price_limits_and_expiry: Some(SOFT_SWAP_PRICE_LIMIT),
						dca_params: Some(DcaParameters { number_of_chunks: 3, chunk_interval: 1 })
					}
				);

				// The user keeps the remainder of their supply not pulled into the swap:
				assert_eq!(
					GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER),
					Ok(pool_remainder_after_init)
				);
			})
			.then_execute_at_next_block(|_| {
				// Simulate full execution of the liquidation swap:
				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
					SWAPPED_PRINCIPAL,
				);

				assert_event_sequence!(
					Test,
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationCompleted {
						borrower_id: BORROWER,
						reason: LiquidationCompletionReason::FullySwapped,
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LiquidationFeeTaken {
						loan_id: LOAN_ID,
						..
					}),
					RuntimeEvent::LendingPools(Event::<Test>::LoanRepaid {
						loan_id: LOAN_ID,
						amount,
						action_type: LoanRepaidActionType::Liquidation { swap_request_id: LIQUIDATION_SWAP }
					}) if amount == PRINCIPAL + ORIGINATION_FEE,
				);

				// Balance should be unchanged:
				assert_eq!(MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET), 0);
				assert_eq!(MockBalance::get_balance(&BORROWER, LOAN_ASSET), PRINCIPAL);

				// Loan settled and account removed. Excess funds after liquidation go into the
				// user's supply pool:
				assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);
				assert_eq!(
					GeneralLendingPools::<Test>::get(LOAN_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					EXCESS_AMOUNT - liquidation_fee
				);
			});
	}

	/// The user has funds supplied in lending pool which are used as collateral,
	/// but not all funds are immediately available for withdrawal. Upon liquidation
	/// the protocol is expected to continue trying to withdraw from the lending pool
	/// to initiate multiple rounds of liquidation swaps.
	#[test]
	fn liquidation_multiple_attempts() {
		const BORROWER_2: u64 = OTHER_LP;
		const COLLATERAL_ASSET_2: Asset = Asset::ArbEth;

		const LOAN_2_ID: LoanId = LoanId(1);

		// Someone will borrow 20% of our supplied collateral
		const PRINCIPAL_2: AssetAmount = INIT_COLLATERAL / 5;

		const ORIGINATION_FEE_2: AssetAmount =
			portion_of_amount(DEFAULT_ORIGINATION_FEE, PRINCIPAL_2);

		// Selling 80% of collateral will recover 80% of principal
		const SWAPPED_PRINCIPAL_1: AssetAmount = 4 * PRINCIPAL / 5;

		let liquidation_fee_1 = CONFIG.liquidation_fee(LOAN_ASSET) * SWAPPED_PRINCIPAL_1;

		const LIQUIDATION_SWAP_2: SwapRequestId = SwapRequestId(1);

		let amount_owed_after_first_liquidation =
			PRINCIPAL + ORIGINATION_FEE - (SWAPPED_PRINCIPAL_1 - liquidation_fee_1);

		let liquidation_fee_2 =
			CONFIG.liquidation_fee(LOAN_ASSET) * amount_owed_after_first_liquidation;

		const EXCESS_AMOUNT: AssetAmount = PRINCIPAL / 10;

		let swapped_principal_2 =
			amount_owed_after_first_liquidation + liquidation_fee_2 + EXCESS_AMOUNT;

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.disable_network_fees()
			.then_execute_with(|_| {
				// Setup another pool where the borrower can supply funds to.
				assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET));
				MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
			})
			.then_execute_with(|_| {
				MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);
				assert_ok!(LendingPools::add_lender_funds(
					RuntimeOrigin::signed(BORROWER),
					COLLATERAL_ASSET,
					INIT_COLLATERAL
				));

				assert_eq!(
					GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER),
					Ok(INIT_COLLATERAL)
				);

				assert_eq!(
					LendingPools::new_loan(BORROWER, LOAN_ASSET, PRINCIPAL, None),
					Ok(LOAN_ID)
				);
			})
			.then_execute_with(|_| {
				// Some of collateral gets borrowed (by another account BORROWER_2), making it
				// unavailable:
				MockBalance::credit_account(&BORROWER_2, COLLATERAL_ASSET_2, INIT_COLLATERAL);
				MockLpRegistration::register_refund_address(BORROWER_2, ForeignChain::Ethereum);
				MockPriceFeedApi::set_price_usd_fine(COLLATERAL_ASSET_2, SWAP_RATE);

				assert_eq!(
					create_loan_and_supply_collateral(
						BORROWER_2,
						COLLATERAL_ASSET,
						PRINCIPAL_2,
						BTreeMap::from([(COLLATERAL_ASSET_2, INIT_COLLATERAL)])
					),
					Ok(LOAN_2_ID)
				);
			})
			.then_execute_with(|_| {
				// Change oracle price to trigger liquidation
				const NEW_SWAP_RATE: u128 = 30;
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
			})
			.then_execute_at_next_block(|_| {
				// Check that liquidation started with (some but not all) supplied funds as
				// collateral
				assert_matches!(
					get_account().liquidation_status,
					LiquidationStatus::Liquidating { .. }
				);

				// Supplied funds should have been automatically withdrawn (except for the part
				// that's borrowed):
				assert_eq!(
					GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER),
					Ok(PRINCIPAL_2 + ORIGINATION_FEE_2)
				);

				let liquidation_swap = MockSwapRequestHandler::<Test>::get_swap_requests()
					.get(&LIQUIDATION_SWAP)
					.expect("No swap request found")
					.clone();

				const AVAILABLE_COLLATERAL: AssetAmount = INIT_COLLATERAL - PRINCIPAL_2;

				assert_eq!(
					liquidation_swap,
					MockSwapRequest {
						input_asset: COLLATERAL_ASSET,
						output_asset: LOAN_ASSET,
						input_amount: AVAILABLE_COLLATERAL,
						remaining_input_amount: AVAILABLE_COLLATERAL,
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
						origin: SwapOrigin::Internal,
						price_limits_and_expiry: Some(HARD_SWAP_PRICE_LIMIT),
						dca_params: Some(DcaParameters { number_of_chunks: 1, chunk_interval: 1 })
					}
				);
			})
			.then_execute_at_next_block(|_| {
				// Simulate full execution of the liquidation swap:
				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
					SWAPPED_PRINCIPAL_1,
				);

				assert_eq!(get_account().liquidation_status, LiquidationStatus::NoLiquidation);

				// The loan should *not* be settled:
				assert_eq!(
					get_account(),
					LoanAccount {
						borrower_id: BORROWER,
						loans: BTreeMap::from([(
							LOAN_ID,
							GeneralLoan {
								id: LOAN_ID,
								asset: LOAN_ASSET,
								created_at_block: 1,
								owed_principal: amount_owed_after_first_liquidation,
								pending_interest: Default::default(),
								broker: None,
							}
						)]),
						liquidation_status: LiquidationStatus::NoLiquidation,
						voluntary_liquidation_requested: false,
					}
				);
			})
			.then_execute_at_next_block(|_| {
				// We expect the protocol to attempt to initiate liquidation swaps, but fail
				// due to 0 available collateral.
				assert_eq!(get_account().liquidation_status, LiquidationStatus::NoLiquidation);

				// Give borrower 2 some extra funds to cover origination fee:
				MockBalance::credit_account(&BORROWER_2, COLLATERAL_ASSET, ORIGINATION_FEE_2);

				// Make the rest of collateral available:
				assert_ok!(LendingPools::try_making_repayment(
					&BORROWER_2,
					LOAN_2_ID,
					RepaymentAmount::Full
				));
			})
			.then_execute_at_next_block(|_| {
				// All collateral has been successfully withdrawn:
				assert_eq!(
					GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER),
					Err(LendingPoolError::LenderNotFoundInPool)
				);

				// Finally we should see new liquidation swaps:
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
						liquidation_type: LiquidationType::Hard
					}
				);

				// Simulate full execution of the liquidation swap:
				LendingPools::process_loan_swap_outcome(
					LIQUIDATION_SWAP_2,
					LendingSwapType::Liquidation { borrower_id: BORROWER, loan_id: LOAN_ID },
					swapped_principal_2,
				);

				// The loan is fully repaid, the account is removed, and the excess swapped amount
				// goes to the user's supply pool.
				assert_eq!(LoanAccounts::<Test>::get(BORROWER), None);
				assert_eq!(
					GeneralLendingPools::<Test>::get(LOAN_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER)
						.unwrap(),
					EXCESS_AMOUNT
				);
			});
	}

	#[test]
	fn withdrawing_supplied_funds_checks_ltv() {
		const REQUIRED_COLLATERAL: AssetAmount = (PRINCIPAL + ORIGINATION_FEE) * SWAP_RATE * 5 / 4;

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.then_execute_with(|_| {
				// Setup another pool to which the borrower can supply funds.
				assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET));
				MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
			})
			.then_execute_with(|_| {
				MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

				assert_eq!(
					create_loan_and_supply_collateral(
						BORROWER,
						LOAN_ASSET,
						PRINCIPAL,
						BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
					),
					Ok(LOAN_ID)
				);

				// The user attempts to withdraw all funds from the lending pool, but
				// can't withdraw everything due to their active loan:
				assert_ok!(general_lending::remove_lender_funds::<Test>(
					BORROWER,
					COLLATERAL_ASSET,
					None
				));

				// The user's LTV should now be right at the target:
				assert_eq!(
					get_account().derive_ltv(&OraclePriceCache::default()).unwrap(),
					CONFIG.ltv_thresholds.target.into()
				);

				assert_eq!(
					GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER),
					Ok(REQUIRED_COLLATERAL)
				);

				assert_eq!(
					MockBalance::get_balance(&BORROWER, COLLATERAL_ASSET),
					INIT_COLLATERAL - REQUIRED_COLLATERAL
				);

				assert_has_event::<Test>(RuntimeEvent::LendingPools(
					Event::<Test>::LendingFundsRemoved {
						lender_id: BORROWER,
						asset: COLLATERAL_ASSET,
						unlocked_amount: INIT_COLLATERAL - REQUIRED_COLLATERAL,
						action_type: SupplyRemovedActionType::Manual,
					},
				));
			});
	}

	/// Same as [liquidation_multiple_attempts], but initially there is no collateral
	/// available at all.
	#[test]
	fn liquidation_no_collateral_available_initially() {
		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.disable_network_fees()
			.then_execute_with(|_| {
				// Setup another pool where the borrower can supply funds to.
				assert_ok!(LendingPools::new_lending_pool(COLLATERAL_ASSET));
				MockLpRegistration::register_refund_address(BORROWER, LOAN_CHAIN);
			})
			.then_execute_with(|_| {
				MockBalance::credit_account(&BORROWER, COLLATERAL_ASSET, INIT_COLLATERAL);

				assert_eq!(
					create_loan_and_supply_collateral(
						BORROWER,
						LOAN_ASSET,
						PRINCIPAL,
						BTreeMap::from([(COLLATERAL_ASSET, INIT_COLLATERAL)])
					),
					Ok(LOAN_ID)
				);
			})
			.then_execute_with(|_| {
				// The simplest way to make all funds unavailable:
				GeneralLendingPools::<Test>::mutate(COLLATERAL_ASSET, |pool| {
					pool.as_mut().unwrap().provide_funds_for_loan(INIT_COLLATERAL).unwrap();
				});
			})
			.then_execute_with(|_| {
				// Change oracle price to trigger liquidation
				const NEW_SWAP_RATE: u128 = 30;
				MockPriceFeedApi::set_price_usd_fine(LOAN_ASSET, NEW_SWAP_RATE);
			})
			.then_execute_at_next_block(|_| {
				// No funds have been withdrawn yet:
				assert_eq!(
					GeneralLendingPools::<Test>::get(COLLATERAL_ASSET)
						.unwrap()
						.get_supply_position_for_account(&BORROWER),
					Ok(INIT_COLLATERAL)
				);

				// There should be no collateral to start liquidation with:
				assert_eq!(get_account().liquidation_status, LiquidationStatus::NoLiquidation);
			})
			.then_execute_with(|_| {
				// Make all of collateral available again:
				GeneralLendingPools::<Test>::mutate(COLLATERAL_ASSET, |pool| {
					pool.as_mut().unwrap().receive_repayment(INIT_COLLATERAL);
				});
			})
			.then_execute_at_next_block(|_| {
				// We only want to check that liquidation started with the full amount
				// (the rest overlaps with existing tests):
				assert_matches!(
					get_account().liquidation_status,
					LiquidationStatus::Liquidating { .. }
				);

				let liquidation_swap = MockSwapRequestHandler::<Test>::get_swap_requests()
					.get(&LIQUIDATION_SWAP)
					.expect("No swap request found")
					.clone();

				assert_eq!(liquidation_swap.input_amount, INIT_COLLATERAL);
			});
	}
}

mod utilisation_cap {
	use cf_traits::mocks::account_role_registry::MockAccountRoleRegistry;

	use super::*;

	const BORROWER_2: u64 = OTHER_LP;
	const COVERAGE_FACTOR: Percent = Percent::from_percent(100);

	fn eth_pool_utilisation() -> Permill {
		GeneralLendingPools::<Test>::get(COLLATERAL_ASSET).unwrap().get_utilisation()
	}

	fn get_utilisation_cap(asset: Asset) -> Permill {
		compute_utilisation_cap::<Test>(
			asset,
			COVERAGE_FACTOR,
			&OraclePriceCache::<Test>::default(),
		)
		.unwrap()
	}

	/// Test that we prevent borrowing it it would exceed the pool's
	/// utilisation cap.
	#[test]
	fn borrow_fails_when_utilisation_cap_exceeded() {
		// After the default loan is taken, ETH pool utilisation cap is ~25%
		// (liquidating the default loan would need 75% of INIT_COLLATERAL).
		// Borrowing 30% of the pool must therefore be rejected; borrowing 20%
		// must succeed.
		const EXCESSIVE_BORROW: AssetAmount = INIT_COLLATERAL * 3 / 10;
		const MODEST_BORROW: AssetAmount = INIT_COLLATERAL * 2 / 10;

		// Sufficient amount of collateral for the second loan (which borrows COLLATERAL_ASSET and
		// uses LOAN_ASSET as collateral):
		const COLLATERAL_AMOUNT_2: AssetAmount = 2 * EXCESSIVE_BORROW / SWAP_RATE;

		let price_cache = OraclePriceCache::<Test>::default();

		new_test_ext()
			.with_funded_pool(INIT_POOL_AMOUNT)
			.with_default_loan()
			.execute_with(|| {
				// Default loan owes `PRINCIPAL + ORIGINATION_FEE` BTC,
				// collateralised entirely in ETH. Required ETH for full liquidation
				// therefore equals the loan's USD value, and the cap is
				// `1 - required / INIT_COLLATERAL`.
				let expected_required_usd =
					price_cache.usd_value_of(LOAN_ASSET, PRINCIPAL + ORIGINATION_FEE).unwrap();
				let total_collateral_usd =
					price_cache.usd_value_of(COLLATERAL_ASSET, INIT_COLLATERAL).unwrap();
				let expected_cap = Permill::one().saturating_sub(Permill::from_rational(
					expected_required_usd,
					total_collateral_usd,
				));

				let utilisation_cap = get_utilisation_cap(COLLATERAL_ASSET);

				assert_eq!(utilisation_cap, expected_cap);

				// Nothing has been borrowed from the ETH pool yet.
				let utilisation_before = eth_pool_utilisation();
				assert_eq!(utilisation_before, Permill::zero());
				assert!(utilisation_before <= utilisation_cap);

				MockLpRegistration::register_refund_address(BORROWER_2, ForeignChain::Ethereum);
				MockBalance::credit_account(&BORROWER_2, LOAN_ASSET, COLLATERAL_AMOUNT_2);

				assert_ok!(LendingPools::add_lender_funds(
					RuntimeOrigin::signed(BORROWER_2),
					LOAN_ASSET,
					COLLATERAL_AMOUNT_2,
				));

				// A large loan should fail due to hitting utilisation cap:
				assert_noop!(
					LendingPools::new_loan(BORROWER_2, COLLATERAL_ASSET, EXCESSIVE_BORROW, None),
					Error::<Test>::UtilisationCapExceeded
				);
				assert_eq!(eth_pool_utilisation(), utilisation_before);

				// Borrowing a smaller amount should succeed and keep utilisation under the cap:
				assert_ok!(LendingPools::new_loan(
					BORROWER_2,
					COLLATERAL_ASSET,
					MODEST_BORROW,
					None
				));
				let utilisation_after = eth_pool_utilisation();
				assert!(utilisation_after > utilisation_before);
				assert!(utilisation_after <= utilisation_cap);
			});
	}

	/// Test that the utilisation cap is computed correctly.
	#[test]
	fn compute_cap_across_multiple_assets() {
		// Multi-asset scenario (all amounts in 6-decimal "whole" units;
		// 1 whole = 10^6 amount-fine units; USD fine matches 10^6 per dollar):
		//
		//     Prices:  BTC = $100_000        USDT = $1        USDC = $1
		//
		//     Supplied BTC:  10              USDT: 200_000    USDC: 800_000
		//
		//     Loan A (BORROWER_A): borrows 180_000 USDT, collateral 3 BTC
		//     Loan B (BORROWER_B): borrows 500_000 USDC, collateral 7 BTC + 100_000 USDT
		//     Loan C (BORROWER_C): borrows 1 BTC,        collateral 130_000 USDT + 40_000 USDC
		//
		// Borrower collateral is held inside the lending pools (shared storage with
		// lender deposits), so each pool's `total_amount` is the sum of lender and
		// borrower contributions. The origination fee is zeroed out below so pool
		// totals stay at those sums after the loans settle.

		const WHOLE: AssetAmount = 1_000_000;

		const BTC_PRICE: AssetAmount = 100_000;
		const USD_PRICE: AssetAmount = 1;

		const USDC_LENDER: AssetAmount = 760_000 * WHOLE;
		const USDT_LENDER: AssetAmount = 120_000 * WHOLE;

		const LOAN_A_USDT: AssetAmount = 180_000 * WHOLE;
		const LOAN_A_BTC_COL: AssetAmount = 3 * WHOLE;

		const LOAN_B_USDC: AssetAmount = 500_000 * WHOLE;
		const LOAN_B_BTC_COL: AssetAmount = 7 * WHOLE;
		const LOAN_B_USDT_COL: AssetAmount = 100_000 * WHOLE;

		const LOAN_C_BTC: AssetAmount = WHOLE;
		const LOAN_C_USDT_COL: AssetAmount = 130_000 * WHOLE;
		const LOAN_C_USDC_COL: AssetAmount = 40_000 * WHOLE;

		const BORROWER_A: u64 = 201;
		const BORROWER_B: u64 = 202;
		const BORROWER_C: u64 = 203;

		new_test_ext().execute_with(|| {

			// ---- Setting up the lending pools and adding supply liquidity ----

			disable_whitelist();

			MockPriceFeedApi::set_price_usd_fine(Asset::Btc, BTC_PRICE);
			MockPriceFeedApi::set_price_usd_fine(Asset::Usdt, USD_PRICE);
			MockPriceFeedApi::set_price_usd_fine(Asset::Usdc, USD_PRICE);

			LendingConfig::<Test>::set(CONFIG);

			assert_ok!(LendingPools::new_lending_pool(Asset::Btc));
			assert_ok!(LendingPools::new_lending_pool(Asset::Usdt));
			assert_ok!(LendingPools::new_lending_pool(Asset::Usdc));

			// Loan C sits at ~83% LTV, so raise the target above the default 80%.
			// Zero the origination fee so pool totals match the lender+collateral sums.
			assert_ok!(Pallet::<Test>::update_pallet_config(
				RuntimeOrigin::root(),
				bounded_vec![
					PalletConfigUpdate::SetLtvThresholds {
						ltv_thresholds: LtvThresholds {
							target: Permill::from_percent(85),
							..CONFIG.ltv_thresholds
						}
					},
					PalletConfigUpdate::SetLendingPoolConfiguration {
						asset: None,
						config: Some(LendingPoolConfiguration {
							origination_fee: Permill::zero(),
							..CONFIG.default_pool_config
						}),
					},
				],
			));

			for acc in &[BORROWER_A, BORROWER_B, BORROWER_C] {
				assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_liquidity_provider(
					acc
				));
				MockLpRegistration::register_refund_address(*acc, ForeignChain::Ethereum);
				MockLpRegistration::register_refund_address(*acc, ForeignChain::Bitcoin);
			}


			// Supply all funds first:
			// BORROWER A supplies 3 BTC
			MockBalance::credit_account(&BORROWER_A, Asset::Btc, LOAN_A_BTC_COL);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BORROWER_A),
				Asset::Btc,
				LOAN_A_BTC_COL,
			));

			// BORROWER B supplies 7 BTC and 100_000 USDT:
			MockBalance::credit_account(&BORROWER_B, Asset::Btc, LOAN_B_BTC_COL);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BORROWER_B),
				Asset::Btc,
				LOAN_B_BTC_COL,
			));
			MockBalance::credit_account(&BORROWER_B, Asset::Usdt, LOAN_B_USDT_COL);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BORROWER_B),
				Asset::Usdt,
				LOAN_B_USDT_COL,
			));

			// BORROWER C supplies 80_000 USDT and 40_000 USDC:
			MockBalance::credit_account(&BORROWER_C, Asset::Usdt, LOAN_C_USDT_COL);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BORROWER_C),
				Asset::Usdt,
				LOAN_C_USDT_COL,
			));
			MockBalance::credit_account(&BORROWER_C, Asset::Usdc, LOAN_C_USDC_COL);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(BORROWER_C),
				Asset::Usdc,
				LOAN_C_USDC_COL,
			));

			// LENDER also supplies some funds but does not borrow
			MockBalance::credit_account(&LENDER, Asset::Usdc, USDC_LENDER);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				Asset::Usdc,
				USDC_LENDER,
			));
			MockBalance::credit_account(&LENDER, Asset::Usdt, USDT_LENDER);
			assert_ok!(LendingPools::add_lender_funds(
				RuntimeOrigin::signed(LENDER),
				Asset::Usdt,
				USDT_LENDER,
			));

			assert_eq!(
				GeneralLendingPools::<Test>::get(Asset::Btc).unwrap().total_amount,
				LOAN_A_BTC_COL + LOAN_B_BTC_COL
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(Asset::Usdc).unwrap().total_amount,
				LOAN_C_USDC_COL + USDC_LENDER
			);

			assert_eq!(
				GeneralLendingPools::<Test>::get(Asset::Usdt).unwrap().total_amount,
				LOAN_B_USDT_COL + LOAN_C_USDT_COL + USDT_LENDER
			);


		}).then_execute_with(|_| {
			// ---- Add loans and check resulting utilisation caps ----

			// Loan A:
			assert_ok!(LendingPools::new_loan(
				BORROWER_A,
				Asset::Usdt,
				LOAN_A_USDT,
				None
			));
			// Loan B:
			assert_ok!(LendingPools::new_loan(
				BORROWER_B,
				Asset::Usdc,
				LOAN_B_USDC,
				None
			));
			// Loan C:
			assert_ok!(LendingPools::new_loan(
				BORROWER_C,
				Asset::Btc,
				LOAN_C_BTC,
				None
			));

			// Check utilisation caps in each pool. The use of hardcoded values is intentional:
			// we want to demonstrate that the values exactly match a previously discussed example
			// (see https://discord.com/channels/775961728608895008/1494300574622023792/1494301004449976401).
			assert_eq!(get_utilisation_cap(Asset::Btc), Permill::from_parts(382_500)); // 38.25%
			assert_eq!(get_utilisation_cap(Asset::Usdc), Permill::from_parts(970_589)); // ~97%
			assert_eq!(get_utilisation_cap(Asset::Usdt), Permill::from_parts(602942)); // ~60.3%

		});
	}
}
