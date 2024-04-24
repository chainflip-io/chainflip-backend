#[cfg(test)]
mod tests;

use frame_support::DefaultNoBound;
use sp_runtime::{
	helpers_128bit::multiply_by_rational_with_rounding, Rounding, SaturatedConversion,
};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

use super::*;

const SCALE_FACTOR: u128 = 1000;
/// Represents 1/SCALE_FACTOR of Asset amount as a way to gain extra precision.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, DefaultNoBound)]
struct ScaledAmount<C: Chain> {
	val: u128,
	_phantom: PhantomData<C>,
}

impl<C: Chain> PartialOrd for ScaledAmount<C> {
	fn partial_cmp(&self, other: &Self) -> Option<scale_info::prelude::cmp::Ordering> {
		self.val.partial_cmp(&other.val)
	}
}

impl<C: Chain> Copy for ScaledAmount<C> {}

impl<C: Chain> From<ScaledAmount<C>> for u128 {
	fn from(amount: ScaledAmount<C>) -> Self {
		amount.val
	}
}

impl<C: Chain> From<u128> for ScaledAmount<C> {
	fn from(val: u128) -> Self {
		ScaledAmount { val, _phantom: PhantomData }
	}
}

impl<C: Chain> ScaledAmount<C> {
	fn from_chain_amount(amount: C::ChainAmount) -> Self {
		let amount: u128 = amount.saturated_into();
		amount.saturating_mul(SCALE_FACTOR).into()
	}

	fn into_chain_amount(self) -> C::ChainAmount {
		self.val
			.checked_div(SCALE_FACTOR)
			.expect("Scale factor is not 0")
			.saturated_into()
	}

	fn checked_sub(self, rhs: Self) -> Option<Self> {
		self.val.checked_sub(rhs.val).map(|val| val.into())
	}

	fn saturating_sub(self, rhs: Self) -> Self {
		self.val.saturating_sub(rhs.val).into()
	}

	#[cfg(test)]
	fn checked_add(self, rhs: Self) -> Option<Self> {
		self.val.checked_add(rhs.val).map(|val| val.into())
	}

	fn saturating_add(self, rhs: Self) -> Self {
		self.val.saturating_add(rhs.val).into()
	}

	fn saturating_accrue(&mut self, rhs: Self) {
		self.val.saturating_accrue(rhs.val)
	}

