#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{assets::any::AssetMap, AnyChain, ForeignChain, ForeignChainAddress};
use cf_primitives::{AccountId, Asset, AssetAmount};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode, AssetWithholding, BalanceApi, Chainflip, EgressApi, KeyProvider,
	LiabilityTracker, ScheduledEgressDetails,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::traits::Saturating,
	storage::transactional::with_storage_layer,
	traits::{DefensiveSaturating, OnKilledAccount},
};
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(0);

pub const REFUND_FEE_MULTIPLE: AssetAmount = 100;

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum ExternalOwner {
	Vault,
	AggKey,
	Account(ForeignChainAddress),
}

impl From<ForeignChainAddress> for ExternalOwner {
	fn from(address: ForeignChainAddress) -> Self {
		ExternalOwner::Account(address)
	}
}

// The implementation for the **Ord** trait is required to use ExternalOwner as a key in a BTreeMap.
impl core::cmp::Ord for ExternalOwner {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		match (self, other) {
			(ExternalOwner::Vault, ExternalOwner::Vault) => core::cmp::Ordering::Equal,
			(ExternalOwner::Vault, _) => core::cmp::Ordering::Less,
			(_, ExternalOwner::Vault) => core::cmp::Ordering::Greater,
			(ExternalOwner::AggKey, ExternalOwner::AggKey) => core::cmp::Ordering::Equal,
			(ExternalOwner::AggKey, ExternalOwner::Account(_)) => core::cmp::Ordering::Less,
			(ExternalOwner::Account(a), ExternalOwner::Account(b)) => a.cmp(b),
			(ExternalOwner::Account(_), _) => core::cmp::Ordering::Greater,
		}
	}
}

impl core::cmp::PartialOrd for ExternalOwner {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl_pallet_safe_mode!(PalletSafeMode; reconciliation_enabled);

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::{dot::PolkadotCrypto, ForeignChain};
	use cf_primitives::EgressId;

	use super::*;
	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Handles egress for all chains.
		type EgressHandler: EgressApi<AnyChain>;

		/// Polkadot environment.
		type PolkadotKeyProvider: KeyProvider<PolkadotCrypto>;

		/// Safe mode configuration.
		type SafeMode: Get<PalletSafeMode>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Refund amount is too low to cover the egress fees. In this case, the refund is skipped.
		RefundAmountTooLow,
		/// The user does not have enough funds.
		InsufficientBalance,
		/// The user has reached the maximum balance.
		BalanceOverflow,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Refund scheduled for a validator.
		RefundScheduled {
			egress_id: EgressId,
			destination: ForeignChainAddress,
			amount: AssetAmount,
		},
		/// The refund was skipped because of the given reason.
		RefundSkipped { reason: DispatchError, chain: ForeignChain, address: ForeignChainAddress },
		/// The Vault is running a deficit: we owe more than we have set aside for refunds.
		VaultDeficitDetected {
			chain: ForeignChain,
			amount_owed: AssetAmount,
			available: AssetAmount,
		},
		/// The account was debited.
		AccountDebited {
			account_id: T::AccountId,
			asset: Asset,
			amount_debited: AssetAmount,
			new_balance: AssetAmount,
		},
		/// The account was credited.
		AccountCredited {
			account_id: T::AccountId,
			asset: Asset,
			amount_credited: AssetAmount,
			new_balance: AssetAmount,
		},
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Liabilities are funds that are owed to some external party.
	#[pallet::storage]
	pub type Liabilities<T: Config> =
		StorageMap<_, Twox64Concat, Asset, BTreeMap<ExternalOwner, AssetAmount>, ValueQuery>;

	/// Funds that have been set aside to refund external [Liabilities].
	#[pallet::storage]
	pub type WithheldAssets<T: Config> =
		StorageMap<_, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	#[pallet::storage]
	/// Storage for user's free balances.
	pub type FreeBalances<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Twox64Concat,
		Asset,
		AssetAmount,
		ValueQuery,
	>;
}

