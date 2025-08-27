use crate::{
	Config, DelegationCapacityFactor, DelegationSnapshots, OperatorSettingsLookup, ValidatorIdOf,
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
		core::cmp::min(
			self.total_delegator_bid(),
			self.total_validator_bid() * DelegationCapacityFactor::<T>::get().into(),
		)
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
			total_validator_stake +
				core::cmp::min(
					total_delegator_stake,
					total_validator_stake *
						Amount::from(DelegationCapacityFactor::<T>::get() as u64),
				),
		) * total;
		let delegators_cut = total - validators_cut;
		let operator_cut =
			Perquintill::from_rational(self.delegation_fee_bps as u64, 10_000u64) * delegators_cut;
		let delegators_cut = delegators_cut - operator_cut;

		debug_assert_eq!(validators_cut + operator_cut + delegators_cut, total);

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
	if let Some(snapshot) = DelegationResolver::<T>::resolve_for_account(validator) {
		snapshot.distribute(total).for_each(|(account, amount)| settle(account, amount));
	} else {
		settle(validator, total);
	}
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, RuntimeDebugNoBound)]
#[scale_info(skip_type_params(T))]
pub enum DelegationResolver<T: Config> {
	Own(DelegationSnapshot<T>),
	Ref(T::AccountId),
}

impl<T: Config> From<DelegationSnapshot<T>> for DelegationResolver<T> {
	fn from(snapshot: DelegationSnapshot<T>) -> Self {
		Self::Own(snapshot)
	}
}

impl<T: Config> DelegationResolver<T> {
	pub fn register_snapshot(epoch_index: EpochIndex, snapshot: DelegationSnapshot<T>) {
		for validator in snapshot.validators.keys() {
			DelegationSnapshots::<T>::insert(
				epoch_index,
				validator.into_ref(),
				Self::Ref(snapshot.operator.clone()),
			);
		}
		DelegationSnapshots::<T>::insert(
			epoch_index,
			snapshot.operator.clone(),
			Self::Own(snapshot),
		);
	}

	#[cfg(test)]
	pub fn unwrap(self) -> DelegationSnapshot<T> {
		match self {
			DelegationResolver::Own(snapshot) => snapshot,
			DelegationResolver::Ref(_) => panic!("Tried to unwrap a DelegationResolver::Ref"),
		}
	}

	pub fn resolve_for_account(account: &T::AccountId) -> Option<DelegationSnapshot<T>> {
		DelegationSnapshots::<T>::get(T::EpochInfo::epoch_index(), account)
			.and_then(Self::inner_resolve)
	}

	pub fn map<F, R>(&self, f: F) -> Option<R>
	where
		F: Fn(&DelegationSnapshot<T>) -> R,
	{
		match self {
			DelegationResolver::Own(snapshot) => Some(f(snapshot)),
			DelegationResolver::Ref(_) => None,
		}
	}

	fn inner_resolve(self) -> Option<DelegationSnapshot<T>> {
		match self {
			DelegationResolver::Own(snapshot) => Some(snapshot),
			DelegationResolver::Ref(operator) => Self::resolve_for_account(&operator),
		}
	}
}
