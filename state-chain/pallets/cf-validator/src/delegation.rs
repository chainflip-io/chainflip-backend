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
use crate::{
	AuctionOutcome, Config, DelegationSnapshots, HistoricalAuthorities, HistoricalBonds, Pallet,
	ValidatorToOperator,
};
use cf_primitives::{AssetAmount, EpochIndex};
use cf_traits::{EpochInfo, RewardsDistribution, Slashing};
use codec::{Decode, DecodeWithMemTracking, Encode, FullCodec, MaxEncodedLen};
use core::iter::Sum;
use frame_support::{
	sp_runtime::{
		traits::{AtLeast32BitUnsigned, Zero},
		FixedPointOperand, Perquintill, Saturating,
	},
	traits::{Get, IsType},
	BoundedBTreeMap, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	marker::PhantomData,
	prelude::*,
};

pub const DEFAULT_MIN_OPERATOR_FEE: u32 = 1_500;
pub const MAX_OPERATOR_FEE: u32 = 10_000;

pub const MAX_VALIDATORS_PER_OPERATOR: usize = 20;

pub enum AssociationToOperator {
	Validator,
	Delegator,
}

#[derive(
	Debug,
	Default,
	Clone,
	Copy,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Serialize,
	Deserialize,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum DelegationAmount<T> {
	#[default]
	Max,
	Some(T),
}

impl<T> DelegationAmount<T> {
	pub fn try_fmap<B, E>(
		self,
		f: impl FnOnce(T) -> Result<B, E>,
	) -> Result<DelegationAmount<B>, E> {
		match self {
			DelegationAmount::Max => Ok(DelegationAmount::Max),
			DelegationAmount::Some(amount) => Ok(DelegationAmount::Some(f(amount)?)),
		}
	}

	pub fn is_max(&self) -> bool {
		matches!(self, DelegationAmount::Max)
	}
}

#[derive(
	Debug,
	Clone,
	Copy,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum Change<T> {
	Increase(T),
	Decrease(T),
}

/// Represents a validator's default stance on accepting delegations
#[derive(
	Copy,
	Clone,
	PartialEq,
	Eq,
	Debug,
	Default,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Deserialize,
	Serialize,
)]
pub enum DelegationAcceptance {
	/// Allow all delegators by default, except those explicitly blocked
	Allow,
	/// Deny all delegators by default, except those explicitly allowed
	#[default] // Default to denying delegations
	Deny,
}

/// Parameters for validator delegation preferences
#[derive(
	Default,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Clone,
	PartialEq,
	Eq,
	Debug,
	Deserialize,
	Serialize,
)]
pub struct OperatorSettings {
	pub fee_bps: u32,
	/// Default delegation acceptance preference for this validator
	pub delegation_acceptance: DelegationAcceptance,
}

/// A delegator's live delegation plan: the set of operators it delegates to and the exact
/// amount pledged to each. `delegate_multi` stores this exactly as submitted -- an
/// over-subscribed plan (total pledged > balance) isn't rejected or scaled down at submission
/// time, it's resolved at auction-resolution time instead (`build_delegation_snapshots` prorates
/// each entry proportionally to whatever balance is actually available).
#[derive(
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
	DebugNoBound,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Serialize,
	Deserialize,
)]
#[scale_info(skip_type_params(N))]
#[serde(
	bound = "Account: Ord + Serialize + for<'a> Deserialize<'a>, Value: Serialize + for<'a> Deserialize<'a>"
)]
pub enum DelegationPlan<
	Account: Ord + Clone + PartialEq + Eq + core::fmt::Debug,
	Value: Clone + PartialEq + Eq + core::fmt::Debug,
	N: Get<u32>,
> {
	Fixed(BoundedBTreeMap<Account, Value, N>),
	// v2, appended later:
	// Proportional(BoundedBTreeMap<Account, Perbill, N>),
}

impl<
		Account: Ord + Clone + PartialEq + Eq + core::fmt::Debug,
		Value: Clone + PartialEq + Eq + core::fmt::Debug,
		N: Get<u32>,
	> Default for DelegationPlan<Account, Value, N>
{
	fn default() -> Self {
		Self::Fixed(Default::default())
	}
}

