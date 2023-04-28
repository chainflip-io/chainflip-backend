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

use cf_chains::RegisterRedemption;
use cf_primitives::EthereumAddress;
use cf_traits::{Bid, BidderProvider, EpochInfo, Funding, SystemStateInfo};
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
/// specified during funding.
///
/// Normally, this means that the account was funded via the 'normal' contract flow. The presence
/// of any other address indicates that the funds were added from a *vesting* contract and can
/// only be redeemed to the specified address.
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

	pub type FundingAttempt<Amount> = (EthereumAddress, Amount);

	pub type FlipBalance<T> = <T as Config>::Balance;

	pub type EthTransactionHash = [u8; 32];

	#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum RedemptionAmount<T: Parameter> {
		Max,
		Exact(T),
	}

	impl<T: Parameter> From<T> for RedemptionAmount<T> {
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

		type FunderId: AsRef<[u8; 32]> + IsType<<Self as frame_system::Config>::AccountId>;

		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ Into<u128>
			+ From<u128>;

		/// The Flip token implementation.
		type Flip: Funding<
			AccountId = <Self as frame_system::Config>::AccountId,
			Balance = Self::Balance,
		>;

		type Broadcaster: Broadcaster<Ethereum, ApiCall = Self::RegisterRedemption>;

		/// Ensure that only threshold signature consensus can post a signature.
		type EnsureThresholdSigned: EnsureOrigin<Self::RuntimeOrigin>;

		/// The implementation of the register redemption transaction.
		type RegisterRedemption: RegisterRedemption<Ethereum> + Member + Parameter;

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

	/// Store the list of funded accounts and whether or not they are a active bidder.
	#[pallet::storage]
	pub type ActiveBidder<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, bool, ValueQuery>;

	/// PendingRedemptions stores a () for the account until the redemption is executed or the
	/// redemption expires.
	#[pallet::storage]
	pub type PendingRedemptions<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, (), OptionQuery>;

	/// Locks a particular account's ability to redeem to a particular ETH address.
	#[pallet::storage]
	pub type WithdrawalAddresses<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, EthereumAddress, OptionQuery>;

	/// Currently just used to record failed funding attempts so that if necessary in the future we
	/// can use it to recover user funds.
	#[pallet::storage]
	pub type FailedFundingAttempts<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, Vec<FundingAttempt<T::Balance>>, ValueQuery>;

	/// The minimum amount a user can fund their account with, and therefore the minimum balance
	/// they must have remaining after they redeem.
	#[pallet::storage]
	pub type MinimumFunding<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	/// TTL for a redemption from the moment of issue.
	#[pallet::storage]
	pub type RedemptionTTLSeconds<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
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
		/// An account has been funded with some FLIP.
		Funded {
			account_id: AccountId<T>,
			tx_hash: EthTransactionHash,
			funds_added: FlipBalance<T>,
			// may include rewards earned
			total_balance: FlipBalance<T>,
		},

		// Someone has requested to redeem some FLIP into their Ethereum wallet.
		RedemptionRequested {
			account_id: AccountId<T>,
			amount: FlipBalance<T>,
			broadcast_id: BroadcastId,
			// Unix time.
			expiry_time: u64,
		},

		/// A node has redeemed their FLIP on the Ethereum chain. \[account_id,
		/// redeemed_amount\]
		RedemptionSettled(AccountId<T>, FlipBalance<T>),

		/// An account has stopped bidding and will no longer take part in auctions.
		StoppedBidding {
			account_id: AccountId<T>,
		},

		/// A previously non-bidding account has started bidding.
		StartedBidding {
			account_id: AccountId<T>,
		},

		/// A redemption has expired without being executed.
		RedemptionExpired {
			account_id: AccountId<T>,
		},

		/// A funding attempt has failed.
		FailedFundingAttempt {
			account_id: AccountId<T>,
			withdrawal_address: EthereumAddress,
			amount: FlipBalance<T>,
		},

		MinimumFundingUpdated {
			new_minimum: T::Balance,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The account is not known.
		UnknownAccount,

		/// An invalid redemption has been witnessed: the account has no pending redemptions.
		NoPendingRedemption,

		/// The redeemer tried to redeem despite having a redemption already pending.
		PendingRedemption,

		/// Can't stop bidding an account if it's already not bidding.
		AlreadyNotBidding,

		/// Can only start bidding if not already bidding.
		AlreadyBidding,

		/// We are in the auction phase
		AuctionPhase,

		/// A withdrawal address is provided, but the account has a different withdrawal address
		/// already associated.
		WithdrawalAddressRestricted,

		/// An invalid redemption has been made
		InvalidRedemption,

		/// When requesting a redemption, you must not have an amount below the minimum.
		BelowMinimumFunding,

		/// The redemption signature could not be found.
		SignatureNotReady,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// **This call can only be dispatched from the configured witness origin.**
		///
		/// Funds have been added to an account via the StateChainGateway Smart Contract.
		///
		/// If the account doesn't exist, we create it.
		///
		/// ## Events
		///
		/// - [FailedFundingAttempt](Event::FailedFundingAttempt)
		/// - [Funded](Event::Funded)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::funded())]
		pub fn funded(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			amount: FlipBalance<T>,
			withdrawal_address: EthereumAddress,
			// Required to ensure this call is unique per funding event.
			tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			if Self::check_withdrawal_address(&account_id, withdrawal_address, amount).is_ok() {
				let total_balance = Self::add_funds_to_account(&account_id, amount);
				Self::deposit_event(Event::Funded {
					account_id,
					tx_hash,
					funds_added: amount,
					total_balance: Self::add_funds_to_account(&account_id, amount),
				});
			}
			Ok(().into())
		}

		/// Get FLIP that is held for me by the system, signed by my authority key.
		///
		/// On success, the implementation of [ThresholdSigner] should emit an event. The attached
		/// redemption request needs to be signed by a threshold of authorities in order to produce
		/// valid data that can be submitted to the StateChainGateway Smart Contract.
		///
		/// An account can only have one pending redemption at a time, the funds wrapped up in the
		/// pending redemption are inaccessible and are not counted towards a Validator's Auction
		/// Bid.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - [PendingRedemption](Error::PendingRedemption)
		/// - [AuctionPhase](Error::AuctionPhase)
		/// - [WithdrawalAddressRestricted](Error::WithdrawalAddressRestricted)
		#[pallet::weight({ if matches!(amount, RedemptionAmount::Exact(_)) { T::WeightInfo::redeem() } else { T::WeightInfo::redeem_all() }})]
		pub fn redeem(
			origin: OriginFor<T>,
			amount: RedemptionAmount<FlipBalance<T>>,
			address: EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;
			T::SystemState::ensure_no_maintenance()?;

			let amount = match amount {
				RedemptionAmount::Max => T::Flip::redeemable_balance(&account_id),
				RedemptionAmount::Exact(amount) => amount,
			};

			ensure!(amount > Zero::zero(), Error::<T>::InvalidRedemption);

			ensure!(address != ETH_ZERO_ADDRESS, Error::<T>::InvalidRedemption);

			// Not allowed to redeem if we are an active bidder in the auction phase
			if T::EpochInfo::is_auction_phase() {
				ensure!(!ActiveBidder::<T>::get(&account_id), Error::<T>::AuctionPhase);
			}

			// The redemption must be executed before a new one can be requested.
			ensure!(
				!PendingRedemptions::<T>::contains_key(&account_id),
				Error::<T>::PendingRedemption
			);

			if let Some(withdrawal_address) = WithdrawalAddresses::<T>::get(&account_id) {
				if withdrawal_address != address {
					return Err(Error::<T>::WithdrawalAddressRestricted.into())
				}
			}

			// Calculate the amount that would remain after this redemption and ensure it won't be
			// less than the system's minimum balance.
			let remaining = T::Flip::account_balance(&account_id)
				.checked_sub(&amount)
				.ok_or(Error::<T>::InvalidRedemption)?;

			ensure!(
				remaining == Zero::zero() || remaining >= MinimumFunding::<T>::get(),
				Error::<T>::BelowMinimumFunding
			);

			// Return an error if the redeemer tries to redeem too much. Otherwise decrement the
			// funds by the amount redeemed.
			T::Flip::try_initiate_redemption(&account_id, amount)?;

			let contract_expiry = T::TimeSource::now().as_secs() + RedemptionTTLSeconds::<T>::get();

			let call = T::RegisterRedemption::new_unsigned(
				<T as Config>::FunderId::from_ref(&account_id).as_ref(),
				amount.into(),
				&address,
				contract_expiry,
			);

			PendingRedemptions::<T>::insert(account_id.clone(), ());

			Self::deposit_event(Event::RedemptionRequested {
				account_id,
				amount,
				broadcast_id: T::Broadcaster::threshold_sign_and_broadcast(call).0,
				expiry_time: contract_expiry,
			});

			Ok(().into())
		}

		/// **This call can only be dispatched from the configured witness origin.**
		///
		/// A redemption request has been finalised.
		///
		/// Note that calling this doesn't initiate any protocol changes - the `redemption` has
		/// already been authorised by authority multisig. This merely signals that the
		/// redeemer has in fact executed the redemption via the StateChainGateway Smart
		/// Contract and has received their funds. This allows us to finalise any on-chain cleanup.
		///
		/// ## Events
		///
		/// - [RedemptionSettled](Event::RedemptionSettled)
		///
		/// ## Errors
		///
		/// - [NoPendingRedemption](Error::NoPendingRedemption)
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::redeemed())]
		pub fn redeemed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			redeemed_amount: FlipBalance<T>,
			// Required to ensure this call is unique per redemption event.
			_tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			PendingRedemptions::<T>::take(&account_id).ok_or(Error::<T>::NoPendingRedemption)?;

			T::Flip::finalize_redemption(&account_id).expect("This should never return an error because we already ensured above that the pending redemption does indeed exist");

			if T::Flip::account_balance(&account_id).is_zero() {
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

			Self::deposit_event(Event::RedemptionSettled(account_id, redeemed_amount));

			Ok(().into())
		}

		#[pallet::weight(T::WeightInfo::redemption_expired())]
		pub fn redemption_expired(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			// The block number uniquely identifies the redemption expiry for a particular account
			// when witnessing.
			_block_number: u64,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			PendingRedemptions::<T>::take(&account_id).ok_or(Error::<T>::NoPendingRedemption)?;

			T::Flip::revert_redemption(&account_id).expect(
				"Pending Redemption should exist since the corresponding redemption existed",
			);

			Self::deposit_event(Event::<T>::RedemptionExpired { account_id });

			Ok(().into())
		}

		/// Signals a node's intent to withdraw their funds after the next auction and desist
		/// from future auctions. Should only be called by accounts that are not already not
		/// bidding.
		///
		/// ## Events
		///
		/// - [ActiveBidder](Event::ActiveBidder)
		///
		/// ## Errors
		///
		/// - [AlreadyNotBidding](Error::AlreadyNotBidding)
		/// - [UnknownAccount](Error::UnknownAccount)
		#[pallet::weight(T::WeightInfo::stop_bidding())]
		pub fn stop_bidding(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let account_id = T::AccountRoleRegistry::ensure_validator(origin)?;

			ensure!(!T::EpochInfo::is_auction_phase(), Error::<T>::AuctionPhase);

			ActiveBidder::<T>::try_mutate_exists(&account_id, |maybe_status| {
				match maybe_status.as_mut() {
					Some(active) => {
						if !*active {
							return Err(Error::<T>::AlreadyNotBidding)
						}
						*active = false;
						Self::deposit_event(Event::StoppedBidding {
							account_id: account_id.clone(),
						});
						Ok(())
					},
					None => Err(Error::UnknownAccount),
				}
			})?;
			Ok(().into())
		}

		/// Signals a non-bidding node's intent to start bidding, and participate in the
		/// next auction. Should only be called if the account is in a non-bidding state.
		///
		/// ## Events
		///
		/// - [StartedBidding](Event::StartedBidding)
		///
		/// ## Errors
		///
		/// - [AlreadyBidding](Error::AlreadyBidding)
		/// - [UnknownAccount](Error::UnknownAccount)
		#[pallet::weight(T::WeightInfo::start_bidding())]
		pub fn start_bidding(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = T::AccountRoleRegistry::ensure_validator(origin)?;
			Self::activate_bidding(&who)?;
			Ok(().into())
		}

		/// Updates the minimum funding required for an account, the extrinsic is gated with
		/// governance.
		///
		/// ## Events
		///
		/// - [MinimumFundingUpdated](Event::MinimumFundingUpdated)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::update_minimum_funding())]
		pub fn update_minimum_funding(
			origin: OriginFor<T>,
			minimum_funding: T::Balance,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			MinimumFunding::<T>::put(minimum_funding);
			Self::deposit_event(Event::MinimumFundingUpdated { new_minimum: minimum_funding });
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub genesis_validators: Vec<(AccountId<T>, T::Balance)>,
		pub minimum_funding: T::Balance,
		pub redemption_ttl: Duration,
		pub redemption_delay_buffer_seconds: u64,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				genesis_validators: vec![],
				minimum_funding: Default::default(),
				redemption_ttl: Default::default(),
				redemption_delay_buffer_seconds: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			MinimumFunding::<T>::set(self.minimum_funding);
			RedemptionTTLSeconds::<T>::set(self.redemption_ttl.as_secs());
			for (account_id, amount) in self.genesis_validators.iter() {
				Pallet::<T>::add_funds_to_account(account_id, *amount);
				Pallet::<T>::activate_bidding(account_id)
					.expect("The account was just created so this can't fail.");
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Checks the withdrawal address requirements and saves the address if provided.
	///
	/// If a non-zero address was provided, then it *must* match the address that was
	/// provided on the initial account-creating funding event.
	fn check_withdrawal_address(
		account_id: &AccountId<T>,
		withdrawal_address: EthereumAddress,
		amount: T::Balance,
	) -> Result<(), Error<T>> {
		if withdrawal_address == ETH_ZERO_ADDRESS {
			return Ok(())
		}
		if !frame_system::Pallet::<T>::account_exists(account_id) {
			// This is the initial account-creating funding event. We store the withdrawal address
			// for this account.
			WithdrawalAddresses::<T>::insert(account_id, withdrawal_address);
			return Ok(())
		}
		// If we reach here, the account already exists, so any provided withdrawal address
		// *must* match the one that was added on the initial account-creating funding event,
		// otherwise this funding event cannot be processed.
		match WithdrawalAddresses::<T>::get(account_id) {
			Some(existing) if withdrawal_address == existing => Ok(()),
			_ => {
				// The funding event was invalid - this should only happen if someone bypasses
				// our standard ethereum contract interfaces. We don't automatically refund here
				// otherwise it's attack vector (refunds require a broadcast, which is
				// expensive).
				//
				// Instead, we keep a record of the failed attempt so that we can potentially
				// investigate and / or consider refunding automatically or via governance.
				FailedFundingAttempts::<T>::append(account_id, (withdrawal_address, amount));
				Self::deposit_event(Event::FailedFundingAttempt {
					account_id: account_id.clone(),
					withdrawal_address,
					amount,
				});
				Err(Error::<T>::WithdrawalAddressRestricted)
			},
		}
	}

	/// Add funds to an account, creating the account if it doesn't exist. An account is not
	/// an implicit bidder and needs to start bidding explicitly.
	fn add_funds_to_account(account_id: &AccountId<T>, amount: T::Balance) -> T::Balance {
		if !frame_system::Pallet::<T>::account_exists(account_id) {
			// Creates an account
			let _ = frame_system::Provider::<T>::created(account_id);
			ActiveBidder::<T>::insert(account_id, false);
		}

		T::Flip::credit_funds(account_id, amount)
	}

	/// Sets the `active` flag associated with the account to true, signalling that the account
	/// wishes to participate in auctions, to become a network authority.
	///
	/// Returns an error if the account is already bidding, or if the account has no funds.
	fn activate_bidding(account_id: &AccountId<T>) -> Result<(), Error<T>> {
		ActiveBidder::<T>::try_mutate_exists(account_id, |maybe_status| {
			match maybe_status.as_mut() {
				Some(active) => {
					if *active {
						return Err(Error::AlreadyBidding)
					}
					*active = true;
					Self::deposit_event(Event::StartedBidding { account_id: account_id.clone() });
					Ok(())
				},
				None => Err(Error::UnknownAccount),
			}
		})
	}
}

impl<T: Config> BidderProvider for Pallet<T> {
	type ValidatorId = <T as frame_system::Config>::AccountId;
	type Amount = T::Balance;

	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>> {
		ActiveBidder::<T>::iter()
			.filter_map(|(bidder_id, active)| {
				if active {
					let amount = T::Flip::account_balance(&bidder_id);
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
/// NB: We deliberately don't include FailedFundingAttempts. Given something went wrong, these can
/// be handled by governance. We don't want to lose track of them.
impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
	fn on_killed_account(account_id: &T::AccountId) {
		WithdrawalAddresses::<T>::remove(account_id);
		ActiveBidder::<T>::remove(account_id);
	}
}
