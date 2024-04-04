use super::*;
use cf_chains::Ethereum;
use cf_primitives::{AssetAmount, EthAmount, FLIPPERINOS_PER_FLIP};

use sp_std::collections::btree_set::BTreeSet;

type AccountId = u32;
type TestPool = BoostPool<AccountId, Ethereum>;
type Amount = <Ethereum as cf_chains::Chain>::ChainAmount;

const BOOSTER_1: AccountId = 1;
const BOOSTER_2: AccountId = 2;
const BOOSTER_3: AccountId = 3;

const BOOST_1: BoostId = 1;
const BOOST_2: BoostId = 2;

#[track_caller]
pub fn check_pool(pool: &TestPool, amounts: impl IntoIterator<Item = (AccountId, Amount)>) {
	assert_eq!(
		BTreeMap::from_iter(
			pool.amounts.iter().map(|(id, amount)| (*id, amount.into_chain_amount()))
		),
		BTreeMap::from_iter(amounts.into_iter()),
		"mismatch in booster amounts"
	);
	let total_amount: ScaledAmount<Ethereum> = pool
		.amounts
		.values()
		.fold(Default::default(), |acc, x| acc.checked_add(*x).unwrap());
	assert_eq!(pool.available_amount, total_amount);
}

#[track_caller]
fn check_pending_boosts(
	pool: &TestPool,
	boosts: impl IntoIterator<Item = (BoostId, Vec<(AccountId, Amount)>)>,
) {
	let expected_boosts: BTreeMap<_, _> = boosts.into_iter().collect();

	assert_eq!(
		BTreeSet::from_iter(pool.pending_boosts.keys().copied()),
		BTreeSet::from_iter(expected_boosts.keys().copied()),
		"mismatch in pending boosts ids"
	);

	for (boost_id, boost_amounts) in &pool.pending_boosts {
		let expected_amounts = &expected_boosts[boost_id];

		assert_eq!(
			BTreeMap::from_iter(expected_amounts.iter().copied()),
			BTreeMap::from_iter(
				boost_amounts.iter().map(|(id, amount)| (*id, amount.into_chain_amount()))
			)
		)
	}
}

#[track_caller]
fn check_pending_withdrawals(
	pool: &TestPool,
	withdrawals: impl IntoIterator<Item = (AccountId, Vec<BoostId>)>,
) {
	let expected_withdrawals: BTreeMap<_, BTreeSet<_>> = withdrawals
		.into_iter()
		.map(|(account_id, boost_ids)| (account_id, boost_ids.into_iter().collect()))
		.collect();

	assert_eq!(pool.pending_withdrawals, expected_withdrawals, "mismatch in pending withdrawals");
}

#[test]
fn test_scaled_amount() {
	use cf_chains::Ethereum;
	// This shows that we can have unreasonably large amounts in chains with
	// a large number of decimals and still fit into u128 after scaling up:

	// 1 trillion FLIP (or ETH; other chains have smaller number of decimals)
	let original: EthAmount = 1_000_000_000_000 * FLIPPERINOS_PER_FLIP;
	let scaled: ScaledAmount<Ethereum> = ScaledAmount::from_chain_amount(original);
	let recovered: EthAmount = scaled.into_chain_amount();
	assert_eq!(original, recovered);
}

#[test]
fn adding_funds() {
	let mut pool = TestPool::new(5);

	pool.add_funds(BOOSTER_1, 1000);
	check_pool(&pool, [(BOOSTER_1, 1000)]);

	pool.add_funds(BOOSTER_1, 500);
	check_pool(&pool, [(BOOSTER_1, 1500)]);

	pool.add_funds(BOOSTER_2, 800);
	check_pool(&pool, [(BOOSTER_1, 1500), (BOOSTER_2, 800)]);
}

#[test]
fn withdrawing_funds() {
	let mut pool = TestPool::new(5);
	pool.add_funds(BOOSTER_1, 1000);
	pool.add_funds(BOOSTER_2, 900);
	pool.add_funds(BOOSTER_3, 800);
	check_pool(&pool, [(BOOSTER_1, 1000), (BOOSTER_2, 900), (BOOSTER_3, 800)]);

	// No pending to receive, should be able to withdraw in full
	assert_eq!(pool.stop_boosting(BOOSTER_1), Ok(1000));
	check_pool(&pool, [(BOOSTER_2, 900), (BOOSTER_3, 800)]);
	check_pending_withdrawals(&pool, []);

	assert_eq!(pool.stop_boosting(BOOSTER_2), Ok(900));
	check_pool(&pool, [(BOOSTER_3, 800)]);

	assert_eq!(pool.stop_boosting(BOOSTER_3), Ok(800));
	check_pool(&pool, []);
}

