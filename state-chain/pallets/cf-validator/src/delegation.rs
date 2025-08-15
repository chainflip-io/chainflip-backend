use crate::{Config, Pallet};

use cf_primitives::FlipBalance;
use cf_traits::Slashing;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_flip::FlipSlasher;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::{traits::AtLeast32BitUnsigned, DispatchError, Perbill};
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData};

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
pub enum DelegationStatus {
	#[default]
	Delegating,
	UnDelegating,
}

pub struct DelegatorFlipSlasher<T>(PhantomData<T>);
impl<T: Config + pallet_cf_flip::Config> Slashing for DelegatorFlipSlasher<T>
where
	T::Balance: Into<FlipBalance>,
{
	type AccountId = T::AccountId;
	type BlockNumber = BlockNumberFor<T>;
	type Balance = T::Balance;

	fn slash(account_id: &Self::AccountId, blocks: Self::BlockNumber) {
		let slash_amount = pallet_cf_flip::Pallet::<T>::calculate_slash_amount(account_id, blocks);
		Self::slash_balance(account_id, slash_amount);
	}

	fn slash_balance(account_id: &Self::AccountId, slash_amount: Self::Balance) {
		distribute_among_delegators::<T>(account_id, slash_amount, |account, slash| {
			FlipSlasher::<T>::slash_balance(account, slash);
		});
	}
}

/// Distribute a settlement to a given validator for the current Epoch.
/// The total amount is shared among all delegators associated with the operator controlling this
/// validator.
pub fn distribute_among_delegators<T: Config + pallet_cf_flip::Config>(
	validator: &T::AccountId,
	total: T::Balance,
	settle: impl Fn(&T::AccountId, T::Balance),
) {
	if let Some((operator_cut, delegator_cuts)) = crate::ManagedValidators::<T>::get(validator)
		.and_then(|operator| {
			crate::OperatorSettingsLookup::<T>::get(&operator).map(|settings| (operator, settings))
		})
		.and_then(|(operator, setting)| {
			let delegators = Pallet::<T>::get_bonded_delegators_for_operator(&operator);
			let total_validator_balance =
				Pallet::<T>::get_total_validator_balance_for_operator(&operator);
			split_amount(total, delegators, setting.fee_bps, total_validator_balance).ok()
		}) {
		// we credit the operator cut to the validator relevant of current reward/slash instead of
		// the operator.
		settle(validator, operator_cut);
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
/// The rest is split proportionally to the amount staked by each delegator.
pub fn split_amount<
	AccountId: Clone + Ord,
	Balance: Default + Copy + Clone + AtLeast32BitUnsigned,
>(
	total: Balance,
	delegator_bids: BTreeSet<(AccountId, Balance)>,
	fee_bps: u32,
	total_validator_balance: Balance,
) -> Result<(Balance, BTreeSet<(AccountId, Balance)>), DispatchError> {
	if delegator_bids.is_empty() {
		return Err("Empty delegator set".into())
	}

	let total_staked = delegator_bids
		.iter()
		.fold(Default::default(), |total_staked, (_, amount)| total_staked + *amount);

	let validators_cut =
		Perbill::from_rational(total_validator_balance, total_validator_balance + total_staked) *
			total;

	let delegation_fee = Perbill::from_rational(fee_bps, 10_000) * (total - validators_cut);
	let remaining = total - validators_cut - delegation_fee;

	let delegator_cuts = delegator_bids
		.iter()
		.map(|(delegator, staked)| {
			(delegator.clone(), Perbill::from_rational(*staked, total_staked) * remaining)
		})
		.collect::<BTreeSet<_>>();

	Ok((delegation_fee + validators_cut, delegator_cuts))
}
