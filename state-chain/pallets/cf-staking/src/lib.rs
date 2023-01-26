#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]
#![feature(is_sorted)]

#[cfg(test)]
mod mock;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod migrations;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod tests;

use cf_chains::RegisterClaim;
use cf_traits::{Bid, BidderProvider, EpochInfo, StakeTransfer, SystemStateInfo};
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	ensure,
	pallet_prelude::Weight,
	traits::{
		EnsureOrigin, HandleLifetime, IsType, OnKilledAccount, OnRuntimeUpgrade, StorageVersion,
		UnixTime,
	},
};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, CheckedSub, Zero};
use sp_std::prelude::*;

/// This address is used by the Ethereum contracts to indicate that no withdrawal address was
/// specified when staking.
///
/// Normally, this means that the staker staked via the 'normal' staking contract flow. The presence
/// of any other address indicates that the funds were staked from the *vesting* contract and can
/// only be withdrawn to the specified address.
pub const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::eth::Ethereum;
	use cf_primitives::BroadcastId;
	use cf_traits::{AccountRoleRegistry, Broadcaster};
	use frame_support::{pallet_prelude::*, Parameter};
	use frame_system::pallet_prelude::*;

	#[allow(unused_imports)]
	use sp_std::time::Duration;

	pub type AccountId<T> = <T as frame_system::Config>::AccountId;

	pub type EthereumAddress = [u8; 20];

	pub type StakeAttempt<Amount> = (EthereumAddress, Amount);

	pub type FlipBalance<T> = <T as Config>::Balance;

	pub type EthTransactionHash = [u8; 32];

	#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum ClaimAmount<T: Parameter> {
		Max,
		Exact(T),
	}

	impl<T: Parameter> From<T> for ClaimAmount<T> {
		fn from(t: T) -> Self {
			Self::Exact(t)
		}
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: cf_traits::Chainflip {
		/// Standard Event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The type containing all calls that are dispatchable from the threshold source.
		type ThresholdCallable: From<Call<Self>>;

		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;

		type StakerId: AsRef<[u8; 32]> + IsType<<Self as frame_system::Config>::AccountId>;

		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;

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

		type Broadcaster: Broadcaster<Ethereum, ApiCall = Self::RegisterClaim>;

		/// Ensure that only threshold signature consensus can post a signature.
		type EnsureThresholdSigned: EnsureOrigin<Self::RuntimeOrigin>;

		/// The implementation of the register claim transaction.
		type RegisterClaim: RegisterClaim<Ethereum> + Member + Parameter;

		/// Something that provides the current time.
		type TimeSource: UnixTime;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	/// Store the list of staked accounts and whether or not they are a active bidder.
	#[pallet::storage]
	pub type ActiveBidder<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, bool, ValueQuery>;

	/// PendingClaims stores a () for the account until the claim is executed.
	#[pallet::storage]
	pub type PendingClaims<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, (), OptionQuery>;

	/// Locks a particular account's ability to claim to a particular ETH address.
	#[pallet::storage]
	pub type WithdrawalAddresses<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, EthereumAddress, OptionQuery>;

	/// Currently just used to record failed staking attempts so that if necessary in the future we
	/// can use it to recover user funds.
	#[pallet::storage]
	pub type FailedStakeAttempts<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, Vec<StakeAttempt<T::Balance>>, ValueQuery>;

	/// List of pairs, mapping the time (in secs since Unix Epoch) at which the PendingClaim of a
	/// particular AccountId expires.
	#[pallet::storage]
	pub type ClaimExpiries<T: Config> = StorageValue<_, Vec<(u64, AccountId<T>)>, ValueQuery>;

	/// The minimum amount a user can stake, and therefore the minimum amount they can have
	/// remaining after they claim.
	#[pallet::storage]
	pub type MinimumStake<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	/// TTL for a claim from the moment of issue.
	#[pallet::storage]
	pub type ClaimTTLSeconds<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// We must ensure the claim expires on the chain *after* it expires on the contract.
	/// We should be extra sure that this is the case, else it opens the possibility for double
	/// claiming.
	#[pallet::storage]
	pub type ClaimDelayBufferSeconds<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Expires any pending claims that have passed their TTL.
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			if ClaimExpiries::<T>::decode_len().unwrap_or_default() == 0 {
				return T::WeightInfo::on_initialize_best_case()
			}

			Self::expire_pending_claims_at(T::TimeSource::now().as_secs())
		}

		fn on_runtime_upgrade() -> Weight {
			migrations::PalletMigration::<T>::on_runtime_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
			migrations::PalletMigration::<T>::pre_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::post_upgrade(state)
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A node has staked some FLIP on the Ethereum chain.
		Staked {
			account_id: AccountId<T>,
			tx_hash: EthTransactionHash,
			stake_added: FlipBalance<T>,
			// may include rewards earned
			total_stake: FlipBalance<T>,
		},

		// Someone has requested to claim some FLIP into their Ethereum wallet.
		ClaimRequested {
			account_id: AccountId<T>,
			amount: FlipBalance<T>,
			broadcast_id: BroadcastId,
			// Unix time.
			expiry_time: u64,
		},

		/// A node has claimed their FLIP on the Ethereum chain. \[account_id,
		/// claimed_amount\]
		ClaimSettled(AccountId<T>, FlipBalance<T>),

		/// An account has retired and will no longer take part in auctions. \[account_id\]
		AccountRetired(AccountId<T>),

		/// A previously retired account has been re-activated. \[account_id\]
		AccountActivated(AccountId<T>),

		/// A claim has expired without being executed.
		ClaimExpired { account_id: AccountId<T> },

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

		/// The claimant tried to claim despite having a claim already pending.
		PendingClaim,

		/// Can't retire an account if it's already retired.
		AlreadyRetired,

		/// Can't activate an account unless it's in a retired state.
		AlreadyActive,

		/// We are in the auction phase
		AuctionPhase,

		/// A withdrawal address is provided, but the account has a different withdrawal address
		/// already associated.
		WithdrawalAddressRestricted,

		/// An invalid claim has been made
		InvalidClaim,

		/// When requesting a claim, you must not have an amount below the minimum stake.
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
			tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			if Self::check_withdrawal_address(&account_id, withdrawal_address, amount).is_ok() {
				let total_stake = Self::stake_account(&account_id, amount);
				Self::deposit_event(Event::Staked {
					account_id,
					tx_hash,
					stake_added: amount,
					total_stake,
				});
			}
			Ok(().into())
		}

		/// Get FLIP that is held for me by the system, signed by my authority key.
		///
		/// On success, the implementation of [ThresholdSigner] should emit an event. The attached
		/// claim request needs to be signed by a threshold of authorities in order to produce valid
		/// data that can be submitted to the StakeManager Smart Contract.
		///
		/// An account can only have one pending claim at a time, the funds wrapped up in the
		/// pending claim are inaccessible and are not counted towards a Validator's Auction Bid.
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
		#[pallet::weight({ if matches!(amount, ClaimAmount::Exact(_)) { T::WeightInfo::claim() } else { T::WeightInfo::claim_all() }})]
		pub fn claim(
			origin: OriginFor<T>,
			amount: ClaimAmount<FlipBalance<T>>,
			address: EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;
			T::SystemState::ensure_no_maintenance()?;

			let amount = match amount {
				ClaimAmount::Max => T::Flip::claimable_balance(&account_id),
				ClaimAmount::Exact(amount) => amount,
			};

			ensure!(amount > Zero::zero(), Error::<T>::InvalidClaim);

			ensure!(address != ETH_ZERO_ADDRESS, Error::<T>::InvalidClaim);

			// Not allowed to claim if we are an active bidder in the auction phase
			if T::EpochInfo::is_auction_phase() {
				ensure!(!ActiveBidder::<T>::get(&account_id), Error::<T>::AuctionPhase);
			}

			// The staker must either execute their claim voucher or wait until expiry before
			// creating a new claim.
			ensure!(!PendingClaims::<T>::contains_key(&account_id), Error::<T>::PendingClaim);

			if let Some(withdrawal_address) = WithdrawalAddresses::<T>::get(&account_id) {
				if withdrawal_address != address {
					return Err(Error::<T>::WithdrawalAddressRestricted.into())
				}
			}

			// Calculate the amount that would remain after this claim and ensure it won't be less
			// than the system's minimum stake.
			let remaining = T::Flip::staked_balance(&account_id)
				.checked_sub(&amount)
				.ok_or(Error::<T>::InvalidClaim)?;

			ensure!(
				remaining == Zero::zero() || remaining >= MinimumStake::<T>::get(),
				Error::<T>::BelowMinimumStake
			);

			// Throw an error if the staker tries to claim too much. Otherwise decrement the stake
			// by the amount claimed.
			T::Flip::try_initiate_claim(&account_id, amount)?;

			let contract_expiry = T::TimeSource::now().as_secs() + ClaimTTLSeconds::<T>::get();

			// IMPORTANT: The claim should *always* expire on the SC *later* than on the contract.
			// If this does not occur, it means there's a window for a user to execute their claim
			// on Ethereum after it has been expired on our chain. This means they get their funds
			// on Ethereum, and the SC will revert the pending claim, giving them back their funds.
			Self::register_claim_expiry(
				account_id.clone(),
				contract_expiry + ClaimDelayBufferSeconds::<T>::get(),
			);

			let call = T::RegisterClaim::new_unsigned(
				<T as Config>::StakerId::from_ref(&account_id).as_ref(),
				amount.into(),
				&address,
				contract_expiry,
			);

			PendingClaims::<T>::insert(account_id.clone(), ());

			Self::deposit_event(Event::ClaimRequested {
				account_id,
				amount,
				broadcast_id: T::Broadcaster::threshold_sign_and_broadcast(call),
				expiry_time: contract_expiry,
			});

			Ok(().into())
		}

		/// **This call can only be dispatched from the configured witness origin.**
		///
		/// Previously staked funds have been reclaimed.
		///
		/// Note that calling this doesn't initiate any protocol changes - the `claim` has already
		/// been authorised by authority multisig. This merely signals that the claimant has in fact
		/// executed the claim via the StakeManager Smart Contract and has received their funds.
		/// This allows us to finalise any on-chain cleanup.
		///
		/// ## Events
		///
		/// - [ClaimSettled](Event::ClaimSettled)
		///
		/// ## Errors
		///
		/// - [NoPendingClaim](Error::NoPendingClaim)
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::claimed())]
		pub fn claimed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			claimed_amount: FlipBalance<T>,
			// Required to ensure this call is unique per claim event.
			_tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			PendingClaims::<T>::take(&account_id).ok_or(Error::<T>::NoPendingClaim)?;

			// Assumption: One claim per account here.
			ClaimExpiries::<T>::mutate(|expiries| {
				expiries.retain(|(_, expiry_account_id)| expiry_account_id != &account_id);
			});

			T::Flip::finalize_claim(&account_id).expect("This should never return an error because we already ensured above that the pending claim does indeed exist");

			if T::Flip::staked_balance(&account_id).is_zero() {
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

		/// Signals a node's intent to withdraw their stake after the next auction and desist
		/// from future auctions. Should only be called by accounts that are not already retired.
		///
		/// ## Events
		///
		/// - [ActiveBidder](Event::ActiveBidder)
		///
		/// ## Errors
		///
		/// - [AlreadyRetired](Error::AlreadyRetired)
		/// - [UnknownAccount](Error::UnknownAccount)
		#[pallet::weight(T::WeightInfo::retire_account())]
		pub fn retire_account(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = T::AccountRoleRegistry::ensure_validator(origin)?;
			Self::retire(&who)?;
			Ok(().into())
		}

		/// Signals a retired node's intent to re-activate their stake and participate in the
		/// next auction. Should only be called if the account is in a retired state.
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
			let who = T::AccountRoleRegistry::ensure_validator(origin)?;
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
		#[pallet::weight(T::WeightInfo::update_minimum_stake())]
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
		pub claim_delay_buffer_seconds: u64,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				genesis_stakers: vec![],
				minimum_stake: Default::default(),
				claim_ttl: Default::default(),
				claim_delay_buffer_seconds: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			MinimumStake::<T>::set(self.minimum_stake);
			ClaimTTLSeconds::<T>::set(self.claim_ttl.as_secs());
			ClaimDelayBufferSeconds::<T>::set(self.claim_delay_buffer_seconds);
			for (staker, amount) in self.genesis_stakers.iter() {
				Pallet::<T>::stake_account(staker, *amount);
				Pallet::<T>::activate(staker)
					.expect("The account was just created so this can't fail.");
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	fn expire_pending_claims_at(secs_since_unix_epoch: u64) -> Weight {
		let mut expiries = ClaimExpiries::<T>::get();

		debug_assert!(
			expiries.iter().is_sorted_by_key(|(expiry_time, _account_id)| expiry_time),
			"Expiries should be sorted on insertion"
		);

		ClaimExpiries::<T>::set(
			expiries.split_off(
				expiries.partition_point(|(expiry, _)| expiry <= &secs_since_unix_epoch),
			),
		);

		// take the len after we've partitioned the expiries.
		let expiries_len = expiries.len() as u32;
		for (_, account_id) in expiries {
			if PendingClaims::<T>::take(&account_id).is_some() {
				// Re-credit the account
				T::Flip::revert_claim(&account_id)
					.expect("Pending Claim should exist since the corresponding expiry exists");

				Self::deposit_event(Event::<T>::ClaimExpired { account_id });
			}
		}

		T::WeightInfo::expire_pending_claims_at(expiries_len)
	}

	/// Checks the withdrawal address requirements and saves the address if provided.
	///
	/// If a non-zero address was provided, then it *must* match the address that was
	/// provided on the initial account-creating staking event.
	fn check_withdrawal_address(
		account_id: &AccountId<T>,
		withdrawal_address: EthereumAddress,
		amount: T::Balance,
	) -> Result<(), Error<T>> {
		if withdrawal_address == ETH_ZERO_ADDRESS {
			return Ok(())
		}
		if !frame_system::Pallet::<T>::account_exists(account_id) {
			// This is the initial account-creating staking event. We store the withdrawal address
			// for this account.
			WithdrawalAddresses::<T>::insert(account_id, withdrawal_address);
			return Ok(())
		}
		// If we reach here, the account already exists, so any provided withdrawal address
		// *must* match the one that was added on the initial account-creating staking event,
		// otherwise this staking event cannot be processed.
		match WithdrawalAddresses::<T>::get(account_id) {
			Some(existing) if withdrawal_address == existing => Ok(()),
			_ => {
				// The staking event was invalid - this should only happen if someone bypasses
				// our standard ethereum contract interfaces. We don't automatically refund here
				// otherwise it's attack vector (refunds require a broadcast, which is
				// expensive).
				//
				// Instead, we keep a record of the failed attempt so that we can potentially
				// investigate and / or consider refunding automatically or via governance.
				FailedStakeAttempts::<T>::append(account_id, (withdrawal_address, amount));
				Self::deposit_event(Event::FailedStakeAttempt(
					account_id.clone(),
					withdrawal_address,
					amount,
				));
				Err(Error::<T>::WithdrawalAddressRestricted)
			},
		}
	}

	/// Add stake to an account, creating the account if it doesn't exist, an account is not
	/// an implicit bidder and needs to be activated manually.
	fn stake_account(account_id: &AccountId<T>, amount: T::Balance) -> T::Balance {
		if !frame_system::Pallet::<T>::account_exists(account_id) {
			// Creates an account
			let _ = frame_system::Provider::<T>::created(account_id);
			ActiveBidder::<T>::insert(account_id, false);
		}

		T::Flip::credit_stake(account_id, amount)
	}

	/// Sets the `active` flag associated with the account to false, signalling that the account no
	/// longer wishes to participate in auctions.
	///
	/// Returns an error if the account has already been retired, if the account has no stake
	/// associated, or if the epoch is currently in the auction phase.
	fn retire(account_id: &AccountId<T>) -> Result<(), Error<T>> {
		ensure!(!T::EpochInfo::is_auction_phase(), Error::<T>::AuctionPhase);

		ActiveBidder::<T>::try_mutate_exists(account_id, |maybe_status| {
			match maybe_status.as_mut() {
				Some(active) => {
					if !*active {
						return Err(Error::AlreadyRetired)
					}
					*active = false;
					Self::deposit_event(Event::AccountRetired(account_id.clone()));
					Ok(())
				},
				None => Err(Error::UnknownAccount),
			}
		})
	}

	/// Sets the `active` flag associated with the account to true, signalling that the account
	/// wishes to participate in auctions, to become a network authority.
	///
	/// Returns an error if the account is already active, or if the account has no stake
	/// associated.
	fn activate(account_id: &AccountId<T>) -> Result<(), Error<T>> {
		ActiveBidder::<T>::try_mutate_exists(account_id, |maybe_status| {
			match maybe_status.as_mut() {
				Some(active) => {
					if *active {
						return Err(Error::AlreadyActive)
					}
					*active = true;
					Self::deposit_event(Event::AccountActivated(account_id.clone()));
					Ok(())
				},
				None => Err(Error::UnknownAccount),
			}
		})
	}

	/// Registers the expiry time for an account's pending claim. At the provided time, any pending
	/// claims for the account are expired.
	fn register_claim_expiry(account_id: AccountId<T>, expiry: u64) {
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
}

impl<T: Config> BidderProvider for Pallet<T> {
	type ValidatorId = <T as frame_system::Config>::AccountId;
	type Amount = T::Balance;

	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>> {
		ActiveBidder::<T>::iter()
			.filter_map(|(bidder_id, active)| {
				if active {
					let amount = T::Flip::staked_balance(&bidder_id);
					Some(Bid { bidder_id, amount })
				} else {
					None
				}
			})
			.collect()
	}
}

/// Ensure we clean up account specific items that definitely won't be required once the account
/// leaves the network.
/// NB: We deliberately don't include FailedStakeAttempts. Given something went wrong, these can
/// be handled by governance. We don't want to lose track of them.
impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
	fn on_killed_account(account_id: &T::AccountId) {
		WithdrawalAddresses::<T>::remove(account_id);
		ActiveBidder::<T>::remove(account_id);
	}
}