#[test]
fn withdrawing_twice_is_no_op() {
	const AMOUNT_1: AssetAmount = 1000;
	const AMOUNT_2: AssetAmount = 750;

	let mut pool = TestPool::new(0);
	pool.add_funds(BOOSTER_1, AMOUNT_1);
	pool.add_funds(BOOSTER_2, AMOUNT_2);

	assert_eq!(pool.stop_boosting(BOOSTER_1), Ok(AMOUNT_1));

	check_pool(&pool, [(BOOSTER_2, AMOUNT_2)]);

	assert!(pool.stop_boosting(BOOSTER_1).is_err());

	// No changes:
	check_pool(&pool, [(BOOSTER_2, AMOUNT_2)]);
}

#[test]
fn boosting_with_fees() {
	let mut pool = TestPool::new(100);

	pool.add_funds(BOOSTER_1, 1000);
	pool.add_funds(BOOSTER_2, 2000);

	check_pool(&pool, [(BOOSTER_1, 1000), (BOOSTER_2, 2000)]);

	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, 1010), Ok((1010, 10)));

	// The recorded amounts include fees (1 is missing due to rounding errors in *test* code)
	check_pending_boosts(&pool, [(BOOST_1, vec![(BOOSTER_1, 333 + 3), (BOOSTER_2, 667 + 6)])]);

	assert_eq!(pool.on_finalised_deposit(BOOST_1), vec![]);

	check_pool(&pool, [(BOOSTER_1, 1003), (BOOSTER_2, 2006)]);
}

#[test]
fn adding_funds_during_pending_withdrawal_from_same_booster() {
	const AMOUNT_1: AssetAmount = 1000;
	const AMOUNT_2: AssetAmount = 3000;
	const DEPOSIT_AMOUNT: AssetAmount = 2000;

	let mut pool = TestPool::new(0);

	pool.add_funds(BOOSTER_1, AMOUNT_1);
	pool.add_funds(BOOSTER_2, AMOUNT_2);

	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, DEPOSIT_AMOUNT), Ok((DEPOSIT_AMOUNT, 0)));
	check_pool(&pool, [(BOOSTER_1, 500), (BOOSTER_2, 1500)]);

	check_pending_boosts(&pool, [(BOOST_1, vec![(BOOSTER_1, 500), (BOOSTER_2, 1500)])]);

	assert_eq!(pool.stop_boosting(BOOSTER_1), Ok(500));

	check_pool(&pool, [(BOOSTER_2, 1500)]);
	check_pending_boosts(&pool, [(BOOST_1, vec![(BOOSTER_1, 500), (BOOSTER_2, 1500)])]);
	check_pending_withdrawals(&pool, [(BOOSTER_1, vec![BOOST_1])]);

	// Booster 1 has a pending withdrawal, but they add more funds, so we assume they
	// no longer want to withdraw:
	pool.add_funds(BOOSTER_1, 1000);
	check_pending_withdrawals(&pool, []);

	// Booster 1 is no longer withdrawing, so pending funds go into available pool
	// on finalisation:
	assert_eq!(pool.on_finalised_deposit(BOOST_1), vec![]);
	check_pool(&pool, [(BOOSTER_1, 1500), (BOOSTER_2, AMOUNT_2)]);
}

#[test]
fn withdrawing_funds_before_finalisation() {
	let mut pool = TestPool::new(0);
	pool.add_funds(BOOSTER_1, 1000);
	pool.add_funds(BOOSTER_2, 1000);

	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, 1000), Ok((1000, 0)));
	check_pool(&pool, [(BOOSTER_1, 500), (BOOSTER_2, 500)]);

	// Only some of the funds are available immediately, and some are in pending withdrawals:
	assert_eq!(pool.stop_boosting(BOOSTER_1), Ok(500));
	check_pool(&pool, [(BOOSTER_2, 500)]);

	assert_eq!(pool.on_finalised_deposit(BOOST_1), vec![(BOOSTER_1, 500)]);
	check_pool(&pool, [(BOOSTER_2, 1000)]);
}

