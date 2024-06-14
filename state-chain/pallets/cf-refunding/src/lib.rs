#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{AnyChain, ForeignChainAddress};
use cf_primitives::{Asset, AssetAmount, EpochIndex};
use cf_traits::{impl_pallet_safe_mode, Chainflip, EgressApi};
use sp_std::collections::btree_map::BTreeMap;

use sp_std::vec;

use frame_support::pallet_prelude::*;
pub use pallet::*;

mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

// pub mod migrations;
// pub mod weights;
// pub use weights::WeightInfo;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(0);

impl_pallet_safe_mode!(PalletSafeMode; ensure_something);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Handles egress for all chains.
		type EgressHandler: EgressApi<AnyChain>;

		// /// API for handling asset egress.
		// type EgressHandler: EgressApi<AnyChain>;

		// type SafeMode: Get<PalletSafeMode>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The user does not have enough funds.
		InsufficientBalance,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		RefundScheduled {
			account_id: ForeignChainAddress,
			asset: Asset,
			amount: AssetAmount,
			epoch: EpochIndex,
		},
		RefundIntegrityCheckFailed {
			epoch: EpochIndex,
			asset: Asset,
		},
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Storage for recorded fees per validator and asset.
	#[pallet::storage]
	pub type RecordedFees<T: Config> =
		StorageMap<_, Twox64Concat, Asset, BTreeMap<ForeignChainAddress, AssetAmount>, ValueQuery>;

	/// Storage for validator's withheld transaction fees.
	#[pallet::storage]
	pub type WithheldTransactionFees<T: Config> =
		StorageMap<_, Twox64Concat, Asset, AssetAmount, ValueQuery>;
}

impl<T: Config> Pallet<T> {
	pub fn record_gas_fee(account_id: ForeignChainAddress, asset: Asset, gas_fee: AssetAmount) {
		RecordedFees::<T>::mutate(asset, |recorded_fees| {
			recorded_fees
				.entry(account_id)
				.and_modify(|fee| *fee += gas_fee)
				.or_insert(gas_fee);
		});
	}
	pub fn withheld_transaction_fee(asset: Asset, amount: AssetAmount) {
		WithheldTransactionFees::<T>::mutate(asset, |fees| *fees += amount);
	}
	pub fn on_distribute_withheld_fees(epoch: EpochIndex) {
		let assets = WithheldTransactionFees::<T>::iter_keys().collect::<Vec<_>>();

		for asset in assets {
			// Integrity check before we start refunding
			if WithheldTransactionFees::<T>::get(asset) <
				RecordedFees::<T>::get(asset).values().sum()
			{
				log::warn!(
                    "Integrity check for refunding failed for asset {:?}. Refunding will be skipped.", asset);
				Self::deposit_event(Event::RefundIntegrityCheckFailed { asset, epoch });
				continue;
			}
			let mut available_funds = WithheldTransactionFees::<T>::take(asset);
			let recorded_fees = RecordedFees::<T>::take(asset);
			for (validator, fee) in recorded_fees {
				if let Some(remaining_funds) = available_funds.checked_sub(fee) {
					available_funds = remaining_funds;
					let _ = T::EgressHandler::schedule_egress(asset, fee, validator.clone(), None);
					Self::deposit_event(Event::RefundScheduled {
						account_id: validator,
						asset,
						amount: fee,
						epoch,
					});
				} else {
					// TODO: This actually can never happen, still better to remember the data in a
					// seperate storage item?
					log::warn!(
                        "Insufficient funds to refund validator {:?} for asset {:?}. Refunding not possible!",
                        validator,
                        asset,
                    );
				}
			}
		}
	}
}
