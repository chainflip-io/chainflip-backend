#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::{DispatchResultWithPostInfo, Dispatchable};

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