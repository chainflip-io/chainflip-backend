#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{decl_error, decl_event, decl_storage, decl_module, dispatch::DispatchResult};
use frame_system::{ensure_signed};
use sp_std::vec::Vec;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod states;

/// Configure the pallet by specifying the parameters and types on which it depends.
pub trait Trait: frame_system::Trait + pallet_cf_validator::Trait {
    /// Because this pallet emits events, it depends on the runtime's definition of an event.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
}

decl_storage!(
    trait Store for Module<T: Trait> as WitnessStorage {
        WitnessMap get(fn witness_map): map hasher(blake2_128_concat) Vec<u8> => Vec<T::AccountId>;
    }
);

// Transaction events
decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as frame_system::Trait>::AccountId,
    {
        // TODO: Write a macro for the things below?
        SwapQuoteAdded(AccountId, states::SwapQuote),
        DepositQuoteAdded(AccountId, states::DepositQuote),
        WithdrawRequestAdded(AccountId, states::WithdrawRequest),
        WitnessAdded(AccountId, states::Witness),
        PoolChangeAdded(AccountId, states::PoolChange),
        DepositAdded(AccountId, states::Deposit),
        WithdrawAdded(AccountId, states::Withdraw),
        OutputAdded(AccountId, states::Output),
        OutputSentAdded(AccountId, states::OutputSent),
        DataAdded(AccountId, Vec<u8>),
        NumberAdded(AccountId, u8),
    }
);

// Errors inform users that something went wrong.
decl_error! {
    pub enum Error for Module<T: Trait> {
        /// Invalid data was provided
        InvalidData,
        ValidatorAlreadySubmittedWitness,
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        type Error = Error<T>;

        fn deposit_event() = default;

        // TODO: Write a macro for the functions below?

        #[weight = 0]
        pub fn set_swap_quote(origin, data: states::SwapQuote) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            // TODO: Validate state

            Self::deposit_event(RawEvent::SwapQuoteAdded(who, data));

            Ok(())
        }

        #[weight = 0]
        pub fn set_deposit_quote(origin, data: states::DepositQuote) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            // TODO: Validate state

            Self::deposit_event(RawEvent::DepositQuoteAdded(who, data));

            Ok(())
        }

        #[weight = 0]
        pub fn set_withdraw_request(origin, data: states::WithdrawRequest) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            // TODO: Validate state

            Self::deposit_event(RawEvent::WithdrawRequestAdded(who, data));

            Ok(())
        }

        #[weight = 0]
        pub fn set_witness(origin, witness: states::Witness) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            if <WitnessMap<T>>::contains_key(&witness.id) {
                // insert an entry into the pre-existing vector
                let mut curr_validators = <WitnessMap<T>>::get(&witness.id);
                // make sure the validator is not already in the set
                match curr_validators.binary_search(&who) {
                    Ok(_) => return Err(Error::<T>::ValidatorAlreadySubmittedWitness.into()),
                    Err(index) => {
                        curr_validators.insert(index, who.clone());
                        <WitnessMap<T>>::insert(&witness.id, curr_validators);
                    }
                }
            } else {
                // insert a new key and initialise the vector with the current value
                let mut validators = Vec::default();
                validators.push(who.clone());
                <WitnessMap<T>>::insert(&witness.id, validators);
            }
            Self::deposit_event(RawEvent::WitnessAdded(who, witness));
            Ok(())
        }

        #[weight = 0]
        pub fn set_pool_change(origin, data: states::PoolChange) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            // TODO: Validate state

            Self::deposit_event(RawEvent::PoolChangeAdded(who, data));

            Ok(())
        }

        #[weight = 0]
        pub fn set_deposit(origin, data: states::Deposit) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            // TODO: Validate state

            Self::deposit_event(RawEvent::DepositAdded(who, data));

            Ok(())
        }

        #[weight = 0]
        pub fn set_withdraw(origin, data: states::Withdraw) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            // TODO: Validate state

            Self::deposit_event(RawEvent::WithdrawAdded(who, data));

            Ok(())
        }

        #[weight = 0]
        pub fn set_output(origin, data: states::Output) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            // TODO: Validate state

            Self::deposit_event(RawEvent::OutputAdded(who, data));

            Ok(())
        }

        #[weight = 0]
        pub fn set_output_sent(origin, data: states::OutputSent) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            // TODO: Validate state

            Self::deposit_event(RawEvent::OutputSentAdded(who, data));

            Ok(())
        }

        // This is for testing
        #[weight = 0]
        pub fn set_data(origin, data: Vec<u8>) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            Self::deposit_event(RawEvent::DataAdded(who, data));

            Ok(())
        }
    }
}

impl<T: Trait> Module<T> {
	pub fn get_valid_witnesses() -> Vec<Vec<u8>> {

        let mut valid_witnesess: Vec<Vec<u8>> = Vec::new();
        let validators = <pallet_cf_validator::Module<T>>::get_validators();
        let num_validators = validators.unwrap_or(Vec::new()).len();
        // super majority
        let threshold = num_validators as f64 * 0.67;
        for (witness_id, validators_of_witness) in <WitnessMap<T>>::iter() {
            // return the witnesses that have more than the number of validators as witnesses
            frame_support::debug::info!("Witness id: {:#?}", witness_id);
            frame_support::debug::info!("Validators for witness: {:#?}", validators_of_witness);

            if validators_of_witness.len() as f64 > threshold {
                valid_witnesess.push(witness_id);
            }
        }

        frame_support::debug::info!("Valid witnesses: {:#?}", valid_witnesess);
        valid_witnesess
    }
}