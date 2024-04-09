#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]
#![feature(is_sorted)]

#[cfg(test)]
mod mock;

mod benchmarking;
pub mod migrations;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod tests;

use cf_chains::{eth::Address as EthereumAddress, RegisterRedemption};
use cf_primitives::AccountRole;
use cf_traits::{
	impl_pallet_safe_mode, AccountInfo, AccountRoleRegistry, Bid, BidderProvider, Broadcaster,
	Chainflip, EpochInfo, FeePayment, Funding,
};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	ensure,
	sp_runtime::{
		traits::{CheckedSub, UniqueSaturatedInto, Zero},
		Saturating,
	},
	traits::{EnsureOrigin, HandleLifetime, IsType, OnKilledAccount, StorageVersion, UnixTime},
};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use scale_info::TypeInfo;
use sp_std::{
	cmp::{max, min},
	collections::btree_map::BTreeMap,
	prelude::*,
};
#[derive(Encode, Decode, PartialEq, Debug, TypeInfo)]
pub enum Pending {
	Pending,
}
pub const PALLET_VERSION: StorageVersion = StorageVersion::new(3);

#[derive(Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct PendingRedemptionInfo<FlipBalance> {
	pub total: FlipBalance,
	pub restricted: FlipBalance,
	pub redeem_address: EthereumAddress,
}

