#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip staking
//!
//! The responsiblities of this [pallet](Pallet) can be broken down into:
//!
//! ## Staking
//!
//! - Stake is added via the Ethereum StakeManager contract. `Staked` events emitted from this contract should trigger
//!   votes via signed calls to `witness_stake`.
//! - Any stake added in this way is considered as an implicit bid for a validator slot.
//! - If stake is added to a non-existent account, it will not be counted and will be refunded instead.
//!
//! ## Claiming
//!
//! - A claim request is made via a signed call to `claim`.
//! - The claimant who is a current validator is subject to the bond - an amount equal to the current bond is locked
//!   and cannot be claimed. For example if an account has 120 FLIP staked, and the current bond is 40 FLIP, their
//!   claimable balance would be 80 FLIP.
//! - If the account has sufficient funds,
//! - TODO: we could have a convenience extrinsic `claim_all_claimable` that delegates to claim and withdraws all funds.
//!
//! ## Retiring
//!
//! - Accounts are considered active (not retired) by default.
//! - Any active account can make a signed call to the [`retire`](Pallet::retire_account) extrinsic to change their status to
//!   retired.
//! - Only active accounts should be included as active bidders for the auction.
//!
//! ## Auctions
//!
//! This pallet implements the [`CandidateProvider`](pallet_cf_validator::CandidateProvider) trait to provide the current list
//! of staked and active validator candidates along with their total stake, *excluding* any claims that are pending.
//!

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use core::time::Duration;
use frame_support::{
	ensure,
	error::BadOrigin,
	traits::{EnsureOrigin, Get, UnixTime},
	weights,
};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::prelude::*;
use cf_traits::{EpochInfo, BidderProvider};

