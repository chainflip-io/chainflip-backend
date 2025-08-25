use crate::{Config, OperatorSettingsLookup, ValidatorIdOf};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::RuntimeDebugNoBound;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::btree_map::BTreeMap;

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

	pub fn total_available_bid(&self) -> T::Amount {
		self.total_validator_bid() + self.total_delegator_bid()
	}

	pub fn effective_validator_bids(&self) -> BTreeMap<ValidatorIdOf<T>, T::Amount> {
		if self.validators.is_empty() {
			return Default::default();
		}
		let avg_bid = self.total_available_bid() / T::Amount::from(self.validators.len() as u32);
		self.validators.keys().map(|validator| (validator.clone(), avg_bid)).collect()
	}
}
