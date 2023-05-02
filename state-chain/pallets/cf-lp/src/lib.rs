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
use sp_runtime::{traits::BlockNumberProvider, Saturating};
use sp_std::vec::Vec;

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

		/// API for handling asset ingress.
		type IngressHandler: IngressApi<
			AnyChain,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;

		/// API for handling asset egress.
		type EgressHandler: EgressApi<AnyChain>;

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

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			let expired = IngressIntentExpiries::<T>::take(n);
			for (intent_id, chain, address) in expired.clone() {
				T::IngressHandler::expire_intent(chain, intent_id, address.clone());
				Self::deposit_event(Event::DepositAddressExpired {
					address: T::AddressConverter::try_to_encoded_address(address).expect("This should not fail since this conversion already succeeded when expiry was scheduled"),
				});
			}
			T::WeightInfo::on_initialize(expired.len() as u32)
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
		DepositAddressReady {
			intent_id: IntentId,
			ingress_address: EncodedAddress,
			expiry_block: T::BlockNumber,
		},
		DepositAddressExpired {
			address: EncodedAddress,
		},
		WithdrawalEgressScheduled {
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			egress_address: EncodedAddress,
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

	/// Stores a block for when an intent will expire against the intent infos.
	#[pallet::storage]
	pub(super) type IngressIntentExpiries<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::BlockNumber,
		Vec<(IntentId, ForeignChain, cf_chains::ForeignChainAddress)>,
		ValueQuery,
	>;

	/// The TTL for liquidity provision intents.
	#[pallet::storage]
	pub type LpTTL<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

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

			let expiry_block =
				frame_system::Pallet::<T>::current_block_number().saturating_add(LpTTL::<T>::get());
			IngressIntentExpiries::<T>::append(
				expiry_block,
				(intent_id, ForeignChain::from(asset), ingress_address.clone()),
			);

			Self::deposit_event(Event::DepositAddressReady {
				intent_id,
				ingress_address: T::AddressConverter::try_to_encoded_address(ingress_address)?,
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
			egress_address: EncodedAddress,
		) -> DispatchResult {
			if amount > 0 {
				T::SystemState::ensure_no_maintenance()?;
				let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

				let egress_address_internal = T::AddressConverter::try_from_encoded_address(
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

		/// Sets the length in which ingress intents are expired in the LP pallet.
		/// Requires Governance
		///
		/// ## Events
		///
		/// - [On update](Event::LpTtlSet)
		#[pallet::weight(T::WeightInfo::set_lp_ttl())]
		pub fn set_lp_ttl(origin: OriginFor<T>, ttl: T::BlockNumber) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
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