impl<
		Account: Ord + Clone + PartialEq + Eq + core::fmt::Debug,
		Value: Clone + PartialEq + Eq + core::fmt::Debug + Zero,
		N: Get<u32>,
	> DelegationPlan<Account, Value, N>
{
	pub fn len(&self) -> usize {
		match self {
			Self::Fixed(entries) => entries.len(),
		}
	}

	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	/// The operators this plan delegates to.
	pub fn iter_operators(&self) -> impl Iterator<Item = &Account> {
		match self {
			Self::Fixed(entries) => entries.keys(),
		}
	}

	/// The amount pledged to `operator`, if this plan has an entry for it.
	pub fn get(&self, operator: &Account) -> Option<&Value> {
		match self {
			Self::Fixed(entries) => entries.get(operator),
		}
	}

	/// The sum of every entry's amount.
	pub fn total(&self) -> Value
	where
		Value: Copy + Sum<Value>,
	{
		match self {
			Self::Fixed(entries) => entries.values().copied().sum(),
		}
	}

	/// Removes `operator`'s entry, if any, returning its amount. Safe to call unconditionally:
	/// removing an entry can only shrink an already-bounded plan.
	pub fn remove(&mut self, operator: &Account) -> Option<Value> {
		match self {
			Self::Fixed(entries) => entries.remove(operator),
		}
	}

	/// Inserts (or replaces) `operator`'s entry. Fails, leaving the plan unchanged, if the plan
	/// is already at its bound and `operator` isn't already an entry, or if the result would no
	/// longer be `is_valid` (e.g. its total no longer fits in `AssetAmount`).
	#[expect(clippy::result_unit_err)]
	pub fn try_insert(&mut self, operator: Account, value: Value) -> Result<Option<Value>, ()>
	where
		Value: Copy + Into<AssetAmount>,
	{
		let mut updated = self.clone();
		let previous = match &mut updated {
			Self::Fixed(entries) => entries.try_insert(operator, value).map_err(|_| ())?,
		};
		if !updated.is_valid(0) {
			return Err(());
		}
		*self = updated;
		Ok(previous)
	}

	pub fn pop(&mut self) -> Option<(Account, Value)> {
		match self {
			Self::Fixed(entries) => {
				let operator = entries.keys().next()?.clone();
				entries.remove(&operator).map(|value| (operator, value))
			},
		}
	}

	/// Resolves each `is_live` entry's amount against `balance`: if the live entries' total
	/// fits within `balance`, each passes through unchanged; otherwise each is scaled down
	/// proportionally, rounded down per entry so the resolved total never exceeds `balance`.
	/// Entries that aren't `is_live`, or that resolve to zero, are omitted -- a zero bid can't
	/// claim anything either way.
	pub fn resolve_bids(
		&self,
		balance: Value,
		is_live: impl Fn(&Account) -> bool,
	) -> BTreeMap<Account, Value>
	where
		Value: Copy + AtLeast32BitUnsigned + FixedPointOperand + From<u64>,
	{
		match self {
			Self::Fixed(entries) => {
				let live: Vec<(&Account, Value)> = entries
					.iter()
					.filter(|(operator, _)| is_live(operator))
					.map(|(operator, bid)| (operator, *bid))
					.collect();

				// `is_valid` (enforced by `delegate_multi` at submission time) already rejects
				// any plan whose total doesn't fit in `Value`, and `live` is a subset of that
				// plan, so this checked sum should never actually overflow.
				let total_committed = live
					.iter()
					.try_fold(Value::zero(), |acc, (_, bid)| acc.checked_add(bid))
					.unwrap_or_else(|| {
						cf_runtime_utilities::log_or_panic!(
							"plan's total overflowed Value -- unreachable, `is_valid` rejects this at submission time"
						);
						Value::max_value()
					});

				live.into_iter()
					.filter_map(|(operator, bid)| {
						let resolved = if total_committed <= balance {
							bid
						} else {
							Perquintill::from_rational(bid, total_committed).mul_floor(balance)
						};
						(!resolved.is_zero()).then(|| (operator.clone(), resolved))
					})
					.collect()
			},
		}
	}

	///  A plan is invalid if:
	/// - any entry's amount is zero a delegator should omit an operator entirely rather than
	///   include it with nothing pledged
	/// - a non-empty plan's total falls below `minimum_total`
	/// - the total doesn't even fit in `AssetAmount`. This is the only place a plan's total is
	///   allowed to not fit -- rejecting it here means every other consumer of a *stored* plan
	///   (which only ever got there via this check) can safely assume its total fits within
	///   `Value`, without needing to re-guard against overflow.
	pub fn is_valid(&self, minimum_total: AssetAmount) -> bool
	where
		Value: Copy + Into<AssetAmount>,
	{
		match self {
			Self::Fixed(entries) =>
				entries.values().all(|amount| !amount.is_zero()) &&
					(entries.is_empty() ||
						match entries
							.values()
							.copied()
							.try_fold(0u128, |acc, amount| acc.checked_add(amount.into()))
						{
							Some(total) => total >= minimum_total,
							None => false,
						}),
		}
	}

	/// Builds a `Fixed` plan directly from an `account -> value` map. Fails if `entries` doesn't
	/// fit within the bound `N`.
	#[expect(clippy::result_unit_err)]
	pub fn try_from_map(entries: BTreeMap<Account, Value>) -> Result<Self, ()> {
		Ok(Self::Fixed(entries.try_into()?))
	}
}

