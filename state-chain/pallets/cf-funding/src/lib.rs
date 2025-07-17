// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;

mod benchmarking;
pub mod migrations;

pub mod weights;
use core::marker::PhantomData;

pub use weights::WeightInfo;

#[cfg(test)]
mod tests;

use cf_chains::{eth::Address as EthereumAddress, RegisterRedemption};
use cf_primitives::{chains::assets::eth::Asset as EthAsset, EthAmount};
use cf_traits::{
	impl_pallet_safe_mode, AccountInfo, AccountRoleRegistry, Broadcaster, Chainflip, FeePayment,
	Funding,
};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::DispatchResult,
	ensure,
	pallet_prelude::DispatchError,
	sp_runtime::{
		traits::{CheckedSub, One, UniqueSaturatedInto, Zero},
		Saturating,
	},
	traits::{
		EnsureOrigin, HandleLifetime, IsType, OnKilledAccount, OriginTrait, StorageVersion,
		UnfilteredDispatchable, UnixTime,
	},
	DebugNoBound,
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
pub const PALLET_VERSION: StorageVersion = StorageVersion::new(4);

#[derive(Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct PendingRedemptionInfo<FlipBalance> {
	pub total: FlipBalance,
	pub restricted: FlipBalance,
	pub redeem_address: EthereumAddress,
}

#[derive(Copy, Clone, Debug)]
pub struct Redemption<T: Config> {
	/// The amount of FLIP that will be redeemed.
	pub redeem_amount: T::Amount,
	/// The Fee that will be charged for the redemption, on top of the redeem_amount.
	pub redemption_fee: T::Amount,
	/// The amount of the redemption that is restricted.
	pub restricted_redeem_amount: T::Amount,
	/// The account that is redeeming.
	pub account_id: T::AccountId,
	/// The Ethereum address to take into account for any redemption restrictions.
	pub redemption_address: Option<EthereumAddress>,
}

impl<T: Config> Redemption<T> {
	pub fn for_redeem(
		account_id: &T::AccountId,
		amount: RedemptionAmount<FlipBalance<T>>,
		redemption_address: &EthereumAddress,
	) -> Result<Self, Error<T>> {
		Self::new(
			account_id,
			amount,
			Some(redemption_address),
			RedemptionTax::<T>::get(),
			&RestrictedBalances::<T>::get(account_id),
			Some(redemption_address),
			MinimumFunding::<T>::get(),
		)
	}
	pub fn for_rebalance(
		source_account_id: &T::AccountId,
		amount: RedemptionAmount<FlipBalance<T>>,
		redemption_address: Option<EthereumAddress>,
	) -> Result<Self, Error<T>> {
		Self::new(
			source_account_id,
			amount,
			redemption_address.as_ref(),
			Zero::zero(),
			&RestrictedBalances::<T>::get(source_account_id),
			redemption_address.as_ref(),
			MinimumFunding::<T>::get(),
		)
	}
	pub fn for_rpc(account_id: &T::AccountId) -> Result<Self, Error<T>> {
		Self::new(
			account_id,
			RedemptionAmount::Max,
			None,
			RedemptionTax::<T>::get(),
			&RestrictedBalances::<T>::get(account_id),
			None,
			MinimumFunding::<T>::get(),
		)
	}

