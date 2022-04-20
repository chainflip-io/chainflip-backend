#![cfg_attr(not(feature = "std"), no_std)]
#![feature(array_map)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod tests;

use cf_chains::{ApiCall, RegisterClaim};
use cf_traits::{Bid, BidderProvider, EpochInfo, NonceProvider, StakeTransfer, ThresholdSigner};
use core::time::Duration;
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	ensure,
	error::BadOrigin,
	traits::{EnsureOrigin, HandleLifetime, IsType, UnixTime},
};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::prelude::*;

use sp_runtime::{
	traits::{AtLeast32BitUnsigned, CheckedSub, Zero},
	DispatchError,
};

use frame_support::pallet_prelude::Weight;
const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::{ApiCall, Ethereum};
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	pub type AccountId<T> = <T as frame_system::Config>::AccountId;

	pub type EthereumAddress = [u8; 20];

	pub type StakeAttempt<Amount> = (EthereumAddress, Amount);

	pub type FlipBalance<T> = <T as Config>::Balance;

	pub type Retired = bool;

	pub type EthTransactionHash = [u8; 32];

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: cf_traits::Chainflip {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The type containing all calls that are dispatchable from the threshold source.
		type ThresholdCallable: From<Call<Self>>;

		type StakerId: AsRef<[u8; 32]> + IsType<<Self as frame_system::Config>::AccountId>;

		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ Into<u128>
			+ From<u128>;

		/// The Flip token implementation.
		type Flip: StakeTransfer<
			AccountId = <Self as frame_system::Config>::AccountId,
			Balance = Self::Balance,
		>;

		/// Something that can provide a nonce for the threshold signature.
		type NonceProvider: NonceProvider<Ethereum>;

		/// Threshold signer.
		type ThresholdSigner: ThresholdSigner<Ethereum, Callback = Self::ThresholdCallable>;

		/// Ensure that only threshold signature consensus can post a signature.
		type EnsureThresholdSigned: EnsureOrigin<Self::Origin>;

		/// The implementation of the register claim transaction.
		type RegisterClaim: RegisterClaim<Ethereum> + Member + Parameter;

		/// Something that provides the current time.
		type TimeSource: UnixTime;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// Store the list of staked accounts and whether or not they are retired
	#[pallet::storage]
	pub type AccountRetired<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, Retired, ValueQuery>;

	#[pallet::storage]
	pub(super) type PendingClaims<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, T::RegisterClaim, OptionQuery>;

	#[pallet::storage]
	pub(super) type WithdrawalAddresses<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, EthereumAddress, OptionQuery>;

	#[pallet::storage]
	pub(super) type FailedStakeAttempts<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, Vec<StakeAttempt<T::Balance>>, ValueQuery>;

	#[pallet::storage]
	pub(super) type ClaimExpiries<T: Config> =
		StorageValue<_, Vec<(Duration, AccountId<T>)>, ValueQuery>;

	#[pallet::storage]
	pub type MinimumStake<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	/// TTL for a claim from the moment of issue.
	#[pallet::storage]
	pub type ClaimTTL<T: Config> = StorageValue<_, Duration, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			Self::expire_pending_claims()
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A validator has staked some FLIP on the Ethereum chain. \[account_id, stake_added,
		/// total_stake\]
		Staked(AccountId<T>, FlipBalance<T>, FlipBalance<T>),

		/// A validator has claimed their FLIP on the Ethereum chain. \[account_id,
		/// claimed_amount\]
		ClaimSettled(AccountId<T>, FlipBalance<T>),

		/// A claim signature has been issued by the signer module. \[account_id, signed_payload\]
		ClaimSignatureIssued(AccountId<T>, Vec<u8>),

		/// An account has retired and will no longer take part in auctions. \[account_id\]
		AccountRetired(AccountId<T>),

		/// A previously retired account  has been re-activated. \[account_id\]
		AccountActivated(AccountId<T>),

		/// A claim has expired without being redeemed. \[account_id, nonce, amount\]
		ClaimExpired(AccountId<T>, FlipBalance<T>),

		/// A stake attempt has failed. \[account_id, eth_address, amount\]
		FailedStakeAttempt(AccountId<T>, EthereumAddress, FlipBalance<T>),

		/// The minimum stake required has been updated. \[new_amount\]
		MinimumStakeUpdated(T::Balance),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account is not known.
		UnknownAccount,

		/// An invalid claim has been witnessed: the account has no pending claims.
		NoPendingClaim,

		/// An invalid claim has been witnessed: the amount claimed, does not match the pending
		/// claim.
		InvalidClaimDetails,

		/// The claimant tried to claim despite having a claim already pending.
		PendingClaim,

		/// An account tried to post a signature to an already-signed claim.
		SignatureAlreadyIssued,

		/// Can't retire an account if it's already retired.
		AlreadyRetired,

		/// Can't activate an account unless it's in a retired state.
		AlreadyActive,

		/// We are in the auction phase
		AuctionPhase,

		/// Failed to encode the signed claim payload.
		ClaimEncodingFailed,

		/// A withdrawal address is provided, but the account has a different withdrawal address
		/// already associated.
		WithdrawalAddressRestricted,

		/// An invalid claim has been made
		InvalidClaim,

		/// Below the minimum stake
		BelowMinimumStake,

		/// The claim signature could not be found.
		SignatureNotReady,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// **This call can only be dispatched from the configured witness origin.**
		///
		/// Funds have been staked to an account via the StakeManager Smart Contract.
		///
		/// If the account doesn't exist, we create it.
		///
		/// ## Events
		///
		/// - [FailedStakeAttempt](Event::FailedStakeAttempt)
		/// - [Staked](Event::Staked)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::staked())]
		pub fn staked(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			amount: FlipBalance<T>,
			withdrawal_address: EthereumAddress,
			// Required to ensure this call is unique per staking event.
			_tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			Self::ensure_witnessed(origin)?;
			if Self::check_withdrawal_address(&account_id, withdrawal_address, amount).is_ok() {
				Self::stake_account(&account_id, amount);
			}
			Ok(().into())
		}

		/// Get FLIP that is held for me by the system, signed by my validator key.
		///
		/// On success, the implementation of [ThresholdSigner] should emit an event. The attached
		/// claim request needs to be signed by a threshold of validators in order to produce valid
		/// data that can be submitted to the StakeManager Smart Contract.
		///
		/// An account can only have one pending claim at a time, and until this claim has been
		/// redeemed or expired, the funds wrapped up in the claim are inaccessible and are not
		/// counted towards a Validator's Auction Bid.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - [PendingClaim](Error::PendingClaim)
		/// - [AuctionPhase](Error::AuctionPhase)
		/// - [WithdrawalAddressRestricted](Error::WithdrawalAddressRestricted)
		///
		/// ## Dependencies
		///
		/// - [ThresholdSigner]
		/// - [StakeTransfer]
		#[pallet::weight(T::WeightInfo::claim())]
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
		/// Same as [claim](Self::claim) except first calculates the maximum claimable amount.
		///
		/// ## Events
		///
		/// - See [claim](Self::claim)
		///
		/// ## Errors
		///
		/// - See [claim](Self::claim)
		#[pallet::weight(T::WeightInfo::claim_all())]
		pub fn claim_all(
			origin: OriginFor<T>,
			address: EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let claimable = T::Flip::claimable_balance(&who);
			Self::do_claim(&who, claimable, address)?;
			Ok(().into())
		}

		/// **This call can only be dispatched from the configured witness origin.**
		///
		/// Previously staked funds have been reclaimed.
		///
		/// Note that calling this doesn't initiate any protocol changes - the `claim` has already
		/// been authorised by validator multisig. This merely signals that the claimant has in fact
		/// redeemed their funds via the StakeManager Smart Contract and allows us to finalise any
		/// on-chain cleanup.
		///
		/// ## Events
		///
		/// - [ClaimSettled](Event::ClaimSettled)
		///
		/// ## Errors
		///
		/// - [NoPendingClaim](Error::NoPendingClaim)
		/// - [InvalidClaimDetails](Error::InvalidClaimDetails)
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::claimed())]
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
				claimed_amount == claim_details.amount().into(),
				Error::<T>::InvalidClaimDetails
			);

			PendingClaims::<T>::remove(&account_id);
			// Remove claim expiry for this account.  We assume one claim per account here.
			// `retain` those elements in their positions and removing the account that has claimed
			let mut expiries = ClaimExpiries::<T>::get();
			expiries.retain(|(_, expiry_account_id)| expiry_account_id != &account_id);
			ClaimExpiries::<T>::set(expiries);

			T::Flip::settle_claim(claimed_amount);

			if T::Flip::stakeable_balance(&account_id).is_zero() {
				frame_system::Provider::<T>::killed(&account_id).unwrap_or_else(|e| {
					// This shouldn't happen, and not much we can do if it does except fix it on a
					// subsequent release. Consequences are minor.
					log::error!(
						"Unexpected reference count error while reaping the account {:?}: {:?}.",
						account_id,
						e
					);
				})
			}

			Self::deposit_event(Event::ClaimSettled(account_id, claimed_amount));

			Ok(().into())
		}

		/// **This call can only be dispatched from the configured witness origin.**
		///
		/// The claim signature generated by the CFE should be posted here so it can be stored
		/// on-chain. The Validators are no longer responsible for the execution of this claim,
		/// since the claiming user is expected to read the signature from claim storage, and use it
		/// to compose a transaction to the StakeManager Smart Contract, which they will then
		/// broadcast themselves.
		///
		/// ## Events
		///
		/// - [ClaimSignatureIssued](Event::ClaimSignatureIssued)
		///
		/// ## Errors
		///
		/// - [NoPendingClaim](Error::NoPendingClaim)
		/// - [SignatureAlreadyIssued](Error::SignatureAlreadyIssued)
		/// - [InvalidClaimDetails](Error::InvalidClaimDetails)
		/// - [SignatureTooLate](Error::SignatureTooLate) the claim.
		#[pallet::weight(T::WeightInfo::post_claim_signature())]
		pub fn post_claim_signature(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			signature_request_id: <T::ThresholdSigner as ThresholdSigner<Ethereum>>::RequestId,
		) -> DispatchResultWithPostInfo {
			T::EnsureThresholdSigned::ensure_origin(origin)?;

			let signature = T::ThresholdSigner::signature_result(signature_request_id)
				.ready_or_else(|r| {
					// This should never happen unless there is a mistake in the implementation.
					log::error!("Callback triggered with no signature. Signature status {:?}", r);
					Error::<T>::SignatureNotReady
				})?;

			let claim_details_signed = PendingClaims::<T>::get(&account_id)
				.ok_or(Error::<T>::NoPendingClaim)?
				.signed(&signature);

			// Notify the claimant.
			Self::deposit_event(Event::ClaimSignatureIssued(
				account_id.clone(),
				claim_details_signed.encoded(),
			));

			// Store the signature.
			PendingClaims::<T>::insert(&account_id, &claim_details_signed);

			Ok(().into())
		}

		/// Signals a validator's intent to withdraw their stake after the next auction and desist
		/// from future auctions. Should only be called by accounts that are not already retired.
		///
		/// ## Events
		///
		/// - [AccountRetired](Event::AccountRetired)
		///
		/// ## Errors
		///
		/// - [AlreadyRetired](Error::AlreadyRetired)
		/// - [UnknownAccount](Error::UnknownAccount)
		#[pallet::weight(T::WeightInfo::retire_account())]
		pub fn retire_account(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::retire(&who)?;
			Ok(().into())
		}

		/// Signals a retired validator's intent to re-activate their stake and participate in the
		/// next validator auction. Should only be called if the account is in a retired state.
		///
		/// ## Events
		///
		/// - [AccountActivated](Event::AccountActivated)
		///
		/// ## Errors
		///
		/// - [AlreadyActive](Error::AlreadyActive)
		/// - [UnknownAccount](Error::UnknownAccount)
		#[pallet::weight(T::WeightInfo::activate_account())]
		pub fn activate_account(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::activate(&who)?;
			Ok(().into())
		}

		/// Updates the minimum stake required for an account, the extrinsic is gated with
		/// governance
		///
		/// ## Events
		///
		/// - [MinimumStakeUpdated](Event::MinimumStakeUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(10_000)]
		pub fn update_minimum_stake(
			origin: OriginFor<T>,
			minimum_stake: T::Balance,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			MinimumStake::<T>::put(minimum_stake);
			Self::deposit_event(Event::MinimumStakeUpdated(minimum_stake));
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub genesis_stakers: Vec<(AccountId<T>, T::Balance)>,
		pub minimum_stake: T::Balance,
		pub claim_ttl: Duration,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { genesis_stakers: vec![], ..Default::default() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			MinimumStake::<T>::set(self.minimum_stake);
			ClaimTTL::<T>::set(self.claim_ttl);
			for (staker, amount) in self.genesis_stakers.iter() {
				Pallet::<T>::stake_account(staker, *amount);
				match Pallet::<T>::activate(staker) {
					Ok(_) => {
						// Activated account successful.
						log::info!("Activated genesis account {:?}", staker);
					},
					Err(Error::AlreadyActive) => {
						// If the account is already active, we don't need to do anything.
						log::warn!("Account already activated {:?}", staker);
					},
					Err(e) => {
						// This should never happen unless there is a mistake in the implementation.
						log::error!("Unexpected error while activating account {:?}", e);
					},
				}
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Checks that the call orginates from the witnesser by delegating to the configured
	/// implementation of [EnsureOrigin].
	fn ensure_witnessed(
		origin: OriginFor<T>,
	) -> Result<<T::EnsureWitnessed as EnsureOrigin<OriginFor<T>>>::Success, BadOrigin> {
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
		Err(Error::<T>::WithdrawalAddressRestricted)
	}

	/// Checks the withdrawal address requirements and saves the address if provided
	fn check_withdrawal_address(
		account_id: &AccountId<T>,
		withdrawal_address: EthereumAddress,
		amount: T::Balance,
	) -> Result<(), Error<T>> {
		if frame_system::Pallet::<T>::account_exists(account_id) {
			let existing_withdrawal_address = WithdrawalAddresses::<T>::get(&account_id);
			match existing_withdrawal_address {
				// User account exists and both addresses hold a value - the value of both addresses
				// is different and not null
				Some(existing)
					if withdrawal_address != existing && withdrawal_address != ETH_ZERO_ADDRESS =>
					Self::log_failed_stake_attempt(account_id, withdrawal_address, amount)?,
				// Only the provided address exists:
				// We only want to add a new withdrawal address if this is the first staking
				// attempt, ie. the account doesn't exist.
				None if withdrawal_address != ETH_ZERO_ADDRESS =>
					Self::log_failed_stake_attempt(account_id, withdrawal_address, amount)?,
				_ => (),
			}
		}
		// Save the withdrawal address if provided
		if withdrawal_address != ETH_ZERO_ADDRESS {
			WithdrawalAddresses::<T>::insert(account_id, withdrawal_address);
		}
		Ok(())
	}

	/// Add stake to an account, creating the account if it doesn't exist, and activating the
	/// account if it is in retired state.
	fn stake_account(account_id: &AccountId<T>, amount: T::Balance) {
		if !frame_system::Pallet::<T>::account_exists(account_id) {
			// Creates an account
			let _ = frame_system::Provider::<T>::created(account_id);
			AccountRetired::<T>::insert(&account_id, true);
		}

		let new_total = T::Flip::credit_stake(account_id, amount);

		Self::deposit_event(Event::Staked(account_id.clone(), amount, new_total));
	}

	fn do_claim(
		account_id: &AccountId<T>,
		amount: T::Balance,
		address: EthereumAddress,
	) -> Result<(), DispatchError> {
		// Ensure we are claiming something
		ensure!(amount > Zero::zero(), Error::<T>::InvalidClaim);

		// Ensure that we're not claiming to the zero address
		ensure!(address != ETH_ZERO_ADDRESS, Error::<T>::InvalidClaim);

		// No new claim requests can be processed if we're currently in an auction phase.
		ensure!(!T::EpochInfo::is_auction_phase(), Error::<T>::AuctionPhase);

		// If a claim already exists, return an error. The validator must either redeem their claim
		// voucher or wait until expiry before creating a new claim.
		ensure!(!PendingClaims::<T>::contains_key(account_id), Error::<T>::PendingClaim);

		// Check if a return address exists - if not just go with the provided claim address
		if let Some(withdrawal_address) = WithdrawalAddresses::<T>::get(account_id) {
			// Check if the address is different from the stored address - if yes error out
			if withdrawal_address != address {
				return Err(Error::<T>::WithdrawalAddressRestricted.into())
			}
		}

		// Calculate the maximum that would remain after this claim and ensure it won't be less than
		// the system's minimum stake.  N.B. This would be caught in `StakeTranser::try_claim()` but
		// this will need to be handled in a refactor of that trait(?)
		let remaining = T::Flip::stakeable_balance(account_id)
			.checked_sub(&amount)
			.ok_or(Error::<T>::InvalidClaim)?;

		ensure!(
			remaining == Zero::zero() || remaining >= MinimumStake::<T>::get(),
			DispatchError::from(Error::<T>::BelowMinimumStake)
		);

		// Throw an error if the validator tries to claim too much. Otherwise decrement the stake by
		// the amount claimed.
		T::Flip::try_claim(account_id, amount)?;

		// Set expiry and build the claim parameters.
		let expiry = T::TimeSource::now() + ClaimTTL::<T>::get();
		Self::register_claim_expiry(account_id.clone(), expiry);

		let call = T::RegisterClaim::new_unsigned(
			T::NonceProvider::next_nonce(),
			<T as Config>::StakerId::from_ref(account_id).as_ref(),
			amount.into(),
			&address,
			expiry.as_secs(),
		);

		// Emit a threshold signature request.
		T::ThresholdSigner::request_signature_with_callback(
			call.threshold_signature_payload(),
			|id| Call::<T>::post_claim_signature(account_id.clone(), id).into(),
		);

		// Store the claim params for later.
		PendingClaims::<T>::insert(account_id, call);

		Ok(())
	}

	/// Sets the `retired` flag associated with the account to true, signalling that the account no
	/// longer wishes to participate in validator auctions.
	///
	/// Returns an error if the account has already been retired, or if the account has no stake
	/// associated.
	fn retire(account_id: &AccountId<T>) -> Result<(), Error<T>> {
		AccountRetired::<T>::try_mutate_exists(account_id, |maybe_status| {
			match maybe_status.as_mut() {
				Some(retired) => {
					if *retired {
						return Err(Error::AlreadyRetired)
					}
					*retired = true;
					Self::deposit_event(Event::AccountRetired(account_id.clone()));
					Ok(())
				},
				None => Err(Error::UnknownAccount),
			}
		})
	}

	/// Sets the `retired` flag associated with the account to false, signalling that the account
	/// wishes to come out of retirement.
	///
	/// Returns an error if the account is not retired, or if the account has no stake associated.
	fn activate(account_id: &AccountId<T>) -> Result<(), Error<T>> {
		AccountRetired::<T>::try_mutate_exists(account_id, |maybe_status| {
			match maybe_status.as_mut() {
				Some(retired) => {
					if !*retired {
						return Err(Error::AlreadyActive)
					}
					*retired = false;
					Self::deposit_event(Event::AccountActivated(account_id.clone()));
					Ok(())
				},
				None => Err(Error::UnknownAccount),
			}
		})
	}

	/// Checks if an account has signalled their intention to retire as a validator. If the account
	/// has never staked any tokens, returns [Error::UnknownAccount].
	pub fn is_retired(account: &AccountId<T>) -> Result<bool, Error<T>> {
		AccountRetired::<T>::try_get(account).map_err(|_| Error::UnknownAccount)
	}

	/// Registers the expiry time for an account's pending claim. At the provided time, any pending
	/// claims for the account are expired.
	fn register_claim_expiry(account_id: AccountId<T>, expiry: Duration) {
		ClaimExpiries::<T>::mutate(|expiries| {
			// We want to ensure this list remains sorted such that the head of the list contains
			// the oldest pending claim (ie. the first to be expired). This means we put the new
			// value on the back of the list since it's quite likely this is the most recent. We
			// then run a stable sort, which is most effient when values are already close to being
			// sorted. So we need to reverse the list, push the *young* value to the front, reverse
			// it again, then sort. We could have used a VecDeque here to have a FIFO queue but
			// VecDeque doesn't support `decode_len` which is used during the expiry check to avoid
			// decoding the whole list.
			expiries.reverse();
			expiries.push((expiry, account_id));
			expiries.reverse();
			expiries.sort_by_key(|tup| tup.0);
		});
	}

	/// Expires any pending claims that have passed their TTL.
	pub fn expire_pending_claims() -> Weight {
		if ClaimExpiries::<T>::decode_len().unwrap_or_default() == 0 {
			// Nothing to expire, should be pretty cheap.
			return T::WeightInfo::on_initialize_best_case()
		}

		let expiries = ClaimExpiries::<T>::get();
		// Expiries are sorted on insertion so we can just partition the slice.
		let expiry_cutoff = expiries.partition_point(|(expiry, _)| *expiry < T::TimeSource::now());

		let (to_expire, remaining) = expiries.split_at(expiry_cutoff);

		ClaimExpiries::<T>::set(remaining.into());

		for (_, account_id) in to_expire {
			if let Some(pending_claim) = PendingClaims::<T>::take(account_id) {
				let claim_amount = pending_claim.amount().into();
				// Notify that the claim has expired.
				Self::deposit_event(Event::<T>::ClaimExpired(account_id.clone(), claim_amount));

				// Re-credit the account
				T::Flip::revert_claim(account_id, claim_amount);
			}
		}

		T::WeightInfo::on_initialize_worst_case(to_expire.len() as u32)
	}
}

impl<T: Config> BidderProvider for Pallet<T> {
	type ValidatorId = <T as frame_system::Config>::AccountId;
	type Amount = T::Balance;

	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>> {
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
