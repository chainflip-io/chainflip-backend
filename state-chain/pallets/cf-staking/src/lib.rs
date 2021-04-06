#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use codec::FullCodec;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::{AtLeast32BitUnsigned, CheckedSub, Zero};
    use sp_std::{fmt::Debug, ops::Add};

    type AccountId<T> = <T as frame_system::Config>::AccountId;

    #[pallet::config]
    pub trait Config: frame_system::Config
    {
        /// Standard Event type.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
    
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

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    {
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    {
        /// Add staked funds to an account.
        #[pallet::weight(10_000)]
        pub fn staked(_origin: OriginFor<T>,
            account_id: AccountId<T>,
            amount: T::StakedAmount) -> DispatchResultWithPostInfo
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

            Self::deposit_event(Event::Staked(account_id, staked_amount));
            Ok(().into())
        }

        /// Previously staked funds have been reclaimed.
        /// Note that calling this doesn't initiate any protocol changes - the `claim` has already been authorised
        /// by validator multisig.
        /// If the claimant tries to claim more funds than are available, we set the claimant's balance to 
        /// zero and raise an error. 
        #[pallet::weight(10_000)]
        pub fn claimed(_origin: OriginFor<T>,
            account_id: AccountId<T>,
            claimed_amount: T::StakedAmount) -> DispatchResultWithPostInfo
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

            Self::deposit_event(Event::Claimed(account_id, claimed_amount, remaining_stake));
            Ok(().into())
        }
    }

    // #[pallet::inherent]

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config>
    {
        /// A validator has staked some FLIP on the Ethereum chain. [validator_id, total_stake]
        Staked(AccountId<T>, T::StakedAmount),
        /// A validator has claimed their FLIP on the Ethereum chain. [validator_id, claimed_amount, remaining_stake]
        Claimed(AccountId<T>, T::StakedAmount, T::StakedAmount),
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Staker is already staked.
        AlreadyStaked,
        /// The account to be staked is not known.
        UnknownAccount,
        /// The claimant doesn't exist
        UnknownClaimant,
        /// The claimant tried to claim more funds than were available
        ExcessFundsClaimed,
    }

    #[pallet::validate_unsigned]
    impl<T: Config> frame_support::unsigned::ValidateUnsigned for Pallet<T> {
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

    #[pallet::storage]
    #[pallet::getter(fn get_stakes)]
    pub type Stakes<T: Config> = StorageMap<_, Identity, AccountId<T>, T::StakedAmount, ValueQuery>;
}