	/// Creates a new Redemption instance, ensuring that the account has sufficient balance.
	fn new(
		account_id: &T::AccountId,
		amount: RedemptionAmount<FlipBalance<T>>,
		redemption_address: Option<&EthereumAddress>,
		redemption_fee: FlipBalance<T>,
		restricted_balances: &BTreeMap<EthereumAddress, FlipBalance<T>>,
		bound_redeem_address: Option<&EthereumAddress>,
		minimum_funding: FlipBalance<T>,
	) -> Result<Self, Error<T>> {
		if let Some(address) = redemption_address {
			if let Some(bound_address) = bound_redeem_address {
				ensure!(
					bound_address == address || restricted_balances.contains_key(address),
					Error::<T>::AccountBindingRestrictionViolated
				);
			}
		}

		let total_restricted = restricted_balances.values().copied().sum::<FlipBalance<T>>();
		let account_balance = T::Flip::balance(account_id);
		let bond = T::Flip::bond(account_id);

		// If the account balance is less than the total restricted balance, we need to
		// capture this deficit and account for it during balance checks. This scenario is
		// unlikely, but possible if an account is slashed, for example.
		//
		// Example:
		// - Account balance: 100
		// - Bond: 0
		// - Total restricted balance: 150 (A: 100, B: 50)
		// - Restricted deficit: 150 - 100 = 50
		//
		// In the above example, even before the withdrawal, the account is 50 FLIP short of the
		// total restricted balance. If we apply restricted balance checks naively (ignoring the
		// deficit), it would be impossible to redeem any funds because the remainder would
		// *always* be too little to cover the restricted total.
		//
		// This is what is captured by `restricted_deficit` below.
		let restricted_deficit = total_restricted.saturating_sub(account_balance);

		// Calculate how much restricted balance would remain after this redemption
		let (restricted_debit_amount, remaining_restricted) =
			if let Some(address) = redemption_address {
				// If redeeming from a specific restricted address, reduce that restriction
				let available_restricted =
					restricted_balances.get(address).copied().unwrap_or_default();
				let restricted_debit_amount = match amount {
					RedemptionAmount::Max => available_restricted,
					RedemptionAmount::Exact(desired) =>
						min(desired + redemption_fee, available_restricted),
				};
				(
					restricted_debit_amount,
					total_restricted
						.saturating_sub(restricted_deficit)
						.saturating_sub(restricted_debit_amount),
				)
			} else {
				// If not redeeming from a restricted address, only account for the deficit
				(Zero::zero(), total_restricted.saturating_sub(restricted_deficit))
			};

		// Available funds are total balance minus the larger of bond or remaining restricted funds
		let liquid_balance = account_balance.saturating_sub(max(bond, remaining_restricted));

		let applied_fee = match amount {
			RedemptionAmount::Max if liquid_balance == account_balance => Zero::zero(),
			_ => redemption_fee,
		};

		let (debit_amount, redeem_amount) = match amount {
			RedemptionAmount::Max => (liquid_balance, liquid_balance.saturating_sub(applied_fee)),
			RedemptionAmount::Exact(amount) => (amount.saturating_add(applied_fee), amount),
		};

		debug_assert_eq!(
			debit_amount.checked_sub(&redeem_amount),
			Some(applied_fee),
			"Debit amount must equal redeem amount plus redemption fee",
		);

		ensure!(debit_amount <= account_balance, Error::<T>::InsufficientBalance);
		let remaining_balance = account_balance.saturating_sub(debit_amount);
		ensure!(remaining_balance >= bond, Error::<T>::BondViolation);
		ensure!(
			remaining_balance >= remaining_restricted,
			Error::<T>::InsufficientUnrestrictedFunds
		);
		ensure!(
			remaining_balance == Zero::zero() || remaining_balance >= minimum_funding,
			Error::<T>::BelowMinimumFunding
		);
		if account_balance == debit_amount {
			ensure!(
				T::AccountRoleRegistry::is_unregistered(account_id),
				Error::<T>::AccountMustBeUnregistered
			);
		}

		Ok(Redemption {
			redeem_amount,
			redemption_fee: applied_fee,
			restricted_redeem_amount: restricted_debit_amount.saturating_sub(applied_fee),
			account_id: account_id.clone(),
			redemption_address: redemption_address.cloned(),
		})
	}

	/// Returns the total debit amount, which is the sum of the redeem amount and the redemption
	/// fee.
	pub fn total_debit_amount(&self) -> T::Amount {
		self.redeem_amount.saturating_add(self.redemption_fee)
	}

