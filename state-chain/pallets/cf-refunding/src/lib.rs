#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{AnyChain, ForeignChainAddress};
use cf_primitives::{AssetAmount, EpochIndex};
use cf_traits::{impl_pallet_safe_mode, Chainflip, EgressApi};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

use sp_std::vec;

use cf_chains::ForeignChain;

use frame_support::pallet_prelude::*;
pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(0);

impl_pallet_safe_mode!(PalletSafeMode; do_refund);

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::ForeignChain;
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

		/// Safe mode configuration.
		type SafeMode: Get<PalletSafeMode>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The user does not have enough funds.
		InsufficientBalance,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Refund scheduled for a validator.
		RefundScheduled {
			account_id: ForeignChainAddress,
			egress_id: EgressId,
			chain: ForeignChain,
			amount: AssetAmount,
			epoch: EpochIndex,
		},
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Storage for recorded fees per validator and asset.
	#[pallet::storage]
	pub type RecordedFees<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ForeignChain,
		BTreeMap<ForeignChainAddress, AssetAmount>,
		OptionQuery,
	>;

	/// Storage for validator's withheld transaction fees.
	#[pallet::storage]
	pub type WithheldTransactionFees<T: Config> =
		StorageMap<_, Twox64Concat, ForeignChain, AssetAmount, ValueQuery>;
}

impl<T: Config> Pallet<T> {
	pub fn record_gas_fee(
		account_id: ForeignChainAddress,
		chain: ForeignChain,
		gas_fee: AssetAmount,
	) {
		RecordedFees::<T>::mutate(chain, |maybe_fees| {
			if let Some(fees) = maybe_fees {
				fees.entry(account_id).and_modify(|fee| *fee += gas_fee).or_insert(gas_fee);
			} else {
				let mut recorded_fees = BTreeMap::new();
				recorded_fees.insert(account_id, gas_fee);
				*maybe_fees = Some(recorded_fees);
			}
		});
	}
	pub fn withhold_transaction_fee(chain: ForeignChain, amount: AssetAmount) {
		WithheldTransactionFees::<T>::mutate(chain, |fees| *fees += amount);
	}
	pub fn on_distribute_withheld_fees(epoch: EpochIndex) {
		if !T::SafeMode::get().do_refund {
			log::info!("Refunding is disabled. Skipping refunding.");
			return;
		}

		let chains = WithheldTransactionFees::<T>::iter_keys().collect::<Vec<_>>();

		for chain in chains {
			let mut available_funds = WithheldTransactionFees::<T>::get(chain);
			let mut failed_egress: BTreeMap<ForeignChainAddress, AssetAmount> = BTreeMap::new();
			if let Some(recorded_fees) = RecordedFees::<T>::take(chain) {
				for (validator, fee) in recorded_fees {
					if let Some(remaining_funds) = available_funds.checked_sub(fee) {
						if let Ok(egress_details) = T::EgressHandler::schedule_egress(
							chain.gas_asset(),
							fee,
							validator.clone(),
							None,
						) {
							available_funds = remaining_funds;
							Self::deposit_event(Event::RefundScheduled {
								account_id: validator,
								egress_id: egress_details.egress_id,
								chain,
								amount: fee,
								epoch,
							});
						} else {
							failed_egress.insert(validator.clone(), fee);
							log::error!(
								"Failed to schedule egress for validator: {:?} on chain: {:?}",
								validator,
								chain
							);
						}
					}
				}
			}
			if !failed_egress.is_empty() {
				RecordedFees::<T>::insert(chain, failed_egress);
			}
			WithheldTransactionFees::<T>::insert(chain, available_funds);
		}
	}
}
