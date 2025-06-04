// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use super::*;

type AccountId = u32;
type CorePool = CoreLendingPool<AccountId>;

const LENDER_1: AccountId = 1;
const LENDER_2: AccountId = 2;
const LENDER_3: AccountId = 3;

const LOAN_1: LoanId = 0;
const LOAN_2: LoanId = 1;

// The exact value in unimportant in tests
const USAGE: LoanUsage = LoanUsage::Boost(0);

#[test]
fn test_scaled_amount() {
	// This shows that we can have unreasonably large amounts in chains with
	// a large number of decimals and still fit into u128 after scaling up:
	use cf_primitives::FLIPPERINOS_PER_FLIP;

	// 1 trillion FLIP (or ETH; other chains have smaller number of decimals)
	let original = 1_000_000_000_000 * FLIPPERINOS_PER_FLIP;
	let scaled: ScaledAmount = ScaledAmount::from_asset_amount(original);
	let recovered = scaled.into_asset_amount();
	assert_eq!(original, recovered);
}

#[track_caller]
pub fn check_pool(pool: &CorePool, amounts: impl IntoIterator<Item = (AccountId, AssetAmount)>) {
	assert_eq!(
		BTreeMap::from_iter(
			pool.amounts.iter().map(|(id, amount)| (*id, amount.into_asset_amount()))
		),
		BTreeMap::from_iter(amounts.into_iter()),
		"mismatch in lender amounts"
	);
	let total_amount: ScaledAmount = pool
		.amounts
		.values()
		.fold(Default::default(), |acc, x| acc.checked_add(*x).unwrap());
	assert_eq!(pool.available_amount, total_amount);
}

#[track_caller]
fn check_pending_withdrawals(
	pool: &CorePool,
	withdrawals: impl IntoIterator<Item = (AccountId, Vec<LoanId>)>,
) {
	let expected_withdrawals: BTreeMap<_, BTreeSet<_>> = withdrawals
		.into_iter()
		.map(|(account_id, loan_id)| (account_id, loan_id.into_iter().collect()))
		.collect();

	assert_eq!(pool.pending_withdrawals, expected_withdrawals, "mismatch in pending withdrawals");
}

#[track_caller]
fn check_pending_loans(
	pool: &CorePool,
	loans: impl IntoIterator<Item = (LoanId, Vec<(AccountId, u16 /* percents */)>)>,
) {
	let expected_loans: BTreeMap<_, _> = loans.into_iter().collect();

	assert_eq!(
		BTreeSet::from_iter(pool.pending_loans.keys().copied()),
		BTreeSet::from_iter(expected_loans.keys().copied()),
		"mismatch in pending loan ids"
	);

	for (loan_id, loan) in &pool.pending_loans {
		let expected_shares = BTreeMap::from_iter(
			expected_loans[loan_id]
				.iter()
				.map(|(acc_id, percent)| (*acc_id, Perquintill::from_percent(*percent as u64))),
		);

		assert_eq!(expected_shares, loan.shares)
	}
}

#[test]
fn adding_funds() {
	let mut pool = CoreLendingPool::default();

	pool.add_funds(LENDER_1, 1000);
	check_pool(&pool, [(LENDER_1, 1000)]);

	pool.add_funds(LENDER_1, 500);
	check_pool(&pool, [(LENDER_1, 1500)]);

	pool.add_funds(LENDER_2, 800);
	check_pool(&pool, [(LENDER_1, 1500), (LENDER_2, 800)]);
}

#[test]
fn basic_lending() {
	const LOAN_AMOUNT: AssetAmount = 1000;

	let mut pool = CoreLendingPool::default();
	pool.add_funds(LENDER_1, LOAN_AMOUNT);

	// Create a loan
	assert_eq!(pool.new_loan(LOAN_AMOUNT, LoanUsage::Boost(0)), Ok(LOAN_1));
	check_pool(&pool, [(LENDER_1, 0)]);
	check_pending_loans(&pool, [(LOAN_1, vec![(LENDER_1, 100)])]);

	// Partial repayment. Should update available amounts, but
	// the loan should still be pending:
	pool.make_repayment(LOAN_1, LOAN_AMOUNT / 2);
	check_pool(&pool, [(LENDER_1, LOAN_AMOUNT / 2)]);
	check_pending_loans(&pool, [(LOAN_1, vec![(LENDER_1, 100)])]);

	// Finalising the loan with the remaining amount. There should
	// now be no pending loans:
	assert_eq!(pool.finalise_loan(LOAN_1, LOAN_AMOUNT / 2), vec![]);
	check_pool(&pool, [(LENDER_1, LOAN_AMOUNT)]);
	check_pending_loans(&pool, []);
}