use codec::FullCodec;
use sp_runtime::traits::{AtLeast32BitUnsigned, CheckedAdd, CheckedSub, One, Saturating, Zero};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::Witnesser;
	use frame_support::pallet_prelude::*;
	use frame_system::{pallet_prelude::*, Account};
	use sp_runtime::app_crypto::RuntimePublic;

	type AccountId<T> = <T as frame_system::Config>::AccountId;

	#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
	pub(super) struct Claim<Amount, Nonce, EthereumAddress, Signature> {
		pub(super) amount: Amount,
		pub(super) nonce: Nonce,
		pub(super) address: EthereumAddress,
		pub(super) signature: Option<Signature>,
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
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

		/// Provides an origin check for witness transactions.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

		/// An implementation of the witnesser to enable
		type Witnesser: Witnesser<
			Call = <Self as Config>::Call,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;

		/// Information about the current epoch.
		type EpochInfo: EpochInfo<
			ValidatorId = <Self as frame_system::Config>::AccountId,
			Amount = Self::TokenAmount,
		>;

		/// Something that provides the current time.
		type TimeSource: UnixTime;

		/// The minimum period before a claim should expire. The main purpose is to make sure
		/// we have some margin for error between the signature being issued and the extrinsic
		/// actually being processed.
		#[pallet::constant]
		type MinClaimTTL: Get<Duration>;
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
	pub(super) type Stakes<T: Config> =
		StorageMap<_, Identity, AccountId<T>, StakeRecord<T>, ValueQuery>;

	#[pallet::storage]
	pub(super) type PendingClaims<T: Config> = StorageMap<
		_,
		Identity,
		AccountId<T>,
		Claim<
			T::TokenAmount,
			T::Nonce,
			T::EthereumAddress,
			<T::EthereumCrypto as RuntimePublic>::Signature,
		>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub(super) type ClaimExpiries<T: Config> =
		StorageValue<_, Vec<(Duration, AccountId<T>)>, ValueQuery>;

	#[pallet::storage]
	pub(super) type Nonces<T: Config> = StorageMap<_, Identity, AccountId<T>, T::Nonce, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			Self::expire_pending_claims()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Witness that a `Staked` event was emitted by the `StakeManager` smart contract.
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
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
		/// If the account doesn't exist, an event is emitted to trigger a refund to the provided eth address.
		///
		/// **This call can only be dispatched from the configured witness origin.**
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
				debug::info!(
					"Unknown staking account id {:?}, proceeding to refund.",
					account_id
				);
				Self::deposit_event(Event::StakeRefund(account_id, amount, refund_address));
			}

			Ok(().into())
		}

		/// Get FLIP that is held for me by the system, signed by my validator key.
		///
		/// On success, emits a [ClaimSigRequested](Events::ClaimSigRequested) event. The attached claim request needs
		/// to be signed by a threshold of validators in order to be become valid.
		///
		/// An account can only have one pending claim at a time, and until this claim has been redeemed or expired,
		/// the funds wrapped up in the claim are inaccessible and are not counted towards validator auction bidding.
		///
		/// ## Error conditions:
		///
		/// - [PendingClaim](Error::PendingClaim): The account may not have a claim already pending. Any pending
		///   claim must be finalized or expired before a new claim can be requested.
		/// - [InsufficientStake](Error::InsufficientStake): The amount requested exceeds available funds.
		#[pallet::weight(10_000)]
		pub fn claim(
			origin: OriginFor<T>,
			amount: T::TokenAmount,
			address: T::EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// No new claim requests can be processed if we're currently in an auction phase.
			ensure!(
				!T::EpochInfo::is_auction_phase(),
				Error::<T>::NoClaimsDuringAuctionPhase
			);

			// If a claim already exists, return an error. The validator must either redeem their claim voucher
			// or wait until expiry before creating a new claim.
			ensure!(
				!PendingClaims::<T>::contains_key(&who),
				Error::<T>::PendingClaim
			);

			// Throw an error if the validator tries to claim too much. Otherwise decrement the stake by the
			// amount claimed.
			Self::subtract_stake(&who, amount)?;

			// Don't check for overflow here - we don't expect more than 2^32 claims.
			let nonce = Nonces::<T>::mutate(&who, |nonce| {
				*nonce += T::Nonce::one();
				*nonce
			});

			// Insert a pending claim without a signature.
			PendingClaims::<T>::insert(
				&who,
				Claim {
					amount,
					nonce,
					address,
					signature: None,
				},
			);

			// Emit the event requesting that the CFE generate the claim voucher.
			Self::deposit_event(Event::<T>::ClaimSigRequested(
				who.clone(),
				address,
				nonce,
				amount,
			));

			Ok(().into())
		}

		/// Witness that a `Claimed` event was emitted by the `StakeManager` smart contract.
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
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
		/// ## Error conditions:
		///
		/// - NoPendingClaim(Error::NoPendingClaim): The provided account does not have any claims pending.
		/// - InvalidClaimAmount(Error::InvalidClaimAmount): The amount provided does not match that of the pending
		///   claim.
		///
		/// **This call can only be dispatched from the configured witness origin.**
		#[pallet::weight(10_000)]
		pub fn claimed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			claimed_amount: T::TokenAmount,
		) -> DispatchResultWithPostInfo {
			Self::ensure_witnessed(origin)?;

			let pending_claim =
				PendingClaims::<T>::get(&account_id).ok_or(Error::<T>::NoPendingClaim)?;

			ensure!(
				claimed_amount == pending_claim.amount,
				Error::<T>::InvalidClaimAmount
			);

			PendingClaims::<T>::remove(&account_id);

			Self::deposit_event(Event::Claimed(account_id, claimed_amount));

			Ok(().into())
		}

		/// The claim signature generated by the CFE should be posted here so it can be stored on-chain.
		///
		/// Error conditions:
		/// - [SignatureAlreadyIssued](Error::SignatureAlreadyIssued): The signature was already issued.
		/// - [NoPendingClaim](Error::NoPendingClaim): There is no pending claim associated with this account.
		#[pallet::weight(10_000)]
		pub fn post_claim_signature(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			amount: T::TokenAmount,
			nonce: T::Nonce,
			address: T::EthereumAddress,
			expiry_time: Duration,
			signature: <T::EthereumCrypto as RuntimePublic>::Signature,
		) -> DispatchResultWithPostInfo {
			// TODO: we should check more than just "is this a valid account" - see clubhouse stories 471 and 473
			let who = ensure_signed(origin)?;

			let time_now = T::TimeSource::now();

			// Make sure the expiry time is sane.
			let min_ttl = T::MinClaimTTL::get();
			let _ = expiry_time
				.checked_sub(time_now)
				.and_then(|ttl| ttl.checked_sub(min_ttl))
				.ok_or(Error::<T>::InvalidExpiry)?;

			let _ =
				PendingClaims::<T>::mutate_exists(&account_id, |maybe_claim| {
					match maybe_claim.as_mut() {
						Some(claim) => match claim.signature {
							Some(_) => Err(Error::<T>::SignatureAlreadyIssued),
							None => {
								claim.signature = Some(signature.clone());
								Ok(())
							}
						},
						None => Err(Error::<T>::NoPendingClaim),
					}
				})?;

			ClaimExpiries::<T>::mutate(|expiries| {
				// We want to ensure this list remains sorted such that the head of the list contains the oldest pending
				// claim (ie. the first to be expired). This means we put the new value on the back of the list since
				// it's quite likely this is the most recent. We then run a stable sort, which is most effient when
				// values are already close to being sorted.
				// So we need to reverse the list, push the *young* value to the front, reverse it again, then sort.
				// We could have used a VecDeque here to have a FIFO queue but VecDeque doesn't support `decode_len`
				// which is used during the expiry check to avoid decoding the whole list.
				expiries.reverse();
				expiries.push((expiry_time, account_id.clone()));
				expiries.reverse();
				expiries.sort_by_key(|tup| tup.0);
			});

			Self::deposit_event(Event::ClaimSignatureIssued(
				who,
				amount,
				nonce,
				address,
				expiry_time,
				signature,
			));

			Ok(().into())
		}

		/// Signals a validator's intent to withdraw their stake after the next auction and desist from future auctions.
		/// Should only be called by accounts that are not already retired.
		///
		/// Error conditions:
		///
		/// - [AlreadyRetired](Error::AlreadyRetired): The account is already retired.
		/// - [AccountNotStaked](Error::AccountNotStaked): The account has no stake associated or doesn't exist.
		#[pallet::weight(10_000)]
		pub fn retire_account(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			Self::retire(&who)?;

			Self::deposit_event(Event::AccountRetired(who));

			Ok(().into())
		}

		/// Signals a retired validator's intent to re-activate their stake and participate in the next validator auction.
		/// Should only be called if the account is in a retired state.
		///
		/// Error conditions:
		///
		/// - [AlreadyActive](Error::AlreadyActive): The account is not in a retired state.
		/// - [AccountNotStaked](Error::AccountNotStaked): The account has no stake associated or doesn't exist.
		#[pallet::weight(10_000)]
		pub fn activate_account(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			Self::activate(&who)?;

			Self::deposit_event(Event::AccountActivated(who));

			Ok(().into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A validator has staked some FLIP on the Ethereum chain. [validator_id, stake_added, total_stake]
		Staked(AccountId<T>, T::TokenAmount, T::TokenAmount),

		/// A validator has claimed their FLIP on the Ethereum chain. [validator_id, claimed_amount]
		Claimed(AccountId<T>, T::TokenAmount),

		/// The staked amount should be refunded to the provided Ethereum address. [node_id, refund_amount, address]
		StakeRefund(AccountId<T>, T::TokenAmount, T::EthereumAddress),

		/// A claim request has been made to provided Ethereum address. [who, address, nonce, amount]
		ClaimSigRequested(AccountId<T>, T::EthereumAddress, T::Nonce, T::TokenAmount),

		/// A claim signature has been issued by the signer module. [issuer, amount, nonce, address, expiry_time, signature]
		ClaimSignatureIssued(
			AccountId<T>,
			T::TokenAmount,
			T::Nonce,
			T::EthereumAddress,
			Duration,
			<T::EthereumCrypto as RuntimePublic>::Signature,
		),

		/// An account has retired and will no longer take part in auctions. [who]
		AccountRetired(AccountId<T>),

		/// A previously retired account  has been re-activated. [who]
		AccountActivated(AccountId<T>),

		/// A claim has expired without being redeemed. [who, amount, nonce]
		ClaimExpired(AccountId<T>, T::Nonce, T::TokenAmount),
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

		/// An account tried to post a signature to an already-signed claim.
		SignatureAlreadyIssued,

		/// Can't retire an account if it's already retired.
		AlreadyRetired,

		/// Certain actions can only be performed if the account has stake associated with it.
		AccountNotStaked,

		/// Can't activate an account unless it's in a retired state.
		AlreadyActive,

		/// Invalid expiry date.
		InvalidExpiry,

		/// Cannot make a claim request while an auction is being resolved.
		NoClaimsDuringAuctionPhase,
	}
}