	pub fn burn_fee(&self) -> Result<(), Error<T>> {
		ensure!(
			T::Flip::try_burn_fee(&self.account_id, self.redemption_fee).is_ok(),
			Error::<T>::InsufficientBalance
		);
		Ok(())
	}

	/// Apply required changes to the restricted balances of affected accounts.
	///
	/// Restricted balances of the source account will be reduced by the total debit amount, and if
	/// a target account is provided, the restricted balance of the target account will be
	/// increased by the redeem amount.
	pub fn update_restricted_balances(
		&self,
		target: Option<&T::AccountId>,
	) -> Result<(), Error<T>> {
		let Some(restricted_address) = self.redemption_address else {
			// If no redemption address is specified, we don't need to update restricted balances.
			return Ok(());
		};

		RestrictedBalances::<T>::mutate(&self.account_id, |source_restricted_balances| {
			// If necessary, update account restrictions.
			if let Some(restricted_balance) =
				source_restricted_balances.get_mut(&restricted_address)
			{
				// Use the full debit amount here - fees are paid by restricted funds by default.
				restricted_balance.saturating_reduce(self.total_debit_amount());
				// ensure that the remaining restricted balance is zero or above MinimumFunding
				ensure!(
					restricted_balance.is_zero() ||
						*restricted_balance >= MinimumFunding::<T>::get(),
					Error::<T>::RestrictedBalanceBelowMinimumFunding
				);

				if restricted_balance.is_zero() {
					source_restricted_balances.remove(&restricted_address);
				}
				if let Some(target) = target {
					RestrictedBalances::<T>::mutate(target, |restrictions| {
						restrictions
							.entry(restricted_address)
							.and_modify(|balance| *balance += self.redeem_amount)
							.or_insert(self.redeem_amount);
					});
				}
				Ok(())
			} else {
				Ok(())
			}
		})
	}
}