	fn saturating_reduce(&mut self, rhs: Self) {
		self.val.saturating_reduce(rhs.val)
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BoostPool<AccountId, C: Chain> {
	// Fee charged by the pool
	fee_bps: BasisPoints,
	// Total available amount (not currently used in any boost)
	available_amount: ScaledAmount<C>,
	// Mapping from booster to the available amount they own in `available_amount`
	amounts: BTreeMap<AccountId, ScaledAmount<C>>,
	// Boosted deposits awaiting finalisation and how much of them is owed to which booster
	pending_boosts: BTreeMap<PrewitnessedDepositId, BTreeMap<AccountId, ScaledAmount<C>>>,
	// Stores boosters who have indicated that they want to stop boosting along with
	// the pending deposits that they have to wait to be finalised
	pending_withdrawals: BTreeMap<AccountId, BTreeSet<PrewitnessedDepositId>>,
}

impl<AccountId, C: Chain> BoostPool<AccountId, C>
where
	AccountId: PartialEq + Ord + Clone + core::fmt::Debug,
	for<'a> &'a AccountId: PartialEq,
	C::ChainAmount: PartialOrd,
{
	pub(crate) fn new(fee_bps: BasisPoints) -> Self {
		Self {
			fee_bps,
			available_amount: Default::default(),
			amounts: Default::default(),
			pending_boosts: Default::default(),
			pending_withdrawals: Default::default(),
		}
	}

	fn compute_fee(&self, amount_to_boost: ScaledAmount<C>) -> ScaledAmount<C> {
		const BASIS_POINTS_PER_MILLION: u32 = 100;
		ScaledAmount {
			val: Permill::from_parts(self.fee_bps as u32 * BASIS_POINTS_PER_MILLION) *
				amount_to_boost.val,
			_phantom: PhantomData,
		}
	}

	fn add_funds_inner(&mut self, booster_id: AccountId, added_amount: ScaledAmount<C>) {
		// To keep things simple, we assume that the booster no longer wants to withdraw
		// if they add more funds:
		self.pending_withdrawals.remove(&booster_id);

		self.amounts.entry(booster_id).or_default().saturating_accrue(added_amount);
		self.available_amount.saturating_accrue(added_amount);
	}

	pub(crate) fn add_funds(&mut self, booster_id: AccountId, added_amount: C::ChainAmount) {
		self.add_funds_inner(booster_id, ScaledAmount::from_chain_amount(added_amount));
	}

	pub fn get_available_amount(&self) -> C::ChainAmount {
		self.available_amount.into_chain_amount()
	}

	/// Attempt to use pool's available funds to boost up to `amount_to_boost`. Returns
	/// (boosted_amount, boost_fee), where "boosted amount" is the amount provided by the pool plus
	/// the boost fee. For example, in the (likely common) case of having sufficient funds in a
	/// single pool the boosted amount will exactly equal the amount prewitnessed.
	pub(crate) fn provide_funds_for_boosting(
		&mut self,
		prewitnessed_deposit_id: PrewitnessedDepositId,
		amount_to_boost: C::ChainAmount,
	) -> Result<(C::ChainAmount, C::ChainAmount), &'static str> {
		let amount_to_boost = ScaledAmount::<C>::from_chain_amount(amount_to_boost);
		let full_amount_fee = self.compute_fee(amount_to_boost);

		let required_amount = amount_to_boost.saturating_sub(full_amount_fee);

		let (provided_amount, fee_amount) = if self.available_amount >= required_amount {
			(required_amount, full_amount_fee)
		} else {
			let provided_amount = self.available_amount;
			let fee = self.compute_fee(provided_amount);

			(provided_amount, fee)
		};

		self.use_funds_for_boosting(prewitnessed_deposit_id, provided_amount, fee_amount)?;

		Ok((
			provided_amount.saturating_add(fee_amount).into_chain_amount(),
			fee_amount.into_chain_amount(),
		))
	}

	/// Records `amount_needed` as being used for boosting and to be re-distributed
	/// among current boosters (along with the fee) upon finalisation
	fn use_funds_for_boosting(
		&mut self,
		prewitnessed_deposit_id: PrewitnessedDepositId,
		required_amount: ScaledAmount<C>,
		boost_fee: ScaledAmount<C>,
	) -> Result<(), &'static str> {
		let current_total_available_amount = self.available_amount;

		self.available_amount = self
			.available_amount
			.checked_sub(required_amount)
			.ok_or("Not enough available funds")?;

		let mut total_contributed = ScaledAmount::<C>::default();
		let mut to_receive_recorded = ScaledAmount::default();

		let amount_to_receive = required_amount.saturating_add(boost_fee);

		let mut boosters_to_receive: BTreeMap<_, _> = self
			.amounts
			.iter_mut()
			.map(|(booster_id, amount)| {
				// Round deducted amount up to ensure that rounding errors don't affect our
				// ability to contribute required amount (note that the result can never be
				// greater than boosters `amount` since we checked that required_amount <=
				// total_available_amount)
				let booster_contribution = multiply_by_rational_with_rounding(
					required_amount.into(),
					(*amount).into(),
					current_total_available_amount.into(),
					Rounding::Up,
				)
				// booster's amount is always <= total amount so default due to overflow should be
				// impossible
				.unwrap_or_default()
				.into();

				// Same as above, but also includes fees (note, however, that we round down
				// to ensure that we don't distribute more than we have)
				let booster_to_receive = multiply_by_rational_with_rounding(
					amount_to_receive.into(),
					(*amount).into(),
					current_total_available_amount.into(),
					Rounding::Down,
				)
				// booster's amount is always <= total amount so default due to overflow should be
				// impossible
				.unwrap_or_default()
				.into();

				// Amount should always be large enough at this point, but saturating to be safe:
				amount.saturating_reduce(booster_contribution);

				total_contributed.saturating_accrue(booster_contribution);
				to_receive_recorded.saturating_accrue(booster_to_receive);

				(booster_id.clone(), booster_to_receive)
			})
			.collect();

		// This shouldn't saturate due to rounding contributions up:
		let excess_contributed = total_contributed.saturating_sub(required_amount);
		// This shouldn't saturate due to rounding amounts to receive down:
		let remaining_to_receive = amount_to_receive.saturating_sub(to_receive_recorded);

		// Some "lucky" booster will receive both of the above (inconsequential) amounts to
		// ensure that we correctly account for every single atomic unit even in presence
		// of rounding errors:
		use nanorand::{Rng, WyRand};
		let lucky_index =
			WyRand::new_seed(prewitnessed_deposit_id).generate_range(0..self.amounts.len());
		if let Some((lucky_id, amount)) = self.amounts.iter_mut().nth(lucky_index) {
			amount.saturating_accrue(excess_contributed);

			if let Some(amount) = boosters_to_receive.get_mut(lucky_id) {
				amount.saturating_accrue(remaining_to_receive)
			}
		}

		// For every active booster, record how much of this particular deposit they are owed,
		// (which is their pool share at the time of boosting):
		self.pending_boosts
			.try_insert(prewitnessed_deposit_id, boosters_to_receive)
			.map_err(|_| "Pending boost id already exists")?;

		Ok(())
	}

