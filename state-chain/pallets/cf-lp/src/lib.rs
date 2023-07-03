#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_primitives::{Asset, AssetAmount, ForeignChain};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::DispatchResult;

use cf_chains::{address::AddressConverter, AnyChain};
use cf_traits::{
	impl_pallet_safe_mode, liquidity::LpBalanceApi, AccountRoleRegistry, Chainflip, DepositApi,
	EgressApi,
};
use sp_runtime::{traits::BlockNumberProvider, Saturating};
use sp_std::vec::Vec;

mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

impl_pallet_safe_mode!(PalletSafeMode; deposit_enabled, withdrawal_enabled);

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::address::EncodedAddress;
	use cf_primitives::{ChannelId, EgressId};

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
		>;

		/// API for handling asset egress.
		type EgressHandler: EgressApi<AnyChain>;

		/// A converter to convert address to and from human readable to internal address
		/// representation.
		type AddressConverter: AddressConverter;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		// The user does not have enough fund.
		InsufficientBalance,
		// The user has reached the maximum balance.
		BalanceOverflow,
		// The caller is not authorized to modify the trading position.
		UnauthorisedToModify,
		// The Asset cannot be egressed to the destination chain.
		InvalidEgressAddress,
		// Liquidity deposit is disabled due to Safe Mode.
		LiquidityDepositDisabled,
		// Withdrawals are disabled due to Safe Mode.
		WithdrawalsDisabled,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			let expired = LiquidityChannelExpiries::<T>::take(n);
			let expired_count = expired.len();
			for (channel_id, address) in expired {
				T::DepositHandler::expire_channel(channel_id, address.clone());
				Self::deposit_event(Event::LiquidityDepositAddressExpired {
					address: T::AddressConverter::to_encoded_address(address),
				});
			}
			T::WeightInfo::on_initialize(expired_count as u32)
		}
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
			deposit_address: EncodedAddress,
			expiry_block: T::BlockNumber,
		},
		LiquidityDepositAddressExpired {
			address: EncodedAddress,
		},
		WithdrawalEgressScheduled {
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			destination_address: EncodedAddress,
		},
		LpTtlSet {
			ttl: T::BlockNumber,
		},
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub lp_ttl: T::BlockNumber,
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			LpTTL::<T>::put(self.lp_ttl);
		}
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { lp_ttl: T::BlockNumber::from(1200u32) }
		}
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	/// Storage for user's free balances/ DoubleMap: (AccountId, Asset) => Balance
	pub type FreeBalances<T: Config> =
		StorageDoubleMap<_, Twox64Concat, T::AccountId, Identity, Asset, AssetAmount>;

	/// For a given block number, stores the list of liquidity deposit channels that expire at that
	/// block.
	#[pallet::storage]
	pub(super) type LiquidityChannelExpiries<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::BlockNumber,
		Vec<(ChannelId, cf_chains::ForeignChainAddress)>,
		ValueQuery,
	>;

	/// The TTL for liquidity channels.
	#[pallet::storage]
	pub type LpTTL<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// For when the user wants to deposit assets into the Chain.
		/// Generates a new deposit address for the user to posit their assets.
		#[pallet::weight(T::WeightInfo::request_liquidity_deposit_address())]
		pub fn request_liquidity_deposit_address(
			origin: OriginFor<T>,
			asset: Asset,
		) -> DispatchResult {
			ensure!(T::SafeMode::get().deposit_enabled, Error::<T>::LiquidityDepositDisabled);

			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			let (channel_id, deposit_address) =
				T::DepositHandler::request_liquidity_deposit_address(account_id, asset)?;

			let expiry_block =
				frame_system::Pallet::<T>::current_block_number().saturating_add(LpTTL::<T>::get());
			LiquidityChannelExpiries::<T>::append(
				expiry_block,
				(channel_id, deposit_address.clone()),
			);

			Self::deposit_event(Event::LiquidityDepositAddressReady {
				channel_id,
				deposit_address: T::AddressConverter::to_encoded_address(deposit_address),
				expiry_block,
			});

			Ok(())
		}

		/// For when the user wants to withdraw their free balances out of the chain.
		/// Requires a valid foreign chain address.
		#[pallet::weight(T::WeightInfo::withdraw_asset())]
		pub fn withdraw_asset(
			origin: OriginFor<T>,
			amount: AssetAmount,
			asset: Asset,
			destination_address: EncodedAddress,
		) -> DispatchResult {
			ensure!(T::SafeMode::get().withdrawal_enabled, Error::<T>::WithdrawalsDisabled);
			if amount > 0 {
				let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

				let destination_address_internal =
					T::AddressConverter::try_from_encoded_address(destination_address.clone())
						.map_err(|_| {
							DispatchError::Other(
								"Invalid Egress Address, cannot decode the address",
							)
						})?;

				// Check validity of Chain and Asset
				ensure!(
					destination_address_internal.chain() == ForeignChain::from(asset),
					Error::<T>::InvalidEgressAddress
				);

				// Debit the asset from the account.
				Self::try_debit_account(&account_id, asset, amount)?;

				let egress_id = T::EgressHandler::schedule_egress(
					asset,
					amount,
					destination_address_internal,
					None,
				);

				Self::deposit_event(Event::<T>::WithdrawalEgressScheduled {
					egress_id,
					asset,
					amount,
					destination_address,
				});
			}
			Ok(())
		}

		/// Register the account as a Liquidity Provider.
		/// Account roles are immutable once registered.
		#[pallet::weight(T::WeightInfo::register_lp_account())]
		pub fn register_lp_account(who: OriginFor<T>) -> DispatchResult {
			let account_id = ensure_signed(who)?;

			T::AccountRoleRegistry::register_as_liquidity_provider(&account_id)?;

			Ok(())
		}

		/// Sets the lifetime of liquidity deposit channels.
		///
		/// Requires Governance
		///
		/// ## Events
		///
		/// - [On update](Event::LpTtlSet)
		#[pallet::weight(T::WeightInfo::set_lp_ttl())]
		pub fn set_lp_ttl(origin: OriginFor<T>, ttl: T::BlockNumber) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			LpTTL::<T>::set(ttl);

			Self::deposit_event(Event::<T>::LpTtlSet { ttl });
			Ok(())
		}
	}
}

impl<T: Config> LpBalanceApi for Pallet<T> {
	type AccountId = <T as frame_system::Config>::AccountId;

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
}