#[test]
fn zero_amount_loan() {
	// A zero amount loan doesn't make much sense, but it is
	// worth showing that it works as expected

	let mut pool = CoreLendingPool::default();
	pool.add_funds(LENDER_1, 500);

	assert_eq!(pool.new_loan(0, USAGE), Ok(LOAN_1));
	check_pool(&pool, [(LENDER_1, 500)]);
	check_pending_loans(&pool, [(LOAN_1, vec![(LENDER_1, 100)])]);

	// If for whatever reason the returned amount isn't zero
	// that works correctly too:
	pool.finalise_loan(LOAN_1, 1000);
	check_pool(&pool, [(LENDER_1, 1500)]);
	check_pending_loans(&pool, []);
}

// Basic withdrawing: no pending loans
#[test]
fn withdrawing_funds() {
	let mut pool = CoreLendingPool::default();
	pool.add_funds(LENDER_1, 1000);
	pool.add_funds(LENDER_2, 900);
	pool.add_funds(LENDER_3, 800);
	check_pool(&pool, [(LENDER_1, 1000), (LENDER_2, 900), (LENDER_3, 800)]);

	// No pending to receive, should be able to withdraw in full
	assert_eq!(pool.stop_lending(LENDER_1), Ok((1000, Default::default())));
	check_pool(&pool, [(LENDER_2, 900), (LENDER_3, 800)]);
	check_pending_withdrawals(&pool, []);

	assert_eq!(pool.stop_lending(LENDER_2), Ok((900, Default::default())));
	check_pool(&pool, [(LENDER_3, 800)]);

	assert_eq!(pool.stop_lending(LENDER_3), Ok((800, Default::default())));
	check_pool(&pool, []);
}

#[test]
fn withdrawing_twice_is_no_op() {
	const AMOUNT_1: AssetAmount = 1000;
	const AMOUNT_2: AssetAmount = 750;

	let mut pool = CoreLendingPool::default();
	pool.add_funds(LENDER_1, AMOUNT_1);
	pool.add_funds(LENDER_2, AMOUNT_2);

	assert_eq!(pool.stop_lending(LENDER_1), Ok((AMOUNT_1, Default::default())));

	check_pool(&pool, [(LENDER_2, AMOUNT_2)]);

	assert_eq!(pool.stop_lending(LENDER_1), Err(Error::AccountNotFoundInPool));

	// No changes:
	check_pool(&pool, [(LENDER_2, AMOUNT_2)]);
}

#[test]
fn withdrawing_with_a_pending_loan() {
	const LOAN_AMOUNT: AssetAmount = 1000;

	let mut pool = CoreLendingPool::default();
	pool.add_funds(LENDER_1, 1500);
	pool.add_funds(LENDER_2, 1500);

	const USAGE_1: LoanUsage = LoanUsage::Boost(2);
	const USAGE_2: LoanUsage = LoanUsage::Boost(3);

	assert_eq!(pool.new_loan(LOAN_AMOUNT, USAGE_1), Ok(LOAN_1));
	assert_eq!(pool.new_loan(LOAN_AMOUNT, USAGE_2), Ok(LOAN_2));
	check_pool(&pool, [(LENDER_1, 500), (LENDER_2, 500)]);

	// Only some of the funds are available immediately, and some are in pending withdrawals:
	assert_eq!(pool.stop_lending(LENDER_1), Ok((500, BTreeSet::from_iter([USAGE_1, USAGE_2]))));
	check_pending_withdrawals(&pool, [(LENDER_1, vec![LOAN_1, LOAN_2])]);
	check_pool(&pool, [(LENDER_2, 500)]);

	// LENDER_1 is withdrawing, so their portion of the repayment leaves the pool
	assert_eq!(pool.finalise_loan(LOAN_1, LOAN_AMOUNT), vec![(LENDER_1, 500)]);
	// LENDER_1 is still waiting for 1 more loan to finalise:
	check_pending_withdrawals(&pool, [(LENDER_1, vec![LOAN_2])]);

	// LOAN_2 is finalised (with 0 amount which could correspond to, for example,
	// a boosted deposit being lost); the exiting lender should now be fully out:
	assert_eq!(pool.finalise_loan(LOAN_2, 0), vec![]);
	check_pending_withdrawals(&pool, []);
	check_pending_loans(&pool, []);
	check_pool(&pool, [(LENDER_2, 1000)]);
}

