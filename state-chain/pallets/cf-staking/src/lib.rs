#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use frame_support::{error::BadOrigin, traits::EnsureOrigin};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;

use codec::FullCodec;
use sp_runtime::{traits::{AtLeast32BitUnsigned, CheckedAdd, CheckedSub, One}};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::Witnesser;
	use frame_support::pallet_prelude::*;
	use frame_system::{Account, pallet_prelude::*};
	use sp_runtime::app_crypto::RuntimePublic;

	type AccountId<T> = <T as frame_system::Config>::AccountId;

	#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
	pub(super) struct Claim<Amount, Nonce, EthereumAddress, Signature> {
		pub(super) amount: Amount,
		pub(super) nonce: Nonce,
		pub(super) address: EthereumAddress,
		pub(super) signature: Option<Signature>
	}

	#[pallet::config]
	pub trait Config: frame_system::Config
	{
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Standard Call type. We need this so we can use it as a constraint in `Witnesser`.
		type Call: From<Call<Self>> + IsType<<Self as frame_system::Config>::Call>;

		/// Numeric type denomination for the staked asset.
		type StakedAmount: Member
			+ FullCodec
			+ Copy
			+ Default
			+ AtLeast32BitUnsigned
			+ MaybeSerializeDeserialize
			+ CheckedSub;
		
		/// Ethereum address type, should correspond to [u8; 20], but defined globally for the runtime.
		type EthereumAddress: Member + FullCodec + Copy;

		/// A Nonce type to be used for claim nonces.
		type Nonce: Member
			+ FullCodec
			+ Copy
			+ Default
			+ AtLeast32BitUnsigned
			+ MaybeSerializeDeserialize
			+ CheckedSub;

		/// A type representing ethereum cryptographic primitives.
		type EthereumCrypto: Member + FullCodec + RuntimePublic;

		/// Base priority of unsigned transactions.
		type UnsignedPriority: Get<TransactionPriority>;

		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

		type Witnesser: cf_traits::Witnesser<
			Call=<Self as Config>::Call, 
			AccountId=<Self as frame_system::Config>::AccountId>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	pub type Stakes<T: Config> = StorageMap<_, Identity, AccountId<T>, T::StakedAmount, ValueQuery>;

	#[pallet::storage]
	pub(super) type PendingClaims<T: Config> = StorageMap<
		_, 
		Identity, 
		AccountId<T>, 
		Claim<T::StakedAmount, T::Nonce, T::EthereumAddress, <T::EthereumCrypto as RuntimePublic>::Signature>, 
		OptionQuery>;

	#[pallet::storage]
	pub type Nonces<T: Config> = StorageMap<_, Identity, AccountId<T>, T::Nonce, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> { }

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Witness that a `Staked` event was emitted by the `StakeManager` smart contract.
		#[pallet::weight(10_000)]
		pub fn witness_staked(
			origin: OriginFor<T>,
			staker_account_id: AccountId<T>,
			amount: T::StakedAmount,
			refund_address: T::EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = Call::staked(staker_account_id, amount, refund_address);

			T::Witnesser::witness(who, call.into())?;

			Ok(().into())
		}

		/// Funds have been staked to an account via the StakeManager smart contract. 
		///
		/// **This is a MultiSig call**
		#[pallet::weight(10_000)]
		pub fn staked(
			origin: OriginFor<T>,
			account_id: T::AccountId,
			amount: T::StakedAmount,
			refund_address: T::EthereumAddress,
		) -> DispatchResultWithPostInfo {
			Self::ensure_witnessed(origin)?;

			if Account::<T>::contains_key(&account_id) {
				let total_stake = Self::add_stake(&account_id, amount)?;
				Self::deposit_event(Event::Staked(account_id, amount, total_stake));
			} else {
				// Account doesn't exist.
				debug::info!("Unknown staking account id {:?}, proceeding to refund.", account_id);
				Self::deposit_event(Event::Refund(amount, refund_address));
			}
			
			Ok(().into())
		}

		/// Get FLIP that is held for me by the system, signed by my validator key.
		///
		/// *QUESTION: should we burn a small amount of FLIP here to disincentivize spam?*
		#[pallet::weight(10_000)]
		pub fn claim(
			origin: OriginFor<T>,
			amount: T::StakedAmount,
			address: T::EthereumAddress,
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
			
			// Emit the event requesting that the CFE generate the claim voucher.
			Self::deposit_event(Event::<T>::ClaimSigRequested(address, nonce, amount));

			// Assume for now that the siging process is successful and simply insert this claim into
			// the pending claims. 
			//
			// TODO: This should be inserted by the CFE signer process including a valid signature.
			PendingClaims::<T>::insert(&who, Claim {
				amount,
				nonce,
				address,
				signature: None,
			});

			Ok(().into())
		}

		/// Witness that a `Claimed` event was emitted by the `StakeManager` smart contract. 
		#[pallet::weight(10_000)]
		pub fn witness_claimed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			claimed_amount: T::StakedAmount,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = Call::claimed(account_id, claimed_amount);

			T::Witnesser::witness(who, call.into())?;

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
		///
		/// **This is a MultiSig call**
		#[pallet::weight(10_000)]
		pub fn claimed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			claimed_amount: T::StakedAmount,
		) -> DispatchResultWithPostInfo {
			Self::ensure_witnessed(origin)?;

			let pending_claim = PendingClaims::<T>::get(&account_id).ok_or(Error::<T>::NoPendingClaim)?;
			
			ensure!(claimed_amount == pending_claim.amount, Error::<T>::InvalidClaimAmount);

			PendingClaims::<T>::remove(&account_id);

			Self::deposit_event(Event::Claimed(account_id, claimed_amount));

			Ok(().into())
		}

		/// The claim signature generated by the CFE should be posted here so it can be stored on-chain.
		#[pallet::weight(10_000)]
		pub fn post_claim_signature(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			amount: T::StakedAmount,
			nonce: T::Nonce,
			address: T::EthereumAddress,
			signature: <T::EthereumCrypto as RuntimePublic>::Signature,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			// TODO: Verify the signature
			// Should we do this here or in the implementation of ValidateUnsigned?
			// We need to be careful since verification is expensive and therefore a potential DOS vector.
			// 
			// For now, assume signature is valid and proceed.

			let _ = PendingClaims::<T>::mutate_exists(&account_id, |maybe_claim| {
				match maybe_claim.as_mut() {
					Some(claim) => {
						claim.signature = Some(signature.clone());
						Ok(())
					},
					None => Err(Error::<T>::NoPendingClaim)
				}
			})?;

			Self::deposit_event(Event::ClaimSignatureIssued(amount, nonce, address, signature));

			Ok(().into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config>
	{
		/// A validator has staked some FLIP on the Ethereum chain. [validator_id, stake_added, total_stake]
		Staked(AccountId<T>, T::StakedAmount, T::StakedAmount),

		/// A validator has claimed their FLIP on the Ethereum chain. [validator_id, claimed_amount]
		Claimed(AccountId<T>, T::StakedAmount),

		/// The staked amount should be refunded to the provided Ethereum address. [refund_amount, address]
		Refund(T::StakedAmount, T::EthereumAddress),

		/// A claim request has been made to provided Ethereum address. [address, nonce, amount]
		ClaimSigRequested(T::EthereumAddress, T::Nonce, T::StakedAmount),

		/// A claim signature has been issued by the signer module. [amount, nonce, address, signature]
		ClaimSignatureIssued(T::StakedAmount, T::Nonce, T::EthereumAddress, <T::EthereumCrypto as RuntimePublic>::Signature)
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account to be staked is not known.
		UnknownAccount,

		/// An invalid claim has been witnessed: the account has no pending claims.
		NoPendingClaim,

		/// An invalid claim has been witnessed: the amount claimed does not match the pending claim amount.
		InvalidClaimAmount,

		/// The claimant doesn't exist.
		InsufficientStake,

		/// The claimant tried to claim despite having a claim already pending.
		PendingClaim,

		/// The claimant tried to claim more funds than were available. 
		ClaimOverflow,

		/// Stake amount caused overflow on addition. Should never happen.
		StakeOverflow,
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(
			source: TransactionSource,
			call: &Self::Call
		) -> TransactionValidity {
			if let Call::post_claim_signature(account_id, amount, address, nonce, sig) = call {
				
				// TODO: Verify signature here.

				ValidTransaction::with_tag_prefix("ClaimSig")
					.priority(T::UnsignedPriority::get())
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
}

impl<T: Config> Module<T> {
	fn add_stake(account_id: &T::AccountId, amount: T::StakedAmount) -> Result<T::StakedAmount, Error<T>> {
		Stakes::<T>::try_mutate(
			account_id, 
			|stake| {
				*stake = stake
					.checked_add(&amount)
					.ok_or(Error::<T>::StakeOverflow)?;
				
				Ok(*stake)
			})
	}

	fn ensure_witnessed(origin: OriginFor<T>) -> Result<<T::EnsureWitnessed as EnsureOrigin<OriginFor<T>>>::Success, BadOrigin> {
		T::EnsureWitnessed::ensure_origin(origin)
	}
}
