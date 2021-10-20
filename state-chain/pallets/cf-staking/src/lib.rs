#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extended_key_value_attributes)]
#![doc = include_str!("../README.md")]

#[cfg(test)]
mod mock;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
use core::convert::TryInto;
pub use weights::WeightInfo;

#[cfg(test)]
mod tests;

use cf_traits::{BidderProvider, EpochInfo, StakeTransfer};
use core::time::Duration;
use frame_support::{
	debug,
	dispatch::DispatchResultWithPostInfo,
	ensure,
	error::BadOrigin,
	traits::{EnsureOrigin, Get, HandleLifetime, UnixTime},
};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::prelude::*;
use sp_std::vec;

use codec::{Encode, FullCodec};
use ethabi::{Bytes, Function, Param, ParamType, StateMutability};
use sp_core::U256;
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, CheckedSub, Hash, Keccak256, UniqueSaturatedInto, Zero},
	DispatchError,
};

use frame_support::pallet_prelude::Weight;

const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	type AccountId<T> = <T as frame_system::Config>::AccountId;

	pub type EthereumAddress = [u8; 20];
	pub type AggKeySignature = U256;

	pub type StakeAttempt<Amount> = (EthereumAddress, Amount);

	/// Details of a claim request required to build the call to StakeManager's 'requestClaim' function.
	#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
	pub struct ClaimDetails<Amount, Nonce> {
		pub(super) msg_hash: Option<U256>,
		pub(super) nonce: Nonce,
		pub(super) signature: Option<AggKeySignature>,
		pub(super) amount: Amount,
		pub(super) address: EthereumAddress,
		pub(super) expiry: Duration,
	}

	pub type FlipBalance<T> = <T as Config>::Balance;

	pub type ClaimDetailsFor<T> = ClaimDetails<FlipBalance<T>, <T as Config>::Nonce>;

	pub type Retired = bool;

	pub type EthTransactionHash = [u8; 32];

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ Into<U256>;

		/// The Flip token implementation.
		type Flip: StakeTransfer<
			AccountId = <Self as frame_system::Config>::AccountId,
			Balance = Self::Balance,
		>;

		/// A Nonce type to be used for claim nonces.
		type Nonce: Member
			+ FullCodec
			+ Copy
			+ Default
			+ AtLeast32BitUnsigned
			+ MaybeSerializeDeserialize
			+ CheckedSub
			+ Into<U256>;

		/// Provides an origin check for witness transactions.
		type EnsureWitnessed: EnsureOrigin<Self::Origin>;

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

		/// TTL for a claim from the moment of issue.
		#[pallet::constant]
		type ClaimTTL: Get<Duration>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// Store the list of staked accounts and whether or not they are retired
	#[pallet::storage]
	pub(super) type AccountRetired<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, Retired, ValueQuery>;

	#[pallet::storage]
	pub(super) type PendingClaims<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, ClaimDetailsFor<T>, OptionQuery>;

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
		/// A validator has staked some FLIP on the Ethereum chain. \[account_id, stake_added, total_stake\]
		Staked(AccountId<T>, FlipBalance<T>, FlipBalance<T>),

		/// A validator has claimed their FLIP on the Ethereum chain. \[account_id, claimed_amount\]
		ClaimSettled(AccountId<T>, FlipBalance<T>),

		/// The staked amount should be refunded to the provided Ethereum address. \[account_id, refund_amount, eth_address\]
		StakeRefund(AccountId<T>, FlipBalance<T>, EthereumAddress),

		/// A claim request has been validated and needs to be signed. \[account_id, msg_hash\]
		ClaimSigRequested(AccountId<T>, U256),

		/// A claim signature has been issued by the signer module. \[msg_hash, nonce, sig, account_id, amount, eth_address, expiry_timestamp\]
		ClaimSignatureIssued(
			U256,
			T::Nonce,
			AggKeySignature,
			AccountId<T>,
			FlipBalance<T>,
			EthereumAddress,
			Duration,
		),

		/// An account has retired and will no longer take part in auctions. \[account_id\]
		AccountRetired(AccountId<T>),

		/// A previously retired account  has been re-activated. \[account_id\]
		AccountActivated(AccountId<T>),

		/// A claim has expired without being redeemed. \[account_id, nonce, amount\]
		ClaimExpired(AccountId<T>, T::Nonce, FlipBalance<T>),

		/// A stake attempt has failed. \[account_id, eth_address, amount\]
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

		/// Failed to encode the claim transaction.
		EthEncodingFailed,

		/// A withdrawal address is provided, but the account has a different withdrawal address already associated.
		WithdrawalAddressRestricted,
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
		/// - [FailedStakeAttempt](Event::FailedStakeAttempt): The stake was rejected. This happens if the
		///   withdrawal address provided does not match the withdrawal address we already have in storage for
		///   the account_id
		/// - [Staked](Event::Staked): The stake has been successfully registered.
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin): The extrinsic was not dispatched by the witness origin.
		#[pallet::weight(T::WeightInfo::staked())]
		pub fn staked(
			origin: OriginFor<T>,
			account_id: T::AccountId,
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
		/// On success, emits a [ClaimSigRequested](Event::ClaimSigRequested) event. The attached claim request needs
		/// to be signed by a threshold of validators in order to produce valid data that can be submitted to the
		/// StakeManager Smart Contract.
		///
		/// An account can only have one pending claim at a time, and until this claim has been redeemed or expired,
		/// the funds wrapped up in the claim are inaccessible and are not counted towards a Validator's Auction Bid.
		///
		/// ## Events
		///
		/// - [ClaimSigRequested](Event::ClaimSigRequested): We successfully requested a signature over the claim details.
		///
		/// ## Errors
		///
		/// - [PendingClaim](Error::PendingClaim): The account may not have a claim already pending. Any pending
		///   claim must be finalized or expired before a new claim can be requested.
		/// - [NoClaimsDuringAuctionPhase](Error::NoClaimsDuringAuctionPhase): No claims can be processed during
		///   auction.
		/// - [WithdrawalAddressRestricted](Error::WithdrawalAddressRestricted): The withdrawal address specified
		///   does not match the one on file, and the one on file is not the ETH_ZERO_ADDRESS
		/// - [EthEncodingFailed](Error::EthEncodingFailed): The claim request could not be encoded as a valid
		///   Ethereum transaction.
		///
		/// ## Dependencies
		///
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
		/// Note that calling this doesn't initiate any protocol changes - the `claim` has already been authorised
		/// by validator multisig. This merely signals that the claimant has in fact redeemed their funds via the
		/// StakeManager Smart Contract and allows us to finalise any on-chain cleanup.
		///
		/// ## Events
		///
		/// - [ClaimSettled](Event::ClaimSettled): The claim was successfully settled and balances are resolved.
		///
		/// ## Errors
		///
		/// - [NoPendingClaim](Error::NoPendingClaim): There is no pending claim associated with this account.
		/// - [InvalidClaimDetails](Error::InvalidClaimDetails): Claimed amount is not the same as witnessed amount.
		/// - [BadOrigin](frame_support::error::BadOrigin): The extrinsic was not dispatched by the witness origin.
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
				claimed_amount == claim_details.amount,
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

		/// **This call can only be dispatched from the configured witness origin.**
		///
		/// The claim signature generated by the CFE should be posted here so it can be stored on-chain. The
		/// Validators are no longer responsible for the execution of this claim, since the claiming user is
		/// expected to read the signature from claim storage, and use it to compose a transaction to the
		/// StakeManager Smart Contract, which they will then broadcast themselves.
		///
		/// ## Events
		///
		/// - [ClaimSignatureIssued](Event::ClaimSignatureIssued): Successfully issued a claim signature
		///   signed by the Validator quorum.
		///
		/// ## Errors
		///
		/// - [NoPendingClaim](Error::NoPendingClaim): There is no pending claim associated with this account.
		/// - [SignatureAlreadyIssued](Error::SignatureAlreadyIssued): The signature was already issued.
		/// - [InvalidClaimDetails](Error::InvalidClaimDetails): The claim is not valid.
		/// - [SignatureTooLate](Error::SignatureTooLate): We're calling this function after the expiration of
		///   the claim.
		#[pallet::weight(T::WeightInfo::post_claim_signature())]
		pub fn post_claim_signature(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			msg_hash: U256,
			signature: AggKeySignature,
		) -> DispatchResultWithPostInfo {
			Self::ensure_witnessed(origin)?;

			let time_now = T::TimeSource::now();

			let mut claim_details =
				PendingClaims::<T>::get(&account_id).ok_or(Error::<T>::NoPendingClaim)?;

			ensure!(
				claim_details.signature.is_none(),
				Error::<T>::SignatureAlreadyIssued
			);
			ensure!(
				claim_details.msg_hash == Some(msg_hash),
				Error::<T>::InvalidClaimDetails
			);

			// Make sure the expiry time is still sane.
			let min_ttl = T::MinClaimTTL::get();
			claim_details
				.expiry
				.checked_sub(time_now)
				.and_then(|ttl| ttl.checked_sub(min_ttl))
				.ok_or(Error::<T>::SignatureTooLate)?;

			// Insert the signature and notify the CFE.
			claim_details.signature = Some(signature.clone());

			PendingClaims::<T>::insert(&account_id, claim_details.clone());

			Self::deposit_event(Event::ClaimSignatureIssued(
				msg_hash,
				claim_details.nonce,
				signature,
				account_id,
				claim_details.amount,
				claim_details.address,
				claim_details.expiry,
			));

			Ok(().into())
		}

		/// Signals a validator's intent to withdraw their stake after the next auction and desist from future auctions.
		/// Should only be called by accounts that are not already retired.
		///
		/// ## Events
		///
		/// - [AccountRetired](Event::AccountRetired): The account has successfully retired from the auction.
		///
		/// ## Errors
		///
		/// - [AlreadyRetired](Error::AlreadyRetired): The account is already retired.
		/// - [UnknownAccount](Error::UnknownAccount): The account has no stake associated or doesn't exist.
		#[pallet::weight(T::WeightInfo::retire_account())]
		pub fn retire_account(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::retire(&who)?;
			Ok(().into())
		}

		/// Signals a retired validator's intent to re-activate their stake and participate in the next validator auction.
		/// Should only be called if the account is in a retired state.
		///
		/// ## Events
		///
		/// - [AccountActivated](Event::AccountActivated): The account has successfully re-activated and will
		///   be re-considrered for future auctions.
		///
		/// ## Errors
		///
		/// - [AlreadyActive](Error::AlreadyActive): The account is not in a retired state.
		/// - [UnknownAccount](Error::UnknownAccount): The account has no stake associated or doesn't exist.
		#[pallet::weight(T::WeightInfo::activate_account())]
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
	/// [EnsureWitnessed](Config::EnsureWitnessed).
	fn ensure_witnessed(
		origin: OriginFor<T>,
	) -> Result<<T::EnsureWitnessed as EnsureOrigin<OriginFor<T>>>::Success, BadOrigin> {
		T::EnsureWitnessed::ensure_origin(origin)
	}

	/// Logs an failed stake attempt
	fn log_failed_stake_attempt(
		account_id: &T::AccountId,
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
		account_id: &T::AccountId,
		withdrawal_address: EthereumAddress,
		amount: T::Balance,
	) -> Result<(), Error<T>> {
		if frame_system::Pallet::<T>::account_exists(account_id) {
			let existing_withdrawal_address = WithdrawalAddresses::<T>::get(&account_id);
			match existing_withdrawal_address {
				// User account exists and both addresses hold a value - the value of both addresses is different
				// and not null
				Some(existing)
					if withdrawal_address != existing && withdrawal_address != ETH_ZERO_ADDRESS =>
				{
					Self::log_failed_stake_attempt(account_id, withdrawal_address, amount)?
				}
				// Only the provided address exists:
				// We only want to add a new withdrawal address if this is the first staking attempt, ie. the account doesn't exist.
				None if withdrawal_address != ETH_ZERO_ADDRESS => {
					Self::log_failed_stake_attempt(account_id, withdrawal_address, amount)?
				}
				_ => (),
			}
		}
		// Save the withdrawal address if provided
		if withdrawal_address != ETH_ZERO_ADDRESS {
			WithdrawalAddresses::<T>::insert(account_id, withdrawal_address);
		}
		Ok(())
	}

	/// Add stake to an account, creating the account if it doesn't exist, and activating the account if it is in retired state.
	fn stake_account(account_id: &T::AccountId, amount: T::Balance) {
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
		account_id: &T::AccountId,
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

		// Try to generate a nonce
		let nonce = Self::generate_nonce();

		// Set expiry and build the claim parameters.
		let expiry = T::TimeSource::now() + T::ClaimTTL::get();
		Self::register_claim_expiry(account_id.clone(), expiry);

		let mut details = ClaimDetails {
			msg_hash: None,
			nonce,
			signature: None,
			amount,
			address,
			expiry,
		};

		// Compute the message hash to be signed.
		match Self::try_encode_claim_request(account_id, &details) {
			Ok(payload) => {
				let msg_hash: U256 = Keccak256::hash(&payload[..]).as_bytes().into();
				details.msg_hash = Some(msg_hash);

				// Store the params for later.
				PendingClaims::<T>::insert(account_id, details);

				// Emit the event requesting that the CFE generate the claim voucher.
				Self::deposit_event(Event::<T>::ClaimSigRequested(account_id.clone(), msg_hash));

				Ok(())
			}
			Err(_) => Err(Error::<T>::EthEncodingFailed.into()),
		}
	}

	/// Generates a unique nonce for the StakeManager contract.
	fn generate_nonce() -> T::Nonce {
		// For now, we expect the nonce to be an u64 to stay compatible with the CFE
		let u64_nonce = T::TimeSource::now().as_nanos() as u64;
		u64_nonce.unique_saturated_into()
	}

	/// Sets the `retired` flag associated with the account to true, signalling that the account no longer wishes to
	/// participate in validator auctions.
	///
	/// Returns an error if the account has already been retired, or if the account has no stake associated.
	fn retire(account_id: &T::AccountId) -> Result<(), Error<T>> {
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

	fn try_encode_claim_request(
		account_id: &T::AccountId,
		claim_details: &ClaimDetailsFor<T>,
	) -> ethabi::Result<Bytes> {
		use ethabi::{Address, Token};
		let register_claim = Function::new(
			"registerClaim",
			vec![
				Param::new(
					"sigData",
					ParamType::Tuple(vec![
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Uint(256),
						ParamType::Address,
					]),
				),
				Param::new("nodeID", ParamType::FixedBytes(32)),
				Param::new("amount", ParamType::Uint(256)),
				Param::new("staker", ParamType::Address),
				Param::new("expiryTime", ParamType::Uint(48)),
			],
			vec![],
			false,
			StateMutability::NonPayable,
		);

		register_claim.encode_input(&vec![
			// sigData: SigData(uint, uint, uint)
			Token::Tuple(vec![
				Token::Uint(ethabi::Uint::zero()),
				Token::Uint(ethabi::Uint::zero()),
				Token::Uint(ethabi::ethereum_types::U256(claim_details.nonce.into().0)),
				Token::Address(Address::from(claim_details.address)),
			]),
			// nodeId: bytes32
			Token::FixedBytes(account_id.using_encoded(|bytes| bytes.to_vec())),
			// amount: uint
			Token::Uint(ethabi::ethereum_types::U256(claim_details.amount.into().0)),
			// staker: address
			Token::Address(Address::from(claim_details.address)),
			// expiryTime: uint48
			Token::Uint(claim_details.expiry.as_secs().into()),
		])
	}

	/// Sets the `retired` flag associated with the account to false, signalling that the account wishes to come
	/// out of retirement.
	///
	/// Returns an error if the account is not retired, or if the account has no stake associated.
	fn activate(account_id: &T::AccountId) -> Result<(), Error<T>> {
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
	pub fn is_retired(account: &T::AccountId) -> Result<bool, Error<T>> {
		AccountRetired::<T>::try_get(account).map_err(|_| Error::UnknownAccount)
	}

	/// Registers the expiry time for an account's pending claim. At the provided time, any pending claims
	/// for the account are expired.
	fn register_claim_expiry(account_id: T::AccountId, expiry: Duration) {
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
	pub fn expire_pending_claims() -> Weight {
		if ClaimExpiries::<T>::decode_len().unwrap_or_default() == 0 {
			// Nothing to expire, should be pretty cheap.
			return T::WeightInfo::on_initialize_best_case();
		}

		let expiries = ClaimExpiries::<T>::get();
		// Expiries are sorted on insertion so we can just partition the slice.
		let expiry_cutoff = expiries.partition_point(|(expiry, _)| *expiry < T::TimeSource::now());

		let (to_expire, remaining) = expiries.split_at(expiry_cutoff);

		ClaimExpiries::<T>::set(remaining.into());

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
			}
		}

		let expired_claims = (to_expire.len() as usize).try_into().unwrap();
		return T::WeightInfo::on_initialize_worst_case(expired_claims);
	}
}

impl<T: Config> BidderProvider for Pallet<T> {
	type ValidatorId = T::AccountId;
	type Amount = T::Balance;

	fn get_bidders() -> Vec<(Self::ValidatorId, Self::Amount)> {
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