impl<T: Config> Pallet<T> {
	/// Adds stake to an account. Errors if the addition overflows.
	fn add_stake(
		account_id: &T::AccountId,
		amount: T::TokenAmount,
	) -> Result<T::TokenAmount, Error<T>> {
		Stakes::<T>::try_mutate(account_id, |rec| {
			rec.try_add_stake(&amount)
				.ok_or(Error::<T>::StakeOverflow)?;
			Ok(rec.stake)
		})
	}

	/// Subtracts an amount from the account's staked token. If the account has insufficient staked tokens, or if the
	/// remaining balance would be less than the bonded amount, returns an [InsufficientStake](Error::InsufficientStake)
	/// error and leaves the balance untouched.
	fn subtract_stake(
		account_id: &T::AccountId,
		amount: T::TokenAmount,
	) -> Result<T::TokenAmount, Error<T>> {
		let bond = Self::get_bond(account_id);
		Stakes::<T>::try_mutate(account_id, |rec| {
			rec.try_subtract_stake(&amount)
				.ok_or(Error::InsufficientStake)?;
			ensure!(rec.stake >= bond, Error::InsufficientStake);
			Ok(rec.stake)
		})
	}

	/// Checks that the call orginates from the witnesser by delegating to the configured implementation of
	/// `[EnsureWitnessed](cf_traits::EnsureWitnessed)`.
	fn ensure_witnessed(
		origin: OriginFor<T>,
	) -> Result<<T::EnsureWitnessed as EnsureOrigin<OriginFor<T>>>::Success, BadOrigin> {
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
		T::EpochInfo::is_validator(account)
	}

