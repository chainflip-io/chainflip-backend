#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{address::AddressConverter, AnyChain, ForeignChainAddress};
use cf_primitives::{Asset, AssetAmount, BasisPoints, ForeignChain};
use cf_traits::{
	impl_pallet_safe_mode, liquidity::LpBalanceApi, AccountRoleRegistry, Chainflip, DepositApi,
	EgressApi, LpDepositHandler, PoolApi, ScheduledEgressDetails,
};

use sp_std::vec;

use cf_chains::assets::any::AssetMap;
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

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(2);

impl_pallet_safe_mode!(PalletSafeMode; deposit_enabled, withdrawal_enabled);

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::Chain;
	use cf_primitives::{ChannelId, EgressId};

	use super::*;

	/// AccountOrAddress is a enum that can represent an internal account or an external address.
	/// This is used to represent the destination address for an egress during a withdrawal or an
	/// internal account to move funds internally.
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, PartialOrd, Ord)]
	pub enum AccountOrAddress<AccountId> {
		Internal(AccountId),
		External(EncodedAddress),
	}

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

		/// Benchmark weights
		type WeightInfo: WeightInfo;

		#[cfg(feature = "runtime-benchmarks")]
		type FeePayment: cf_traits::FeePayment<
			Amount = <Self as Chainflip>::Amount,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;
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
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AccountDebited {
			account_id: T::AccountId,
			asset: Asset,
			amount_debited: AssetAmount,
		},
		AccountCredited {
			account_id: T::AccountId,
			asset: Asset,
			amount_credited: AssetAmount,
		},
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
		LiquidityDepositCredited {
			account_id: T::AccountId,
			asset: Asset,
			amount_credited: AssetAmount,
		},
		AssetMoved {
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

	#[pallet::storage]
	/// Storage for user's free balances/ DoubleMap: (AccountId, Asset) => Balance
	pub type FreeBalances<T: Config> =
		StorageDoubleMap<_, Twox64Concat, T::AccountId, Identity, Asset, AssetAmount>;

	#[pallet::storage]
	/// Historical earned fees for an account.
	pub type HistoricalEarnedFees<T: Config> =
		StorageDoubleMap<_, Identity, T::AccountId, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	/// Stores the registered energency withdrawal address for an Account
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

			ensure!(
				LiquidityRefundAddress::<T>::contains_key(&account_id, ForeignChain::from(asset)),
				Error::<T>::NoLiquidityRefundAddressRegistered
			);

			let (channel_id, deposit_address, expiry_block, channel_opening_fee) =
				T::DepositHandler::request_liquidity_deposit_address(
					account_id.clone(),
					asset,
					boost_fee,
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
		}

		/// For when the user wants to withdraw their free balances out of the chain.
		/// Requires a valid foreign chain address.
		/// Note: This is a V1 version of the withdraw_asset call. The V2 version is recommended.
		#[deprecated(note = "Use withdraw_asset_v2 instead")]
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::withdraw_asset())]
		pub fn withdraw_asset(
			origin: OriginFor<T>,
			amount: AssetAmount,
			asset: Asset,
			destination_address: EncodedAddress,
		) -> DispatchResult {
			Self::withdraw_asset_inner(
				origin,
				amount,
				asset,
				AccountOrAddress::External(destination_address),
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
		///
		/// ## Events
		///
		/// - [On Success](Event::LiquidityRefundAddressRegistered)
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
			const STABLE_ASSET: Asset = Asset::Usdc;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(who)?;

			ensure!(
				Asset::all().filter(|asset| *asset != STABLE_ASSET).all(|asset| {
					T::PoolApi::open_order_count(&account_id, asset, STABLE_ASSET)
						.unwrap_or_default() == 0
				}),
				Error::<T>::OpenOrdersRemaining
			);
			ensure!(
				Self::asset_balances(&account_id)?.iter().all(|(_, amount)| *amount == 0),
				Error::<T>::FundsRemaining
			);

			let _ = FreeBalances::<T>::clear_prefix(&account_id, u32::MAX, None);
			let _ = LiquidityRefundAddress::<T>::clear_prefix(&account_id, u32::MAX, None);
			let _ = HistoricalEarnedFees::<T>::clear_prefix(&account_id, u32::MAX, None);

			T::AccountRoleRegistry::deregister_as_liquidity_provider(&account_id)?;

			Ok(())
		}

		/// For when the user wants to withdraw or move their free balances. Requires a valid
		/// foreign chain address or an internal account. If an address is provided, the assets are
		/// getting withdrawn to the external chain. If an account is provided, the assets are
		/// getting moved internally.
		/// Note: This is a V2 version of the withdraw_asset call. The V1 version is deprecated.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::withdraw_asset())]
		pub fn withdraw_asset_v2(
			origin: OriginFor<T>,
			amount: AssetAmount,
			asset: Asset,
			destination: AccountOrAddress<T::AccountId>,
		) -> DispatchResult {
			Self::withdraw_asset_inner(origin, amount, asset, destination)
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn withdraw_asset_inner(
		origin: OriginFor<T>,
		amount: AssetAmount,
		asset: Asset,
		destination: AccountOrAddress<T::AccountId>,
	) -> DispatchResult {
		ensure!(T::SafeMode::get().withdrawal_enabled, Error::<T>::WithdrawalsDisabled);
		if amount > 0 {
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			match destination {
				AccountOrAddress::Internal(destination_account) => {
					// Sweep earned fees
					T::PoolApi::sweep(&account_id)?;

					// Debit the asset from the account.
					Self::try_debit_account(&account_id, asset, amount)?;

					// Credit the asset to the destination account.
					Self::try_credit_account(&destination_account, asset, amount)?;

					Self::deposit_event(Event::AssetMoved {
						from: account_id,
						to: destination_account,
						asset,
						amount,
					});
				},
				AccountOrAddress::External(destination_address) => {
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
					Self::try_debit_account(&account_id, asset, amount)?;

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

impl<T: Config> LpDepositHandler for Pallet<T> {
	type AccountId = <T as frame_system::Config>::AccountId;

	fn add_deposit(
		account_id: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> frame_support::pallet_prelude::DispatchResult {
		Self::try_credit_account(account_id, asset, amount)?;

		Self::deposit_event(Event::LiquidityDepositCredited {
			account_id: account_id.clone(),
			asset,
			amount_credited: amount,
		});

		Ok(())
	}
}

impl<T: Config> LpBalanceApi for Pallet<T> {
	type AccountId = <T as frame_system::Config>::AccountId;

	#[cfg(feature = "runtime-benchmarks")]
	fn register_liquidity_refund_address(
		account_id: &Self::AccountId,
		address: ForeignChainAddress,
	) {
		LiquidityRefundAddress::<T>::insert(account_id, address.chain(), address);
	}

	fn ensure_has_refund_address_for_pair(
		account_id: &Self::AccountId,
		base_asset: Asset,
		quote_asset: Asset,
	) -> DispatchResult {
		ensure!(
			LiquidityRefundAddress::<T>::contains_key(account_id, ForeignChain::from(base_asset)) &&
				LiquidityRefundAddress::<T>::contains_key(
					account_id,
					ForeignChain::from(quote_asset)
				),
			Error::<T>::NoLiquidityRefundAddressRegistered
		);
		Ok(())
	}

	fn try_credit_account(
		account_id: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult {
		if amount == 0 {
			return Ok(())
		}

		let mut balance = FreeBalances::<T>::get(account_id, asset).unwrap_or_default();
		balance = balance.checked_add(amount).ok_or(Error::<T>::BalanceOverflow)?;
		FreeBalances::<T>::insert(account_id, asset, balance);

		Self::deposit_event(Event::AccountCredited {
			account_id: account_id.clone(),
			asset,
			amount_credited: amount,
		});
		Ok(())
	}

	fn try_debit_account(
		account_id: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult {
		if amount == 0 {
			return Ok(())
		}

		let mut balance = FreeBalances::<T>::get(account_id, asset).unwrap_or_default();
		ensure!(balance >= amount, Error::<T>::InsufficientBalance);
		balance = balance.saturating_sub(amount);
		FreeBalances::<T>::insert(account_id, asset, balance);

		Self::deposit_event(Event::AccountDebited {
			account_id: account_id.clone(),
			asset,
			amount_debited: amount,
		});
		Ok(())
	}

	fn record_fees(account_id: &Self::AccountId, amount: AssetAmount, asset: Asset) {
		HistoricalEarnedFees::<T>::mutate(account_id, asset, |balance| {
			balance.saturating_add(amount)
		});
	}

	fn asset_balances(who: &Self::AccountId) -> Result<AssetMap<AssetAmount>, DispatchError> {
		T::PoolApi::sweep(who)?;
		Ok(AssetMap::from_fn(|asset| FreeBalances::<T>::get(who, asset).unwrap_or_default()))
	}
}
