#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{dot::api::PolkadotEnvironment, AnyChain, ForeignChainAddress};
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

/// Holds all infos we need to refund a validator/vault/aggKey for any chain type.
#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum RefundingInfo {
	/// Only a single key to refund that is either always the current key or not relevant.
	Single(AssetAmount),
	/// Many keys/validators to refund.
	Multiple(BTreeMap<ForeignChainAddress, AssetAmount>),
}

impl RefundingInfo {
	pub fn sum(&self) -> AssetAmount {
		match self {
			RefundingInfo::Single(amount) => *amount,
			RefundingInfo::Multiple(map) => map.values().sum(),
		}
	}

	pub fn get_as_multiple(&self) -> Option<&BTreeMap<ForeignChainAddress, AssetAmount>> {
		match self {
			RefundingInfo::Multiple(map) => Some(map),
			_ => None,
		}
	}

	pub fn get_as_single(&self) -> Option<AssetAmount> {
		match self {
			RefundingInfo::Single(amount) => Some(*amount),
			_ => None,
		}
	}
}

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

		/// Polkadot environment.
		type PolkadotEnvironment: PolkadotEnvironment;

		/// Safe mode configuration.
		type SafeMode: Get<PalletSafeMode>;
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
		/// We paied more transaction fees than we withheld.
		VaultBleeding { chain: ForeignChain, collected: AssetAmount, withheld: AssetAmount },
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Storage for recorded fees per validator and asset.
	#[pallet::storage]
	pub type RecordedFees<T: Config> =
		StorageMap<_, Twox64Concat, ForeignChain, RefundingInfo, OptionQuery>;

	/// Storage for validator's withheld transaction fees.
	#[pallet::storage]
	pub type WithheldTransactionFees<T: Config> =
		StorageMap<_, Twox64Concat, ForeignChain, AssetAmount, ValueQuery>;
}

impl<T: Config> Pallet<T> {
	fn do_egress(
		chain: ForeignChain,
		address: ForeignChainAddress,
		amount: AssetAmount,
		epoch: EpochIndex,
	) -> Result<(), ()> {
		if amount == 0 {
			return Err(())
		}
		if let Ok(egress_details) =
			T::EgressHandler::schedule_egress(chain.gas_asset(), amount, address.clone(), None)
		{
			Self::deposit_event(Event::RefundScheduled {
				account_id: address,
				egress_id: egress_details.egress_id,
				chain,
				amount,
				epoch,
			});
			Ok(())
		} else {
			log::error!(
				"Failed to schedule egress for address: {:?} on chain: {:?}",
				address,
				chain
			);
			Err(())
		}
	}

	pub fn record_gas_fee(
		account_id: ForeignChainAddress,
		chain: ForeignChain,
		gas_fee: AssetAmount,
	) {
		RecordedFees::<T>::mutate(chain, |maybe_fees| match chain {
			ForeignChain::Ethereum | ForeignChain::Arbitrum => {
				if let Some(RefundingInfo::Multiple(fees)) = maybe_fees {
					fees.entry(account_id).and_modify(|fee| *fee += gas_fee).or_insert(gas_fee);
				} else {
					let mut recorded_fees = BTreeMap::new();
					recorded_fees.insert(account_id, gas_fee);
					*maybe_fees = Some(RefundingInfo::Multiple(recorded_fees));
				}
			},
			_ =>
				if let Some(RefundingInfo::Single(amount)) = maybe_fees {
					*maybe_fees = Some(RefundingInfo::Single(amount.saturating_add(gas_fee)));
				} else {
					*maybe_fees = Some(RefundingInfo::Single(gas_fee));
				},
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
			let mut withheld_fees = WithheldTransactionFees::<T>::get(chain);
			let recorded_fees = RecordedFees::<T>::take(chain);
			let sum_recorded_fees: AssetAmount =
				recorded_fees.as_ref().map_or(0, |fees| fees.sum());
			if withheld_fees < sum_recorded_fees {
				// We are refunding more than we withheld -> That indicates that we have to adjust
				// the fees.
				Self::deposit_event(Event::VaultBleeding {
					chain,
					collected: sum_recorded_fees,
					withheld: withheld_fees,
				});
			}
			match chain {
				ForeignChain::Ethereum | ForeignChain::Arbitrum => {
					let mut retry_next_epoch: BTreeMap<ForeignChainAddress, AssetAmount> =
						BTreeMap::new();
					if let Some(recorded_fees) = recorded_fees {
						if let Some(recorded_fees) = recorded_fees.get_as_multiple() {
							for (address, fee) in recorded_fees {
								if withheld_fees.checked_sub(*fee).is_none() {
									retry_next_epoch.insert(address.clone(), *fee);
									break;
								}
								if Self::do_egress(chain, address.clone(), *fee, epoch).is_ok() {
									withheld_fees = withheld_fees.saturating_sub(*fee);
								} else {
									retry_next_epoch.insert(address.clone(), *fee);
								}
							}
						}
					}
					// If an egress failed or we have not enough funds left we should remember the
					// funds we still have to refund.
					if !retry_next_epoch.is_empty() {
						RecordedFees::<T>::insert(chain, RefundingInfo::Multiple(retry_next_epoch));
					}
					WithheldTransactionFees::<T>::insert(chain, withheld_fees);
				},
				ForeignChain::Bitcoin | ForeignChain::Solana =>
					if withheld_fees >= sum_recorded_fees {
						withheld_fees = withheld_fees.saturating_sub(sum_recorded_fees);
						WithheldTransactionFees::<T>::insert(chain, withheld_fees);
					},
				ForeignChain::Polkadot => {
					if let Some(vault_address) = T::PolkadotEnvironment::try_vault_account() {
						if Self::do_egress(
							chain,
							cf_chains::ForeignChainAddress::Dot(vault_address),
							sum_recorded_fees,
							epoch,
						)
						.is_ok()
						{
							withheld_fees = withheld_fees.saturating_sub(sum_recorded_fees);
						} else {
							RecordedFees::<T>::insert(
								chain,
								RefundingInfo::Single(sum_recorded_fees),
							);
						}
					}
					WithheldTransactionFees::<T>::insert(chain, withheld_fees);
				},
			}
		}
	}
}