	pub(crate) fn on_finalised_deposit(
		&mut self,
		prewitnessed_deposit_id: PrewitnessedDepositId,
	) -> Vec<(AccountId, C::ChainAmount)> {
		let Some(boost_contributions) = self.pending_boosts.remove(&prewitnessed_deposit_id) else {
			// The deposit hadn't been boosted
			return vec![];
		};

		let mut unlocked_funds = vec![];

		for (booster_id, amount) in boost_contributions {
			// Depending on whether the booster is withdrawing, add deposits to
			// their free balance or back to the available boost pool:
			if let Some(pending_deposits) = self.pending_withdrawals.get_mut(&booster_id) {
				if !pending_deposits.remove(&prewitnessed_deposit_id) {
					log::warn!("Withdrawing booster contributed to boost {prewitnessed_deposit_id}, but it is not in pending withdrawals");
				}

				if pending_deposits.is_empty() {
					self.pending_withdrawals.remove(&booster_id);
				}

				unlocked_funds.push((booster_id, amount.into_chain_amount()));
			} else {
				self.add_funds_inner(booster_id, amount);
			}
		}

		unlocked_funds
	}

	// Returns the number of boosters affected
	pub fn on_lost_deposit(&mut self, prewitnessed_deposit_id: PrewitnessedDepositId) -> usize {
		let Some(booster_contributions) = self.pending_boosts.remove(&prewitnessed_deposit_id)
		else {
			log_or_panic!(
				"Failed to find boost record for a lost deposit: {prewitnessed_deposit_id}"
			);
			return 0;
		};

		for booster_id in booster_contributions.keys() {
			if let Some(pending_deposits) = self.pending_withdrawals.get_mut(booster_id) {
				if !pending_deposits.remove(&prewitnessed_deposit_id) {
					log::warn!("Withdrawing booster contributed to boost {prewitnessed_deposit_id}, but it is not in pending withdrawals");
				}

				if pending_deposits.is_empty() {
					self.pending_withdrawals.remove(booster_id);
				}
			}
		}

		booster_contributions.len()
	}

	// Return the amount immediately unlocked for the booster and a list of all pending boosts that
	// the booster is still a part of.
	pub fn stop_boosting(
		&mut self,
		booster_id: AccountId,
	) -> Result<(C::ChainAmount, BTreeSet<PrewitnessedDepositId>), &'static str> {
		let Some(booster_active_amount) = self.amounts.remove(&booster_id) else {
			return Err("Account not found in boost pool")
		};

		self.available_amount.saturating_reduce(booster_active_amount);

		let pending_deposits: BTreeSet<_> = self
			.pending_boosts
			.iter()
			.filter(|(_, owed_amounts)| owed_amounts.contains_key(&booster_id))
			.map(|(prewitnessed_deposit_id, _)| *prewitnessed_deposit_id)
			.collect();

		if !pending_deposits.is_empty() {
			self.pending_withdrawals.insert(booster_id, pending_deposits.clone());
		}

		Ok((booster_active_amount.into_chain_amount(), pending_deposits))
	}

	#[cfg(test)]
	pub fn get_pending_boosts(&self) -> Vec<PrewitnessedDepositId> {
		self.pending_boosts.keys().copied().collect()
	}
	#[cfg(test)]
	pub fn get_available_amount_for_account(
		&self,
		booster_id: &AccountId,
	) -> Option<C::ChainAmount> {
		self.amounts.get(booster_id).copied().map(|a| a.into_chain_amount())
	}
}
