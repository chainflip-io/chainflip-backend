use super::*;

/// This keeps track of each lender's share in the pool, the total amount of funds
/// owed to lenders (and how much of it is available to be borrowed), and the collected
/// fees in various assets that are yet to be swapped into the pool's asset.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(AccountId))]
pub struct LendingPool<AccountId>
where
	AccountId: Decode + Encode + Ord + Clone,
{
	/// Total amount owed to active lenders (includes what's currently in loans, but excluded
	/// what's owed to the network).
	pub total_amount: AssetAmount,
	/// Amount available to be borrowed
	pub available_amount: AssetAmount,
	/// Maps lenders to their shares in the pool; each lender is effectively owed their `share` *
	/// `total_amount` of the pool's asset.
	pub lender_shares: BTreeMap<AccountId, Perquintill>,
	/// Interest is paid upon loan repayment so this field keeps tracks of how much in pool's
	/// asset is owed to the network. In practice we will still try to pay the network regularly
	/// by taking from available funds, but this field is necessary in case available funds aren't
	/// enough (i.e. utilisation is near 100%).
	pub owed_to_network: AssetAmount,
}

#[derive(PartialEq, Debug)]
pub enum LendingPoolError {
	LenderNotFoundInPool,
	InsufficientLiquidity,
}

impl<T: Config> From<LendingPoolError> for Error<T> {
	fn from(error: LendingPoolError) -> Self {
		match error {
			LendingPoolError::InsufficientLiquidity => Error::<T>::InsufficientLiquidity,
			LendingPoolError::LenderNotFoundInPool => Error::<T>::LenderNotFoundInPool,
		}
	}
}

/// Result of a successful execution of [LendingPool::remove_funds]
#[derive(Debug, PartialEq, Eq)]
pub struct WithdrawnAndRemainingAmounts {
	/// This much has been withdrawn from the pool (this amount needs to be
	/// assigned/credited elsewhere).
	pub withdrawn_amount: AssetAmount,
	/// This much is left in the pool.
	pub remaining_amount: AssetAmount,
}

