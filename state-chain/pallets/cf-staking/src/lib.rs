#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip staking
//!
//! The responsiblities of this [pallet](Pallet) can be broken down into:
//!
//! ## Staking
//!
//! - Stake is added via the Ethereum StakeManager contract. `Staked` events emitted from this contract should trigger
//!   a witnessed call to [Pallet::staked].
//! - Any stake added in this way is considered as an implicit bid for a validator slot.
//! - If stake is added to a non-existent account, it will not be counted and will be refunded instead.
//!
//! ## Claiming
//!
//! - A claim request is made via a signed call to `claim`.
//! - The claimant who is a current validator is subject to the bond - an amount equal to the current bond is locked
//!   and cannot be claimed. For example if an account has 120 FLIP staked, and the current bond is 40 FLIP, their
//!   claimable balance would be 80 FLIP.
//! - An event is emitted with the claim parameters that need to be signed by the CFE signing module.
//! - Once a valid signature is generated, this is posted to the state chain along with the expiry timestamp.
//!
//! ## Claim expiry
//!
//! - When a claim expires, it will no longer be claimable on ethereum, so is re-credited to the originating account.
//!
//! ## Retiring
//!
//! - Accounts are considered active (not retired) by default.
//! - Any active account can make a signed call to the [`retire`](Pallet::retire_account) extrinsic to change their status to
//!   retired.
//! - Only active accounts should be included as active bidders for the auction.
//!
//! ## Account creation and deletion
//!
//! - When a staker adds stake for the first time, this creates an account.
//! - When a user claims all remaining funds, their account is deleted.
//!

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use cf_traits::{BidderProvider, EpochInfo, NonceIdentifier, NonceProvider, StakeTransfer, ThresholdSigner};
use core::time::Duration;
use frame_support::{
	debug,
	dispatch::DispatchResultWithPostInfo,
	ensure,
	error::BadOrigin,
	traits::{EnsureOrigin, Get, HandleLifetime, IsType, UnixTime},
	weights,
};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use cf_chains::{
	eth::{register_claim::RegisterClaim, SchnorrSignature, ChainflipContractCall},
	Ethereum,
};
use sp_std::prelude::*;
use sp_std::vec;

