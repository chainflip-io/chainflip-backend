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
    use frame_system::pallet::Account;
    use sp_runtime::{app_crypto::RuntimePublic, traits::{AtLeast32BitUnsigned, CheckedSub, Zero}};
    use sp_std::{fmt::Debug, ops::Add};

    type AccountId<T> = <T as frame_system::Config>::AccountId;

    struct ClaimState<T: Config> {
        claim_nonce: u32,
        pending_claim: Option<Claim<T>>,
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
    struct Claim<T: Config> {
        amount: T::StakedAmount,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config
    {
        /// Standard Event type.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
    
        /// Numeric type based on the `Balance` type from `Currency` trait. Defined inline for now, but we
        /// might want to consider using the `Balances` pallet in future.
        type StakedAmount: Member
            + FullCodec
            + Copy
            + Default
            + AtLeast32BitUnsigned
            + MaybeSerializeDeserialize
            + Zero
            + Add
            + CheckedSub;
        
		type EthereumPubKey: Member + FullCodec + RuntimePublic;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::storage]
    #[pallet::getter(fn get_stakes)]
    pub type Stakes<T: Config> = StorageMap<_, Identity, AccountId<T>, T::StakedAmount, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_claim_states)]
    pub type ClaimStates<T: Config> = StorageMap<_, Identity, AccountId<T>, ClaimState<T>, ValueQuery>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    {
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    {
        /// Called as a witness for a new stake submitted through the StakeManager contract.
        /// 
        #[pallet::weight(10_000)]
        pub fn witness_staked_event(
            origin: OriginFor<T>,
            staker_account_id: AccountId<T>,
            amount: T::StakedAmount,
			eth_pubkey: T::EthereumPubKey,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            debug::info!("Witnessed `staked` event!");

            if Account::<T>::contains_key(who) {
                // Vote to call `stake` through the multisig. 
            } else {
                // Vote to call `refund` through multisig
            }

            Ok(().into())
        }

        /// Add staked funds to an account. 
		#[pallet::weight(10_000)]
		pub fn stake(
			origin: OriginFor<T>,
			account_id: T::AccountId,
			amount: T::StakedAmount,
			_eth_pubkey: T::EthereumPubKey,
		) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            // TODO: Assert that the calling origin is the MultiSig origin. 
            
            let total_stake: T::StakedAmount = Stakes::<T>::mutate_exists(
                &account_id, 
                |storage| {
                    let total_stake = storage.unwrap_or(T::StakedAmount::zero()) + amount;
                    *storage = Some(total_stake);
                    total_stake
                });

            Self::deposit_event(Event::Staked(account_id, amount, total_stake));

			todo!()
		}

        /// Get FLIP that is held for me by the system, signed by a validator key.
        #[pallet::weight(10_000)]
        pub fn claim(
            origin: OriginFor<T>,
            amount: T::StakedAmount,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin);

            // TODO: 
            // Is enough balance available? 
            // Are any unexpired claims pending? If so, return the pending claim instead.
            // Reserve the balance so it can't be re-claimed. 
            // Emit ClaimSigRequested(nonce)

            Ok(().into())
        }

        /// Previously staked funds have been reclaimed.
        ///
        /// Note that calling this doesn't initiate any protocol changes - the `claim` has already been authorised
        /// by validator multisig. This merely signals that the claimant has in fact redeemed their funds via the 
        /// `StakeManager` contract. 
        ///
        /// If the claimant tries to claim more funds than are available, we set the claimant's balance to 
        /// zero and raise an error. 
        #[pallet::weight(10_000)]
        pub fn claimed(
            origin: OriginFor<T>,
            account_id: AccountId<T>,
            claimed_amount: T::StakedAmount,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            debug::info!("Witnessed `claimed` event!");

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

            // QUESTION: Is it ok to do this, ie. raise an error *after* changing the state? 
            if overflow {
                Err(Error::<T>::ExcessFundsClaimed)?;
            }

            Self::deposit_event(Event::Claimed(account_id, claimed_amount, remaining_stake));
            Ok(().into())
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config>
    {
        /// A validator has staked some FLIP on the Ethereum chain. [validator_id, stake_added, total_stake]
        Staked(AccountId<T>, T::StakedAmount, T::StakedAmount),
        /// A validator has claimed their FLIP on the Ethereum chain. [validator_id, claimed_amount, remaining_stake]
        Claimed(AccountId<T>, T::StakedAmount, T::StakedAmount),
        /// The staked amount should be refunded to the provided Ethereum address. [refund_amount, address]
        Refund(T::StakedAmount, T::EthereumPubKey),
    }

    #[pallet::error]
    pub enum Error<T> {
        /// The account to be staked is not known.
        UnknownAccount,
        /// The claimant doesn't exist
        UnknownClaimant,
        /// The claimant tried to claim more funds than were available
        ExcessFundsClaimed,
    }
}



