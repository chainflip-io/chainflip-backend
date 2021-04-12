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
    use sp_runtime::{app_crypto::RuntimePublic, traits::{AtLeast32BitUnsigned, CheckedSub, One, Zero}};

    type AccountId<T> = <T as frame_system::Config>::AccountId;

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
            + CheckedSub;
        
        type EthereumPubKey: Member + FullCodec + RuntimePublic;

        type Nonce: Member
            + FullCodec
            + Copy
            + Default
            + AtLeast32BitUnsigned
            + MaybeSerializeDeserialize
            + CheckedSub;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::storage]
    pub type Stakes<T: Config> = StorageMap<_, Identity, AccountId<T>, T::StakedAmount, ValueQuery>;

    #[pallet::storage]
    pub type PendingClaims<T: Config> = StorageMap<
        _, 
        Identity, 
        AccountId<T>, 
        (T::Nonce, T::StakedAmount), 
        ValueQuery>;

    #[pallet::storage]
    pub type Nonces<T: Config> = StorageMap<_, Identity, AccountId<T>, T::Nonce, ValueQuery>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    {
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    {
        /// Witness that a `Staked` event was emitted by the `StakeManager` smart contract.
        #[pallet::weight(10_000)]
        pub fn witness_staked(
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
                // Account doesn't exist.
                // Vote to call `refund` through multisig.
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

            Ok(().into())
		}

        /// Get FLIP that is held for me by the system, signed by my validator key.
        ///
        /// *QUESTION: should we burn a small amount of FLIP here to disincentivize spam?*
        #[pallet::weight(10_000)]
        pub fn claim(
            origin: OriginFor<T>,
            amount: T::StakedAmount,
            eth_address: T::EthereumPubKey,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            // If a claim already exists, return an error. The validator must either redeem their claim voucher
            // or wait until expiry before creating a new claim.
            ensure!(!PendingClaims::<T>::contains_key(&who), Error::<T>::PendingClaim);

            // Throw an error if the validator tries to claim too much. Otherwise decrement the stake by the 
            // amount claimed.
            Stakes::<T>::try_mutate::<_,_,Error::<T>,_>(&who, |stake| {
                *stake = stake.checked_sub(&amount).ok_or(Error::<T>::InsufficientStake)?;
                Ok(())
            })?;

            // Don't check for overflow here - we don't expect more than 2^32 claims.
            let nonce = Nonces::<T>::mutate(&who, |nonce| {
                *nonce += T::Nonce::one();
                *nonce
            });
            
            // Emit the event requesting that the CFE to generate the claim voucher.
            Self::deposit_event(Event::<T>::ClaimSigRequested(eth_address, nonce, amount));

            // Assume for now that the siging process is successful and simply insert this claim into
            // the pending claims. 
            //
            // TODO: This should be inserted by the CFE signer process including a valid signature.
            PendingClaims::<T>::insert(&who, (nonce, amount));

            Ok(().into())
        }

        /// Witness that a `Claimed` event was emitted by the `StakeManager` smart contract. 
        ///
        /// This implies that a valid claim has been 
        #[pallet::weight(10_000)]
        pub fn witness_claimed(
            origin: OriginFor<T>,
            account_id: AccountId<T>,
            claimed_amount: T::StakedAmount,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            debug::info!("Witnessed `claimed` event!");

            // If a claim exists, remove it.
            // If it doesn't exist, something bad has happened.

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
            
            // TODO: Assert that the calling origin is the MultiSig origin.

            let (remaining_stake, overflow) = Stakes::<T>::try_mutate_exists::<_,_,Error::<T>,_>(&account_id, |storage| {
                let mut overflow = false;

                *storage = match storage {
                    Some(staked_amount) => {
                        match staked_amount.checked_sub(&claimed_amount) {

                            // If the validator has no more balance, remove the storage entry.
                            Some(balance) if balance == T::StakedAmount::zero() => Ok(None),

                            // Otherwise update the storage entry with the new stake balance. 
                            Some(balance) => Ok(Some(balance)),

                            // `None` indicates overflow occurred. This should never happen, however, if it does, 
                            // we set an overflow flag and delete the record.
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
            ensure!(!overflow, Error::<T>::ClaimOverflow);

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

        /// A claim request has been made to provided Ethereum address. [address, nonce, amount]
        ClaimSigRequested(T::EthereumPubKey, T::Nonce, T::StakedAmount),
    }

    #[pallet::error]
    pub enum Error<T> {
        /// The account to be staked is not known.
        UnknownAccount,

        /// The claimant doesn't exist.
        UnknownClaimant,

        /// The claimant doesn't exist.
        InsufficientStake,

        /// The claimant tried to claim despite having a claim already pending.
        PendingClaim,

        /// The claimant tried to claim more funds than were available.
        ClaimOverflow,
    }
}