/// A snapshot of delegations to an operator for a specific epoch, including all
/// necessary information for reward distribution.
#[derive(
	Clone,
	PartialEq,
	Eq,
	Default,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Debug,
	Serialize,
	Deserialize,
)]
pub struct DelegationSnapshot<Account: Ord, Bid> {
	pub operator: Account,
	/// Map of validator accounts to their bid amounts.
	pub validators: BTreeMap<Account, Bid>,
	/// Map of delegator accounts to their bid amounts.
	pub delegators: BTreeMap<Account, Bid>,
	/// Operator fee at time of snapshot creation.
	pub delegation_fee_bps: u32,
}

impl<Account: Ord + Clone + FullCodec + 'static, Bid: FullCodec + 'static>
	DelegationSnapshot<Account, Bid>
{
	/// Stores the validator mappings and snapshot information for the given epoch.
	pub fn register_for_epoch<T: Config<AccountId = Account, Amount = Bid>>(
		self,
		epoch_index: EpochIndex,
	) {
		let operator = self.operator.clone();
		for validator in self.validators.keys() {
			ValidatorToOperator::<T>::insert(epoch_index, validator, operator.clone());
		}
		DelegationSnapshots::<T>::insert(epoch_index, operator, self);
	}

	pub fn clear_epoch_registrations<T: Config<AccountId = Account, Amount = Bid>>(
		epoch_index: EpochIndex,
	) {
		let _ = DelegationSnapshots::<T>::clear_prefix(epoch_index, u32::MAX, None);
		let _ = ValidatorToOperator::<T>::clear_prefix(epoch_index, u32::MAX, None);
	}
}

