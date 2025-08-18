use crate::Config;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{sp_runtime::traits::Zero, RuntimeDebugNoBound};
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
	/// Map of delegator accounts to their bid amounts.
	pub delegators: BTreeMap<T::AccountId, T::Amount>,
	/// Map of validator accounts to their bid amounts.
	pub validators: BTreeMap<T::AccountId, T::Amount>,
	/// Operator fee at time of snapshot creation.
	pub delegation_fee_bps: u32,
}

impl<T: Config> DelegationSnapshot<T> {
	pub fn average_validator_bid(&self) -> T::Amount {
		if self.validators.is_empty() {
			return T::Amount::zero();
		}
		let total: T::Amount = self.validators.values().copied().sum();
		total / T::Amount::from(self.validators.len() as u32)
	}
}
