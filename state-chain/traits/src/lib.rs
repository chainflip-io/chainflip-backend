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

/// Something that can provide information on the current validator bond amount.
pub trait BondProvider {
	/// The denomination of the bonded token.
	type Amount;

	/// Returns the bond amount for the current Epoch.
	fn current_bond() -> Self::Amount;
}

pub trait ValidatorProvider {
	/// The id type used for the validators. 
	type ValidatorId;

	/// Returns a list of validators for the current Epoch. 
	fn current_validators() -> Vec<Self::ValidatorId>;

	/// Checks if the account is currently a validator.
	fn is_validator(account: &Self::ValidatorId) -> bool;
}