#[test]
fn adding_funds_with_pending_withdrawals() {
	let mut pool = TestPool::new(0);
	pool.add_funds(BOOSTER_1, 1000);
	pool.add_funds(BOOSTER_2, 1000);

	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, 1000), Ok((1000, 0)));

	check_pool(&pool, [(BOOSTER_1, 500), (BOOSTER_2, 500)]);

	// Only some of the funds are available immediately, and some are in pending withdrawals:
	assert_eq!(pool.stop_boosting(BOOSTER_1), Ok(500));
	check_pool(&pool, [(BOOSTER_2, 500)]);

	pool.add_funds(BOOSTER_3, 1000);
	check_pool(&pool, [(BOOSTER_2, 500), (BOOSTER_3, 1000)]);

	assert_eq!(pool.on_finalised_deposit(BOOST_1), vec![(BOOSTER_1, 500)]);
	check_pool(&pool, [(BOOSTER_2, 1000), (BOOSTER_3, 1000)]);
}

#[test]
fn deposit_is_lost_no_withdrawal() {
	let mut pool = TestPool::new(0);
	pool.add_funds(BOOSTER_1, 1000);
	pool.add_funds(BOOSTER_2, 1000);
	check_pool(&pool, [(BOOSTER_1, 1000), (BOOSTER_2, 1000)]);

	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, 1000), Ok((1000, 0)));
	pool.on_lost_deposit(BOOST_1);
	check_pool(&pool, [(BOOSTER_1, 500), (BOOSTER_2, 500)]);
}

#[test]
fn deposit_is_lost_while_withdrawing() {
	let mut pool = TestPool::new(0);
	pool.add_funds(BOOSTER_1, 1000);
	pool.add_funds(BOOSTER_2, 1000);
	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, 1000), Ok((1000, 0)));
	assert_eq!(pool.stop_boosting(BOOSTER_1), Ok(500));

	check_pool(&pool, [(BOOSTER_2, 500)]);
	check_pending_boosts(&pool, [(BOOST_1, vec![(BOOSTER_1, 500), (BOOSTER_2, 500)])]);
	check_pending_withdrawals(&pool, [(BOOSTER_1, vec![BOOST_1])]);

	pool.on_lost_deposit(BOOST_1);

	check_pool(&pool, [(BOOSTER_2, 500)]);
	// BOOSTER_1 is not considered "withdrawing" because they no longer await
	// for any deposits to finalise:
	check_pending_boosts(&pool, []);
}

#[test]
fn partially_losing_pending_withdrawals() {
	let mut pool = TestPool::new(0);
	pool.add_funds(BOOSTER_1, 1000);
	pool.add_funds(BOOSTER_2, 1000);

	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, 500), Ok((500, 0)));
	assert_eq!(pool.provide_funds_for_boosting(BOOST_2, 1000), Ok((1000, 0)));

	check_pool(&pool, [(BOOSTER_1, 250), (BOOSTER_2, 250)]);

	assert_eq!(pool.stop_boosting(BOOSTER_1), Ok(250));

	check_pending_withdrawals(&pool, [(BOOSTER_1, vec![BOOST_1, BOOST_2])]);

	check_pool(&pool, [(BOOSTER_2, 250)]);
	check_pending_boosts(
		&pool,
		[
			(BOOST_1, vec![(BOOSTER_1, 250), (BOOSTER_2, 250)]),
			(BOOST_2, vec![(BOOSTER_1, 500), (BOOSTER_2, 500)]),
		],
	);

	// Deposit of 500 is finalised, BOOSTER 1 gets 250 here, the other 250 goes into
	// Booster 2's available boost amount:
	{
		assert_eq!(pool.on_finalised_deposit(BOOST_1), vec![(BOOSTER_1, 250)]);

		check_pool(&pool, [(BOOSTER_2, 500)]);
		check_pending_withdrawals(&pool, [(BOOSTER_1, vec![BOOST_2])]);
		check_pending_boosts(&pool, [(BOOST_2, vec![(BOOSTER_1, 500), (BOOSTER_2, 500)])]);
	}

	// The other deposit is lost:
	{
		pool.on_lost_deposit(BOOST_2);
		check_pool(&pool, [(BOOSTER_2, 500)]);

		// BOOSTER_1 is no longer withdrawing:
		check_pending_withdrawals(&pool, []);

		check_pending_boosts(&pool, []);
	}
}