impl<Account, Bid> DelegationSnapshot<Account, Bid>
where
	Account: Ord + Clone,
	Bid: Default + Copy + From<u64> + AtLeast32BitUnsigned + Sum,
{
	pub fn init(operator: &Account, delegation_fee_bps: u32) -> Self {
		Self {
			operator: operator.clone(),
			delegators: Default::default(),
			validators: Default::default(),
			delegation_fee_bps,
		}
	}

	fn total_validator_bid(&self) -> Bid {
		self.validators.values().copied().sum()
	}

	fn total_delegator_bid(&self) -> Bid {
		self.delegators.values().copied().sum()
	}

	pub fn total_available_bid(&self) -> Bid {
		self.total_validator_bid() + self.total_delegator_bid()
	}

	pub fn effective_validator_bids(&self) -> BTreeMap<Account, Bid> {
		if self.validators.is_empty() {
			return Default::default();
		}
		let avg_bid = self.avg_bid();
		self.validators.keys().map(|validator| (validator.clone(), avg_bid)).collect()
	}

	/// Returns a mapping of the bond amounts for each validator such that
	/// the full bond is accounted for.
	pub fn validator_bond_distribution(&self, bond: Bid) -> BTreeMap<Account, Bid> {
		let mut total_bond = bond * Bid::from(self.validators.len() as u32);
		let mut validator_bids = self.validators.clone().into_iter().collect::<Vec<_>>();
		validator_bids.sort_by_key(|(_, v)| *v);
		validator_bids
			.into_iter()
			.map(|(id, bid)| {
				let individual_bond = core::cmp::min(bid, total_bond);
				total_bond.saturating_reduce(individual_bond);
				(id, individual_bond)
			})
			.collect()
	}

	pub fn distribute<Amount>(
		&self,
		total: Amount,
		bond: Amount,
	) -> impl Iterator<Item = (&Account, Amount)>
	where
		Amount: From<Bid> + AtLeast32BitUnsigned + Copy + Sum + From<u64>,
	{
		let total_delegator_stake: Amount = self.total_delegator_bid().into();
		let total_validator_stake: Amount = self.total_validator_bid().into();

		// The validators' cut is based on their proportion of the current epoch's bond.
		let scaled_bond = bond * Bid::from(self.validators.len() as u32).into();
		let validators_cut = if total_validator_stake < scaled_bond {
			Perquintill::from_rational(total_validator_stake, scaled_bond)
		} else {
			Perquintill::one()
		} * total;

		let operator_share = Perquintill::from_rational(self.delegation_fee_bps as u64, 10_000u64);
		let delegators_cut = (Perquintill::one() - operator_share) * (total - validators_cut);

		let validator_cuts = self.validators.iter().map(move |(validator, individual_stake)| {
			let share =
				Perquintill::from_rational((*individual_stake).into(), total_validator_stake);
			(validator.into_ref(), share * validators_cut)
		});
		let delegator_cuts = self.delegators.iter().map(move |(delegator, individual_stake)| {
			// Note we need to use the *uncapped* total delegator stake here to determine shares.
			let share =
				Perquintill::from_rational((*individual_stake).into(), total_delegator_stake);
			(delegator, share * delegators_cut)
		});

		// Ensures that all cuts sum to the total amount.
		let operator_cut = total
			.saturating_sub(validator_cuts.clone().map(|(_, stake)| stake).sum::<Amount>())
			.saturating_sub(delegator_cuts.clone().map(|(_, stake)| stake).sum::<Amount>());

		core::iter::once((&self.operator, operator_cut))
			.chain(validator_cuts)
			.chain(delegator_cuts)
	}

	pub fn avg_bid(&self) -> Bid {
		self.total_available_bid() / Bid::from(self.validators.len() as u32)
	}

	fn move_lowest_validator_to_delegator(&mut self) {
		if let Some((validator, amount)) =
			self.validators.clone().into_iter().min_by_key(|(_, v)| *v)
		{
			self.validators.remove(&validator);
			self.delegators.insert(validator.into_ref().clone(), amount);
		}
	}

	pub fn maybe_optimize_bid(&mut self, auction_outcome: &AuctionOutcome<Account, Bid>) {
		while self.validators.len() > 1 && self.avg_bid() <= auction_outcome.bond {
			// in the case where the operator's nodes are at the boundary, maybe some of the
			// validators didnt make the set and so we can optimize further where we reduce one node
			// and increase the avg bid which would allow us to potentially add more of the
			// operator's nodes to the set thereby increasing the number of nodes in the set.
			if self.avg_bid() == auction_outcome.bond {
				if self.validators.iter().any(|(val, _)| !auction_outcome.winners.contains(val)) {
					self.move_lowest_validator_to_delegator();
				} else {
					break;
				}
			}
			// in case where all of operator's nodes are below bond, we increase the avg bid
			// sequentially until either the avg bid is equal to bond or greater.
			else {
				self.move_lowest_validator_to_delegator();
			}
		}
	}
}

impl<Account: Ord + Clone, Bid> DelegationSnapshot<Account, Bid> {
	pub fn map_bids<B>(self, f: impl Fn(Bid) -> B) -> DelegationSnapshot<Account, B> {
		DelegationSnapshot {
			operator: self.operator,
			validators: self.validators.into_iter().map(|(acct, v)| (acct, f(v))).collect(),
			delegators: self.delegators.into_iter().map(|(acct, v)| (acct, f(v))).collect(),
			delegation_fee_bps: self.delegation_fee_bps,
		}
	}

