#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_primitives::{Asset, AssetAmount, ForeignChain};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::DispatchResult;

use cf_chains::{address::AddressConverter, AnyChain};
use cf_traits::{
	liquidity::LpBalanceApi, AccountRoleRegistry, Chainflip, EgressApi, IngressApi, SystemStateInfo,
};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::address::EncodedAddress;
	use cf_primitives::{EgressId, IntentId};

	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;

		/// API for handling asset ingress.
		type IngressHandler: IngressApi<
			AnyChain,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;

		/// API for handling asset egress.
		type EgressHandler: EgressApi<AnyChain>;

		/// For governance checks.
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;

		/// A converter to convert address to and from human readable to internal address
		/// representation.
		type AddressConverter: AddressConverter;

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
		DepositAddressReady {
			intent_id: IntentId,
			ingress_address: EncodedAddress,
		},
		WithdrawalEgressScheduled {
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			egress_address: EncodedAddress,
		},
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	/// Storage for user's free balances/ DoubleMap: (AccountId, Asset) => Balance
	pub type FreeBalances<T: Config> =
		StorageDoubleMap<_, Twox64Concat, T::AccountId, Identity, Asset, AssetAmount>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// For when the user wants to deposit assets into the Chain.
		/// Generates a new ingress address for the user to posit their assets.
		#[pallet::weight(T::WeightInfo::request_deposit_address())]
		pub fn request_deposit_address(origin: OriginFor<T>, asset: Asset) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			let (intent_id, ingress_address) =
				T::IngressHandler::register_liquidity_ingress_intent(account_id, asset)?;

			Self::deposit_event(Event::DepositAddressReady {
				intent_id,
				ingress_address: T::AddressConverter::to_encoded_address(ingress_address)?,
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
			egress_address: EncodedAddress,
		) -> DispatchResult {
			if amount > 0 {
				T::SystemState::ensure_no_maintenance()?;
				let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

				let egress_address_internal = T::AddressConverter::from_encoded_address(
					egress_address.clone(),
				)
				.map_err(|_| {
					DispatchError::Other("Invalid Egress Address, cannot decode the address")
				})?;

				// Check validity of Chain and Asset
				ensure!(
					ForeignChain::from(egress_address_internal.clone()) ==
						ForeignChain::from(asset),
					Error::<T>::InvalidEgressAddress
				);

				// Debit the asset from the account.
				Self::try_debit_account(&account_id, asset, amount)?;

				let egress_id =
					T::EgressHandler::schedule_egress(asset, amount, egress_address_internal, None);

				Self::deposit_event(Event::<T>::WithdrawalEgressScheduled {
					egress_id,
					asset,
					amount,
					egress_address,
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
