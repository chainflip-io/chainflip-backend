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
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{address::AddressConverter, AccountOrAddress, AnyChain, ForeignChainAddress};
use cf_primitives::{AccountRole, Asset, AssetAmount, BasisPoints, DcaParameters, ForeignChain};
use cf_traits::{
	impl_pallet_safe_mode, AccountRoleRegistry, BalanceApi, BoostBalancesApi, Chainflip,
	DepositApi, EgressApi, LpRegistration, PoolApi, ScheduledEgressDetails, SwapRequestHandler,
};

use sp_std::vec;

use frame_support::{pallet_prelude::*, sp_runtime::DispatchResult};
use frame_system::pallet_prelude::*;
pub use pallet::*;

mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod migrations;
pub mod weights;
pub use weights::WeightInfo;

use cf_chains::address::EncodedAddress;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(3);

impl_pallet_safe_mode!(PalletSafeMode; deposit_enabled, withdrawal_enabled, internal_swaps_enabled);

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::{AccountOrAddress, Chain};
	use cf_primitives::{BlockNumber, ChannelId, EgressId, Price};
	use cf_traits::MinimumDeposit;

	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// API for handling asset deposits.
		type DepositHandler: DepositApi<
			AnyChain,
			AccountId = <Self as frame_system::Config>::AccountId,
			Amount = <Self as Chainflip>::Amount,
		>;

		/// API for handling asset egress.
		type EgressHandler: EgressApi<AnyChain>;

		/// A converter to convert address to and from human readable to internal address
		/// representation.
		type AddressConverter: AddressConverter;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode>;

		/// The interface for sweeping funds from pools into free balance
		type PoolApi: PoolApi<AccountId = <Self as frame_system::Config>::AccountId>;

		/// The interface to managing balances.
		type BalanceApi: BalanceApi<AccountId = <Self as frame_system::Config>::AccountId>;

		/// The interface to access boosted balances
		type BoostBalancesApi: BoostBalancesApi<
			AccountId = <Self as frame_system::Config>::AccountId,
		>;

		type SwapRequestHandler: SwapRequestHandler<AccountId = Self::AccountId>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;

		#[cfg(feature = "runtime-benchmarks")]
		type FeePayment: cf_traits::FeePayment<
			Amount = <Self as Chainflip>::Amount,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;

		/// The interface to access the minimum deposit amount for each asset
		type MinimumDeposit: MinimumDeposit;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The user does not have enough funds.
		InsufficientBalance,
		/// The user has reached the maximum balance.
		BalanceOverflow,
		/// The caller is not authorized to modify the trading position.
		UnauthorisedToModify,
		/// The Asset cannot be egressed because the destination address is not invalid.
		InvalidEgressAddress,
		/// Then given encoded address cannot be decoded into a valid ForeignChainAddress.
		InvalidEncodedAddress,
		/// A liquidity refund address must be set by the user for the chain before a
		/// deposit address can be requested.
		NoLiquidityRefundAddressRegistered,
		/// Liquidity deposit is disabled due to Safe Mode.
		LiquidityDepositDisabled,
		/// Withdrawals are disabled due to Safe Mode.
		WithdrawalsDisabled,
		/// The account still has open orders remaining.
		OpenOrdersRemaining,
		/// The account still has funds remaining in the free balances.
		FundsRemaining,
		/// The destination account is not a liquidity provider.
		DestinationAccountNotLiquidityProvider,
		/// The account cannot transfer to itself.
		CannotTransferToOriginAccount,
		/// The account still has funds remaining in the boost pools
		BoostedFundsRemaining,
		/// The input amount of on-chain swaps must be at least the minimum deposit amount.
		InternalSwapBelowMinimumDepositAmount,
		/// Internal swaps disabled due to safe mode.
		InternalSwapsDisabled,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		LiquidityDepositAddressReady {
			channel_id: ChannelId,
			asset: Asset,
			deposit_address: EncodedAddress,
			// account the funds will be credited to upon deposit
			account_id: T::AccountId,
			deposit_chain_expiry_block: <AnyChain as Chain>::ChainBlockNumber,
			boost_fee: BasisPoints,
			channel_opening_fee: T::Amount,
		},
		WithdrawalEgressScheduled {
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			destination_address: EncodedAddress,
			fee: AssetAmount,
		},
		LiquidityRefundAddressRegistered {
			account_id: T::AccountId,
			chain: ForeignChain,
			address: ForeignChainAddress,
		},
		AssetTransferred {
			from: T::AccountId,
			to: T::AccountId,
			asset: Asset,
			amount: AssetAmount,
		},
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Stores the registered emergency withdrawal address for an Account
	#[pallet::storage]
	pub type LiquidityRefundAddress<T: Config> = StorageDoubleMap<
		_,
		Identity,
		T::AccountId,
		Twox64Concat,
		ForeignChain,
		ForeignChainAddress,
	>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// For when the user wants to deposit assets into the Chain.
		/// Generates a new deposit address for the user to posit their assets.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::request_liquidity_deposit_address())]
		pub fn request_liquidity_deposit_address(
			origin: OriginFor<T>,
			asset: Asset,
			boost_fee: BasisPoints,
		) -> DispatchResult {
			ensure!(T::SafeMode::get().deposit_enabled, Error::<T>::LiquidityDepositDisabled);

			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			if let Some(refund_address) =
				LiquidityRefundAddress::<T>::get(&account_id, ForeignChain::from(asset))
			{
				let (channel_id, deposit_address, expiry_block, channel_opening_fee) =
					T::DepositHandler::request_liquidity_deposit_address(
						account_id.clone(),
						asset,
						boost_fee,
						refund_address,
					)?;

				Self::deposit_event(Event::LiquidityDepositAddressReady {
					channel_id,
					asset,
					deposit_address: T::AddressConverter::to_encoded_address(deposit_address),
					account_id,
					deposit_chain_expiry_block: expiry_block,
					boost_fee,
					channel_opening_fee,
				});

				Ok(())
			} else {
				Err(Error::<T>::NoLiquidityRefundAddressRegistered.into())
			}
		}

		/// Withdraw some amount of an asset from the free balance to an external address.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::withdraw_asset())]
		pub fn withdraw_asset(
			origin: OriginFor<T>,
			amount: AssetAmount,
			asset: Asset,
			destination_address: EncodedAddress,
		) -> DispatchResult {
			Self::transfer_or_withdraw(
				origin,
				amount,
				asset,
				AccountOrAddress::ExternalAddress(destination_address),
			)
		}

		/// Register the account as a Liquidity Provider.
		/// Account roles are immutable once registered.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::register_lp_account())]
		pub fn register_lp_account(who: OriginFor<T>) -> DispatchResult {
			let account_id = ensure_signed(who)?;

			T::AccountRoleRegistry::register_as_liquidity_provider(&account_id)?;

			Ok(())
		}

		/// Registers a Liquidity Refund Address(LRA) for an account.
		///
		/// To request a deposit address for a chain, an LRA must be registered for that chain.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::register_liquidity_refund_address())]
		pub fn register_liquidity_refund_address(
			origin: OriginFor<T>,
			address: EncodedAddress,
		) -> DispatchResult {
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let decoded_address = T::AddressConverter::try_from_encoded_address(address)
				.map_err(|()| Error::<T>::InvalidEncodedAddress)?;

			LiquidityRefundAddress::<T>::insert(
				&account_id,
				decoded_address.chain(),
				decoded_address.clone(),
			);

			Self::deposit_event(Event::<T>::LiquidityRefundAddressRegistered {
				account_id,
				chain: decoded_address.chain(),
				address: decoded_address,
			});
			Ok(())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::deregister_lp_account())]
		pub fn deregister_lp_account(who: OriginFor<T>) -> DispatchResult {
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(who)?;
			T::PoolApi::sweep(&account_id)?;

			ensure!(
				T::PoolApi::pools().iter().all(|asset_pair| {
					T::PoolApi::open_order_count(&account_id, asset_pair).unwrap_or_default() == 0
				}),
				Error::<T>::OpenOrdersRemaining
			);
			ensure!(
				T::BalanceApi::free_balances(&account_id).iter().all(|(_, amount)| *amount == 0),
				Error::<T>::FundsRemaining
			);

			for asset in Asset::all() {
				ensure!(
					T::BoostBalancesApi::boost_pool_account_balance(&account_id, asset) == 0,
					Error::<T>::BoostedFundsRemaining
				);
			}

			let _ = LiquidityRefundAddress::<T>::clear_prefix(&account_id, u32::MAX, None);

			T::AccountRoleRegistry::deregister_as_liquidity_provider(&account_id)?;

			Ok(())
		}

		/// Transfer some amount of an asset from the free balance to the free balance of another LP
		/// account on the Chainflip network.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::withdraw_asset())]
		pub fn transfer_asset(
			origin: OriginFor<T>,
			amount: AssetAmount,
			asset: Asset,
			destination: T::AccountId,
		) -> DispatchResult {
			Self::transfer_or_withdraw(
				origin,
				amount,
				asset,
				AccountOrAddress::InternalAccount(destination),
			)
		}

		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::schedule_swap())]
		pub fn schedule_swap(
			origin: OriginFor<T>,
			amount: AssetAmount,
			input_asset: Asset,
			output_asset: Asset,
			retry_duration: BlockNumber,
			min_price: Price,
			dca_params: Option<DcaParameters>,
		) -> DispatchResult {
			ensure!(T::SafeMode::get().internal_swaps_enabled, Error::<T>::InternalSwapsDisabled);

			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			ensure!(
				amount >= T::MinimumDeposit::get(input_asset),
				Error::<T>::InternalSwapBelowMinimumDepositAmount
			);

			Self::ensure_has_refund_address_for_asset(&account_id, output_asset)?;

			T::PoolApi::sweep(&account_id)?;

			T::BalanceApi::try_debit_account(&account_id, input_asset, amount)
				.map_err(|_| Error::<T>::InsufficientBalance)?;

			T::SwapRequestHandler::init_internal_swap_request(
				input_asset,
				amount,
				output_asset,
				retry_duration,
				min_price,
				dca_params,
				account_id,
			);

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn transfer_or_withdraw(
		origin: OriginFor<T>,
		amount: AssetAmount,
		asset: Asset,
		destination: AccountOrAddress<T::AccountId, EncodedAddress>,
	) -> DispatchResult {
		ensure!(T::SafeMode::get().withdrawal_enabled, Error::<T>::WithdrawalsDisabled);
		let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

		if amount > 0 {
			match destination {
				AccountOrAddress::InternalAccount(destination_account) => {
					ensure!(
						account_id != destination_account,
						Error::<T>::CannotTransferToOriginAccount
					);
					// Check if the destination account has the role liquidity provider.
					ensure!(
						T::AccountRoleRegistry::has_account_role(
							&destination_account,
							AccountRole::LiquidityProvider,
						),
						Error::<T>::DestinationAccountNotLiquidityProvider
					);
					ensure!(
						LiquidityRefundAddress::<T>::contains_key(
							&destination_account,
							ForeignChain::from(asset)
						),
						Error::<T>::NoLiquidityRefundAddressRegistered
					);
					// Sweep earned fees
					T::PoolApi::sweep(&account_id)?;

					// Debit the asset from the account.
					T::BalanceApi::try_debit_account(&account_id, asset, amount)?;

					// Credit the asset to the destination account.
					T::BalanceApi::credit_account(&destination_account, asset, amount);

					Self::deposit_event(Event::AssetTransferred {
						from: account_id,
						to: destination_account,
						asset,
						amount,
					});
				},
				AccountOrAddress::ExternalAddress(destination_address) => {
					let destination_address_internal =
						T::AddressConverter::try_from_encoded_address(destination_address.clone())
							.map_err(|_| Error::<T>::InvalidEgressAddress)?;

					// Check validity of Chain and Asset
					ensure!(
						destination_address_internal.chain() == ForeignChain::from(asset),
						Error::<T>::InvalidEgressAddress
					);

					// Sweep earned fees
					T::PoolApi::sweep(&account_id)?;

					// Debit the asset from the account.
					T::BalanceApi::try_debit_account(&account_id, asset, amount)?;

					let ScheduledEgressDetails { egress_id, egress_amount, fee_withheld } =
						T::EgressHandler::schedule_egress(
							asset,
							amount,
							destination_address_internal,
							None,
						)
						.map_err(Into::into)?;

					Self::deposit_event(Event::<T>::WithdrawalEgressScheduled {
						egress_id,
						asset,
						amount: egress_amount,
						destination_address,
						fee: fee_withheld,
					});
				},
			}
		}
		Ok(())
	}
}

impl<T: Config> LpRegistration for Pallet<T> {
	type AccountId = <T as frame_system::Config>::AccountId;

	#[cfg(feature = "runtime-benchmarks")]
	fn register_liquidity_refund_address(
		account_id: &Self::AccountId,
		address: ForeignChainAddress,
	) {
		LiquidityRefundAddress::<T>::insert(account_id, address.chain(), address);
	}

	fn ensure_has_refund_address_for_asset(
		account_id: &Self::AccountId,
		asset: Asset,
	) -> DispatchResult {
		ensure!(
			LiquidityRefundAddress::<T>::contains_key(account_id, ForeignChain::from(asset)),
			Error::<T>::NoLiquidityRefundAddressRegistered
		);
		Ok(())
	}
}
