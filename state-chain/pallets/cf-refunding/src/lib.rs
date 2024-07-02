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
		RefundedMoreThanWithheld {
			chain: ForeignChain,
			refunded: AssetAmount,
			withhold: AssetAmount,
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
	fn do_egress(
		chain: ForeignChain,
		fee: AssetAmount,
		validator: ForeignChainAddress,
		epoch: EpochIndex,
		remaining_funds: AssetAmount,
	) -> Result<(), ()> {
		let amount = if remaining_funds < fee {
			log::error!(
				"Insufficient funds to schedule egress for validator: {:?} on chain: {:?}",
				validator,
				chain
			);
			Self::deposit_event(Event::RefundedMoreThanWithheld {
				chain,
				refunded: fee,
				withhold: remaining_funds,
			});
			fee
		} else {
			fee
		};
		if let Ok(egress_details) =
			T::EgressHandler::schedule_egress(chain.gas_asset(), amount, validator.clone(), None)
		{
			Self::deposit_event(Event::RefundScheduled {
				account_id: validator,
				egress_id: egress_details.egress_id,
				chain,
				amount,
				epoch,
			});
			Ok(())
		} else {
			log::error!(
				"Failed to schedule egress for validator: {:?} on chain: {:?}",
				validator,
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
			let mut withheld_fees = WithheldTransactionFees::<T>::get(chain);
			let recorded_fees = RecordedFees::<T>::take(chain);
			let sum_recorded_fees: AssetAmount =
				recorded_fees.clone().unwrap_or_default().values().sum();
			if withheld_fees < sum_recorded_fees {
				Self::deposit_event(Event::RefundedMoreThanWithheld {
					chain,
					refunded: sum_recorded_fees,
					withhold: withheld_fees,
				});
			}
			match chain {
				ForeignChain::Ethereum | ForeignChain::Arbitrum => {
					let mut failed_egress: BTreeMap<ForeignChainAddress, AssetAmount> =
						BTreeMap::new();
					if let Some(recorded_fees) = recorded_fees {
						for (validator, fee) in recorded_fees {
							if Self::do_egress(chain, fee, validator.clone(), epoch, withheld_fees)
								.is_ok()
							{
								withheld_fees = withheld_fees.saturating_sub(fee);
							} else {
								failed_egress.insert(validator.clone(), fee);
							}
						}
					}
					if !failed_egress.is_empty() {
						RecordedFees::<T>::insert(chain, failed_egress);
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
							sum_recorded_fees,
							cf_chains::ForeignChainAddress::Dot(vault_address),
							epoch,
							withheld_fees,
						)
						.is_ok()
						{
							withheld_fees = withheld_fees.saturating_sub(sum_recorded_fees);
						}
					}
					WithheldTransactionFees::<T>::insert(chain, withheld_fees);
				},
			}
		}
	}
}