impl<AccountId> LendingPool<AccountId>
where
	AccountId: Decode + Encode + Ord + Clone,
{
	pub fn new() -> Self {
		Self {
			total_amount: 0,
			available_amount: 0,
			owed_to_network: 0,
			lender_shares: BTreeMap::new(),
		}
	}

	/// Adds funds increasing `lender`'s share in the pool.
	pub fn add_funds(&mut self, lender: &AccountId, amount: AssetAmount) {
		let new_total_amount = self.total_amount.saturating_add(amount);
		let scaling_factor = Perquintill::from_rational(self.total_amount, new_total_amount);
		self.total_amount = new_total_amount;
		self.available_amount.saturating_accrue(amount);

		// First update all existing shares given the new total amount
		let mut remaining_shares = Perquintill::one();
		for (_, share) in self.lender_shares.iter_mut() {
			*share = *share * scaling_factor;
			remaining_shares.saturating_reduce(*share);
		}

		// The share of `lender` is a value that would be bring the total shares to 100%
		let old_share = self.lender_shares.entry(lender.clone()).or_default();
		*old_share = *old_share + remaining_shares;
	}

	/// Remove funds owed to `lender` reducing their share in the pool. The funds are removed
	/// partially if the pool does not have enough available.
	pub fn remove_funds(
		&mut self,
		lender: &AccountId,
		amount: Option<AssetAmount>,
	) -> Result<WithdrawnAndRemainingAmounts, LendingPoolError> {
		let share = self
			.lender_shares
			.get_mut(lender)
			.ok_or(LendingPoolError::LenderNotFoundInPool)?;

		let total_owed_amount = *share * self.total_amount;

		// Amount requested, but capped by the total amount owed to the lender:
		let required_amount = core::cmp::min(total_owed_amount, amount.unwrap_or(u128::MAX));
		// This is further capped by the amount available in the pool:
		let amount_to_withdraw = core::cmp::min(required_amount, self.available_amount);

		let old_total_amount = self.total_amount;
		self.total_amount.saturating_reduce(amount_to_withdraw);
		self.available_amount.saturating_reduce(amount_to_withdraw);

		let remaining_owed_amount = total_owed_amount.saturating_sub(amount_to_withdraw);

		// Update `lender`'s share but don't take the change of the total amount into account yet:
		*share = Perquintill::from_rational(remaining_owed_amount, old_total_amount);

		if *share == Perquintill::zero() {
			self.lender_shares.remove(lender);
		}

		// Recomputing everyone's shares taking the new total amount into account:
		self.lender_shares = utils::distribute_proportionally(
			Perquintill::one().deconstruct(),
			self.lender_shares.iter().map(|(id, share)| (id, share.deconstruct().into())),
		)
		.into_iter()
		.map(|(id, parts)| (id.clone(), Perquintill::from_parts(parts)))
		.collect();

		Ok(WithdrawnAndRemainingAmounts {
			withdrawn_amount: amount_to_withdraw,
			remaining_amount: remaining_owed_amount,
		})
	}

	pub fn provide_funds_for_loan(&mut self, amount: AssetAmount) -> Result<(), LendingPoolError> {
		let Some(remaining_amount) = self.available_amount.checked_sub(amount) else {
			return Err(LendingPoolError::InsufficientLiquidity);
		};

		self.available_amount = remaining_amount;

		Ok(())
	}

	pub fn get_utilisation(&self) -> Permill {
		let available_for_borrowing = self.available_amount.saturating_sub(self.owed_to_network);
		let in_use = self.total_amount.saturating_sub(available_for_borrowing);

		// Note: `from_rational` does not panic on invalid inputs and instead returns 100%.
		Permill::from_rational(in_use, self.total_amount)
	}

	/// Receives fees in the pool's asset. Unlike `record_pool_fee` the amount
	/// is provided (e.g. via liquidation) and immediately available.
	pub fn receive_fees_in_pools_asset(&mut self, amount: AssetAmount) {
		// Fees increase both the total and available amount
		self.available_amount.saturating_accrue(amount);
		self.total_amount.saturating_accrue(amount);
	}

	/// Record `amount` as a fee that's owed to the pool (it is to be added to some
	/// loan's principal). This increases the pool's total "value", but the extra funds will only
	/// become available upon loan repayment.
	pub fn record_pool_fee(&mut self, amount: AssetAmount) {
		self.total_amount.saturating_accrue(amount);
	}

	/// Record `amount` as a fee that's owed to the network (it is to be added to some
	/// loan's principal). This does not increase the total "value" of the pool,
	/// but `amount` will be repaid as part of loan repayment, so we record this to know
	/// how much the network can collect from `available_amount`. The method then tries
	/// to collect as much as possible (it is possible that we can't collect all owed fees
	/// immediately in case utilisation approaches 100%).
	pub fn record_and_collect_network_fee(&mut self, amount: AssetAmount) -> AssetAmount {
		self.owed_to_network.saturating_accrue(amount);

		let available_for_collection = core::cmp::min(self.available_amount, self.owed_to_network);
		self.owed_to_network.saturating_reduce(available_for_collection);
		self.available_amount.saturating_reduce(available_for_collection);
		available_for_collection
	}

	/// Inform the pool that it won't be receiving `amount` as a result of account liquidation
	/// not being able to recover the debt in full. This effectively socialises the loss by
	/// reducing the pool's total amount.
	pub fn write_off_unrecoverable_debt(&mut self, amount: AssetAmount) {
		self.total_amount.saturating_reduce(amount);
	}

	/// Receives repayment funds in the pool's asset (after they has been swapped)
	pub fn receive_repayment(&mut self, amount: AssetAmount) {
		// Principal repayment only affects the available amount,
		// not the total amount (as we never deduct borrowed funds from the total amount
		// in the first place)
		self.available_amount.saturating_accrue(amount);
	}

	pub fn get_supply_position_for_account(
		&self,
		account_id: &AccountId,
	) -> Result<AssetAmount, LendingPoolError> {
		let share = self
			.lender_shares
			.get(account_id)
			.ok_or(LendingPoolError::LenderNotFoundInPool)?;

		Ok(*share * self.total_amount)
	}

	pub fn get_all_supply_positions(&self) -> Vec<LendingSupplyPosition<AccountId, AssetAmount>> {
		self.lender_shares
			.iter()
			.map(|(lp_id, share)| LendingSupplyPosition {
				lp_id: lp_id.clone(),
				total_amount: *share * self.total_amount,
			})
			.collect()
	}
}

#[cfg(test)]
mod tests {

	use super::*;
	use frame_support::assert_ok;
	use mocks::AccountId;

	// Note that the precision of expected values is lower because we want to ignore rounding
	// errors.
	#[track_caller]
	fn check_shares(
		chp_pool: &LendingPool<AccountId>,
		expected_shares: impl IntoIterator<Item = (AccountId, Perquintill)>,
	) {
		if let Some(total_shares) =
			chp_pool.lender_shares.values().copied().reduce(|acc, share| acc + share)
		{
			assert_eq!(total_shares, Perquintill::one());
		}

		let mut expected_shares_count = 0;
		for (lender, expected_share) in expected_shares {
			let actual_share = chp_pool.lender_shares.get(&lender).expect("Lender should exist");

			let abs_diff = expected_share.deconstruct().abs_diff(actual_share.deconstruct());

			assert!(abs_diff <= 10, "Large error: {abs_diff}");

			expected_shares_count += 1;
		}

		assert_eq!(expected_shares_count, chp_pool.lender_shares.len());
	}

	const LENDER_1: AccountId = 123;
	const LENDER_2: AccountId = 234;
	const LENDER_3: AccountId = 345;