#[test]
fn adding_funds_during_pending_withdrawal_from_same_lender() {
	const AMOUNT_1: AssetAmount = 1000;
	const AMOUNT_2: AssetAmount = 3000;
	const LOAN_AMOUNT: AssetAmount = 2000;

	let mut pool = CoreLendingPool::default();

	pool.add_funds(LENDER_1, AMOUNT_1);
	pool.add_funds(LENDER_2, AMOUNT_2);

	assert_eq!(pool.new_loan(LOAN_AMOUNT, USAGE), Ok(LOAN_1));
	check_pool(&pool, [(LENDER_1, 500), (LENDER_2, 1500)]);

	check_pending_loans(&pool, [(LOAN_1, vec![(LENDER_1, 25), (LENDER_2, 75)])]);

	assert_eq!(pool.stop_lending(LENDER_1), Ok((500, BTreeSet::from_iter([USAGE]))));

	check_pending_withdrawals(&pool, [(LENDER_1, vec![LOAN_1])]);
	check_pool(&pool, [(LENDER_2, 1500)]);

	// Stop lending should not have any affect on pending loans:
	check_pending_loans(&pool, [(LOAN_1, vec![(LENDER_1, 25), (LENDER_2, 75)])]);

	// Lender 1 has a pending withdrawal, but they add more funds, so we assume they
	// no longer want to withdraw:
	pool.add_funds(LENDER_1, 1000);
	check_pending_withdrawals(&pool, []);
	check_pool(&pool, [(LENDER_1, 1000), (LENDER_2, 1500)]);

	// Lender 1 is no longer withdrawing, so pending funds go into available pool
	// on finalisation:
	assert_eq!(pool.finalise_loan(LOAN_1, LOAN_AMOUNT), vec![]);
	check_pool(&pool, [(LENDER_1, 1500), (LENDER_2, AMOUNT_2)]);
}

#[test]
fn new_lender_only_affects_new_loans() {
	const LOAN_AMOUNT: AssetAmount = 1000;

	let mut pool = CoreLendingPool::default();
	pool.add_funds(LENDER_1, 1000);
	pool.add_funds(LENDER_2, 1000);

	assert_eq!(pool.new_loan(LOAN_AMOUNT, USAGE), Ok(LOAN_1));

	check_pool(&pool, [(LENDER_1, 500), (LENDER_2, 500)]);
	assert_eq!(pool.stop_lending(LENDER_1), Ok((500, BTreeSet::from_iter([USAGE]))));
	check_pool(&pool, [(LENDER_2, 500)]);

	// A new lender adding funds should not affect the other accounts, including the
	// pending withdrawal of LENDER_1:
	pool.add_funds(LENDER_3, 1500);
	check_pool(&pool, [(LENDER_2, 500), (LENDER_3, 1500)]);

	assert_eq!(pool.new_loan(LOAN_AMOUNT, USAGE), Ok(LOAN_2));
	// Note that the shares are computed based on available amounts (the funds owed
	// to Lender 2 aren't taken into account):
	check_pending_loans(
		&pool,
		[
			(LOAN_1, vec![(LENDER_1, 50), (LENDER_2, 50)]),
			(LOAN_2, vec![(LENDER_2, 25), (LENDER_3, 75)]),
		],
	);
	check_pool(&pool, [(LENDER_2, 250), (LENDER_3, 750)]);

	assert_eq!(pool.finalise_loan(LOAN_1, LOAN_AMOUNT), vec![(LENDER_1, 500)]);

	// LENDER_2 does not get credited for a loan in which they didn't participate:
	check_pool(&pool, [(LENDER_2, 750), (LENDER_3, 750)]);

	// LENDER_2 does get credited for Loan 2:
	assert_eq!(pool.finalise_loan(LOAN_2, LOAN_AMOUNT), vec![]);
	check_pool(&pool, [(LENDER_2, 1000), (LENDER_3, 1500)]);
}