	pub fn try_map_bids<B, E>(
		self,
		f: impl Fn(Bid) -> Result<B, E>,
	) -> Result<DelegationSnapshot<Account, B>, E> {
		Ok(DelegationSnapshot {
			operator: self.operator,
			validators: self
				.validators
				.into_iter()
				.map(|(acct, v)| Ok((acct, f(v)?)))
				.try_collect()?,
			delegators: self
				.delegators
				.into_iter()
				.map(|(acct, v)| Ok((acct, f(v)?)))
				.try_collect()?,
			delegation_fee_bps: self.delegation_fee_bps,
		})
	}
}

pub struct DelegatedRewardsDistribution<T>(PhantomData<T>);

impl<T> RewardsDistribution for DelegatedRewardsDistribution<T>
where
	T: Config,
{
	type Balance = T::Amount;
	type AccountId = T::AccountId;

	fn distribute(
		epoch_index: EpochIndex,
		reward_amount: Self::Balance,
		beneficiary: &Self::AccountId,
		settle: impl FnMut(&T::AccountId, T::Amount),
	) {
		distribute::<T>(epoch_index, beneficiary, reward_amount, settle);
	}

	fn distribute_all(
		epoch_index: EpochIndex,
		total_amount: Self::Balance,
		mut settle: impl FnMut(&T::AccountId, T::Amount),
	) {
		let mut authorities_to_reward: BTreeSet<T::AccountId> =
			HistoricalAuthorities::<T>::get(epoch_index)
				.into_iter()
				.map(Into::into)
				.collect();
		if authorities_to_reward.is_empty() {
			return;
		}
		let per_authority_amount = total_amount / (authorities_to_reward.len() as u32).into();
		let bond = HistoricalBonds::<T>::get(epoch_index);

		// Snapshots are registered for *all* operators at auction resolution, including those
		// whose pooled stake didn't clear the bond: their last remaining validator is never
		// demoted to delegator, so `snapshot.validators` can contain a non-authority. Only
		// authority validators earn a share of the rewards. `remove` doubles as the membership
		// check, draining `authorities` so that only independent (operator-less) authorities
		// remain for the loop below.
		for (operator, snapshot) in DelegationSnapshots::<T>::iter_prefix(epoch_index) {
			let authority_count =
				snapshot.validators.keys().filter(|v| authorities_to_reward.remove(*v)).count()
					as u32;
			if authority_count == 0 {
				continue;
			}
			if (authority_count as usize) < snapshot.validators.len() {
				// The auction fixed point guarantees a snapshot's validators are all-in or
				// all-out of the authority set; a mixed snapshot means that invariant broke.
				cf_runtime_utilities::log_or_panic!(
					"Delegation snapshot of operator {:?} for epoch {} contains non-authority validators.",
					operator,
					epoch_index
				);
			}
			let total = per_authority_amount.saturating_mul(authority_count.into());
			snapshot
				.distribute(total, bond)
				.for_each(|(account, amount)| settle(account, amount));
		}

		for authority in &authorities_to_reward {
			settle(authority, per_authority_amount);
		}
	}
}

pub struct DelegationSlasher<T, S>(PhantomData<(T, S)>);

impl<T, FlipSlasher> Slashing for DelegationSlasher<T, FlipSlasher>
where
	T: Config,
	FlipSlasher:
		Slashing<Balance = T::Amount, AccountId = T::AccountId, BlockNumber = BlockNumberFor<T>>,
{
	type AccountId = FlipSlasher::AccountId;
	type BlockNumber = FlipSlasher::BlockNumber;
	type Balance = FlipSlasher::Balance;

	fn slash_balance(account_id: &Self::AccountId, slash_amount: Self::Balance) {
		distribute::<T>(
			Pallet::<T>::epoch_index(),
			account_id,
			slash_amount,
			FlipSlasher::slash_balance,
		);
	}

	fn calculate_slash_amount(
		account_id: &Self::AccountId,
		blocks_offline: Self::BlockNumber,
	) -> Self::Balance {
		FlipSlasher::calculate_slash_amount(account_id, blocks_offline)
	}
}

