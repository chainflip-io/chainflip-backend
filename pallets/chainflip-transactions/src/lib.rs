#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{decl_error, decl_event, decl_module, dispatch::DispatchResult};
use frame_system::ensure_signed;
use sp_std::vec::Vec;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod states;

/// Configure the pallet by specifying the parameters and types on which it depends.
pub trait Trait: frame_system::Trait {
    /// Because this pallet emits events, it depends on the runtime's definition of an event.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
}

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
    }
);

// Errors inform users that something went wrong.
decl_error! {
    pub enum Error for Module<T: Trait> {
        /// Invalid data was provided
        InvalidData,
        // TODO: Add more errors?
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
        pub fn set_witness(origin, data: states::Witness) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            // TODO: Validate state

            Self::deposit_event(RawEvent::WitnessAdded(who, data));

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

        #[weight = 0]
        pub fn set_data(origin, data: Vec<u8>) -> DispatchResult {
            // Ensure extrinsic is signed
            let who = ensure_signed(origin)?;

            // TODO: Validate state

            Self::deposit_event(RawEvent::DataAdded(who, data));

            Ok(())
        }
    }
}