/// Check that lenders with small contributions can still earn rewards that
/// can be accumulated to non-zero asset amounts
#[test]
fn small_rewards_accumulate() {
	// Lender 2 only owns a small fraction of the pool:
	let mut pool = CoreLendingPool::default();
	pool.add_funds(LENDER_1, 1000);
	pool.add_funds(LENDER_2, 50);

	const SMALL_DEPOSIT: AssetAmount = 500;
	const FEE: AssetAmount = 5;

	assert_eq!(pool.new_loan(SMALL_DEPOSIT, USAGE), Ok(LOAN_1));

	pool.finalise_loan(LOAN_1, SMALL_DEPOSIT + FEE);

	// LENDER_2 earns ~0.25 (it is rounded down when converted to AssetAmount,
	// but the fractional part isn't lost)
	check_pool(&pool, [(LENDER_1, 1004), (LENDER_2, 50)]);

	// 4 more loans like that and LENDER_2 should have withdrawable fees:
	for loan_id in (LOAN_1 + 1)..=(LOAN_1 + 4) {
		assert_eq!(pool.new_loan(SMALL_DEPOSIT, USAGE), Ok(loan_id));
		pool.finalise_loan(loan_id, SMALL_DEPOSIT + FEE);
	}

	// Note the increase in LENDER_2's balance:
	check_pool(&pool, [(LENDER_1, 1023), (LENDER_2, 51)]);
}

#[test]
fn use_max_available_amount() {
	const LOAN_AMOUNT: AssetAmount = 1_000_000;

	let mut pool = CoreLendingPool::default();
	pool.add_funds(LENDER_1, LOAN_AMOUNT);

	assert_eq!(pool.new_loan(LOAN_AMOUNT, USAGE), Ok(LOAN_1));

	check_pool(&pool, [(LENDER_1, 0)]);
}

#[test]
fn handling_rounding_errors() {
	let mut pool = CoreLendingPool::default();

	const LOAN_AMOUNT: AssetAmount = 1;
	// A number of lenders that would lead to rounding errors:
	const LENDER_COUNT: u32 = 7;
	const LENDER_FUNDS: AssetAmount = 1;

	for lender_id in 1..=LENDER_COUNT {
		pool.add_funds(lender_id, LENDER_FUNDS);
	}

	assert_eq!(pool.new_loan(LOAN_AMOUNT, USAGE), Ok(LOAN_1));

	// Note that one of the values (happens to be the first, but the index is chosen at random)
	// is larger than the rest, due to how we handle rounding errors:
	const EXPECTED_REMAINING_AMOUNTS: [u128; 7] = [858, 857, 857, 857, 857, 857, 857];

	// Note that we compare scaled amounts since that's where errors are perceptible:
	assert_eq!(
		&pool.amounts.values().map(|scaled| scaled.as_raw()).collect::<Vec<_>>(),
		&EXPECTED_REMAINING_AMOUNTS
	);

	// Importantly, we total amount is as expected:
	assert_eq!(EXPECTED_REMAINING_AMOUNTS.into_iter().sum::<u128>(), 6_000);

	pool.finalise_loan(LOAN_1, LOAN_AMOUNT);

	// Again, one of the amounts is larger after loan repayment (the index is the same
	// because we use loan id as the seed into rng):
	const EXPECTED_NEW_AMOUNTS: [u128; 7] = [1006, 999, 999, 999, 999, 999, 999];
	assert_eq!(
		&pool.amounts.values().map(|scaled| scaled.as_raw()).collect::<Vec<_>>(),
		&EXPECTED_NEW_AMOUNTS
	);

	// Importantly, we total amount is as expected:
	assert_eq!(EXPECTED_NEW_AMOUNTS.into_iter().sum::<u128>(), 7_000);
}