use codec::{Encode, FullCodec};
use ethabi::{Bytes, Function, Param, ParamType, StateMutability};
use sp_core::U256;
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, CheckedSub, Hash, Keccak256, UniqueSaturatedInto, Zero},
	DispatchError,
};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	pub type AccountId<T> = <T as frame_system::Config>::AccountId;

	pub type EthereumAddress = [u8; 20];
	pub type AggKeySignature = U256;

	pub type StakeAttempt<Amount> = (EthereumAddress, Amount);

	pub type FlipBalance<T> = <T as Config>::Balance;

	pub type Retired = bool;

	pub type EthTransactionHash = [u8; 32];

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: cf_traits::Chainflip {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type AccountId: AsRef<[u8; 32]> + IsType<<Self as frame_system::Config>::AccountId>;

		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ From<u128>
			+ Into<U256>;

		/// The Flip token implementation.
		type Flip: StakeTransfer<
			AccountId = <Self as frame_system::Config>::AccountId,
			Balance = Self::Balance,
		>;

		/// Information about the current epoch.
		type EpochInfo: EpochInfo<
			AccountId = <Self as frame_system::Config>::AccountId,
			Amount = FlipBalance<Self>,
		>;

		/// Something that can provide a nonce for the threshold signature.
		type NonceProvider: NonceProvider;

		/// Top-level Ethereum signing context needs to support `RegisterClaim`.
		type SigningContext: From<RegisterClaim>;

		/// Threshold signer.
		type ThresholdSigner: ThresholdSigner<Self, Context = Self::SigningContext>;

		/// Something that provides the current time.
		type TimeSource: UnixTime;

		/// The minimum period before a claim should expire. The main purpose is to make sure
		/// we have some margin for error between the signature being issued and the extrinsic
		/// actually being processed.
		#[pallet::constant]
		type MinClaimTTL: Get<Duration>;

		/// TTL for a claim from the moment of issue.
		#[pallet::constant]
		type ClaimTTL: Get<Duration>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	pub(super) type AccountRetired<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, Retired, ValueQuery>;

	#[pallet::storage]
	pub(super) type PendingClaims<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, RegisterClaim, OptionQuery>;

	#[pallet::storage]
	pub(super) type WithdrawalAddresses<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, EthereumAddress, OptionQuery>;

	#[pallet::storage]
	pub(super) type FailedStakeAttempts<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, Vec<StakeAttempt<T::Balance>>, ValueQuery>;

	#[pallet::storage]
	pub(super) type ClaimExpiries<T: Config> =
		StorageValue<_, Vec<(Duration, AccountId<T>)>, ValueQuery>;

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
		StakeRefund(AccountId<T>, FlipBalance<T>, EthereumAddress),

		/// A claim signature has been issued by the signer module. [node_id, signed_payload]
		ClaimSignatureIssued(AccountId<T>, Vec<u8>),

		/// An account has retired and will no longer take part in auctions. [who]
		AccountRetired(AccountId<T>),

		/// A previously retired account  has been re-activated. [who]
		AccountActivated(AccountId<T>),

		/// A claim has expired without being redeemed. [who, nonce, amount]
		ClaimExpired(AccountId<T>, FlipBalance<T>),

		/// A stake attempt has failed. [who, address, amount]
		FailedStakeAttempt(AccountId<T>, EthereumAddress, FlipBalance<T>),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account is not known.
		UnknownAccount,

		/// An invalid claim has been witnessed: the account has no pending claims.
		NoPendingClaim,

		/// An invalid claim has been witnessed: the amount claimed, does not match the pending claim.
		InvalidClaimDetails,

		/// The claimant tried to claim despite having a claim already pending.
		PendingClaim,

		/// An account tried to post a signature to an already-signed claim.
		SignatureAlreadyIssued,

		/// Can't retire an account if it's already retired.
		AlreadyRetired,

		/// Can't activate an account unless it's in a retired state.
		AlreadyActive,

		/// Signature posted too close to expiry time or for an already-expired claim.
		SignatureTooLate,

		/// Cannot make a claim request while an auction is being resolved.
		NoClaimsDuringAuctionPhase,

		/// Failed to encode the signed claim payload.
		ClaimEncodingFailed,

		/// A withdrawal address is provided, but the account has a different withdrawal address already associated.
		WithdrawalAddressRestricted,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Funds have been staked to an account via the StakeManager smart contract.
		///
		/// If the account doesn't exist, we create it.
		///
		/// **This call can only be dispatched from the configured witness origin.**
		#[pallet::weight(10_000)]
		pub fn staked(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			amount: FlipBalance<T>,
			withdrawal_address: Option<EthereumAddress>,
			// Required to ensure this call is unique per staking event.
			_tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			Self::ensure_witnessed(origin)?;
			if Self::check_withdrawal_address(&account_id, withdrawal_address, amount).is_err() {
				Ok(().into())
			} else {
				Self::stake_account(&account_id, amount);
				Ok(().into())
			}
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
			address: EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::do_claim(&who, amount, address)?;
			Ok(().into())
		}

		/// Get *all* FLIP that is held for me by the system, signed by my validator key.
		///
		/// Same as [claim] except calculate the maximum claimable amount and submits a claim for that.
		#[pallet::weight(10_000)]
		pub fn claim_all(
			origin: OriginFor<T>,
			address: EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let claimable = T::Flip::claimable_balance(&who);
			Self::do_claim(&who, claimable, address)?;
			Ok(().into())
		}

		/// Previously staked funds have been reclaimed.
		///
		/// Note that calling this doesn't initiate any protocol changes - the `claim` has already been authorised
		/// by validator multisig. This merely signals that the claimant has in fact redeemed their funds via the
		/// `StakeManager` contract and allows us finalise any on-chain cleanup.
		///
		/// ## Error conditions:
		///
		/// - [NoPendingClaim](Error::NoPendingClaim)
		/// - [InvalidClaimDetails](Error::InvalidClaimDetails)
		///
		/// **This call can only be dispatched from the configured witness origin.**
		#[pallet::weight(10_000)]
		pub fn claimed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			claimed_amount: FlipBalance<T>,
			// Required to ensure this call is unique per claim event.
			_tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			Self::ensure_witnessed(origin)?;

			let claim_details =
				PendingClaims::<T>::get(&account_id).ok_or(Error::<T>::NoPendingClaim)?;

			ensure!(
				claimed_amount == claim_details.amount.low_u128().unique_saturated_into(),
				Error::<T>::InvalidClaimDetails
			);

			PendingClaims::<T>::remove(&account_id);
			T::Flip::settle_claim(claimed_amount);

			if T::Flip::stakeable_balance(&account_id).is_zero() {
				frame_system::Provider::<T>::killed(&account_id).unwrap_or_else(|e| {
					// This shouldn't happen, and not much we can do if it does except fix it on a subsequent release.
					// Consequences are minor.
					debug::error!(
						"Unexpected reference count error while reaping the account {:?}: {:?}.",
						account_id,
						e
					);
				})
			}

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
			signature: SchnorrSignature,
		) -> DispatchResultWithPostInfo {
			Self::ensure_witnessed(origin)?;

			let time_now = T::TimeSource::now();

			let mut claim_details =
				PendingClaims::<T>::get(&account_id).ok_or(Error::<T>::NoPendingClaim)?;

			// TODO: Verify the signature.

			// Make sure the expiry time is still sane.
			let min_ttl = T::MinClaimTTL::get();
			let _ = claim_details.expiry
				.low_u64()
				.checked_sub(time_now.as_secs())
				.and_then(|ttl| ttl.checked_sub(min_ttl.as_secs()))
				.ok_or(Error::<T>::SignatureTooLate)?;

			// Insert the signature and notify the CFE.
			claim_details.sign(&signature);
			PendingClaims::<T>::insert(&account_id, &claim_details);

			Self::deposit_event(Event::ClaimSignatureIssued(
				account_id,
				claim_details.abi_encoded()
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
	) -> Result<<T::EnsureWitnessed as EnsureOrigin<OriginFor<T>>>::Success, BadOrigin>
	{
		T::EnsureWitnessed::ensure_origin(origin)
	}

	/// Logs an failed stake attempt
	fn log_failed_stake_attempt(
		account_id: &AccountId<T>,
		withdrawal_address: EthereumAddress,
		amount: T::Balance,
	) -> Result<(), Error<T>> {
		FailedStakeAttempts::<T>::mutate(&account_id, |staking_attempts| {
			staking_attempts.push((withdrawal_address, amount));
		});
		Self::deposit_event(Event::FailedStakeAttempt(
			account_id.clone(),
			withdrawal_address,
			amount,
		));
		Err(Error::<T>::WithdrawalAddressRestricted)?
	}

	/// Checks the withdrawal address requirements and saves the address if provided
	fn check_withdrawal_address(
		account_id: &AccountId<T>,
		withdrawal_address: Option<EthereumAddress>,
		amount: T::Balance,
	) -> Result<(), Error<T>> {
		if frame_system::Pallet::<T>::account_exists(account_id) {
			let existing_withdrawal_address = WithdrawalAddresses::<T>::get(&account_id);
			match (withdrawal_address, existing_withdrawal_address) {
				// User account exists and both addresses hold a value - the value of both addresses is different
				(Some(provided), Some(existing)) if provided != existing => {
					Self::log_failed_stake_attempt(account_id, provided, amount)?
				}
				// Only the provided address exists:
				// We only want to add a new withdrawal address if this is the first staking attempt, ie. the account doesn't exist.
				(Some(provided), None) => {
					Self::log_failed_stake_attempt(account_id, provided, amount)?
				}
				_ => (),
			}
		}
		//Save the withdrawal address if provided
		if let Some(provided) = withdrawal_address {
			WithdrawalAddresses::<T>::insert(account_id, provided);
		}
		Ok(())
	}

	/// Add stake to an account, creating the account if it doesn't exist, and activating the account if it is in retired state.
	fn stake_account(account_id: &AccountId<T>, amount: T::Balance) {
		if !frame_system::Pallet::<T>::account_exists(account_id) {
			frame_system::Provider::<T>::created(account_id).unwrap_or_else(|e| {
				// The standard impl of this in the system pallet never fails.
				debug::error!(
					"Unexpected error when creating an account upon staking: {:?}",
					e
				);
			});
		}

		let new_total = T::Flip::credit_stake(&account_id, amount);

		// Staking implicitly activates the account. Ignore the error.
		let _ = AccountRetired::<T>::mutate(&account_id, |retired| *retired = false);

		Self::deposit_event(Event::Staked(account_id.clone(), amount, new_total));
	}

	fn do_claim(
		account_id: &AccountId<T>,
		amount: T::Balance,
		address: EthereumAddress,
	) -> Result<(), DispatchError> {
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

		// Check if a return address exists - if not just go with the provided claim address
		if let Some(withdrawal_address) = WithdrawalAddresses::<T>::get(account_id) {
			// Check if the address is different from the stored address - if yes error out
			if withdrawal_address != address {
				Err(Error::<T>::WithdrawalAddressRestricted)?
			}
		}

		// Throw an error if the validator tries to claim too much. Otherwise decrement the stake by the
		// amount claimed.
		T::Flip::try_claim(account_id, amount)?;

		// Set expiry and build the claim parameters.
		let expiry = T::TimeSource::now() + T::ClaimTTL::get();
		Self::register_claim_expiry(account_id.clone(), expiry);

		let transaction = RegisterClaim::new_unsigned(
			T::NonceProvider::next_nonce(NonceIdentifier::Ethereum),
			<T as Config>::AccountId::from_ref(account_id).as_ref(),
			amount,
			&address,
			expiry.as_secs(),
		);

		// Emit a signature request.
		T::ThresholdSigner::request_transaction_signature(transaction.clone());

		// Store the claim params for later.
		PendingClaims::<T>::insert(account_id, transaction);

		Ok(())
	}

	/// Sets the `retired` flag associated with the account to true, signalling that the account no longer wishes to
	/// participate in validator auctions.
	///
	/// Returns an error if the account has already been retired, or if the account has no stake associated.
	fn retire(account_id: &AccountId<T>) -> Result<(), Error<T>> {
		AccountRetired::<T>::try_mutate_exists(account_id, |maybe_status| {
			match maybe_status.as_mut() {
				Some(retired) => {
					if *retired {
						Err(Error::AlreadyRetired)?;
					}
					*retired = true;
					Self::deposit_event(Event::AccountRetired(account_id.clone()));
					Ok(())
				}
				None => Err(Error::UnknownAccount)?,
			}
		})
	}

	/// Sets the `retired` flag associated with the account to false, signalling that the account wishes to come
	/// out of retirement.
	///
	/// Returns an error if the account is not retired, or if the account has no stake associated.
	fn activate(account_id: &AccountId<T>) -> Result<(), Error<T>> {
		AccountRetired::<T>::try_mutate_exists(account_id, |maybe_status| {
			match maybe_status.as_mut() {
				Some(retired) => {
					if !*retired {
						Err(Error::AlreadyActive)?;
					}
					*retired = false;
					Self::deposit_event(Event::AccountActivated(account_id.clone()));
					Ok(())
				}
				None => Err(Error::UnknownAccount)?,
			}
		})
	}

	/// Checks if an account has signalled their intention to retire as a validator. If the account has never staked
	/// any tokens, returns [Error::UnknownAccount].
	pub fn is_retired(account: &AccountId<T>) -> Result<bool, Error<T>> {
		AccountRetired::<T>::try_get(account).map_err(|_| Error::UnknownAccount)
	}

	/// Registers the expiry time for an account's pending claim. At the provided time, any pending claims
	/// for the account are expired.
	fn register_claim_expiry(account_id: AccountId<T>, expiry: Duration) {
		ClaimExpiries::<T>::mutate(|expiries| {
			// We want to ensure this list remains sorted such that the head of the list contains the oldest pending
			// claim (ie. the first to be expired). This means we put the new value on the back of the list since
			// it's quite likely this is the most recent. We then run a stable sort, which is most effient when
			// values are already close to being sorted.
			// So we need to reverse the list, push the *young* value to the front, reverse it again, then sort.
			// We could have used a VecDeque here to have a FIFO queue but VecDeque doesn't support `decode_len`
			// which is used during the expiry check to avoid decoding the whole list.
			expiries.reverse();
			expiries.push((expiry, account_id));
			expiries.reverse();
			expiries.sort_by_key(|tup| tup.0);
		});
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
				let claim_amount = pending_claim.amount.low_u128().into();
				// Notify that the claim has expired.
				Self::deposit_event(Event::<T>::ClaimExpired(
					account_id.clone(),
					claim_amount,
				));

				// Re-credit the account
				T::Flip::revert_claim(&account_id, claim_amount);

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
	type AccountId = T::AccountId;
	type Amount = T::Balance;

	fn get_bidders() -> Vec<(Self::AccountId, Self::Amount)> {
		AccountRetired::<T>::iter()
			.filter_map(|(acct, retired)| {
				if retired {
					None
				} else {
					let stake = T::Flip::stakeable_balance(&acct);
					Some((acct, stake))
				}
			})
			.collect()
	}
}