	#[test]
	fn adding_and_removing_funds() {
		let mut chp_pool = LendingPool::<u64>::new();

		chp_pool.add_funds(&LENDER_1, 100);

		assert_eq!(chp_pool.total_amount, 100);
		assert_eq!(chp_pool.available_amount, 100);
		check_shares(&chp_pool, [(LENDER_1, Perquintill::one())]);

		chp_pool.add_funds(&LENDER_1, 200);

		assert_eq!(chp_pool.total_amount, 300);
		assert_eq!(chp_pool.available_amount, 300);
		check_shares(&chp_pool, [(LENDER_1, Perquintill::one())]);

		chp_pool.add_funds(&LENDER_2, 200);

		assert_eq!(chp_pool.total_amount, 500);
		assert_eq!(chp_pool.available_amount, 500);
		check_shares(
			&chp_pool,
			[(LENDER_1, Perquintill::from_percent(60)), (LENDER_2, Perquintill::from_percent(40))],
		);

		chp_pool.add_funds(&LENDER_2, 100);

		assert_eq!(chp_pool.total_amount, 600);
		assert_eq!(chp_pool.available_amount, 600);
		check_shares(
			&chp_pool,
			[(LENDER_1, Perquintill::from_percent(50)), (LENDER_2, Perquintill::from_percent(50))],
		);

		chp_pool.add_funds(&LENDER_1, 200);

		assert_eq!(chp_pool.total_amount, 800);
		assert_eq!(chp_pool.available_amount, 800);
		check_shares(
			&chp_pool,
			[
				(LENDER_1, Perquintill::from_rational(5u64, 8)),
				(LENDER_2, Perquintill::from_rational(3u64, 8)),
			],
		);

		chp_pool.add_funds(&LENDER_3, 300);

		assert_eq!(chp_pool.total_amount, 1100);
		assert_eq!(chp_pool.available_amount, 1100);
		check_shares(
			&chp_pool,
			[
				(LENDER_1, Perquintill::from_rational(5u64, 11u64)),
				(LENDER_2, Perquintill::from_rational(3u64, 11u64)),
				(LENDER_3, Perquintill::from_rational(3u64, 11u64)),
			],
		);

		// --- Start removing funds here ---
		assert_eq!(
			chp_pool.remove_funds(&LENDER_2, None),
			Ok(WithdrawnAndRemainingAmounts { withdrawn_amount: 300, remaining_amount: 0 })
		);
		assert_eq!(chp_pool.total_amount, 800);
		assert_eq!(chp_pool.available_amount, 800);

		check_shares(
			&chp_pool,
			[
				(LENDER_1, Perquintill::from_rational(5u64, 8)),
				(LENDER_3, Perquintill::from_rational(3u64, 8)),
			],
		);

		assert_eq!(
			chp_pool.remove_funds(&LENDER_3, None),
			Ok(WithdrawnAndRemainingAmounts { withdrawn_amount: 300, remaining_amount: 0 })
		);
		assert_eq!(chp_pool.total_amount, 500);
		assert_eq!(chp_pool.available_amount, 500);

		check_shares(&chp_pool, [(LENDER_1, Perquintill::from_percent(100))]);

		assert_eq!(
			chp_pool.remove_funds(&LENDER_1, None),
			Ok(WithdrawnAndRemainingAmounts { withdrawn_amount: 500, remaining_amount: 0 })
		);
		assert_eq!(chp_pool.total_amount, 0);
		assert_eq!(chp_pool.available_amount, 0);

		check_shares(&chp_pool, []);
	}

	#[test]
	fn remove_funds_partially() {
		let mut chp_pool = LendingPool::<AccountId>::new();

		chp_pool.add_funds(&LENDER_1, 500);
		chp_pool.add_funds(&LENDER_2, 400);
		chp_pool.add_funds(&LENDER_3, 100);

		assert_ok!(chp_pool.provide_funds_for_loan(600));

		assert_eq!(chp_pool.get_utilisation(), Permill::from_percent(60));

		// Lender 1 requests only a portion of their funds:
		assert_eq!(
			chp_pool.remove_funds(&LENDER_1, Some(100)),
			Ok(WithdrawnAndRemainingAmounts { withdrawn_amount: 100, remaining_amount: 400 })
		);

		assert_eq!(chp_pool.total_amount, 900);
		assert_eq!(chp_pool.available_amount, 300);

		check_shares(
			&chp_pool,
			[
				(LENDER_1, Perquintill::from_rational(4u64, 9)),
				(LENDER_2, Perquintill::from_rational(4u64, 9)),
				(LENDER_3, Perquintill::from_rational(1u64, 9)),
			],
		);

		// Lender 1 now requests all remaining funds, but can only get some of them:
		assert_eq!(
			chp_pool.remove_funds(&LENDER_1, None),
			Ok(WithdrawnAndRemainingAmounts { withdrawn_amount: 300, remaining_amount: 100 })
		);

		assert_eq!(chp_pool.get_utilisation(), Permill::from_percent(100));

		assert_eq!(chp_pool.total_amount, 600);
		assert_eq!(chp_pool.available_amount, 0);

		// Lender 1 still has a share in the pool:
		check_shares(
			&chp_pool,
			[
				(LENDER_1, Perquintill::from_rational(1u64, 6)),
				(LENDER_2, Perquintill::from_rational(2u64, 3)),
				(LENDER_3, Perquintill::from_rational(1u64, 6)),
			],
		);
	}
}