	/// Gets the bond amount for the current epoch. If the account is not a validator account, returns zero.
	fn get_bond(account: &T::AccountId) -> T::TokenAmount {
		if Self::is_validator(account) {
			T::EpochInfo::bond()
		} else {
			Zero::zero()
		}
	}

	/// Sets the `retired` flag associated with the account to true, signalling that the account no longer wishes to
	/// participate in validator auctions.
	///
	/// Returns an error if the account has already been retired, or if the account has no stake associated.
	fn retire(account: &T::AccountId) -> Result<(), Error<T>> {
		Stakes::<T>::try_mutate_exists(account, |maybe_account| match maybe_account.as_mut() {
			Some(account) => {
				if account.retired {
					Err(Error::AlreadyRetired)?;
				}
				account.retired = true;
				Ok(())
			}
			None => Err(Error::AccountNotStaked)?,
		})
	}

	/// Sets the `retired` flag associated with the account to false, signalling that the account wishes to come
	/// out of retirement.
	///
	/// Returns an error if the account is not retired, or if the account has no stake associated.
	fn activate(account: &T::AccountId) -> Result<(), Error<T>> {
		Stakes::<T>::try_mutate_exists(account, |maybe_account| match maybe_account.as_mut() {
			Some(account) => {
				if !account.retired {
					Err(Error::AlreadyActive)?;
				}
				account.retired = false;
				Ok(())
			}
			None => Err(Error::AccountNotStaked)?,
		})
	}

	/// Checks if an account has signalled their intention to retire as a validator. If the account has never staked
	/// any tokens, returns [Error::AccountNotStaked].
	pub fn is_retired(account: &T::AccountId) -> Result<bool, Error<T>> {
		Stakes::<T>::try_get(account)
			.map(|s| s.retired)
			.map_err(|_| Error::AccountNotStaked)
	}

	/// Expires any pending claims that have passed their TTL.
	pub fn expire_pending_claims() -> weights::Weight {
		let mut weight = weights::constants::ExtrinsicBaseWeight::get();

		if ClaimExpiries::<T>::decode_len().unwrap_or_default() == 0 {
			// Nothing to expire, should be pretty cheap.
			return weight;
		}

		let expiries = ClaimExpiries::<T>::get();
		let time_now = T::TimeSource::now();

		weight = weight.saturating_add(T::DbWeight::get().reads(2));

		// Expiries are sorted on insertion so we can just partition the slice.
		let expiry_cutoff = expiries.partition_point(|(expiry, _)| *expiry < time_now);

		if expiry_cutoff == 0 {
			return weight;
		}

		let (to_expire, remaining) = expiries.split_at(expiry_cutoff);

		ClaimExpiries::<T>::set(remaining.into());

		weight = weight.saturating_add(T::DbWeight::get().writes(1));

		for (_, account_id) in to_expire {
			if let Some(pending_claim) = PendingClaims::<T>::take(account_id) {
				// Notify that the claim has expired.
				Self::deposit_event(Event::<T>::ClaimExpired(
					account_id.clone(),
					pending_claim.nonce,
					pending_claim.amount,
				));

				// Re-credit the account
				let _ = Self::add_stake(&account_id, pending_claim.amount);

				// Add weight: One read/write each for deleting the claim and updating the stake.
				weight = weight
					.saturating_add(T::DbWeight::get().reads(2))
					.saturating_add(T::DbWeight::get().writes(2));
			}
		}

		weight
	}
}

/// This implementation of [pallet_cf_validator::CandidateProvider] simply returns a list of `(account_id, stake)` for
/// all non-retired accounts.
impl<T: Config> BidderProvider for Pallet<T> {
	type ValidatorId = T::AccountId;
	type Amount = T::TokenAmount;
	
	fn get_bidders() -> Vec<(Self::ValidatorId, Self::Amount)> {
		Stakes::<T>::iter()
			.filter_map(
				|(acct, StakeRecord { stake, retired })| {
					if retired {
						None
					} else {
						Some((acct, stake))
					}
				},
			)
			.collect()
	}
}
