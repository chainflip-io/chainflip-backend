#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use std::todo;

use frame_support::{ensure, error::BadOrigin, traits::EnsureOrigin};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use cf_traits::{Witnesser, BondProvider, ValidatorProvider};

use codec::FullCodec;
use sp_runtime::{traits::{AtLeast32BitUnsigned, CheckedAdd, CheckedSub, One, Saturating, Zero}};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
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
		type TokenAmount: Member
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

		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

		type Witnesser: Witnesser<
			Call=<Self as Config>::Call, 
			AccountId=<Self as frame_system::Config>::AccountId>;
		
		type BondProvider: BondProvider<Amount=Self::TokenAmount>;

		type ValidatorProvider: ValidatorProvider<ValidatorId=<Self as frame_system::Config>::AccountId>;
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub(super) struct StakeRecord<T: Config> {
		pub stake: T::TokenAmount,
		pub retired: bool,
	}

	impl<T: Config> StakeRecord<T> {
		pub fn try_subtract_stake(&mut self, amount: &T::TokenAmount) -> Option<()> {
			self.stake.checked_sub(amount).map(|result| {
				self.stake = result;
			})
		}

		pub fn try_add_stake(&mut self, amount: &T::TokenAmount) -> Option<()> {
			self.stake.checked_add(amount).map(|result| {
				self.stake = result;
			})
		}
	}

	impl<T: Config> Default for StakeRecord<T> {
		fn default() -> Self {
			StakeRecord {
				stake: Zero::zero(),
				retired: false,
			}
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	pub(super) type Stakes<T: Config> = StorageMap<_, Identity, AccountId<T>, StakeRecord<T>, ValueQuery>;

	#[pallet::storage]
	pub(super) type PendingClaims<T: Config> = StorageMap<
		_, 
		Identity, 
		AccountId<T>, 
		Claim<T::TokenAmount, T::Nonce, T::EthereumAddress, <T::EthereumCrypto as RuntimePublic>::Signature>, 
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
			amount: T::TokenAmount,
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
			amount: T::TokenAmount,
			refund_address: T::EthereumAddress,
		) -> DispatchResultWithPostInfo {
			Self::ensure_witnessed(origin)?;

			if Account::<T>::contains_key(&account_id) {
				let total_stake = Self::add_stake(&account_id, amount)?;
				Self::deposit_event(Event::Staked(account_id, amount, total_stake));
			} else {
				// Account doesn't exist.
				debug::info!("Unknown staking account id {:?}, proceeding to refund.", account_id);
				Self::deposit_event(Event::StakeRefund(account_id, amount, refund_address));
			}
			
			Ok(().into())
		}

		/// Get FLIP that is held for me by the system, signed by my validator key.
		///
		/// *QUESTION: should we burn a small amount of FLIP here to disincentivize spam?*
		#[pallet::weight(10_000)]
		pub fn claim(
			origin: OriginFor<T>,
			amount: T::TokenAmount,
			address: T::EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// If a claim already exists, return an error. The validator must either redeem their claim voucher
			// or wait until expiry before creating a new claim.
			ensure!(!PendingClaims::<T>::contains_key(&who), Error::<T>::PendingClaim);
			
			// Throw an error if the validator tries to claim too much. Otherwise decrement the stake by the 
			// amount claimed.
			Self::subtract_stake(&who, amount)?;

			// Don't check for overflow here - we don't expect more than 2^32 claims.
			let nonce = Nonces::<T>::mutate(&who, |nonce| {
				*nonce += T::Nonce::one();
				*nonce
			});
			
			// Insert a pending claim without a signature.
			PendingClaims::<T>::insert(&who, Claim {
				amount,
				nonce,
				address,
				signature: None,
			});

			// Emit the event requesting that the CFE generate the claim voucher.
			Self::deposit_event(Event::<T>::ClaimSigRequested(who.clone(), address, nonce, amount));

			Ok(().into())
		}

		/// Witness that a `Claimed` event was emitted by the `StakeManager` smart contract. 
		#[pallet::weight(10_000)]
		pub fn witness_claimed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			claimed_amount: T::TokenAmount,
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
			claimed_amount: T::TokenAmount,
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
			amount: T::TokenAmount,
			nonce: T::Nonce,
			address: T::EthereumAddress,
			signature: <T::EthereumCrypto as RuntimePublic>::Signature,
		) -> DispatchResultWithPostInfo {
			// TODO: we should check more than just "is this a valid account" - see clubhouse stories 471 and 473
			let who = ensure_signed(origin)?;

			let _ = PendingClaims::<T>::mutate_exists(&account_id, |maybe_claim| {
				match maybe_claim.as_mut() {
					Some(claim) => {
						match claim.signature {
							Some(_) => Err(Error::<T>::SignatureAlreadyIssued),
							None => {
								claim.signature = Some(signature.clone());
								Ok(())
							},
						}
					},
					None => Err(Error::<T>::NoPendingClaim)
				}
			})?;

			Self::deposit_event(Event::ClaimSignatureIssued(who, amount, nonce, address, signature));

			Ok(().into())
		}

		/// Signals a validator's intent to withdraw their stake after the next auction and desist from future auctions.
		#[pallet::weight(10_000)]
		pub fn retire_account(
			origin: OriginFor<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			Self::retire(&who)?;

			Self::deposit_event(Event::AccountRetired(who));

			Ok(().into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config>
	{
		/// A validator has staked some FLIP on the Ethereum chain. [validator_id, stake_added, total_stake]
		Staked(AccountId<T>, T::TokenAmount, T::TokenAmount),

		/// A validator has claimed their FLIP on the Ethereum chain. [validator_id, claimed_amount]
		Claimed(AccountId<T>, T::TokenAmount),

		/// The staked amount should be refunded to the provided Ethereum address. [node_id, refund_amount, address]
		StakeRefund(AccountId<T>, T::TokenAmount, T::EthereumAddress),

		/// A claim request has been made to provided Ethereum address. [who, address, nonce, amount]
		ClaimSigRequested(AccountId<T>, T::EthereumAddress, T::Nonce, T::TokenAmount),

		/// A claim signature has been issued by the signer module. [issuer, amount, nonce, address, signature]
		ClaimSignatureIssued(AccountId<T>, T::TokenAmount, T::Nonce, T::EthereumAddress, <T::EthereumCrypto as RuntimePublic>::Signature),

		/// An account has retired and will no longer take part in auctions [who].
		AccountRetired(AccountId<T>)
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account is not known.
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

		/// An account tried to post a signature to an alread-signed claim. 
		SignatureAlreadyIssued,

		/// Can't retire an account if it's already retired.
		AlreadyRetired,

		/// Certain action can only be performed if the account has stake associated with it. 
		AccountNotStaked
	}
}

impl<T: Config> Pallet<T> {
	/// Adds stake to an account. Errors if the addition overflows.
	fn add_stake(account_id: &T::AccountId, amount: T::TokenAmount) -> Result<T::TokenAmount, Error<T>> {
		Stakes::<T>::try_mutate(
			account_id, 
			|rec| {
				rec.try_add_stake(&amount).ok_or(Error::<T>::StakeOverflow)?;
				Ok(rec.stake)
			})
	}

	/// Subtracts an amount from the account's staked token. If the account has insufficient staked tokens, or if the 
	/// remaining balance would be less than the bonded amount, returns an [Error::InsufficientStake]
	fn subtract_stake(account_id: &T::AccountId, amount: T::TokenAmount) -> Result<T::TokenAmount, Error<T>> {
		let bond = Self::get_bond(account_id);
		Stakes::<T>::try_mutate(
			account_id, 
			|rec| {
				rec.try_subtract_stake(&amount).ok_or(Error::InsufficientStake)?;
				ensure!(rec.stake >= bond, Error::InsufficientStake);
				Ok(rec.stake)
			})
	}

	/// Checks that the call orginates from the witnesser by delegating to the configured implementation of 
	/// `
	fn ensure_witnessed(origin: OriginFor<T>) -> Result<<T::EnsureWitnessed as EnsureOrigin<OriginFor<T>>>::Success, BadOrigin> {
		T::EnsureWitnessed::ensure_origin(origin)
	}

	/// Returns the total stake associated with this account.
	pub fn get_total_stake(account: &T::AccountId) -> T::TokenAmount {
		Stakes::<T>::get(account).stake
	}

	/// Returns the amount of stake an account can withdraw via a `claim`. Equal to the total stake minus any bond that 
	/// applies to this account. 
	pub fn get_claimable_stake(account: &T::AccountId) -> T::TokenAmount {
		Self::get_total_stake(account).saturating_sub(Self::get_bond(account))
	}

	/// Checks if the account is currently a validator.
	pub fn is_validator(account: &T::AccountId) -> bool {
		T::ValidatorProvider::is_validator(account)
	}

	/// Gets the bond amount for the current epoch. If no bond has been set, returns zero.
	fn get_bond(account: &T::AccountId) -> T::TokenAmount {
		if Self::is_validator(account) {
			T::BondProvider::current_bond()
		} else {
			Zero::zero()
		}
	}

	/// Sets the `retired` flag associated with the account, sigalling that the account no longer wishes to participate
	/// in validator auctions. 
	/// 
	/// Returns an error if the account has already been retired, or if the account has no stake associated. 
	fn retire(account: &T::AccountId) -> Result<(), Error::<T>> {
		Stakes::<T>::try_mutate_exists(account, |maybe_account| {
			match maybe_account.as_mut() {
				Some(account) => {
					if account.retired {
						Err(Error::AlreadyRetired)?;
					}
					account.retired = true;
					Ok(())
				}
				None => Err(Error::AccountNotStaked)?,
			}
		})
	}

	/// Checks if an account has signalled their intention to retire as a validator. If the account has never staked
	/// any tokens, returns [Error::AccountNotStaked]. 
	pub fn is_retired(account: &T::AccountId) -> Result<bool, Error::<T>> {
		Stakes::<T>::try_get(account).map(|s| s.retired).map_err(|_| Error::AccountNotStaked)
	}
}

/// This implementation of [pallet_cf_validator::CandidateProvider] simply returns a list of `(account_id, stake)` for
/// all non-retired accounts.
impl<T: Config> pallet_cf_validator::CandidateProvider for Pallet<T> {
	type ValidatorId = T::AccountId;
	type Stake = T::TokenAmount;
	
	fn get_candidates() -> Vec<(Self::ValidatorId, Self::Stake)> {
		Stakes::<T>::iter()
			.filter_map(|(acct, StakeRecord { stake, retired })| {
				if retired { 
					None 
				} else { 
					Some((acct, stake)) 
				}
			})
			.collect()
	}
}
