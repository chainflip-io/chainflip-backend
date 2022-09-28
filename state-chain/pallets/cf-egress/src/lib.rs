#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
mod weights;

use cf_chains::{AllBatch, Ethereum, TransferAssetParams};
use cf_primitives::{
	AssetAmount, EgressBatch, ForeignChain, ForeignChainAddress, ForeignChainAsset,
};
use cf_traits::{Broadcaster, EgressApi, EthereumAssetsAddressProvider, ReplayProtectionProvider};
use frame_support::pallet_prelude::*;
pub use pallet::*;
use sp_std::vec;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	use cf_traits::Chainflip;
	use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Replay protection.
		type EthereumReplayProtection: ReplayProtectionProvider<Ethereum>;

		/// The type of the chain-native transaction.
		type EthereumEgressTransaction: AllBatch<Ethereum>;

		/// A broadcaster instance.
		type EthereumBroadcaster: Broadcaster<Ethereum, ApiCall = Self::EthereumEgressTransaction>;

		/// Governance origin to manage allowed assets
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// An API for getting Ethereum related parameters
		type EthereumAssetsAddressProvider: EthereumAssetsAddressProvider;

		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

	#[pallet::storage]
	pub(crate) type ScheduledEgress<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ForeignChainAsset,
		EgressBatch<AssetAmount, ForeignChainAddress>,
		ValueQuery,
	>;

	// Stores the list of assets that are not allowed to be egressed.
	#[pallet::storage]
	pub(crate) type DisabledEgressAssets<T: Config> =
		StorageMap<_, Twox64Concat, ForeignChainAsset, (), OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AssetEgressDisabled {
			asset: ForeignChainAsset,
			disabled: bool,
		},
		EgressScheduled {
			foreign_asset: ForeignChainAsset,
			amount: AssetAmount,
			egress_address: ForeignChainAddress,
		},
		EgressBroadcasted {
			foreign_asset: ForeignChainAsset,
			batch_size: u32,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		// The given asset is not allowed to be Egressed
		AssetEgressDisabled,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Take a batch of scheduled Egress and send them out
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut weights_left = remaining_weight;

			ScheduledEgress::<T>::iter().for_each(|(asset, batch)| {
				if DisabledEgressAssets::<T>::get(asset).is_none() {
					let egress_weight = T::WeightInfo::send_batch_egress(batch.len() as u32);
					if weights_left >= egress_weight {
						Self::send_scheduled_batch_transaction(asset, batch);
						weights_left = weights_left.saturating_sub(egress_weight);
						ScheduledEgress::<T>::remove(&asset);
					}
				}
			});

			remaining_weight.saturating_sub(weights_left)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets if an asset is not allowed to be sent out of the chain via Egress.
		/// Requires Governance
		///
		/// ## Events
		///
		/// - [On update](Event::AssetEgressDisabled)
		#[pallet::weight(T::WeightInfo::disable_asset_egress())]
		pub fn disable_asset_egress(
			origin: OriginFor<T>,
			asset: ForeignChainAsset,
			disabled: bool,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			let asset_is_disabled = DisabledEgressAssets::<T>::contains_key(asset);
			if disabled && !asset_is_disabled {
				DisabledEgressAssets::<T>::insert(asset, ());
			} else if !disabled && asset_is_disabled {
				DisabledEgressAssets::<T>::remove(asset);
			}

			Self::deposit_event(Event::<T>::AssetEgressDisabled { asset, disabled });

			Ok(())
		}

		/// Send all scheduled egress out for an asset, ignoring weight constraint.
		/// Requires governance
		///
		/// ## Events
		///
		/// - [on_success](Event::EgressBroadcasted)
		#[pallet::weight(T::WeightInfo::send_batch_egress(0))]
		pub fn send_scheduled_egress_for_asset(
			origin: OriginFor<T>,
			foreign_asset: ForeignChainAsset,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			ensure!(
				DisabledEgressAssets::<T>::get(foreign_asset).is_none(),
				Error::<T>::AssetEgressDisabled
			);

			Self::send_scheduled_batch_transaction(
				foreign_asset,
				ScheduledEgress::<T>::take(foreign_asset),
			);

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	// Take all scheduled batch Egress for an asset and send them out as a batch.
	fn send_scheduled_batch_transaction(
		foreign_asset: ForeignChainAsset,
		batch: EgressBatch<AssetAmount, ForeignChainAddress>,
	) {
		// Take the scheduled Egress calls to be sent out of storage.
		let batch_size = batch.len() as u32;
		if batch_size == 0 {
			return
		}

		// Construct the Egress Tx and send it out.
		// NOTE: currently, we only support Ethereum chain.
		if foreign_asset.chain == ForeignChain::Ethereum {
			let asset_address =
				T::EthereumAssetsAddressProvider::try_get_asset_address(foreign_asset.asset)
					.expect("Asset is guaranteed to be supported.");
			let egress_transaction = T::EthereumEgressTransaction::new_unsigned(
				T::EthereumReplayProtection::replay_protection(),
				vec![], // No incoming asset
				batch
					.iter()
					.filter_map(|(amount, address)| match address {
						ForeignChainAddress::Eth(eth_address) => Some(TransferAssetParams {
							asset: asset_address.into(),
							to: eth_address.into(),
							amount: *amount,
						}),
						_ => None,
					})
					.collect(), // All outgoing asset info
			);
			T::EthereumBroadcaster::threshold_sign_and_broadcast(egress_transaction);
			Self::deposit_event(Event::<T>::EgressBroadcasted { foreign_asset, batch_size });
		}
	}
}

impl<T: Config> EgressApi for Pallet<T> {
	type Amount = AssetAmount;
	type EgressAddress = ForeignChainAddress;

	fn schedule_egress(
		foreign_asset: ForeignChainAsset,
		amount: Self::Amount,
		egress_address: Self::EgressAddress,
	) -> DispatchResult {
		ensure!(
			DisabledEgressAssets::<T>::get(foreign_asset).is_none(),
			Error::<T>::AssetEgressDisabled
		);

		debug_assert!(
			Self::is_egress_valid(&foreign_asset, &egress_address),
			"Egress validity is checked by calling functions."
		);

		ScheduledEgress::<T>::append(&foreign_asset, (amount, egress_address));

		Self::deposit_event(Event::<T>::EgressScheduled { foreign_asset, amount, egress_address });

		Ok(())
	}

	fn is_egress_valid(
		foreign_asset: &ForeignChainAsset,
		egress_address: &Self::EgressAddress,
	) -> bool {
		match foreign_asset.chain {
			ForeignChain::Ethereum =>
				matches!(egress_address, ForeignChainAddress::Eth(..)) &&
					T::EthereumAssetsAddressProvider::try_get_asset_address(foreign_asset.asset)
						.is_some(),
			ForeignChain::Polkadot => matches!(egress_address, ForeignChainAddress::Dot(..)),
		}
	}
}