#[test]
fn booster_joins_then_funds_lost() {
	let mut pool = TestPool::new(0);
	pool.add_funds(BOOSTER_1, 1000);
	pool.add_funds(BOOSTER_2, 1000);

	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, 500), Ok((500, 0)));
	assert_eq!(pool.provide_funds_for_boosting(BOOST_2, 1000), Ok((1000, 0)));

	assert_eq!(pool.stop_boosting(BOOSTER_1), Ok(250));
	check_pool(&pool, [(BOOSTER_2, 250)]);

	// New booster joins while we have a pending withdrawal:
	pool.add_funds(BOOSTER_3, 1000);
	check_pool(&pool, [(BOOSTER_2, 250), (BOOSTER_3, 1000)]);

	// Deposit of 500 is finalised. Importantly this doesn't affect Booster 3 as they
	// didn't participate in the boost:
	assert_eq!(pool.on_finalised_deposit(BOOST_1), vec![(BOOSTER_1, 250)]);
	check_pool(&pool, [(BOOSTER_2, 500), (BOOSTER_3, 1000)]);

	// The other deposit is lost, which removes the pending withdrawal and
	// inactive amount from the pool. Booster 3 is not affected:
	pool.on_lost_deposit(BOOST_2);
	check_pool(&pool, [(BOOSTER_2, 500), (BOOSTER_3, 1000)]);
}

#[test]
fn booster_joins_between_boosts() {
	let mut pool = TestPool::new(200);
	pool.add_funds(BOOSTER_1, 1000);
	pool.add_funds(BOOSTER_2, 1000);

	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, 500), Ok((500, 10)));
	check_pool(&pool, [(BOOSTER_1, 755), (BOOSTER_2, 755)]);
	check_pending_boosts(&pool, [(BOOST_1, vec![(BOOSTER_1, 250), (BOOSTER_2, 250)])]);

	assert_eq!(pool.stop_boosting(BOOSTER_1), Ok(755));
	check_pool(&pool, [(BOOSTER_2, 755)]);

	// New booster joins while we have a pending withdrawal:
	pool.add_funds(BOOSTER_3, 2000);
	check_pool(&pool, [(BOOSTER_2, 755), (BOOSTER_3, 2000)]);

	// The amount used for boosting from a given booster is proportional
	// to their share in the available pool:
	assert_eq!(pool.provide_funds_for_boosting(BOOST_2, 1000), Ok((1000, 20)));
	check_pool(&pool, [(BOOSTER_2, 486), (BOOSTER_3, 1288)]);
	check_pending_boosts(
		&pool,
		[
			(BOOST_1, vec![(BOOSTER_1, 250), (BOOSTER_2, 250)]),
			(BOOST_2, vec![(BOOSTER_2, 274), (BOOSTER_3, 725)]),
		],
	);

	// Deposit of 500 is finalised, 250 goes to Booster 1's free balance, and the
	// remaining 250 goes to Booster 2; Booster 3 joined after this boost, so they
	// get nothing; there is only one pending boost now (Boost 2):
	assert_eq!(pool.on_finalised_deposit(BOOST_1), vec![(BOOSTER_1, 250)]);
	check_pool(&pool, [(BOOSTER_2, 736), (BOOSTER_3, 1288)]);
	check_pending_boosts(&pool, [(BOOST_2, vec![(BOOSTER_2, 274), (BOOSTER_3, 725)])]);

	{
		// Scenario A: the second deposit is lost; available amounts remain the same,
		// but there is no more pending boosts:
		let mut pool = pool.clone();
		pool.on_lost_deposit(BOOST_2);
		check_pool(&pool, [(BOOSTER_2, 736), (BOOSTER_3, 1288)]);
		check_pending_boosts(&pool, []);
	}

	{
		// Scenario B: the second deposit is received and distributed back between
		// the contributed boosters:
		let mut pool = pool.clone();
		assert_eq!(pool.on_finalised_deposit(BOOST_2), vec![]);
		check_pool(&pool, [(BOOSTER_2, 1010), (BOOSTER_3, 2014)]);
		check_pending_boosts(&pool, []);
	}
}

