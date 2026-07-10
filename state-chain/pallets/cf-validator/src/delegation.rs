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
	AuctionOutcome, Config, DelegationSnapshots, HistoricalBonds, Pallet, ValidatorToOperator,
};
use cf_primitives::EpochIndex;
use cf_traits::{EpochInfo, RewardsDistribution, Slashing};
use codec::{Decode, DecodeWithMemTracking, Encode, FullCodec, MaxEncodedLen};
use core::iter::Sum;
use frame_support::{
	sp_runtime::{traits::AtLeast32BitUnsigned, Perquintill, Saturating},
	traits::IsType,
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

	pub fn total_validator_bid(&self) -> Bid {
		self.validators.values().copied().sum()
	}

	pub fn total_delegator_bid(&self) -> Bid {
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

	/// `beneficiaries` must be exactly `epoch_index`'s full authority set. For a partial
	/// set, call `distribute` directly per beneficiary instead.
	fn distribute_all(
		epoch_index: EpochIndex,
		total_amount: Self::Balance,
		beneficiaries: &[Self::AccountId],
		mut settle: impl FnMut(&T::AccountId, T::Amount),
	) {
		if beneficiaries.is_empty() {
			return;
		}
		let per_beneficiary_amount = total_amount / (beneficiaries.len() as u32).into();

		// `snapshot.validators` is exactly this operator's set of `epoch_index` authorities
		// (it's the source `ValidatorToOperator` is populated from at registration), so the
		// snapshot alone tells us both which operators have authorities this epoch and how
		// many - no per-authority `ValidatorToOperator` lookup needed.
		let mut rewarded: BTreeSet<T::AccountId> = BTreeSet::new();
		for (_operator, snapshot) in DelegationSnapshots::<T>::iter_prefix(epoch_index) {
			let count = snapshot.validators.len() as u32;
			if count == 0 {
				continue;
			}
			rewarded.extend(snapshot.validators.keys().cloned());
			let total = per_beneficiary_amount.saturating_mul(count.into());
			snapshot
				.distribute(total, HistoricalBonds::<T>::get(epoch_index))
				.for_each(|(account, amount)| settle(account, amount));
		}

		for beneficiary in beneficiaries {
			if !rewarded.contains(beneficiary) {
				settle(beneficiary, per_beneficiary_amount);
			}
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
	use crate::mock::*;
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
			// Every validator in the snapshot is a beneficiary this epoch - the precondition
			// `distribute_all` relies on to derive authority counts from the snapshot alone.
			let beneficiaries: Vec<ValidatorId> = validators.keys().cloned().collect();
			// Constructed as an exact multiple of beneficiaries.len() so the internal division
			// in `distribute_all` recovers `per_beneficiary_amount` exactly (no remainder).
			let total_amount = per_beneficiary_amount * beneficiaries.len() as u128;

			new_test_ext().execute_with(|| {
				crate::HistoricalBonds::<Test>::insert(EPOCH, bond);

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
					&beneficiaries,
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
					total_bonded == core::cmp::min(
						total_available_stake
						,max_expected_bond

					)
				);

				Ok(())
			});
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
