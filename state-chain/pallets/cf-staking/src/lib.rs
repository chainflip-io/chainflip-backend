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

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use cf_traits::{EpochInfo, StakeTransfer};
use core::time::Duration;
use frame_support::{dispatch::DispatchResultWithPostInfo, ensure, error::BadOrigin, traits::{EnsureOrigin, Get, UnixTime}, weights};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::prelude::*;

use codec::FullCodec;
use sp_runtime::{DispatchError, traits::{AtLeast32BitUnsigned, CheckedSub, One}};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::Witnesser;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_runtime::app_crypto::RuntimePublic;

	type AccountId<T> = <T as frame_system::Config>::AccountId;

	#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
	pub struct ClaimDetails<Amount, Nonce, EthereumAddress, Signature> {
		pub(super) amount: Amount,
		pub(super) nonce: Nonce,
		pub(super) address: EthereumAddress,
		pub(super) signature: Option<Signature>,
	}

	pub type FlipBalance<T> = <T as Config>::Balance;

	pub type ClaimDetailsFor<T> = ClaimDetails<
		FlipBalance<T>,
		<T as Config>::Nonce,
		<T as Config>::EthereumAddress,
		<<T as Config>::EthereumCrypto as RuntimePublic>::Signature,
	>;

	pub type Retired = bool;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Standard Call type. We need this so we can use it as a constraint in `Witnesser`.
		type Call: From<Call<Self>> + IsType<<Self as frame_system::Config>::Call>;

		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize;
		
		/// The Flip token implementation.
		type Flip: StakeTransfer<
			AccountId=<Self as frame_system::Config>::AccountId,
			Balance=Self::Balance>;

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
			Amount = FlipBalance<Self>,
		>;

		/// Something that provides the current time.
		type TimeSource: UnixTime;

		/// The minimum period before a claim should expire. The main purpose is to make sure
		/// we have some margin for error between the signature being issued and the extrinsic
		/// actually being processed.
		#[pallet::constant]
		type MinClaimTTL: Get<Duration>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	pub(super) type AccountRetired<T: Config> =
		StorageMap<_, Identity, AccountId<T>, Retired, ValueQuery>;

	#[pallet::storage]
	pub(super) type PendingClaims<T: Config> = StorageMap<
		_,
		Identity,
		AccountId<T>,
		ClaimDetailsFor<T>,
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

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A validator has staked some FLIP on the Ethereum chain. [validator_id, stake_added, total_stake]
		Staked(AccountId<T>, FlipBalance<T>, FlipBalance<T>),

		/// A validator has claimed their FLIP on the Ethereum chain. [validator_id, claimed_amount]
		ClaimSettled(AccountId<T>, FlipBalance<T>),

		/// The staked amount should be refunded to the provided Ethereum address. [node_id, refund_amount, address]
		StakeRefund(AccountId<T>, FlipBalance<T>, T::EthereumAddress),

		/// A claim request has been made to provided Ethereum address. [who, address, nonce, amount]
		ClaimSigRequested(AccountId<T>, T::EthereumAddress, T::Nonce, FlipBalance<T>),

		/// A claim signature has been issued by the signer module. [issuer, amount, nonce, address, expiry_time, signature]
		ClaimSignatureIssued(
			AccountId<T>,
			FlipBalance<T>,
			T::Nonce,
			T::EthereumAddress,
			Duration,
			<T::EthereumCrypto as RuntimePublic>::Signature,
		),

		/// An account has retired and will no longer take part in auctions. [who]
		AccountRetired(AccountId<T>),

		/// A previously retired account  has been re-activated. [who]
		AccountActivated(AccountId<T>),

		/// A claim has expired without being redeemed. [who, nonce, amount]
		ClaimExpired(AccountId<T>, T::Nonce, FlipBalance<T>),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account is not known.
		UnknownAccount,

		/// An invalid claim has been witnessed: the account has no pending claims.
		NoPendingClaim,

		/// An invalid claim has been witnessed: the amount claimed does not match the pending claim amount.
		InvalidClaimAmount,

		/// The claimant tried to claim despite having a claim already pending.
		PendingClaim,

		/// The claimant tried to claim more funds than were available.
		ClaimOverflow,

		/// Stake amount violated the total issuance of the token.
		StakeOverflow,

		/// An account tried to post a signature to an already-signed claim.
		SignatureAlreadyIssued,

		/// Can't retire an account if it's already retired.
		AlreadyRetired,

		/// Can't activate an account unless it's in a retired state.
		AlreadyActive,

		/// Invalid expiry date.
		InvalidExpiry,

		/// Cannot make a claim request while an auction is being resolved.
		NoClaimsDuringAuctionPhase,
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
			amount: FlipBalance<T>,
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
			amount: FlipBalance<T>,
			// TODO: remove this. Leaving it here for now for compatibility
			refund_address: T::EthereumAddress,
		) -> DispatchResultWithPostInfo {
			Self::ensure_witnessed(origin)?;
			Self::stake_account(&account_id, amount);
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
		/// - [NoClaimsDuringAuctionPhase](Error::NoClaimsDuringAuctionPhase): No claims can be processed during 
		///   auction.
		/// - [InsufficientLiquidity](pallet_cf_flip::Error::InsufficientStake): The amount requested exceeds available
		///   funds.
		#[pallet::weight(10_000)]
		pub fn claim(
			origin: OriginFor<T>,
			amount: FlipBalance<T>,
			address: T::EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::do_claim(&who, amount, address)?;
			Ok(().into())
		}

		#[pallet::weight(10_000)]
		pub fn claim_all(
			origin: OriginFor<T>,
			address: T::EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let claimable = T::Flip::claimable_balance(&who);
			Self::do_claim(&who, claimable, address)?;
			Ok(().into())
		}


		/// Witness that a `Claimed` event was emitted by the `StakeManager` smart contract.
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
		#[pallet::weight(10_000)]
		pub fn witness_claimed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			claimed_amount: FlipBalance<T>,
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
			claimed_amount: FlipBalance<T>,
		) -> DispatchResultWithPostInfo {
			Self::ensure_witnessed(origin)?;

			let claim_details =
				PendingClaims::<T>::get(&account_id).ok_or(Error::<T>::NoPendingClaim)?;

			ensure!(
				claimed_amount == claim_details.amount,
				Error::<T>::InvalidClaimAmount
			);

			PendingClaims::<T>::remove(&account_id);
			T::Flip::settle_claim(claimed_amount);

			Self::deposit_event(Event::ClaimSettled(account_id, claimed_amount));

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
			amount: FlipBalance<T>,
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
		/// - [UnknownAccount](Error::UnknownAccount): The account has no stake associated or doesn't exist.
		#[pallet::weight(10_000)]
		pub fn retire_account(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			Self::retire(&who)?;

			Ok(().into())
		}

		/// Signals a retired validator's intent to re-activate their stake and participate in the next validator auction.
		/// Should only be called if the account is in a retired state.
		///
		/// Error conditions:
		///
		/// - [AlreadyActive](Error::AlreadyActive): The account is not in a retired state.
		/// - [UnknownAccount](Error::UnknownAccount): The account has no stake associated or doesn't exist.
		#[pallet::weight(10_000)]
		pub fn activate_account(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			Self::activate(&who)?;

			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub genesis_stakers: Vec<(AccountId<T>, T::Balance)>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				genesis_stakers: vec![],
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			for (staker, amount) in self.genesis_stakers.iter() {
				Pallet::<T>::stake_account(staker, *amount);
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Checks that the call orginates from the witnesser by delegating to the configured implementation of
	/// `[EnsureWitnessed](cf_traits::EnsureWitnessed)`.
	fn ensure_witnessed(
		origin: OriginFor<T>,
	) -> Result<<T::EnsureWitnessed as EnsureOrigin<OriginFor<T>>>::Success, BadOrigin> {
		T::EnsureWitnessed::ensure_origin(origin)
	}

	/// Add stake to an account, activating the account if it is in retired state.
	fn stake_account(account_id: &T::AccountId, amount: T::Balance) {
		let new_total = T::Flip::credit_stake(&account_id, amount);

		// Staking implicitly activates the account. Ignore the error.
		let _ = AccountRetired::<T>::mutate(&account_id, |retired| *retired = false);

		Self::deposit_event(Event::Staked(account_id.clone(), amount, new_total));
	}

	fn do_claim(
		account_id: &T::AccountId,
		amount: T::Balance,
		address: T::EthereumAddress) -> Result<(), DispatchError> {
		// No new claim requests can be processed if we're currently in an auction phase.
		ensure!(
			!T::EpochInfo::is_auction_phase(),
			Error::<T>::NoClaimsDuringAuctionPhase
		);

		// If a claim already exists, return an error. The validator must either redeem their claim voucher
		// or wait until expiry before creating a new claim.
		ensure!(
			!PendingClaims::<T>::contains_key(account_id),
			Error::<T>::PendingClaim
		);

		// Throw an error if the validator tries to claim too much. Otherwise decrement the stake by the
		// amount claimed.
		T::Flip::try_claim(account_id, amount)?;

		// Don't check for overflow here - we don't expect more than 2^32 claims.
		let nonce = Nonces::<T>::mutate(account_id, |nonce| {
			*nonce += T::Nonce::one();
			*nonce
		});

		// Insert a pending claim without a signature.
		PendingClaims::<T>::insert(
			account_id,
			ClaimDetails {
				amount,
				nonce,
				address,
				signature: None,
			},
		);

		// Emit the event requesting that the CFE generate the claim voucher.
		Self::deposit_event(Event::<T>::ClaimSigRequested(
			account_id.clone(),
			address,
			nonce,
			amount,
		));

		Ok(())
	}

	/// Sets the `retired` flag associated with the account to true, signalling that the account no longer wishes to
	/// participate in validator auctions.
	///
	/// Returns an error if the account has already been retired, or if the account has no stake associated.
	fn retire(account_id: &T::AccountId) -> Result<(), Error<T>> {
		AccountRetired::<T>::try_mutate_exists(account_id, |maybe_status| match maybe_status.as_mut() {
			Some(retired) => {
				if *retired {
					Err(Error::AlreadyRetired)?;
				}
				*retired = true;
				Self::deposit_event(Event::AccountRetired(account_id.clone()));
				Ok(())
			}
			None => Err(Error::UnknownAccount)?,
		})
	}

	/// Sets the `retired` flag associated with the account to false, signalling that the account wishes to come
	/// out of retirement.
	///
	/// Returns an error if the account is not retired, or if the account has no stake associated.
	fn activate(account_id: &T::AccountId) -> Result<(), Error<T>> {
		AccountRetired::<T>::try_mutate_exists(account_id, |maybe_status| match maybe_status.as_mut() {
			Some(retired) => {
				if !*retired {
					Err(Error::AlreadyActive)?;
				}
				*retired = false;
				Self::deposit_event(Event::AccountActivated(account_id.clone()));
				Ok(())
			}
			None => Err(Error::UnknownAccount)?,
		})
	}

	/// Checks if an account has signalled their intention to retire as a validator. If the account has never staked
	/// any tokens, returns [Error::UnknownAccount].
	pub fn is_retired(account: &T::AccountId) -> Result<bool, Error<T>> {
		AccountRetired::<T>::try_get(account)
			.map_err(|_| Error::UnknownAccount)
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
				T::Flip::revert_claim(&account_id, pending_claim.amount);

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
impl<T: Config> cf_traits::CandidateProvider for Pallet<T> {
	type ValidatorId = T::AccountId;
	type Amount = T::Balance;

	fn get_candidates() -> Vec<(Self::ValidatorId, Self::Amount)> {
		AccountRetired::<T>::iter()
			.filter_map(
				|(acct, retired)| {
					if retired {
						None
					} else {
						let stake = T::Flip::stakeable_balance(&acct);
						Some((acct, stake))
					}
				},
			)
			.collect()
	}
}