impl<T: Config> Pallet<T> {
	fn refund_via_egress(
		chain: ForeignChain,
		address: ForeignChainAddress,
		amount: AssetAmount,
	) -> Result<(), DispatchError> {
		match with_storage_layer(|| {
			T::EgressHandler::schedule_egress(chain.gas_asset(), amount, address.clone(), None)
				.map_err(Into::into)
				.and_then(
					|result @ ScheduledEgressDetails { egress_amount, fee_withheld, .. }| {
						if egress_amount < REFUND_FEE_MULTIPLE * fee_withheld {
							Err(Error::<T>::RefundAmountTooLow.into())
						} else {
							Ok(result)
						}
					},
				)
		}) {
			Ok(ScheduledEgressDetails { egress_id, .. }) => {
				Self::deposit_event(Event::RefundScheduled {
					egress_id,
					destination: address,
					amount,
				});
				Ok(())
			},
			Err(err) => {
				Self::deposit_event(Event::RefundSkipped { reason: err, chain, address });
				Err(err)
			},
		}
	}

	// Reconciles the amount owed with the amount available for distribution.
	//
	// The owed and available amount are mutated in place.
	//
	// For Ethereum and Arbitrum, we expect to pay out the validators via egress to their
	// accounts. For Polkadot, we expect to pay out to the current AggKey account.
	// For Bitcoin and Solana, the vault pays the fees directly so we don't need to egress
	// anything.
	//
	// Note that we refund to accounts atomically (we never partially refund an account), whereas
	// refunds to vaults or agg-keys can be made incrementally.
	fn reconcile(
		chain: ForeignChain,
		owner: &ExternalOwner,
		amount_owed: &mut AssetAmount,
		available: &mut AssetAmount,
	) -> Result<(), DispatchError> {
		if *amount_owed > *available {
			Self::deposit_event(Event::VaultDeficitDetected {
				chain,
				amount_owed: *amount_owed,
				available: *available,
			});
		}
		let amount_reconciled = match chain {
			ForeignChain::Ethereum | ForeignChain::Arbitrum => match owner {
				ExternalOwner::Account(address) =>
					if *amount_owed > *available {
						0
					} else {
						Self::refund_via_egress(chain, address.clone(), *amount_owed)?;
						*amount_owed
					},
				other => {
					log_or_panic!(
						"Expected ExternalOwner::Account for EVM chains, got {:?}.",
						other
					);
					0
				},
			},
			ForeignChain::Polkadot => match owner {
				ExternalOwner::AggKey => {
					if let Some(active_key) = T::PolkadotKeyProvider::active_epoch_key() {
						let refund_amount = core::cmp::min(*amount_owed, *available);
						Self::refund_via_egress(
							chain,
							ForeignChainAddress::Dot(active_key.key),
							refund_amount,
						)?;
						refund_amount
					} else {
						log_or_panic!("No active epoch key found for Polkadot.");
						0
					}
				},
				other => {
					log_or_panic!(
						"{:?} Liabilities are not supported for chain {:?}.",
						other,
						chain
					);
					0
				},
			},
			ForeignChain::Bitcoin | ForeignChain::Solana => match owner {
				ExternalOwner::Vault => core::cmp::min(*amount_owed, *available),
				other => {
					log_or_panic!(
						"{:?} Liabilities are not supported for chain {:?}.",
						other,
						chain
					);
					0
				},
			},
		};

		available.defensive_saturating_reduce(amount_reconciled);
		amount_owed.defensive_saturating_reduce(amount_reconciled);

		Ok(())
	}

	/// Triggers the reconciliation process for all chains.
	/// This function will refund the owed assets to the appropriate accounts following the chain
	/// specific requirements.
	pub fn trigger_reconciliation() {
		if !T::SafeMode::get().reconciliation_enabled {
			log::info!("Reconciliation is disabled. Skipping reconciliation.");
			return;
		}

		for chain in ForeignChain::iter() {
			WithheldAssets::<T>::mutate(chain.gas_asset(), |total_withheld| {
				let mut owed_assets =
					Liabilities::<T>::take(chain.gas_asset()).into_iter().collect::<Vec<_>>();
				owed_assets.sort_by_key(|(_, amount)| core::cmp::Reverse(*amount));

				for (destination, amount) in owed_assets.iter_mut() {
					debug_assert!(*amount > 0);
					let _ = Self::reconcile(chain, destination, amount, total_withheld);
					if *total_withheld == 0 {
						break;
					}
				}

				owed_assets.retain(|(_, amount)| *amount > 0);
				if !owed_assets.is_empty() {
					Liabilities::<T>::insert(
						chain.gas_asset(),
						owed_assets.into_iter().collect::<BTreeMap<_, _>>(),
					);
				}
			});
		}
	}