/// Distribute a settlement to a given validator for `epoch_index`.
/// The total amount is shared among all delegators and validators associated with the operator
/// controlling this validator for that epoch.
pub fn distribute<T: Config>(
	epoch_index: EpochIndex,
	validator: &T::AccountId,
	total: T::Amount,
	mut settle: impl FnMut(&T::AccountId, T::Amount),
) {
	use frame_support::sp_runtime::traits::Zero;
	if total.is_zero() {
		return;
	}

	if let Some(operator) = ValidatorToOperator::<T>::get(epoch_index, validator) {
		if let Some(snapshot) = DelegationSnapshots::<T>::get(epoch_index, &operator) {
			snapshot
				.distribute(total, HistoricalBonds::<T>::get(epoch_index))
				.for_each(|(account, amount)| settle(account, amount));
		} else {
			settle(validator, total);
			cf_runtime_utilities::log_or_panic!(
				"Validator {:?} is mapped to operator {:?} for epoch {}, but no delegation snapshot found. Settling directly with validator.",
				validator,
				operator,
				epoch_index
			);
		}
	} else {
		settle(validator, total);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{mock::*, DelegationPlanOf};
	use cf_primitives::FLIPPERINOS_PER_FLIP;
	use proptest::{prelude::*, proptest};

	proptest! {
		#[test]
		fn distribute_always_sums_to_total(
			validator_amounts in prop::collection::vec(1u128..1_000_000u128, 1..10),
			delegator_amounts in prop::collection::vec(1u128..1_000_000u128, 1..100),
			total_to_distribute in 1u128..10_000u128,
			delegation_fee_bps in 2_000u32..10_000u32,
			bond in 100_000u128..10_000_000u128,
		) {
			// Create a delegation snapshot
			let operator_account = 1u64;
			let mut snapshot = DelegationSnapshot::<ValidatorId, u128> {
				operator: operator_account,
				validators: BTreeMap::new(),
				delegators: BTreeMap::new(),
				delegation_fee_bps,
			};
			let total_to_distribute = total_to_distribute * FLIPPERINOS_PER_FLIP;

			// Add validators
			for (i, amount) in validator_amounts.iter().enumerate() {
				snapshot.validators.insert(i as u64 + 100, *amount * FLIPPERINOS_PER_FLIP);
			}

			// Add delegators
			for (i, amount) in delegator_amounts.iter().enumerate() {
				snapshot.delegators.insert(i as u64 + 1000, *amount * FLIPPERINOS_PER_FLIP);
			}

			// Distribute the total amount
			new_test_ext().execute_with(|| {
				let distributions: Vec<_> = snapshot.distribute(total_to_distribute, bond * FLIPPERINOS_PER_FLIP).collect();
				let sum: u128 = distributions.iter().map(|(_, amount)| *amount).sum();

				// Property: The sum of all distributed amounts equals the input total
				prop_assert_eq!(
					sum, total_to_distribute,
					"Sum of distributions ({}) does not equal total ({})",
					sum, total_to_distribute
				);

				Ok(())
			});
		}
	}

	proptest! {
		#[test]
		fn distribute_all_sums_to_total(
			validator_amounts in prop::collection::vec(1u128..1_000_000u128, 1..10),
			delegator_amounts in prop::collection::vec(1u128..1_000_000u128, 0..50),
			per_beneficiary_amount in 1u128..10_000u128,
			delegation_fee_bps in 2_000u32..10_000u32,
			bond in 100_000u128..10_000_000u128,
		) {
			const EPOCH: EpochIndex = 1;
			let operator_account = 1u64;
			let per_beneficiary_amount = per_beneficiary_amount * FLIPPERINOS_PER_FLIP;
			let bond = bond * FLIPPERINOS_PER_FLIP;

			let validators: BTreeMap<ValidatorId, u128> = validator_amounts.iter().enumerate()
				.map(|(i, amount)| (i as u64 + 100, *amount * FLIPPERINOS_PER_FLIP))
				.collect();
			let delegators: BTreeMap<ValidatorId, u128> = delegator_amounts.iter().enumerate()
				.map(|(i, amount)| (i as u64 + 1000, *amount * FLIPPERINOS_PER_FLIP))
				.collect();
			// This proptest covers the all-in case: the authority set is exactly this operator's
			// snapshot validators, so distribute_all routes the whole total through its pool.
			// (All-out snapshots from losing operators, which distribute_all skips, are covered
			// by `distribute_all_matches_looped_distribute`.)
			let beneficiaries: Vec<ValidatorId> = validators.keys().cloned().collect();
			// Constructed as an exact multiple of beneficiaries.len() so the internal division
			// in `distribute_all` recovers `per_beneficiary_amount` exactly (no remainder).
			let total_amount = per_beneficiary_amount * beneficiaries.len() as u128;

			new_test_ext().execute_with(|| {
				crate::HistoricalBonds::<Test>::insert(EPOCH, bond);
				crate::HistoricalAuthorities::<Test>::insert(EPOCH, &beneficiaries);

				DelegationSnapshot::<ValidatorId, u128> {
					operator: operator_account,
					validators,
					delegators,
					delegation_fee_bps,
				}.register_for_epoch::<Test>(EPOCH);

				let mut settled: BTreeMap<ValidatorId, u128> = BTreeMap::new();
				DelegatedRewardsDistribution::<Test>::distribute_all(
					EPOCH,
					total_amount,
					|account, amount| {
						settled.entry(*account).and_modify(|a| *a += amount).or_insert(amount);
					},
				);

				// Property: the sum of all settled amounts equals total_amount, exactly like
				// beneficiaries.len() separate `distribute` calls would sum to.
				let sum: u128 = settled.values().sum();
				prop_assert_eq!(
					sum, total_amount,
					"Sum of settled amounts ({}) does not equal expected total ({})",
					sum, total_amount
				);

				Ok(())
			});
		}
	}

	proptest! {
		#[test]
		fn validator_bond_amounts_capped(
			validator_amounts in prop::collection::vec(1u128..1_000_000u128, 1..10),
			bond in 1u128..1_000_000u128,
		) {
			// Create a delegation snapshot
			let snapshot = DelegationSnapshot::<ValidatorId, u128> {
				validators: validator_amounts.iter().enumerate()
					.map(|(i, bid)| (i as u64 + 100, *bid))
					.collect(),
				..Default::default()
			};

			// Distribute the total amount
			new_test_ext().execute_with(|| {
				let dist = snapshot.validator_bond_distribution(bond);

				// Total bond cannot exceed
				let max_expected_bond = bond * snapshot.validators.len() as u128;
				let total_bonded = dist.values().sum::<u128>();
				let total_available_stake = snapshot.validators.values().sum();

				prop_assert!(
					total_bonded == core::cmp::min(total_available_stake, max_expected_bond)
				);

				// No validator is bonded above its own bid, regardless of how high its
				// pool-mates bid. Since the bid is already capped by the validator's max bid,
				// this is what keeps pooling from bonding anyone above their max bid.
				for (validator, individual_bond) in &dist {
					prop_assert!(
						individual_bond <= snapshot.validators.get(validator).unwrap(),
						"validator {} bonded {} above its bid {}",
						validator, individual_bond, snapshot.validators[validator]
					);
				}

				Ok(())
			});
		}
	}

	proptest! {
		#[test]
		fn is_valid_matches_reference_computation(
			entries in prop::collection::vec((1u64..1000, 0u128..1_000_000_000_000u128), 0..20),
			minimum_total in 0u128..1_000_000_000_000u128,
		) {
			let map: BTreeMap<u64, u128> = entries.into_iter().collect();
			let plan = DelegationPlanOf::<Test>::try_from_map(map.clone()).unwrap();

			let has_zero_entry = map.values().any(|amount| *amount == 0);
			// Safe: bounded generator, well within u128's range regardless of how many entries.
			let total: u128 = map.values().sum();
			let expected = !has_zero_entry && (map.is_empty() || total >= minimum_total);

			prop_assert_eq!(
				plan.is_valid(minimum_total), expected,
				"is_valid({}) disagreed with the reference computation for {:?}", minimum_total, map
			);
		}
	}

	proptest! {
		#[test]
		fn try_insert_either_commits_a_valid_plan_or_leaves_it_unchanged(
			existing in prop::collection::vec((1u64..1000, 1u128..1_000_000_000_000u128), 0..20),
			operator in 1u64..1000,
			value in 1u128..1_000_000_000_000u128,
		) {
			let plan_before =
				DelegationPlanOf::<Test>::try_from_map(existing.into_iter().collect()).unwrap();
			let mut plan = plan_before.clone();

			match plan.try_insert(operator, value) {
				Ok(_) => {
					prop_assert!(
						plan.is_valid(0),
						"try_insert committed a plan that fails its own validity check"
					);
					prop_assert_eq!(
						plan.get(&operator), Some(&value),
						"the committed entry doesn't match what was inserted"
					);
				},
				Err(()) => {
					prop_assert_eq!(
						plan, plan_before,
						"try_insert mutated the plan despite returning an error"
					);
				},
			}
		}
	}

	proptest! {
		#[test]
		fn resolve_bids_sum_never_exceeds_balance(
			entries in prop::collection::vec((1u64..1000, 1u128..1_000_000_000_000u128), 1..20),
			balance in 0u128..2_000_000_000_000u128,
		) {
			// Proptest, I1: for arbitrary plans and balances, the sum of effective bids never
			// exceeds the balance.
			let map: BTreeMap<u64, u128> = entries.into_iter().collect();
			let plan = DelegationPlanOf::<Test>::try_from_map(map).unwrap();

			let resolved = plan.resolve_bids(balance, |_| true);
			let resolved_total: u128 = resolved.values().sum();

			prop_assert!(
				resolved_total <= balance,
				"resolved total {} exceeds balance {}", resolved_total, balance
			);
		}
	}

	proptest! {
		#[test]
		fn resolve_bids_scale_down_respects_individual_caps(
			entries in prop::collection::vec((1u64..1000, 1u128..1_000_000_000_000u128), 1..20),
			balance in 0u128..2_000_000_000_000u128,
		) {
			// Proptest, degradation: pro-rata scale-down never exceeds any individual cap and
			// never sums above the balance (mirrors `validator_bond_amounts_capped`).
			let map: BTreeMap<u64, u128> = entries.into_iter().collect();
			let total_committed: u128 = map.values().sum();
			let plan = DelegationPlanOf::<Test>::try_from_map(map.clone()).unwrap();

			let resolved = plan.resolve_bids(balance, |_| true);

			for (operator, resolved_bid) in &resolved {
				prop_assert!(
					*resolved_bid <= map[operator],
					"operator {} resolved to {} above its own bid {}",
					operator, resolved_bid, map[operator]
				);
			}

			let resolved_total: u128 = resolved.values().sum();
			if total_committed <= balance {
				prop_assert_eq!(resolved_total, total_committed);
			} else {
				prop_assert!(resolved_total <= balance);
			}
		}
	}

	proptest! {
		#[test]
		fn resolve_bids_slashing_never_exceeds_the_delegators_own_pool_allocation(
			entries in prop::collection::vec((1u64..1000, 1u128..1_000_000_000_000u128), 2..20),
			// A fraction (1-99%) of the committed total, so `balance` always ends up strictly
			// less than `total_committed` -- simulating a balance reduction from slashing that
			// leaves the delegator's plan over-subscribed.
			shortfall_percent in 1u32..100,
		) {
			// Proptest, slashing bound: a delegator can never be attributed, at any one
			// operator, more than what they originally allocated to it -- regardless of how
			// much of their balance a slash consumed.
			let map: BTreeMap<u64, u128> = entries.into_iter().collect();
			let total_committed: u128 = map.values().sum();
			prop_assume!(total_committed > 0);
			let balance = total_committed * (100 - shortfall_percent) as u128 / 100;

			let plan = DelegationPlanOf::<Test>::try_from_map(map.clone()).unwrap();
			let resolved = plan.resolve_bids(balance, |_| true);

			for (operator, original_bid) in &map {
				let resolved_bid = resolved.get(operator).copied().unwrap_or(0);
				prop_assert!(
					resolved_bid <= *original_bid,
					"operator {} was attributed {} but the delegator only ever pledged {} to it",
					operator, resolved_bid, original_bid
				);
			}

			let resolved_total: u128 = resolved.values().sum();
			prop_assert!(resolved_total <= balance);
		}
	}

	#[test]
	fn test_validator_bond_distribution() {
		let snapshot = DelegationSnapshot::<u64, u128> {
			operator: 1,
			validators: [(100, 800), (101, 300), (102, 200)].into_iter().collect(),
			..Default::default()
		};

		let bond = 400;
		let distribution = snapshot.validator_bond_distribution(bond);

		assert_eq!(distribution[&100], 700);
		assert_eq!(distribution[&101], 300);
		assert_eq!(distribution[&102], 200);
	}
}
