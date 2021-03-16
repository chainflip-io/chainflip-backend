// Code mostly taken from here: https://github.com/gautamdhameja/substrate-validator-set
// modifications to it, such as validation (since we're not using sudo to add validators)
// will come later

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{decl_error, decl_event, decl_module, decl_storage, dispatch, StorageValue};
use frame_system::{self as system, ensure_signed};
use sp_runtime::traits::Convert;
use sp_std::prelude::*;

mod mock;
mod tests;

pub trait Config: system::Config + pallet_session::Config {
    type Event: From<Event<Self>> + Into<<Self as system::Config>::Event>;
}

decl_storage! {
    trait Store for Module<T: Config> as Validator {
        pub Validators get(fn validators) config(): Option<Vec<T::AccountId>>;
        Flag get(fn flag): bool;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Config>::AccountId,
    {
        // New validator added.
        ValidatorAdded(AccountId),

        // Validator removed.
        ValidatorRemoved(AccountId),
    }
);

decl_error! {
    /// Errors for the module.
    pub enum Error for Module<T: Config> {
        NoValidators,
    }
}

decl_module! {
    pub struct Module<T: Config> for enum Call where origin: T::Origin {
        fn deposit_event() = default;

        /// New validator's session keys should be set in session module before calling this.
        #[weight = 0]
        pub fn add_validator(origin, validator_id: T::AccountId) -> dispatch::DispatchResult {
            ensure_signed(origin)?;
            let mut validators = Self::validators().ok_or(Error::<T>::NoValidators)?;
            validators.push(validator_id.clone());
            <Validators<T>>::put(validators);
            // Calling rotate_session to queue the new session keys.
            <pallet_session::Module<T>>::rotate_session();
            Self::deposit_event(RawEvent::ValidatorAdded(validator_id));

            // Triggering rotate session again for the queued keys to take effect immediately
            Flag::put(true);
            Ok(())
        }

        #[weight = 0]
        pub fn remove_validator(origin, validator_id: T::AccountId) -> dispatch::DispatchResult {
            ensure_signed(origin)?;
            let mut validators = Self::validators().ok_or(Error::<T>::NoValidators)?;
            // Assuming that this will be a PoA network for enterprise use-cases,
            // the validator count may not be too big; the for loop shouldn't be too heavy.
            // In case the validator count is large, we need to find another way.
            for (i, v) in validators.clone().into_iter().enumerate() {
                if v == validator_id {
                    validators.swap_remove(i);
                }
            }
            <Validators<T>>::put(validators);
            // Calling rotate_session to queue the new session keys.
            <pallet_session::Module<T>>::rotate_session();
            Self::deposit_event(RawEvent::ValidatorRemoved(validator_id));

            // Triggering rotate session again for the queued keys to take effect.
            Flag::put(true);
            Ok(())
        }
    }
}

/// Indicates to the session module if the session should be rotated.
/// We set this flag to true when we add/remove a validator.
impl<T: Config> pallet_session::ShouldEndSession<T::BlockNumber> for Module<T> {
    fn should_end_session(_now: T::BlockNumber) -> bool {
        Self::flag()
    }
}

/// Provides the new set of validators to the session module when session is being rotated.
impl<T: Config> pallet_session::SessionManager<T::AccountId> for Module<T> {
    fn new_session(_new_index: u32) -> Option<Vec<T::AccountId>> {
        // Flag is set to false so that the session doesn't keep rotating.
        Flag::put(false);

        Self::validators()
    }

    fn end_session(_end_index: u32) {}

    fn start_session(_start_index: u32) {}
}

impl<T: Config> frame_support::traits::EstimateNextSessionRotation<T::BlockNumber> for Module<T> {
    fn estimate_next_session_rotation(_now: T::BlockNumber) -> Option<T::BlockNumber> {
        None
    }

    // The validity of this weight depends on the implementation of `estimate_next_session_rotation`
    fn weight(_now: T::BlockNumber) -> u64 {
        0
    }
}

/// Implementation of Convert trait for mapping ValidatorId with AccountId.
/// This is mainly used to map stash and controller keys.
/// In this module, for simplicity, we just return the same AccountId.
pub struct ValidatorOf<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> Convert<T::AccountId, Option<T::AccountId>> for ValidatorOf<T> {
    fn convert(account: T::AccountId) -> Option<T::AccountId> {
        Some(account)
    }
}

impl<T: Config> Module<T> {
    pub fn get_validators() -> Result<Vec<T::AccountId>, &'static str> {
        match Self::validators().ok_or(Error::<T>::NoValidators) {
            Ok(validators) => {
                frame_support::debug::info!(
                    "Fetching the {} validators on the network",
                    validators.len()
                );
                return Ok(validators);
            }
            Err(e) => {
                frame_support::debug::error!("Failed to get validators: {:#?}", e);
                return Err("No validators found");
            }
        };
    }

    pub fn is_validator(account_id: &T::AccountId) -> bool {
        if let Some(vs) = <Validators<T>>::get() {
            return vs.contains(account_id);
        }

        return false;
    }
}
