// Code mostly taken from here: https://github.com/gautamdhameja/substrate-validator-set
// modifications to it, such as validation (since we're not using sudo to add validators)
// will come later

#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::{
	prelude::*,
};
use sp_runtime::traits::Convert;
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
		pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;
	use super::*;

    #[pallet::config]
	pub trait Config: frame_system::Config + pallet_session::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
    }
    
    // Simple declaration of the `Pallet` type. It is placeholder we use to implement traits and
	// method.
	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

    #[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		// New validator added.
        ValidatorAdded(T::AccountId),
        // Validator removed.
        ValidatorRemoved(T::AccountId),
	}

    #[pallet::error]
    pub enum Error<T> {
        NoValidators,
    }
    
    // Pallet implements [`Hooks`] trait to define some logic to execute in some context.
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::call]
	impl<T: Config> Pallet<T> {
        /// New validator's session keys should be set in session module before calling this.
        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub(super) fn add_validator(origin: OriginFor<T>, validator_id: T::AccountId) -> DispatchResultWithPostInfo {
            ensure_signed(origin)?;
            let mut validators = Self::validators().ok_or(Error::<T>::NoValidators)?;
            validators.push(validator_id.clone());
            <Validators<T>>::put(validators);
            // Calling rotate_session to queue the new session keys.
            <pallet_session::Module<T>>::rotate_session();
            Self::deposit_event(Event::ValidatorAdded(validator_id));
            
            // Triggering rotate session again for the queued keys to take effect immediately
            <Flag<T>>::put(true);
            Ok(().into())
        }

        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub(super) fn remove_validator(origin: OriginFor<T>, validator_id: T::AccountId) -> DispatchResultWithPostInfo {
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
            Self::deposit_event(Event::ValidatorRemoved(validator_id));

            // Triggering rotate session again for the queued keys to take effect.
            <Flag<T>>::put(true);
            Ok(().into())
        }
    }

    #[pallet::storage]
	#[pallet::getter(fn flag)]
	pub(super) type Flag<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
	#[pallet::getter(fn validators)]
	pub(super) type Validators<T: Config> = StorageValue<_, Vec<T::AccountId>, OptionQuery>;
    
    // The genesis config type.
	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub validators: Vec<T::AccountId>,
	}


    	// The default value for the genesis config type.
	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				validators: Default::default(),
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			// <Dummy<T>>::put(&self.dummy);
			// for (a, b) in &self.bar {
			// 	<Bar<T>>::insert(a, b);
			// }
			// <Foo<T>>::put(&self.foo);
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
        <Flag<T>>::put(false);
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

impl<T: Config> Pallet<T> {
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
