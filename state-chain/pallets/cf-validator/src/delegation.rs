use crate::{AuctionOutcome, Config, DelegationSnapshots, Pallet, ValidatorToOperator};
use cf_primitives::EpochIndex;
use cf_traits::{EpochInfo, Issuance, RewardsDistribution, Slashing};
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use core::iter::Sum;
use frame_support::{
	sp_runtime::{traits::AtLeast32BitUnsigned, Perquintill},
	traits::IsType,
};
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData, prelude::*};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
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
	Clone, PartialEq, Eq, Default, Encode, Decode, TypeInfo, Debug, Serialize, Deserialize,
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

pub struct DelegatedRewardsDistribution<T, I>(PhantomData<(T, I)>);

impl<T, I> RewardsDistribution for DelegatedRewardsDistribution<T, I>
where
	T: Config,
	I: Issuance<AccountId = T::AccountId, Balance = T::Amount>,
{
	type Balance = I::Balance;
	type AccountId = I::AccountId;

	fn distribute(reward_amount: Self::Balance, beneficiary: &Self::AccountId) {
		distribute::<T>(beneficiary, reward_amount, I::mint);
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
		distribute::<T>(account_id, slash_amount, FlipSlasher::slash_balance);
	}

	fn calculate_slash_amount(
		account_id: &Self::AccountId,
		blocks_offline: Self::BlockNumber,
	) -> Self::Balance {
		FlipSlasher::calculate_slash_amount(account_id, blocks_offline)
	}
}

/// Distribute a settlement to a given validator for the current Epoch.
/// The total amount is shared among all delegators and validators associated with the operator
/// controlling this validator.
pub fn distribute<T: Config>(
	validator: &T::AccountId,
	total: T::Amount,
	settle: impl Fn(&T::AccountId, T::Amount),
) {
	let epoch_index = Pallet::<T>::epoch_index();

	if let Some(operator) = ValidatorToOperator::<T>::get(epoch_index, validator) {
		if let Some(snapshot) = DelegationSnapshots::<T>::get(epoch_index, &operator) {
			snapshot
				.distribute(total, Pallet::<T>::bond())
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
				assert_eq!(
					sum, total_to_distribute,
					"Sum of distributions ({}) does not equal total ({})",
					sum, total_to_distribute
				);
			});
		}
	}
}