impl_pallet_safe_mode!(PalletSafeMode; redeem_enabled);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::eth::Ethereum;
	use cf_primitives::BroadcastId;
	use cf_traits::RedemptionCheck;
	use frame_support::{pallet_prelude::*, Parameter};
	use frame_system::pallet_prelude::*;

	use frame_support::sp_runtime::AccountId32;

	#[allow(unused_imports)]
	use sp_std::time::Duration;

	pub type AccountId<T> = <T as frame_system::Config>::AccountId;

	pub type RuntimeOrigin<T> = <T as frame_system::Config>::RuntimeOrigin;

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
			+ AccountInfo<AccountId = Self::AccountId, Amount = Self::Amount>
			+ FeePayment<Amount = Self::Amount, AccountId = <Self as frame_system::Config>::AccountId>;

		type Broadcaster: Broadcaster<Ethereum, ApiCall = Self::RegisterRedemption>;

		/// Ensure that only threshold signature consensus can post a signature.
		type EnsureThresholdSigned: EnsureOrigin<Self::RuntimeOrigin>;

		/// The implementation of the register redemption transaction.
		type RegisterRedemption: RegisterRedemption + Member + Parameter;

		/// Something that provides the current time.
		type TimeSource: UnixTime;

		/// Provide information on current bidders
		type RedemptionChecker: RedemptionCheck<ValidatorId = Self::AccountId>;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

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

		/// A redemption has expired without being executed.
		RedemptionExpired {
			account_id: AccountId<T>,
		},

		/// A new restricted address has been added
		AddedRestrictedAddress {
			address: EthereumAddress,
		},

		/// A restricted address has been removed
		RemovedRestrictedAddress {
			address: EthereumAddress,
		},

		/// A funding attempt has failed.
		FailedFundingAttempt {
			account_id: AccountId<T>,
			withdrawal_address: EthereumAddress,
			amount: FlipBalance<T>,
		},

		/// The minimum funding amount has been updated.
		MinimumFundingUpdated {
			new_minimum: T::Amount,
		},

		/// The Withdrawal Tax has been updated.
		RedemptionTaxAmountUpdated {
			amount: T::Amount,
		},

		/// The redemption amount was zero, so no redemption was made. The tax was still levied.
		RedemptionAmountZero {
			account_id: AccountId<T>,
		},

		/// An account has been bound to an address.
		BoundRedeemAddress {
			account_id: AccountId<T>,
			address: EthereumAddress,
		},

		/// An account has been bound to an executor address.
		BoundExecutorAddress {
			account_id: AccountId<T>,
			address: EthereumAddress,
		},

		/// A rebalance between two accounts has been executed.
		Rebalance {
			source_account_id: AccountId<T>,
			recipient_account_id: AccountId<T>,
			amount: FlipBalance<T>,
		},
		SCCallExecuted {
			sc_call: DepositAndSCCallViaEthereum<T>,
			tx_hash: EthTransactionHash,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An invalid redemption has been witnessed: the account has no pending redemptions.
		NoPendingRedemption,

		/// The redeemer tried to redeem despite having a redemption already pending.
		PendingRedemption,

		/// When requesting a redemption, you must not have an amount below the minimum.
		BelowMinimumFunding,

		/// When requesting a redemption, all restricted balances must be above the minimum.
		RestrictedBalanceBelowMinimumFunding,

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

		/// The executor for this account is bound to another address.
		ExecutorBindingRestrictionViolated,

		/// The account is already bound to an executor address.
		ExecutorAddressAlreadyBound,

		/// The account cannot be reaped before it is unregistered.
		AccountMustBeUnregistered,

		/// During auction phase its not possible to rebalance to a non-bidding validator if the
		/// source validator is currently bidding.
		CanNotRebalanceToNotBiddingValidator,

		/// The withdrawal would leave the account with a balance below the bond.
		BondViolation,

		/// Funds can only be sent to accounts that already exist.
		AccountMustExist,

		/// The rebalance amount must be at least the minimum funding amount.
		MinimumRebalanceAmount,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// **This call can only be dispatched from the configured witness origin.**
		///
		/// Funds have been added to an account via the StateChainGateway Smart Contract.
		///
		/// If the account doesn't exist, we create it.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::funded())]
		pub fn funded(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			amount: FlipBalance<T>,
			funder: EthereumAddress,
			// Required to ensure this call is unique per funding event.
			tx_hash: EthTransactionHash,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;
			Self::fund_sc_account(account_id, funder, amount, tx_hash);
			Ok(())
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
		) -> DispatchResult {
			let account_id = ensure_signed(origin)?;

			ensure!(T::SafeMode::get().redeem_enabled, Error::<T>::RedeemDisabled);

			// Not allowed to redeem if we are an active bidder in the auction phase
			T::RedemptionChecker::ensure_can_redeem(&account_id)?;

			// The redemption must be executed before a new one can be requested.
			ensure!(
				!PendingRedemptions::<T>::contains_key(&account_id),
				Error::<T>::PendingRedemption
			);

			let restricted_balances = RestrictedBalances::<T>::get(&account_id);

			// Ignore executor binding restrictions for withdrawals of restricted funds.
			if !restricted_balances.contains_key(&address) {
				if let Some(bound_executor) = BoundExecutorAddress::<T>::get(&account_id) {
					ensure!(
						executor == Some(bound_executor),
						Error::<T>::ExecutorBindingRestrictionViolated
					);
				}
			}
			if let Some(bound_address) = BoundRedeemAddress::<T>::get(&account_id) {
				ensure!(
					bound_address == address || restricted_balances.contains_key(&address),
					Error::<T>::AccountBindingRestrictionViolated
				);
			}

			let redemption @ Redemption { redeem_amount, restricted_redeem_amount, .. } =
				Redemption::<T>::for_redeem(&account_id, amount, &address)?;
			redemption.burn_fee()?;
			redemption.update_restricted_balances(None)?;

			// Update the account balance.
			if redeem_amount > Zero::zero() {
				T::Flip::try_initiate_redemption(&account_id, redeem_amount)?;

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
						restricted: restricted_redeem_amount,
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

			Ok(())
		}

		/// **This call can only be dispatched from the configured witness origin.**
		///
		/// A redemption request has been finalised.
		///
		/// Note that calling this doesn't initiate any protocol changes - the `redemption` has
		/// already been authorised by authority multisig. This merely signals that the
		/// redeemer has in fact executed the redemption via the StateChainGateway Smart
		/// Contract and has received their funds. This allows us to finalise any on-chain cleanup.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::redeemed())]
		pub fn redeemed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			redeemed_amount: FlipBalance<T>,
			// Required to ensure this call is unique per redemption event.
			_tx_hash: EthTransactionHash,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let _ = PendingRedemptions::<T>::take(&account_id)
				.ok_or(Error::<T>::NoPendingRedemption)?;

			T::Flip::finalize_redemption(&account_id)
				.expect("This should never return an error because we already ensured above that the pending redemption does indeed exist");

			Self::kill_account_if_zero_balance(&account_id);

			Self::deposit_event(Event::RedemptionSettled(account_id, redeemed_amount));

			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::redemption_expired())]
		pub fn redemption_expired(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			// The block number uniquely identifies the redemption expiry for a particular account
			// when witnessing.
			_block_number: u64,
		) -> DispatchResult {
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

			Ok(())
		}

		/// Updates the minimum funding required for an account, the extrinsic is gated with
		/// governance.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::update_minimum_funding())]
		pub fn update_minimum_funding(
			origin: OriginFor<T>,
			minimum_funding: T::Amount,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				minimum_funding > RedemptionTax::<T>::get(),
				Error::<T>::InvalidMinimumFundingUpdate
			);
			MinimumFunding::<T>::put(minimum_funding);
			Self::deposit_event(Event::MinimumFundingUpdated { new_minimum: minimum_funding });
			Ok(())
		}

		/// Adds/Removes restricted addresses to the list of restricted addresses.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::update_restricted_addresses(addresses_to_add.len() as u32, addresses_to_remove.len() as u32, 10_u32))]
		pub fn update_restricted_addresses(
			origin: OriginFor<T>,
			addresses_to_add: Vec<EthereumAddress>,
			addresses_to_remove: Vec<EthereumAddress>,
		) -> DispatchResult {
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
			Ok(())
		}

		/// Binds an account to a redeem address. This is used to allow an account to redeem
		/// their funds only to a specific address.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::bind_redeem_address())]
		pub fn bind_redeem_address(
			origin: OriginFor<T>,
			address: EthereumAddress,
		) -> DispatchResult {
			let account_id = ensure_signed(origin)?;
			ensure!(
				!BoundRedeemAddress::<T>::contains_key(&account_id),
				Error::<T>::AccountAlreadyBound
			);
			BoundRedeemAddress::<T>::insert(&account_id, address);
			Self::deposit_event(Event::BoundRedeemAddress { account_id, address });
			Ok(())
		}

		/// Updates the Withdrawal Tax, which is the amount levied on each withdrawal request.
		///
		/// Requires Governance
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
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::bind_executor_address())]
		pub fn bind_executor_address(
			origin: OriginFor<T>,
			executor_address: EthereumAddress,
		) -> DispatchResult {
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
			Ok(())
		}

		/// Rebalance funds between two validator accounts.
		///
		/// The minimum amount that can be rebalanced is the minimum funding amount.
		///
		/// The destination account must exist, and restrictions on the destination account must be
		/// at least as strict than the source account:
		///   - If the source account has a bound executor address, the destination account must
		///     also have the same bound executor address.
		///   - If the source account has a bound redeem address, the destination account must also
		///     have the same bound redeem address.
		///   - If the source account is currently bidding, the destination account must also be
		///     bidding.
		///   - If the funds to be transferred are associated with a restricted address, the
		///     restrictions will be transferred to the destination account.
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::rebalance())]
		pub fn rebalance(
			origin: OriginFor<T>,
			recipient_account_id: AccountId<T>,
			redemption_address: Option<EthereumAddress>,
			amount: RedemptionAmount<FlipBalance<T>>,
		) -> DispatchResult {
			let source_account_id = ensure_signed(origin)?;
			ensure!(
				frame_system::Pallet::<T>::account_exists(&recipient_account_id),
				Error::<T>::AccountMustExist
			);

			if let RedemptionAmount::Exact(amount) = amount {
				ensure!(amount >= MinimumFunding::<T>::get(), Error::<T>::MinimumRebalanceAmount);
			}

			if !T::RedemptionChecker::can_redeem(&source_account_id) {
				ensure!(
					!T::RedemptionChecker::can_redeem(&recipient_account_id),
					Error::<T>::CanNotRebalanceToNotBiddingValidator
				);
			}

			ensure!(
				BoundExecutorAddress::<T>::get(&source_account_id) ==
					BoundExecutorAddress::<T>::get(&recipient_account_id),
				Error::<T>::ExecutorBindingRestrictionViolated
			);

			ensure!(
				BoundRedeemAddress::<T>::get(&source_account_id) ==
					BoundRedeemAddress::<T>::get(&recipient_account_id),
				Error::<T>::AccountBindingRestrictionViolated
			);

			let redemption =
				Redemption::<T>::for_rebalance(&source_account_id, amount, redemption_address)?;
			redemption.burn_fee()?;
			redemption.update_restricted_balances(Some(&recipient_account_id))?;

			T::Flip::try_transfer(
				redemption.redeem_amount,
				&source_account_id,
				&recipient_account_id,
			)?;

			Self::deposit_event(Event::Rebalance {
				source_account_id: source_account_id.clone(),
				recipient_account_id,
				amount: redemption.redeem_amount,
			});

			Self::kill_account_if_zero_balance(&source_account_id);

			Ok(())
		}

		#[pallet::call_index(12)]
		#[allow(clippy::let_unit_value)]
		#[pallet::weight(Weight::zero())]
		pub fn execute_sc_call(
			origin: OriginFor<T>,
			sc_call: Vec<u8>,
			mut deposit_and_call: DepositAndSCCallViaEthereum<T>,
			caller: EthereumAddress,
			// Required to ensure this call is unique per funding event.
			tx_hash: EthTransactionHash,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			// use 0 padded ethereum address as account_id which the flip funds are
			// associated with on SC
			let caller_account_id = AccountId32::new(
				[[0u8; 12].as_slice(), caller.0.as_slice()].concat().try_into().unwrap(),
			);

			match deposit_and_call {
				DepositAndSCCallViaEthereum::FlipToSCGatewayAndCall { amount, ref mut call } => {
					Self::fund_sc_account(
						caller_account_id.clone(),
						caller,
						amount.into(),
						tx_hash,
					);

					*call = AllowedCallsViaSCGateway::decode(&mut &sc_call[..]).map_or_else(
						|e| {
							log::warn!("SC call couldn't be decoded: {:?}", e);
							None
						},
						Some,
					);
				},
				// Deposit and calls via vault or transfers will be handled here in the future
				_ => {},
			}

			// If the call fails to execute, we still succeed this extrinsic since we need to
			// successfully process the deposit above. In this case, the deposit will be processed
			// and no call will be executed.
			if let Err(e) = deposit_and_call
				.clone()
				.dispatch_bypass_filter(RuntimeOrigin::<T>::signed(caller_account_id))
			{
				log::warn!("SC call couldn't be executed. It returned an error: {:?}", e);
			}

			Self::deposit_event(Event::SCCallExecuted { sc_call: deposit_and_call, tx_hash });
			Ok(())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub genesis_accounts: Vec<(AccountId<T>, T::Amount)>,
		pub redemption_tax: T::Amount,
		pub minimum_funding: T::Amount,
		pub redemption_ttl: Duration,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				genesis_accounts: Default::default(),
				redemption_tax: Default::default(),
				minimum_funding: One::one(),
				redemption_ttl: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			assert!(
				self.redemption_tax < self.minimum_funding,
				"Redemption tax must be less than minimum funding. Redemption tax: {:?}, Minimum_funding: {:?}", 
				self.redemption_tax, self.minimum_funding,
			);
			MinimumFunding::<T>::set(self.minimum_funding);
			RedemptionTax::<T>::set(self.redemption_tax);
			RedemptionTTLSeconds::<T>::set(self.redemption_ttl.as_secs());
			for (account_id, amount) in self.genesis_accounts.iter() {
				Pallet::<T>::add_funds_to_account(account_id, *amount);
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

	fn kill_account_if_zero_balance(account_id: &T::AccountId) {
		if T::Flip::balance(account_id).is_zero() {
			frame_system::Provider::<T>::killed(account_id).unwrap_or_else(|e| {
				log::error!(
					"Unexpected reference count error while reaping the account {:?}: {:?}.",
					account_id,
					e
				);
			});
		}
	}

	fn fund_sc_account(
		account_id: AccountId<T>,
		funder: EthereumAddress,
		amount: FlipBalance<T>,
		tx_hash: EthTransactionHash,
	) {
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
	}
}

/// Ensure we clean up account specific items that definitely won't be required once the account
/// leaves the network.
impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
	fn on_killed_account(account_id: &T::AccountId) {
		RestrictedBalances::<T>::remove(account_id);
		BoundExecutorAddress::<T>::remove(account_id);
		BoundRedeemAddress::<T>::remove(account_id);
	}
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, DebugNoBound)]
#[scale_info(skip_type_params(T))]
pub enum AllowedCallsViaSCGateway<T: Config> {
	Delegate {
		delegator: EthereumAddress, // Ethereum Address of the delegator
		operator: T::AccountId,     // Operator the amount to delegate to
	},
	Undelegate {
		delegator: EthereumAddress, // Ethereum Address of the delegator
		operator: T::AccountId,     // Operator the amount was delegated to
	},
}

