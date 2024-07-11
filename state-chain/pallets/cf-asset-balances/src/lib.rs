#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{AnyChain, ForeignChain, ForeignChainAddress};
use cf_primitives::{Asset, AssetAmount};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode, AssetWithholding, Chainflip, EgressApi, KeyProvider, LiabilityTracker,
};
use frame_support::{
	pallet_prelude::*, sp_runtime::traits::Saturating, traits::DefensiveSaturating,
};
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(0);

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

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Refund scheduled for a validator.
		RefundScheduled {
			egress_id: EgressId,
			destination: ForeignChainAddress,
			amount: AssetAmount,
		},
		/// The Vault is running a deficit: we owe more than we have set aside for refunds.
		VaultDeficitDetected {
			chain: ForeignChain,
			amount_owed: AssetAmount,
			available: AssetAmount,
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
}

impl<T: Config> Pallet<T> {
	fn refund_via_egress(
		chain: ForeignChain,
		address: ForeignChainAddress,
		amount: AssetAmount,
	) -> Result<(), DispatchError> {
		let egress_details =
			T::EgressHandler::schedule_egress(chain.gas_asset(), amount, address.clone(), None)
				.map_err(Into::into)?;
		Self::deposit_event(Event::RefundScheduled {
			egress_id: egress_details.egress_id,
			destination: address,
			amount,
		});
		Ok(())
	}

	/// Reconciles the amount owed with the amount available for distribution.
	///
	/// The owed and available amount are mutated in place.
	///
	/// For Ethereum and Arbitrum, we expect to the validators and pay out via egress to their
	/// accounts. For Polkadot, we expect to pay out to the current AggKey account.
	/// For Bitcoin and Solana, the vault pays the fees directly so we don't need to egress
	/// anything.
	///
	/// Note that we refund to accounts atomically (we never partially refund an account), whereas
	/// refunds to vaults or aggkeys can be made incrementally.
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
						"{:?} Liabilities are not supported for chain {:?}.",
						other,
						chain
					);
					0
				},
			},
			ForeignChain::Polkadot => {
				match owner {
					ExternalOwner::AggKey => {
						let address = ForeignChainAddress::Dot(
							T::PolkadotKeyProvider::active_epoch_key()
								// TODO: make this more defensive
								.expect("Key must exist otherwise we would have nothing to refund.")
								.key,
						);
						let refund_amount = core::cmp::min(*amount_owed, *available);
						Self::refund_via_egress(chain, address, refund_amount)?;
						refund_amount
					},
					other => {
						log_or_panic!(
							"{:?} Liabilities are not supported for chain {:?}.",
							other,
							chain
						);
						0
					},
				}
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

	pub fn trigger_reconciliation() {
		if !T::SafeMode::get().reconciliation_enabled {
			log::info!("Reconciliation is disabled. Skipping reconciliation.");
			return;
		}

		for chain in ForeignChain::iter() {
			let owed_assets = Liabilities::<T>::take(chain.gas_asset());
			WithheldAssets::<T>::mutate(chain.gas_asset(), |total_withheld| {
				let mut owed_assets = owed_assets.into_iter().collect::<Vec<_>>();
				owed_assets.sort_by_key(|(_, amount)| core::cmp::Reverse(*amount));

				for (destination, amount) in &mut owed_assets {
					debug_assert!(*amount > 0);
					match Self::reconcile(chain, destination, amount, total_withheld) {
						Ok(_) =>
							if *total_withheld == 0 {
								break;
							},
						Err(_) => break,
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