impl_pallet_safe_mode!(PalletSafeMode; redeem_enabled, start_bidding_enabled, stop_bidding_enabled);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::eth::Ethereum;
	use cf_primitives::BroadcastId;
	use frame_support::{pallet_prelude::*, Parameter};
	use frame_system::pallet_prelude::*;

	#[allow(unused_imports)]
	use sp_std::time::Duration;

	pub type AccountId<T> = <T as frame_system::Config>::AccountId;

	pub type FundingAttempt<Amount> = (EthereumAddress, Amount);

	pub type FlipBalance<T> = <T as Chainflip>::Amount;

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
	pub trait Config: Chainflip {
		/// Standard Event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The type containing all calls that are dispatchable from the threshold source.
		type ThresholdCallable: From<Call<Self>>;

		type FunderId: AsRef<[u8; 32]> + IsType<<Self as frame_system::Config>::AccountId>;

		/// The Flip token implementation.
		type Flip: Funding<AccountId = <Self as frame_system::Config>::AccountId, Balance = Self::Amount>
			+ AccountInfo<Self>
			+ FeePayment<Amount = Self::Amount, AccountId = <Self as frame_system::Config>::AccountId>;

		type Broadcaster: Broadcaster<Ethereum, ApiCall = Self::RegisterRedemption>;

		/// Ensure that only threshold signature consensus can post a signature.
		type EnsureThresholdSigned: EnsureOrigin<Self::RuntimeOrigin>;

		/// The implementation of the register redemption transaction.
		type RegisterRedemption: RegisterRedemption + Member + Parameter;

		/// Something that provides the current time.
		type TimeSource: UnixTime;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Store the list of funded accounts and whether or not they are a active bidder.
	#[pallet::storage]
	pub type ActiveBidder<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, bool, ValueQuery>;

	/// PendingRedemptions stores a Pending enum for the account until the redemption is executed
	/// or the redemption expires.
	#[pallet::storage]
	pub type PendingRedemptions<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		AccountId<T>,
		PendingRedemptionInfo<FlipBalance<T>>,
		OptionQuery,
	>;

	/// The minimum amount a user can fund their account with, and therefore the minimum balance
	/// they must have remaining after they redeem.
	#[pallet::storage]
	pub type MinimumFunding<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	/// TTL for a redemption from the moment of issue.
	#[pallet::storage]
	pub type RedemptionTTLSeconds<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Registered addresses for an executor.
	#[pallet::storage]
	pub type BoundExecutorAddress<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, EthereumAddress, OptionQuery>;

	/// List of restricted addresses
	#[pallet::storage]
	pub type RestrictedAddresses<T: Config> =
		StorageMap<_, Blake2_128Concat, EthereumAddress, (), OptionQuery>;

	/// Map that bookkeeps the restricted balances for each address
	#[pallet::storage]
	pub type RestrictedBalances<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		AccountId<T>,
		BTreeMap<EthereumAddress, FlipBalance<T>>,
		ValueQuery,
	>;

	/// Map of bound addresses for accounts.
	#[pallet::storage]
	pub type BoundRedeemAddress<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountId<T>, EthereumAddress>;

	/// The fee levied for every redemption request. Can be updated by Governance.
	#[pallet::storage]
	pub type RedemptionTax<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

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
		StoppedBidding { account_id: AccountId<T> },

		/// A previously non-bidding account has started bidding.
		StartedBidding { account_id: AccountId<T> },

		/// A redemption has expired without being executed.
		RedemptionExpired { account_id: AccountId<T> },

		/// A new restricted address has been added
		AddedRestrictedAddress { address: EthereumAddress },

		/// A restricted address has been removed
		RemovedRestrictedAddress { address: EthereumAddress },

		/// A funding attempt has failed.
		FailedFundingAttempt {
			account_id: AccountId<T>,
			withdrawal_address: EthereumAddress,
			amount: FlipBalance<T>,
		},

		/// The minimum funding amount has been updated.
		MinimumFundingUpdated { new_minimum: T::Amount },

		/// The Withdrawal Tax has been updated.
		RedemptionTaxAmountUpdated { amount: T::Amount },

		/// The redemption amount was zero, so no redemption was made. The tax was still levied.
		RedemptionAmountZero { account_id: AccountId<T> },

		/// An account has been bound to an address.
		BoundRedeemAddress { account_id: AccountId<T>, address: EthereumAddress },

		/// An account has been bound to an executor address.
		BoundExecutorAddress { account_id: AccountId<T>, address: EthereumAddress },
	}

	#[pallet::error]
	pub enum Error<T> {
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

		/// When requesting a redemption, you must not have an amount below the minimum.
		BelowMinimumFunding,

		/// There are not enough unrestricted funds to process the redemption.
		InsufficientUnrestrictedFunds,

		/// Minimum funding amount must be greater than the redemption tax.
		InvalidMinimumFundingUpdate,

		/// Redemption tax must be less than the minimum funding amount.
		InvalidRedemptionTaxUpdate,

		/// The account has insufficient funds to pay for the redemption.
		InsufficientBalance,

		/// The account is already bound to an address.
		AccountAlreadyBound,

		/// The account is bound to a withdrawal address.
		AccountBindingRestrictionViolated,

		/// Redeem is disabled due to Safe Mode.
		RedeemDisabled,

		/// Start Bidding is disabled due to Safe Mode.
		StartBiddingDisabled,

		/// Stop Bidding is disabled due to Safe Mode.
		StopBiddingDisabled,

		/// The executor for this account is bound to another address.
		ExecutorBindingRestrictionViolated,

		/// The account is already bound to an executor address.
		ExecutorAddressAlreadyBound,

		/// The account cannot be reaped before it is unregstered.
		AccountMustBeUnregistered,
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
		/// - [Funded](Event::Funded)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::funded())]
		pub fn funded(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			amount: FlipBalance<T>,
			funder: EthereumAddress,
			// Required to ensure this call is unique per funding event.
			tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let total_balance = Self::add_funds_to_account(&account_id, amount);

			if RestrictedAddresses::<T>::contains_key(funder) {
				RestrictedBalances::<T>::mutate(account_id.clone(), |map| {
					map.entry(funder).and_modify(|balance| *balance += amount).or_insert(amount);
				});
			}

			Self::deposit_event(Event::Funded {
				account_id,
				tx_hash,
				funds_added: amount,
				total_balance,
			});
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
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::redeem())]
		pub fn redeem(
			origin: OriginFor<T>,
			amount: RedemptionAmount<FlipBalance<T>>,
			address: EthereumAddress,
			// Only this address can execute the redemption.
			executor: Option<EthereumAddress>,
		) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;

			ensure!(T::SafeMode::get().redeem_enabled, Error::<T>::RedeemDisabled);

			// Not allowed to redeem if we are an active bidder in the auction phase
			if T::EpochInfo::is_auction_phase() {
				ensure!(!ActiveBidder::<T>::get(&account_id), Error::<T>::AuctionPhase);
			}

			// The redemption must be executed before a new one can be requested.
			ensure!(
				!PendingRedemptions::<T>::contains_key(&account_id),
				Error::<T>::PendingRedemption
			);

			let mut restricted_balances = RestrictedBalances::<T>::get(&account_id);

			// Ignore executor binding restrictions for withdrawals of restricted funds.
			if !restricted_balances.contains_key(&address) {
				if let Some(bound_executor) = BoundExecutorAddress::<T>::get(&account_id) {
					ensure!(
						executor == Some(bound_executor),
						Error::<T>::ExecutorBindingRestrictionViolated
					);
				}
			}

			let redemption_fee = RedemptionTax::<T>::get();

			if let Some(bound_address) = BoundRedeemAddress::<T>::get(&account_id) {
				ensure!(
					bound_address == address || restricted_balances.contains_key(&address),
					Error::<T>::AccountBindingRestrictionViolated
				);
			}

			// In case the balance is lower than the sum of restricted addresses we take this
			// discrepancy into account so that restricted addresses can still redeem.
			let restricted_deficit: FlipBalance<T> = restricted_balances
				.values()
				.copied()
				.sum::<FlipBalance<T>>()
				.saturating_sub(T::Flip::balance(&account_id));

			// The available funds are the total balance minus whichever is larger from:
			// - The bond.
			// - The total restricted funds that need to remain in the account after the redemption.
			let liquid_balance = T::Flip::balance(&account_id).saturating_sub(max(
				T::Flip::bond(&account_id),
				restricted_balances.values().copied().sum::<FlipBalance<T>>().saturating_sub(
					restricted_deficit +
						restricted_balances.get(&address).copied().unwrap_or_default(),
				),
			));

			let (debit_amount, redeem_amount) = match amount {
				RedemptionAmount::Max =>
					(liquid_balance, liquid_balance.saturating_sub(redemption_fee)),
				RedemptionAmount::Exact(amount) => (amount.saturating_add(redemption_fee), amount),
			};

			ensure!(
				T::Flip::try_burn_fee(&account_id, redemption_fee).is_ok(),
				Error::<T>::InsufficientBalance
			);

			let mut total_restricted_balance: FlipBalance<T> = T::Amount::zero();

			// If necessary, update account restrictions.
			if let Some(restricted_balance) = restricted_balances.get_mut(&address) {
				// Use the full debit amount here - fees are paid by restricted funds by default.
				total_restricted_balance = *restricted_balance;
				restricted_balance.saturating_reduce(debit_amount);
				if restricted_balance.is_zero() {
					restricted_balances.remove(&address);
				}
				RestrictedBalances::<T>::insert(&account_id, &restricted_balances);
			}

			let remaining_balance = T::Flip::balance(&account_id)
				.checked_sub(&redeem_amount)
				.ok_or(Error::<T>::InsufficientBalance)?;

			ensure!(
				remaining_balance == Zero::zero() ||
					remaining_balance >= MinimumFunding::<T>::get(),
				Error::<T>::BelowMinimumFunding
			);
			ensure!(
				remaining_balance >=
					restricted_balances.values().copied().sum::<FlipBalance<T>>() -
						restricted_deficit,
				Error::<T>::InsufficientUnrestrictedFunds
			);

			// Update the account balance.
			if redeem_amount > Zero::zero() {
				T::Flip::try_initiate_redemption(&account_id, redeem_amount)?;
				if T::Flip::balance(&account_id).is_zero() {
					ensure!(
						T::AccountRoleRegistry::is_unregistered(&account_id),
						Error::<T>::AccountMustBeUnregistered
					);
				}

				// Send the transaction.
				let contract_expiry =
					T::TimeSource::now().as_secs() + RedemptionTTLSeconds::<T>::get();
				let call = T::RegisterRedemption::new_unsigned(
					<T as Config>::FunderId::from_ref(&account_id).as_ref(),
					redeem_amount.unique_saturated_into(),
					address.as_fixed_bytes(),
					contract_expiry,
					executor,
				);

				PendingRedemptions::<T>::insert(
					&account_id,
					PendingRedemptionInfo {
						total: redeem_amount,
						restricted: min(
							total_restricted_balance.saturating_sub(redemption_fee),
							redeem_amount,
						),
						redeem_address: address,
					},
				);

				Self::deposit_event(Event::RedemptionRequested {
					account_id,
					amount: redeem_amount,
					broadcast_id: T::Broadcaster::threshold_sign_and_broadcast(call).0,
					expiry_time: contract_expiry,
				});
			} else {
				Self::deposit_event(Event::RedemptionAmountZero { account_id })
			}

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
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::redeemed())]
		pub fn redeemed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			redeemed_amount: FlipBalance<T>,
			// Required to ensure this call is unique per redemption event.
			_tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let _ = PendingRedemptions::<T>::take(&account_id)
				.ok_or(Error::<T>::NoPendingRedemption)?;

			T::Flip::finalize_redemption(&account_id)
				.expect("This should never return an error because we already ensured above that the pending redemption does indeed exist");

			if T::Flip::balance(&account_id).is_zero() {
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

		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::redemption_expired())]
		pub fn redemption_expired(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			// The block number uniquely identifies the redemption expiry for a particular account
			// when witnessing.
			_block_number: u64,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let pending_redemption = PendingRedemptions::<T>::take(&account_id)
				.ok_or(Error::<T>::NoPendingRedemption)?;

			T::Flip::revert_redemption(&account_id).expect(
				"Pending Redemption should exist since the corresponding redemption existed",
			);

			// If the address is still restricted, we update the restricted balances again.
			if RestrictedAddresses::<T>::contains_key(pending_redemption.redeem_address) {
				RestrictedBalances::<T>::mutate(&account_id, |restricted_balances| {
					restricted_balances
						.entry(pending_redemption.redeem_address)
						.and_modify(|balance| *balance += pending_redemption.restricted)
						.or_insert(pending_redemption.restricted);
				});
			}

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
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::stop_bidding())]
		pub fn stop_bidding(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure!(T::SafeMode::get().stop_bidding_enabled, Error::<T>::StopBiddingDisabled);

			let account_id = T::AccountRoleRegistry::ensure_validator(origin)?;

			ensure!(!T::EpochInfo::is_auction_phase(), Error::<T>::AuctionPhase);

			ActiveBidder::<T>::try_mutate(&account_id, |is_active_bidder| {
				if *is_active_bidder {
					*is_active_bidder = false;
					Ok(())
				} else {
					Err(Error::<T>::AlreadyNotBidding)
				}
			})?;
			Self::deposit_event(Event::StoppedBidding { account_id });
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
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::start_bidding())]
		pub fn start_bidding(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure!(T::SafeMode::get().start_bidding_enabled, Error::<T>::StartBiddingDisabled);
			let account_id = T::AccountRoleRegistry::ensure_validator(origin)?;
			Self::activate_bidding(&account_id)?;
			Self::deposit_event(Event::StartedBidding { account_id });
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
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::update_minimum_funding())]
		pub fn update_minimum_funding(
			origin: OriginFor<T>,
			minimum_funding: T::Amount,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				minimum_funding > RedemptionTax::<T>::get(),
				Error::<T>::InvalidMinimumFundingUpdate
			);
			MinimumFunding::<T>::put(minimum_funding);
			Self::deposit_event(Event::MinimumFundingUpdated { new_minimum: minimum_funding });
			Ok(().into())
		}

		/// Adds/Removes restricted addresses to the list of restricted addresses.
		///
		/// ## Events
		///
		/// - [AddedRestrictedAddress](Event::AddedRestrictedAddress)
		/// - [RemovedRestrictedAddress](Event::RemovedRestrictedAddress)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::update_restricted_addresses(addresses_to_add.len() as u32, addresses_to_remove.len() as u32, 10_u32))]
		pub fn update_restricted_addresses(
			origin: OriginFor<T>,
			addresses_to_add: Vec<EthereumAddress>,
			addresses_to_remove: Vec<EthereumAddress>,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			for address in addresses_to_add {
				RestrictedAddresses::<T>::insert(address, ());
				Self::deposit_event(Event::AddedRestrictedAddress { address });
			}
			for address in addresses_to_remove {
				RestrictedAddresses::<T>::remove(address);
				for account_id in RestrictedBalances::<T>::iter_keys() {
					RestrictedBalances::<T>::mutate(&account_id, |balances| {
						if balances.contains_key(&address) {
							balances.remove(&address);
						}
					});
				}
				Self::deposit_event(Event::RemovedRestrictedAddress { address });
			}
			Ok(().into())
		}

		/// Binds an account to a redeem address. This is used to allow an account to redeem
		/// their funds only to a specific address.
		///
		/// ## Errors
		///
		/// - [AccountAlreadyBound](Error::AccountAlreadyBound)
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::bind_redeem_address())]
		pub fn bind_redeem_address(
			origin: OriginFor<T>,
			address: EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;
			ensure!(
				!BoundRedeemAddress::<T>::contains_key(&account_id),
				Error::<T>::AccountAlreadyBound
			);
			BoundRedeemAddress::<T>::insert(&account_id, address);
			Self::deposit_event(Event::BoundRedeemAddress { account_id, address });
			Ok(().into())
		}

		/// Updates the Withdrawal Tax, which is the amount levied on each withdrawal request.
		///
		/// Requires Governance
		///
		/// ## Events
		///
		/// - [On update](Event::RedemptionTaxAmountUpdated)
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::update_redemption_tax())]
		pub fn update_redemption_tax(origin: OriginFor<T>, amount: T::Amount) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(amount < MinimumFunding::<T>::get(), Error::<T>::InvalidRedemptionTaxUpdate);
			RedemptionTax::<T>::set(amount);
			Self::deposit_event(Event::<T>::RedemptionTaxAmountUpdated { amount });
			Ok(())
		}

		/// Binds executor address to an account.
		///
		/// ## Events
		///
		/// - [BoundExecutorAddress](Event::BoundExecutorAddress)
		///
		/// ## Errors
		///
		/// - [ExecutorAddressAlreadyBound](Error::ExecutorAddressAlreadyBound)
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::bind_executor_address())]
		pub fn bind_executor_address(
			origin: OriginFor<T>,
			executor_address: EthereumAddress,
		) -> DispatchResultWithPostInfo {
			let account_id = ensure_signed(origin)?;
			ensure!(
				!BoundExecutorAddress::<T>::contains_key(&account_id),
				Error::<T>::ExecutorAddressAlreadyBound,
			);
			BoundExecutorAddress::<T>::insert(account_id.clone(), executor_address);
			Self::deposit_event(Event::BoundExecutorAddress {
				account_id,
				address: executor_address,
			});
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub genesis_accounts: Vec<(AccountId<T>, AccountRole, T::Amount)>,
		pub redemption_tax: T::Amount,
		pub minimum_funding: T::Amount,
		pub redemption_ttl: Duration,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				genesis_accounts: Default::default(),
				redemption_tax: Default::default(),
				minimum_funding: Default::default(),
				redemption_ttl: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			assert!(
				self.redemption_tax < self.minimum_funding,
				"Redemption tax must be less than minimum funding"
			);
			MinimumFunding::<T>::set(self.minimum_funding);
			RedemptionTax::<T>::set(self.redemption_tax);
			RedemptionTTLSeconds::<T>::set(self.redemption_ttl.as_secs());
			for (account_id, role, amount) in self.genesis_accounts.iter() {
				Pallet::<T>::add_funds_to_account(account_id, *amount);
				if matches!(role, AccountRole::Validator) {
					Pallet::<T>::activate_bidding(account_id)
						.expect("The account was just created so this can't fail.");
				}
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Add funds to an account, creating the account if it doesn't exist. An account is not
	/// an implicit bidder and needs to start bidding explicitly.
	fn add_funds_to_account(account_id: &AccountId<T>, amount: T::Amount) -> T::Amount {
		if !frame_system::Pallet::<T>::account_exists(account_id) {
			// Creates an account
			let _ = frame_system::Provider::<T>::created(account_id);
		}

		T::Flip::credit_funds(account_id, amount)
	}

	/// Sets the `active` flag associated with the account to true, signalling that the account
	/// wishes to participate in auctions, to become a network authority.
	///
	/// Returns an error if the account is already bidding.
	fn activate_bidding(account_id: &AccountId<T>) -> Result<(), Error<T>> {
		ActiveBidder::<T>::try_mutate(account_id, |is_active_bidder| {
			if *is_active_bidder {
				Err(Error::AlreadyBidding)
			} else {
				*is_active_bidder = true;
				Ok(())
			}
		})
	}
}

impl<T: Config> BidderProvider for Pallet<T> {
	type ValidatorId = <T as frame_system::Config>::AccountId;
	type Amount = T::Amount;

	fn get_bidders() -> Vec<Bid<Self::ValidatorId, Self::Amount>> {
		ActiveBidder::<T>::iter()
			.filter_map(|(bidder_id, active)| {
				if active {
					let amount = T::Flip::balance(&bidder_id);
					Some(Bid { bidder_id, amount })
				} else {
					None
				}
			})
			.collect()
	}

	fn is_bidder(validator_id: &Self::ValidatorId) -> bool {
		ActiveBidder::<T>::get(validator_id)
	}
}

/// Ensure we clean up account specific items that definitely won't be required once the account
/// leaves the network.
impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
	fn on_killed_account(account_id: &T::AccountId) {
		ActiveBidder::<T>::remove(account_id);
		RestrictedBalances::<T>::remove(account_id);
		BoundExecutorAddress::<T>::remove(account_id);
		BoundRedeemAddress::<T>::remove(account_id);
	}
}