// calls via vault and transfers for future use
#[derive(Copy, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, DebugNoBound)]
pub enum AllowedCallsViaVault {}
#[derive(Copy, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, DebugNoBound)]
pub enum AllowedCallsViaTransfer {}
#[derive(Copy, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, DebugNoBound)]
pub enum AllowedCallsOnlyCalls {}

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, DebugNoBound)]
#[scale_info(skip_type_params(T))]
pub enum DepositAndSCCallViaEthereum<T: Config> {
	FlipToSCGatewayAndCall {
		amount: EthAmount,
		call: Option<AllowedCallsViaSCGateway<T>>,
	},
	ViaVault {
		asset: EthAsset,
		amount: EthAmount,
		call: Option<AllowedCallsViaVault>,
	},
	TransferAndCall {
		asset: EthAsset,
		amount: EthAmount,
		destination: EthereumAddress,
		call: Option<AllowedCallsViaTransfer>,
	},
	NoDepositOnlyCall {
		call: Option<AllowedCallsOnlyCalls>,
	},
	_Marker(PhantomData<T>),
}

impl<T: Config> UnfilteredDispatchable for DepositAndSCCallViaEthereum<T> {
	type RuntimeOrigin = T::RuntimeOrigin;
	fn dispatch_bypass_filter(
		self,
		_origin: Self::RuntimeOrigin,
	) -> frame_support::dispatch::DispatchResultWithPostInfo {
		match self {
			DepositAndSCCallViaEthereum::FlipToSCGatewayAndCall { amount: _, call } => match call {
				Some(AllowedCallsViaSCGateway::Delegate { delegator: _, operator: _ }) => todo!(),
				Some(AllowedCallsViaSCGateway::Undelegate { delegator: _, operator: _ }) => todo!(),
				None => Err(DispatchError::Other("Call does not exist or failed to decode").into()),
			},

			// Calls via vault and transfer will be supported in the future
			_ => Ok(().into()),
		}
	}
}
