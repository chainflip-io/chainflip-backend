#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::DispatchResult;
use sp_std::cmp::{Ord, Ordering};

use cf_chains::AnyChain;
use cf_primitives::{
	AmmRange, Asset, AssetAmount, ForeignChain, ForeignChainAddress, IntentId, Liquidity, PoolSide,
};
use cf_traits::{
	liquidity::LpProvisioningApi, AccountRoleRegistry, Chainflip, EgressApi, IngressApi,
	LiquidityPoolApi, SystemStateInfo,
};

#[cfg(feature = "std")]
// #[cfg(feature = "runtime-benchmarks")]
// mod benchmarking;
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use cf_primitives::EgressId;

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

		/// API to interface with exchange Pools
		type LiquidityPoolApi: LiquidityPoolApi<Self::AccountId>;

		/// For governance checks.
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::error]
	pub enum Error<T> {
		// The user does not have enough fund.
		InsufficientBalance,
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
			ingress_address: ForeignChainAddress,
		},
		WithdrawalEgressScheduled {
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			egress_address: ForeignChainAddress,
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
		#[pallet::weight(0)]
		pub fn request_deposit_address(origin: OriginFor<T>, asset: Asset) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			let (intent_id, ingress_address) =
				T::IngressHandler::register_liquidity_ingress_intent(account_id, asset)?;

			Self::deposit_event(Event::DepositAddressReady { intent_id, ingress_address });

			Ok(())
		}

		/// For when the user wants to withdraw their free balances out of the chain.
		/// Requires a valid foreign chain address.
		#[pallet::weight(0)]
		pub fn withdraw_asset(
			origin: OriginFor<T>,
			amount: AssetAmount,
			asset: Asset,
			egress_address: ForeignChainAddress,
		) -> DispatchResult {
			if amount > 0 {
				T::SystemState::ensure_no_maintenance()?;
				let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

				// Check validity of Chain and Asset
				ensure!(
					ForeignChain::from(egress_address) == ForeignChain::from(asset),
					Error::<T>::InvalidEgressAddress
				);

				// Debit the asset from the account.
				Self::try_debit(&account_id, asset, amount)?;

				let egress_id = T::EgressHandler::schedule_egress(asset, amount, egress_address);

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
		#[pallet::weight(0)]
		pub fn register_lp_account(who: OriginFor<T>) -> DispatchResult {
			let account_id = ensure_signed(who)?;

			T::AccountRoleRegistry::register_as_liquidity_provider(&account_id)?;

			Ok(())
		}

		/// Adjust the current liquidity position for a liquidity pool.
		/// Compare the current liquidity level for the given pool/position with provided one  
		/// and automatically mint/burn liquidity to match the target.
		/// Adding non-zero amount to an non-existant position will create the position.
		/// Adding Zero amount to an existing position will fully burn all liquidity in the
		/// position.
		#[pallet::weight(0)]
		pub fn update_position(
			origin: OriginFor<T>,
			asset: Asset,
			range: AmmRange,
			liquidity_target: Liquidity,
		) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;

			let current_liquidity =
				T::LiquidityPoolApi::minted_liquidity(&account_id, &asset, range);

			match current_liquidity.cmp(&liquidity_target) {
				// Burn the difference
				Ordering::Greater => Self::burn_liquidity(
					account_id,
					asset,
					range,
					current_liquidity.saturating_sub(liquidity_target),
				),
				// Mint the difference
				Ordering::Less => Self::mint_liquidity(
					account_id,
					asset,
					range,
					liquidity_target.saturating_sub(current_liquidity),
				),
				// Do nothing if the liquidity matches.
				Ordering::Equal => Ok(()),
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	fn try_debit(account_id: &T::AccountId, asset: Asset, amount: AssetAmount) -> DispatchResult {
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

	fn credit(account_id: &T::AccountId, asset: Asset, amount: AssetAmount) -> DispatchResult {
		if amount == 0 {
			return Ok(())
		}

		FreeBalances::<T>::mutate(account_id, asset, |maybe_balance| {
			let mut balance = maybe_balance.unwrap_or_default();
			balance = balance.saturating_add(amount);
			*maybe_balance = Some(balance);
		});

		Self::deposit_event(Event::AccountCredited {
			account_id: account_id.clone(),
			asset,
			amount_credited: amount,
		});
		Ok(())
	}

	pub fn mint_liquidity(
		account_id: T::AccountId,
		asset: Asset,
		range: AmmRange,
		liquidity_amount: Liquidity,
	) -> DispatchResult {
		let fees_harvested = T::LiquidityPoolApi::mint(
			account_id.clone(),
			asset,
			range,
			liquidity_amount,
			|amount_to_be_debited| {
				Self::try_debit(&account_id, asset, amount_to_be_debited[PoolSide::Asset0])?;
				Self::try_debit(
					&account_id,
					T::LiquidityPoolApi::STABLE_ASSET,
					amount_to_be_debited[PoolSide::Asset1],
				)?;
				Ok(())
			},
		)?;

		Self::credit(&account_id, asset, fees_harvested[PoolSide::Asset0])?;
		Self::credit(
			&account_id,
			T::LiquidityPoolApi::STABLE_ASSET,
			fees_harvested[PoolSide::Asset1],
		)?;
		Ok(())
	}

	pub fn burn_liquidity(
		account_id: T::AccountId,
		asset: Asset,
		range: AmmRange,
		liquidity_amount: Liquidity,
	) -> DispatchResult {
		let burn_result =
			T::LiquidityPoolApi::burn(account_id.clone(), asset, range, liquidity_amount)?;

		// Credit the user's asset into their account.
		Self::credit(
			&account_id,
			asset,
			burn_result.assets_returned[PoolSide::Asset0]
				.saturating_add(burn_result.fees_accrued[PoolSide::Asset0]),
		)?;
		Self::credit(
			&account_id,
			T::LiquidityPoolApi::STABLE_ASSET,
			burn_result.assets_returned[PoolSide::Asset1]
				.saturating_add(burn_result.fees_accrued[PoolSide::Asset1]),
		)?;

		Ok(())
	}
}

impl<T: Config> LpProvisioningApi for Pallet<T> {
	type AccountId = <T as frame_system::Config>::AccountId;

	fn provision_account(
		account_id: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult {
		Self::credit(account_id, asset, amount).map_err(Into::into)
	}
}
