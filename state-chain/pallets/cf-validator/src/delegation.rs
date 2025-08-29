use crate::{
	Config, DelegationCapacityFactor, DelegationSnapshots, OperatorSettingsLookup, ValidatorIdOf,
	ValidatorToOperator,
};
use cf_primitives::EpochIndex;
use cf_traits::{EpochInfo, Issuance, RewardsDistribution, Slashing};
use codec::{Decode, Encode, MaxEncodedLen};
use core::iter::Sum;
use frame_support::{
	sp_runtime::{traits::AtLeast32BitUnsigned, Perquintill},
	traits::IsType,
	Parameter, RuntimeDebugNoBound,
};
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

/// The minimum delegation fee that can be charged, in basis points.
pub const MIN_OPERATOR_FEE: u32 = 200;

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
#[derive(Clone, PartialEq, Eq, Default, Encode, Decode, TypeInfo, RuntimeDebugNoBound)]
#[scale_info(skip_type_params(T))]
pub struct DelegationSnapshot<T: Config> {
	pub operator: T::AccountId,
	/// Map of validator accounts to their bid amounts.
	pub validators: BTreeMap<ValidatorIdOf<T>, T::Amount>,
	/// Map of delegator accounts to their bid amounts.
	pub delegators: BTreeMap<T::AccountId, T::Amount>,
	/// Operator fee at time of snapshot creation.
	pub delegation_fee_bps: u32,
	/// Capacity factor at time of snapshot creation.
	pub capacity_factor: Option<u32>,
}

impl<T: Config> DelegationSnapshot<T> {
	pub fn init(operator: &T::AccountId) -> Self {
		Self {
			operator: operator.clone(),
			delegators: Default::default(),
			validators: Default::default(),
			delegation_fee_bps: OperatorSettingsLookup::<T>::get(operator)
				.map(|settings| settings.fee_bps)
				.unwrap_or(0),
			capacity_factor: DelegationCapacityFactor::<T>::get(),
		}
	}

	pub fn total_validator_bid(&self) -> T::Amount {
		self.validators.values().copied().sum()
	}

	pub fn total_delegator_bid(&self) -> T::Amount {
		self.delegators.values().copied().sum()
	}

	/// The total delegator bid, capped based on the delegation multiple and the total validator
	/// bid.
	pub fn total_delegator_bid_capped(&self) -> T::Amount {
		let total = self.total_delegator_bid();
		self.capacity_factor
			.map_or(total, |f| core::cmp::min(total, self.total_validator_bid() * f.into()))
	}

	/// Stores the validator mappings and snapshot information for the given epoch.
	pub fn register_for_epoch(self, epoch_index: EpochIndex) {
		let operator = self.operator.clone();
		for validator in self.validators.keys() {
			ValidatorToOperator::<T>::insert(epoch_index, validator.into_ref(), operator.clone());
		}
		DelegationSnapshots::<T>::insert(epoch_index, operator, self);
	}

	pub fn total_available_bid(&self) -> T::Amount {
		self.total_validator_bid() + self.total_delegator_bid_capped()
	}

	pub fn effective_validator_bids(&self) -> BTreeMap<ValidatorIdOf<T>, T::Amount> {
		if self.validators.is_empty() {
			return Default::default();
		}
		let avg_bid = self.total_available_bid() / T::Amount::from(self.validators.len() as u32);
		self.validators.keys().map(|validator| (validator.clone(), avg_bid)).collect()
	}

	pub fn distribute<Amount>(&self, total: Amount) -> impl Iterator<Item = (&T::AccountId, Amount)>
	where
		Amount: From<T::Amount>
			+ AtLeast32BitUnsigned
			+ Copy
			+ Clone
			+ Default
			+ Sum
			+ From<u64>
			+ Parameter,
	{
		let total_delegator_stake: Amount = self.total_delegator_bid().into();
		let total_validator_stake: Amount = self.total_validator_bid().into();

		// The validator's cut is based on the capped delegation amount.
		let validators_cut = Perquintill::from_rational(
			total_validator_stake,
			total_validator_stake + self.total_delegator_bid_capped().into(),
		) * total;
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
	let epoch_index = T::EpochInfo::epoch_index();

	if let Some(operator) = ValidatorToOperator::<T>::get(epoch_index, validator) {
		if let Some(snapshot) = DelegationSnapshots::<T>::get(epoch_index, &operator) {
			snapshot.distribute(total).for_each(|(account, amount)| settle(account, amount));
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
	use proptest::{prelude::*, proptest};

	proptest! {
		#[test]
		fn distribute_always_sums_to_total(
			validator_amounts in prop::collection::vec(1u128..1_000_000u128, 1..10),
			delegator_amounts in prop::collection::vec(1u128..1_000_000u128, 1..100),
			total_to_distribute in 1u128..10_000_000_000_000_000_000u128,
			delegation_fee_bps in 2_000u32..10_000u32,
			capacity_factor in prop::option::of(0u32..100u32),
		) {
			// Create a delegation snapshot
			let operator_account = 1u64;
			let mut snapshot = DelegationSnapshot::<Test> {
				operator: operator_account,
				validators: BTreeMap::new(),
				delegators: BTreeMap::new(),
				delegation_fee_bps,
				capacity_factor,
			};

			// Add validators
			for (i, amount) in validator_amounts.iter().enumerate() {
				snapshot.validators.insert(i as u64 + 100, *amount);
			}

			// Add delegators
			for (i, amount) in delegator_amounts.iter().enumerate() {
				snapshot.delegators.insert(i as u64 + 1000, *amount);
			}

			// Distribute the total amount
			let distributions: Vec<_> = snapshot.distribute(total_to_distribute).collect();
			let sum: u128 = distributions.iter().map(|(_, amount)| *amount).sum();

			// Property: The sum of all distributed amounts equals the input total
			assert_eq!(
				sum, total_to_distribute,
				"Sum of distributions ({}) does not equal total ({})",
				sum, total_to_distribute
			);
		}
	}
}
