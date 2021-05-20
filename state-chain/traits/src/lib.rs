#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{DispatchResultWithPostInfo, Dispatchable};
use sp_std::prelude::*;

/// A trait abstracting the functionality of the witnesser
pub trait Witnesser {
	/// The type of accounts that can witness.
	type AccountId;

	/// The call type of the runtime. 
	type Call: Dispatchable;

	/// Witness an event. The event is represented by a call, which should be
	/// dispatched when a threshold number of witnesses have been made.
	fn witness(who: Self::AccountId, call: Self::Call) -> DispatchResultWithPostInfo;
}

pub trait EpochInfo {
	/// The id type used for the validators.
	type ValidatorId;
	/// An amount
	type Amount;
	/// The index of an epoch
	type EpochIndex;

	/// The current set of validators
	fn current_validators() -> Vec<Self::ValidatorId>;

	/// Checks if the account is currently a validator.
	fn is_validator(account: &Self::ValidatorId) -> bool;

	/// If we are in auction phase then the proposed set to validate once the auction is
	/// confirmed else an empty vector
	fn next_validators() -> Vec<Self::ValidatorId>;

	/// The amount to be used as bond, this is the minimum stake needed to get into the
	/// candidate validator set
	fn bond() -> Self::Amount;

	/// The current epoch we are in
	fn epoch_index() -> Self::EpochIndex;
}

#[derive(Debug, PartialEq)]
pub enum PermissionError {
	/// Failure in setting scope for an account
	FailedToSetScope,
	/// Failure to find the account
	AccountNotFound,
}

/// Scope or permissions for accounts
pub trait Permissions {
	/// The id used for an account
	type AccountId;
	/// A level or scope of permission
	type Scope;
	/// Our verifier
	type Verifier: PermissionVerifier;

	/// The scope for the account
	fn scope(account: Self::AccountId) -> Result<Self::Scope, PermissionError>;
	/// At the scope for the account
	fn set_scope(account: Self::AccountId, scope: Self::Scope) -> Result<(), PermissionError>;
	/// Revoke all permissions from account
	fn revoke(account: Self::AccountId) -> Result<(), PermissionError>;
}

/// Handler to verify change of scopes
pub trait PermissionVerifier {
	/// The id used for an account
	type AccountId;
	/// A level or scope of permission
	type Scope;
	/// Verify that we are happy this account has this scope
	fn verify_scope(account: &Self::AccountId, scope: &Self::Scope) -> bool;
}