	pub fn vault_imbalance(asset: Asset) -> VaultImbalance<AssetAmount> {
		let owed = Liabilities::<T>::get(asset).values().sum::<u128>();
		let withheld = WithheldAssets::<T>::get(asset);
		if owed > withheld {
			VaultImbalance::Deficit(owed - withheld)
		} else {
			VaultImbalance::Surplus(withheld - owed)
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug, Serialize, Deserialize)]
pub enum VaultImbalance<A> {
	/// There are more withheld assets than what is owed.
	Surplus(A),
	/// There are more assets owed than what is withheld.
	Deficit(A),
}

impl<A> VaultImbalance<A> {
	pub fn map<B>(&self, f: impl FnOnce(&A) -> B) -> VaultImbalance<B> {
		match self {
			VaultImbalance::Surplus(amount) => VaultImbalance::Surplus(f(amount)),
			VaultImbalance::Deficit(amount) => VaultImbalance::Deficit(f(amount)),
		}
	}
}

impl<T: Config> LiabilityTracker for Pallet<T> {
	fn record_liability(address: ForeignChainAddress, asset: Asset, amount: AssetAmount) {
		debug_assert_eq!(ForeignChain::from(asset), address.chain());
		Liabilities::<T>::mutate(asset, |fees| {
			fees.entry(match ForeignChain::from(asset) {
				ForeignChain::Ethereum | ForeignChain::Arbitrum => address.into(),
				ForeignChain::Polkadot => ExternalOwner::AggKey,
				ForeignChain::Bitcoin | ForeignChain::Solana => ExternalOwner::Vault,
			})
			.and_modify(|fee| fee.saturating_accrue(amount))
			.or_insert(amount);
		});
	}

	#[cfg(feature = "try-runtime")]
	fn total_liabilities(asset: Asset) -> u128 {
		Liabilities::<T>::get(asset).values().sum()
	}
}

impl<T: Config> AssetWithholding for Pallet<T> {
	fn withhold_assets(asset: Asset, amount: AssetAmount) {
		WithheldAssets::<T>::mutate(asset, |fees| {
			fees.saturating_accrue(amount);
		});
	}

	#[cfg(feature = "try-runtime")]
	fn withheld_assets(asset: Asset) -> AssetAmount {
		WithheldAssets::<T>::get(asset)
	}
}

impl<T: Config> BalanceApi for Pallet<T>
where
	Vec<(AccountId, cf_primitives::Asset, u128)>:
		From<Vec<(<T as frame_system::Config>::AccountId, cf_primitives::Asset, u128)>>,
{
	type AccountId = T::AccountId;

	fn try_credit_account(
		account_id: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult {
		if amount == 0 {
			return Ok(())
		}

		let new_balance = FreeBalances::<T>::try_mutate(account_id, asset, |balance| {
			*balance = balance.checked_add(amount).ok_or(Error::<T>::BalanceOverflow)?;
			Ok::<_, Error<T>>(*balance)
		})?;

		Self::deposit_event(Event::AccountCredited {
			account_id: account_id.clone(),
			asset,
			amount_credited: amount,
			new_balance,
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

		let new_balance = FreeBalances::<T>::try_mutate_exists(account_id, asset, |balance| {
			let new_balance = match balance.take() {
				None => Err(Error::<T>::InsufficientBalance),
				Some(balance) =>
					Ok(balance.checked_sub(amount).ok_or(Error::<T>::InsufficientBalance)?),
			}?;
			if new_balance > 0 {
				*balance = Some(new_balance);
			}
			Ok::<_, Error<T>>(new_balance)
		})?;

		Self::deposit_event(Event::AccountDebited {
			account_id: account_id.clone(),
			asset,
			amount_debited: amount,
			new_balance,
		});

		Ok(())
	}

	fn free_balances(who: &Self::AccountId) -> AssetMap<AssetAmount> {
		AssetMap::from_fn(|asset| FreeBalances::<T>::get(who, asset))
	}

	fn get_balance(who: &Self::AccountId, asset: Asset) -> AssetAmount {
		FreeBalances::<T>::get(who, asset)
	}
}

pub struct DeleteAccount<T: Config>(PhantomData<T>);

impl<T: Config> OnKilledAccount<T::AccountId> for DeleteAccount<T> {
	fn on_killed_account(who: &T::AccountId) {
		let _ = FreeBalances::<T>::clear_prefix(who, u32::MAX, None);
	}
}
