#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, FullCodec};
use frame_support::{
    debug, decl_error, decl_event, decl_module, decl_storage, dispatch::DispatchResult,
    unsigned::TransactionValidity, Parameter,
};
use sp_runtime::traits::{AtLeast32BitUnsigned, MaybeSerializeDeserialize, Zero, CheckedSub};
use sp_runtime::transaction_validity::{InvalidTransaction, TransactionSource, ValidTransaction};
use sp_std::{fmt::Debug, ops::Add, prelude::*};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

/// Configure the pallet by specifying the parameters and types on which it depends.
pub trait Trait: frame_system::Trait {
    /// Standard Event type.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

    /// Numeric type based on the `Balance` type from `Currency` trait. Defined inline for now, but we
    /// might want to consider using the `Balances` pallet in future.
    type StakedAmount: Parameter
        + AtLeast32BitUnsigned
        + FullCodec
        + Copy
        + MaybeSerializeDeserialize
        + Debug
        + Default
        + Zero
        + Add
        + CheckedSub;
}

type AccountId<T> = <T as frame_system::Trait>::AccountId;

decl_storage! {
    trait Store for Module<T: Trait> as StakedFlip {
        pub Stakes get(fn get_stakes): map hasher(identity) AccountId<T> => T::StakedAmount;
    }
}

decl_event! {
    pub enum Event<T> where
        AccountId = <T as frame_system::Trait>::AccountId,
        Amount = <T as Trait>::StakedAmount,
    {
        /// A validator has staked some FLIP on the Ethereum chain. [validator_id, total_stake]
        Staked(AccountId,Amount),
        /// A validator has claimed their FLIP on the Ethereum chain. [validator_id, claimed_amount, remaining_stake]
        Claimed(AccountId,Amount,Amount),
    }
}

decl_error! {
    pub enum Error for Module<T: Trait> {
        /// Staker is already staked.
        AlreadyStaked,
        /// The account to be staked is not known.
        UnknownAccount,
        /// The claimant doesn't exist
        UnknownClaimant,
        /// The claimant tried to claim more funds than were available
        ExcessFundsClaimed,
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        // Errors must be initialized if they are used by the pallet.
        type Error = Error<T>;

        // Events must be initialized if they are used by the pallet.
        fn deposit_event() = default;

        /// Add staked funds to an account.
        #[weight = 10_000]
        pub fn staked(_origin,
            account_id: AccountId<T>,
            amount: T::StakedAmount) -> DispatchResult
        {
            // TODO:
            // Checks:
            // - Is this a validator that is already staked? (does it make a difference?)
            // - Are we currently mid-auction? (does it make a difference?)
            // Questions:
            // - do we need to segregate "pending" stake from "active" stake?
            debug::info!("Received `staked` event!");

            let staked_amount: T::StakedAmount = Stakes::<T>::mutate_exists(&account_id, |storage| {
                let staked_amount = storage.unwrap_or(T::StakedAmount::zero()) + amount;
                *storage = Some(staked_amount);
                staked_amount
            });

            Self::deposit_event(RawEvent::Staked(account_id, staked_amount));
            Ok(())
        }

        /// Previously staked funds have been reclaimed.
        /// Note that calling this doesn't initiate any protocol changes - the `claim` has already been authorised
        /// by validator multisig.
        /// If the claimant tries to claim more funds than are available, we set the claimant's balance to 
        /// zero and raise an error. 
        #[weight = 10_000]
        pub fn claimed(_origin,
            account_id: AccountId<T>,
            claimed_amount: T::StakedAmount) -> DispatchResult
        {
            // TODO:
            // Checks:
            // - Are we currently mid-auction? (does it make a difference?)
            debug::info!("Received `claimed` event!");

            let (remaining_stake, overflow) = Stakes::<T>::try_mutate_exists::<_,_,Error::<T>,_>(&account_id, |storage| {
                let mut overflow = false;

                *storage = match storage {
                    Some(staked_amount) => {
                        match staked_amount.checked_sub(&claimed_amount) {
                            Some(balance) if balance == T::StakedAmount::zero() => Ok(None),
                            Some(balance) => Ok(Some(balance)),
                            None => {
                                overflow = true;
                                Ok(None)
                            }
                        }
                    },
                    None => Err(Error::<T>::UnknownClaimant)
                }?;

                Ok((storage.unwrap_or(T::StakedAmount::zero()), overflow))
            })?;

            if overflow {
                Err(Error::<T>::ExcessFundsClaimed)?;
            }

            Self::deposit_event(RawEvent::Claimed(account_id, claimed_amount, remaining_stake));
            Ok(())
        }
    }
}

impl<T: Trait> frame_support::unsigned::ValidateUnsigned for Module<T> {
    type Call = Call<T>;

    fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
        if let Call::staked(account_id, amount) = call {
            // TODO: What restrictions to impose on this?
            // - Should be signed by a known validator (read up on SignedExtension / SignedExtra)
            // - Does it need to be propagated?
            // - Constrain the TransactionSource?
            // - Set longevity to make sure txn expires if not applied.

            ValidTransaction::with_tag_prefix("StakeManager")
                // TODO: there should be a system-wide default priority for unsigned txns
                .priority(100)
                //
                // .and_requires()
                // `provides` are necessary for transaction validity so we need to include something. Since
                // we have no `requires`, the only effect of this is to make sure only a single unsigned
                // transaction with the below criteria will get into the transaction pool in a single block.
                .and_provides((
                    frame_system::Module::<T>::block_number(),
                    account_id,
                    amount,
                ))
                // .longevity(TryInto::<u64>::try_into(
                // 	T::SessionDuration::get() / 2u32.into()
                // ).unwrap_or(64_u64))
                .propagate(true)
                .build()
        } else {
            InvalidTransaction::Call.into()
        }
    }
}
