use crate::{Config, Pallet};

use cf_traits::{Issuance, RewardsDistribution, Slashing};
use codec::{Decode, Encode, MaxEncodedLen};
use core::iter::Sum;
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::{traits::AtLeast32BitUnsigned, DispatchError, Perquintill};
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

/// The minimum delegation fee that can be charged, in basis points.
pub const MIN_OPERATOR_FEE: u32 = 200;

pub enum AssociationToOperator {
	Validator,
	Delegator,
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
	if let Some((operator_cut, validator_cut, delegator_cuts, operator)) =
		crate::ManagedValidators::<T>::get(validator)
			.and_then(|operator| {
				crate::OperatorSettingsLookup::<T>::get(&operator)
					.map(|settings| (operator, settings))
			})
			.and_then(|(operator, setting)| {
				let delegators = Pallet::<T>::get_bonded_delegators_for_operator(&operator);
				let total_validator_balance =
					Pallet::<T>::get_total_validator_balance_for_operator(&operator);
				split_amount(total, delegators, setting.fee_bps, total_validator_balance)
					.map(|(o, v, d)| (o, v, d, operator))
					.ok()
			}) {
		settle(validator, validator_cut);
		settle(&operator, operator_cut);
		for (delegator, fees) in delegator_cuts.iter() {
			settle(delegator, *fees);
		}
	} else {
		settle(validator, total);
	}
}

/// Splits the total amount for the given operator. Can be used to distribute reward or
/// calculate slashing.
/// A proportion is given to the operator.
/// The rest is split proportionally to the amount staked by each delegator minus the fees (which
/// are given to the operator).
#[allow(clippy::type_complexity)]
pub fn split_amount<
	AccountId: Clone + Ord,
	Balance: Default + Copy + Clone + AtLeast32BitUnsigned + From<u64> + Sum,
>(
	total: Balance,
	delegator_bids: BTreeMap<AccountId, Balance>,
	fee_bps: u32,
	total_validator_balance: Balance,
) -> Result<(Balance, Balance, BTreeMap<AccountId, Balance>), DispatchError> {
	if delegator_bids.is_empty() {
		return Err("Empty delegator set".into())
	}

	let total_staked = delegator_bids.values().copied().sum();

	let validators_cut_proportion =
		Perquintill::from_rational(total_validator_balance, total_validator_balance + total_staked);

	let delegation_fee_proportion = Perquintill::from_rational(fee_bps.into(), 10_000u64) *
		(Perquintill::one() - validators_cut_proportion);
	let remaining_proportion =
		Perquintill::one() - validators_cut_proportion - delegation_fee_proportion;

	let total_delegator_cut = remaining_proportion * total;
	let delegator_cuts = delegator_bids
		.into_iter()
		.map(|(delegator, staked)| {
			(delegator, Perquintill::from_rational(staked, total_staked) * total_delegator_cut)
		})
		.collect();

	Ok((delegation_fee_proportion * total, validators_cut_proportion * total, delegator_cuts))
}