/// Check that boosters with small contributions can boost can earn rewards that
/// can be accumulated to non-zero chain amounts
#[test]
fn small_rewards_accumulate() {
	// Booster 2 only owns a small fraction of the pool:
	let mut pool = TestPool::new(100);
	pool.add_funds(BOOSTER_1, 1000);
	pool.add_funds(BOOSTER_2, 50);

	const SMALL_DEPOSIT: AssetAmount = 500;

	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, SMALL_DEPOSIT), Ok((SMALL_DEPOSIT, 5)));
	assert_eq!(pool.on_finalised_deposit(BOOST_1), vec![]);

	// BOOSTER 2 earns ~0.25 (it is rounded down when converted to AssetAmount,
	// but the fractional part isn't lost)
	check_pool(&pool, [(BOOSTER_1, 1004), (BOOSTER_2, 50)]);

	// 4 more boost like that and BOOSTER 2 should have withdrawable fees:
	for boost_id in 1..=4 {
		assert_eq!(
			pool.provide_funds_for_boosting(boost_id, SMALL_DEPOSIT),
			Ok((SMALL_DEPOSIT, 5))
		);
		assert_eq!(pool.on_finalised_deposit(boost_id), vec![]);
	}

	// Note the increase in Booster 2's balance:
	check_pool(&pool, [(BOOSTER_1, 1023), (BOOSTER_2, 51)]);
}

#[test]
fn use_max_available_amount() {
	let mut pool = TestPool::new(100);
	pool.add_funds(BOOSTER_1, 1000);

	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, 1010), Ok((1010, 10)));

	check_pool(&pool, [(BOOSTER_1, 0)]);

	assert_eq!(pool.stop_boosting(BOOSTER_1), Ok(0));

	pool.add_funds(BOOSTER_1, 200);

	assert_eq!(pool.on_finalised_deposit(BOOST_1), vec![]);

	check_pool(&pool, [(BOOSTER_1, 1210)]);
}

#[test]
fn handling_rounding_errors() {
	type C = Ethereum;
	const FEE_BPS: u16 = 100;
	let mut pool = TestPool::new(100);

	const DEPOSIT_AMOUNT: AssetAmount = 1;
	// A number of boosters that would lead to rounding errors:
	const BOOSTER_COUNT: u32 = 7;
	const BOOSTER_FUNDS: AssetAmount = 1;

	for booster_id in 1..=BOOSTER_COUNT {
		pool.add_funds(booster_id, BOOSTER_FUNDS);
	}

	assert_eq!(pool.provide_funds_for_boosting(BOOST_1, DEPOSIT_AMOUNT), Ok((DEPOSIT_AMOUNT, 0)));

	// Note that one of the values is larger than the rest, due to how we handle rounding errors:
	const EXPECTED_REMAINING_AMOUNTS: [u128; 7] = [858, 858, 858, 858, 858, 862, 858];

	assert_eq!(
		&pool.amounts.values().map(|scaled_amount| scaled_amount.val).collect::<Vec<_>>(),
		&EXPECTED_REMAINING_AMOUNTS
	);

	// Despite rounding errors, we the total available amount in the pool is as expected:
	let deposit_amount = ScaledAmount::<C>::from_chain_amount(DEPOSIT_AMOUNT).val;
	{
		let booster_funds = ScaledAmount::<C>::from_chain_amount(BOOSTER_FUNDS).val;
		let fee = deposit_amount * FEE_BPS as u128 / 10_000;
		let expected_total_amount = BOOSTER_COUNT as u128 * booster_funds - deposit_amount + fee;

		assert_eq!(EXPECTED_REMAINING_AMOUNTS.into_iter().sum::<u128>(), expected_total_amount);
	}

	// Again, one of the values is larger than the rest due to rounding errors:
	const EXPECTED_AMOUNTS_TO_RECEIVE: [u128; 7] = [142, 142, 142, 142, 142, 148, 142];

	assert_eq!(
		&pool.pending_boosts[&BOOST_1]
			.values()
			.map(|scaled_amount| scaled_amount.val)
			.collect::<Vec<_>>(),
		&EXPECTED_AMOUNTS_TO_RECEIVE
	);

	// Despite rounding errors, the total amount to receive is as expected:
	assert_eq!(EXPECTED_AMOUNTS_TO_RECEIVE.into_iter().sum::<u128>(), deposit_amount);
}
