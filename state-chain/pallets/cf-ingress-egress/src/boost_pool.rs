#[cfg(test)]
mod tests;

use sp_runtime::{
	helpers_128bit::multiply_by_rational_with_rounding, Rounding, SaturatedConversion,
};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

use super::*;

const SCALE_FACTOR: u128 = 1000;
/// Represents 1/SCALE_FACTOR of Asset amount as a way to gain extra precision.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
struct ScaledAmount<C: Chain> {
	val: u128,
	_phantom: PhantomData<C>,
}

// Manually implementing Default because deriving didn't work due to generic parameter:
impl<C: Chain> Default for ScaledAmount<C> {
	fn default() -> Self {
		Self { val: Default::default(), _phantom: Default::default() }
	}
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
	pending_boosts: BTreeMap<BoostId, BTreeMap<AccountId, ScaledAmount<C>>>,
	// Stores boosters who have indicated that they want to stop boosting along with
	// the pending deposits that they have to wait to be finalised
	pending_withdrawals: BTreeMap<AccountId, BTreeSet<BoostId>>,
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

	fn add_funds_inner(&mut self, account_id: AccountId, added_amount: ScaledAmount<C>) {
		// To keep things simple, we assume that the booster no longer wants to withdraw
		// if they add more funds:
		self.pending_withdrawals.remove(&account_id);

		self.amounts.entry(account_id).or_default().saturating_accrue(added_amount);
		self.available_amount.saturating_accrue(added_amount);
	}

	pub(crate) fn add_funds(&mut self, account_id: AccountId, added_amount: C::ChainAmount) {
		self.add_funds_inner(account_id, ScaledAmount::from_chain_amount(added_amount));
	}

	pub(crate) fn get_available_amount(&self) -> C::ChainAmount {
		self.available_amount.into_chain_amount()
	}

	pub(crate) fn provide_funds_for_boosting(
		&mut self,
		boost_id: BoostId,
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

		self.use_funds_for_boosting(boost_id, provided_amount, fee_amount)?;

		Ok((
			provided_amount.saturating_add(fee_amount).into_chain_amount(),
			fee_amount.into_chain_amount(),
		))
	}

	/// Records `amount_needed` as being used for boosting and to be re-distributed
	/// among current boosters (along with the fee) upon finalisation
	fn use_funds_for_boosting(
		&mut self,
		boost_id: BoostId,
		required_amount: ScaledAmount<C>,
		boost_fee: ScaledAmount<C>,
	) -> Result<(), &'static str> {
		let current_total_active_amount = self.available_amount;

		self.available_amount = self
			.available_amount
			.checked_sub(required_amount)
			.ok_or("Not enough active funds")?;

		let mut amounts = self.amounts.iter_mut();

		// We must have at least one entry because the available amount is non-zero:
		let (first_booster_account, first_booster_amount) =
			amounts.next().ok_or("No boost amount entries found")?;

		let mut total_contributed = ScaledAmount::<C>::default();
		let mut to_receive_recorded = ScaledAmount::default();

		let amount_to_receive = required_amount.saturating_add(boost_fee);

		let mut boosters_to_receive: BTreeMap<_, _> = amounts
			.map(|(booster_id, amount)| {
				let booster_contribution = multiply_by_rational_with_rounding(
					required_amount.into(),
					(*amount).into(),
					current_total_active_amount.into(),
					Rounding::NearestPrefDown,
				)
				// booster's amount is always <= total amount so default due to overflow should be
				// impossible
				.unwrap_or_default()
				.into();

				// Same as above, but also includes fees:
				let booster_to_receive = multiply_by_rational_with_rounding(
					amount_to_receive.into(),
					(*amount).into(),
					current_total_active_amount.into(),
					Rounding::NearestPrefDown,
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

		// Compute the amount for the first contributor working backwards from the amount needed
		// to negate any rounding errors made so far:
		let remaining_required = required_amount.saturating_sub(total_contributed);
		let remaining_to_receive = amount_to_receive.saturating_sub(to_receive_recorded);
		first_booster_amount.saturating_reduce(remaining_required);
		boosters_to_receive.insert(first_booster_account.clone(), remaining_to_receive);

		// For every active booster, record how much of this particular deposit they are owed,
		// (which is their pool share at the time of boosting):
		self.pending_boosts
			.try_insert(boost_id, boosters_to_receive)
			.map_err(|_| "Pending boost id already exists")?;

		Ok(())
	}

	pub(crate) fn on_finalised_deposit(
		&mut self,
		boost_id: BoostId,
	) -> Vec<(AccountId, C::ChainAmount)> {
		let Some(boost_contributions) = self.pending_boosts.remove(&boost_id) else {
			// The deposit hadn't been boosted
			return vec![];
		};

		let mut unlocked_funds = vec![];

		for (booster_id, amount) in boost_contributions {
			// Depending on whether the booster is withdrawing, add deposits to
			// their free balance or back to the available boost pool:
			if let Some(pending_deposits) = self.pending_withdrawals.get_mut(&booster_id) {
				if !pending_deposits.remove(&boost_id) {
					log::warn!("Withdrawing booster contributed to boost {boost_id}, but it is not in pending withdrawals");
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

	pub fn on_lost_deposit(&mut self, boost_id: BoostId) {
		let Some(booster_contributions) = self.pending_boosts.remove(&boost_id) else {
			log_or_panic!("Failed to find boost record for a lost deposit: {boost_id}");
			return;
		};

		for booster_id in booster_contributions.keys() {
			if let Some(pending_deposits) = self.pending_withdrawals.get_mut(booster_id) {
				if !pending_deposits.remove(&boost_id) {
					log::warn!("Withdrawing booster contributed to boost {boost_id}, but it is not in pending withdrawals");
				}

				if pending_deposits.is_empty() {
					self.pending_withdrawals.remove(booster_id);
				}
			}
		}
	}

	// Return the amount immediately available for booster
	pub fn stop_boosting(&mut self, booster_id: AccountId) -> Result<C::ChainAmount, &'static str> {
		let Some(booster_active_amount) = self.amounts.remove(&booster_id) else {
			return Err("Account not found in boost pool")
		};

		self.available_amount.saturating_reduce(booster_active_amount);

		let pending_deposits: BTreeSet<_> = self
			.pending_boosts
			.iter()
			.filter(|(_, owed_amounts)| owed_amounts.contains_key(&booster_id))
			.map(|(boost_id, _)| *boost_id)
			.collect();

		if !pending_deposits.is_empty() {
			self.pending_withdrawals.insert(booster_id, pending_deposits);
		}

		Ok(booster_active_amount.into_chain_amount())
	}

	#[cfg(test)]
	pub fn get_pending_boosts(&self) -> Vec<BoostId> {
		self.pending_boosts.keys().copied().collect()
	}
}
