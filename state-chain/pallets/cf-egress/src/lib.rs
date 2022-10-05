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

use cf_chains::{
	eth::ingress_address::get_salt, AllBatch, Ethereum, FetchAssetParams, TransferAssetParams,
};
use cf_primitives::{
	Asset, AssetAmount, EgressBatch, EthereumAddress, FetchParameter, ForeignChain,
	ForeignChainAddress, ForeignChainAsset, IntentId, ETHEREUM_ETH_ADDRESS,
};
use cf_traits::{
	Broadcaster, EgressApi, EthereumAssetsAddressProvider, IngressFetchApi,
	ReplayProtectionProvider,
};
use frame_support::pallet_prelude::*;
pub use pallet::*;
pub use sp_std::{vec, vec::Vec};
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

	/// Scheduled egress for all supported chains.
	/// TODO: Enforce chain and address consistency via type.
	#[pallet::storage]
	pub(crate) type ScheduledEgress<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ForeignChainAsset,
		EgressBatch<AssetAmount, ForeignChainAddress>,
		ValueQuery,
	>;

	/// Scheduled fetch requests for the Ethereum chain.
	#[pallet::storage]
	pub(crate) type EthereumScheduledIngressFetch<T: Config> =
		StorageMap<_, Twox64Concat, Asset, Vec<IntentId>, ValueQuery>;

	/// Stores the list of assets that are not allowed to be egressed.
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
			foreign_assets: Vec<ForeignChainAsset>,
			egress_batch_size: u32,
			fetch_batch_size: u32,
		},
		EthereumIngressFetchesScheduled {
			fetches_added: u32,
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
			let mut egress_batch_size = 0u32;
			let mut fetch_batch_size = 0u32;
			let mut assets_to_send = vec![];

			ScheduledEgress::<T>::iter().for_each(|(asset, batch)| {
				if DisabledEgressAssets::<T>::get(asset).is_none() {
					let new_egress_batch_size =
						egress_batch_size.saturating_add(batch.len() as u32);
					let new_fetch_batch_size = fetch_batch_size.saturating_add(
						EthereumScheduledIngressFetch::<T>::get(asset.asset).len() as u32,
					);
					let egress_weight = T::WeightInfo::send_batch_egress(
						new_egress_batch_size,
						new_fetch_batch_size,
					);
					if remaining_weight >= egress_weight {
						assets_to_send.push(asset);
						egress_batch_size = new_egress_batch_size;
						fetch_batch_size = new_fetch_batch_size;
					}
				}
			});

			Self::send_scheduled_batch_for_assets(assets_to_send);
			T::WeightInfo::send_batch_egress(egress_batch_size, fetch_batch_size)
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
		#[pallet::weight(T::WeightInfo::send_batch_egress(0, 0))]
		pub fn send_scheduled_egress_for_asset(
			origin: OriginFor<T>,
			foreign_asset: ForeignChainAsset,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			ensure!(
				DisabledEgressAssets::<T>::get(foreign_asset).is_none(),
				Error::<T>::AssetEgressDisabled
			);

			Self::send_scheduled_batch_for_assets(vec![foreign_asset]);

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	// Take all scheduled batch Egress for an asset and send them out as a batch.
	fn send_scheduled_batch_for_assets(foreign_assets: Vec<ForeignChainAsset>) {
		let mut ethereum_egress_batch: Vec<TransferAssetParams<Ethereum>> = vec![];
		let mut ethereum_fetch_batch: Vec<FetchAssetParams<Ethereum>> = vec![];

		// Build the egress and fetch batch
		foreign_assets.iter().for_each(|foreign_asset| {
			// NOTE: currently, we only support Ethereum chain.
			if foreign_asset.chain == ForeignChain::Ethereum {
				let asset_address = Self::get_asset_ethereum_address(foreign_asset.asset)
					.expect("Asset is guaranteed to be supported.");

				EthereumScheduledIngressFetch::<T>::take(foreign_asset.asset)
					.iter()
					.map(|&intent_id| FetchAssetParams {
						swap_id: get_salt(intent_id),
						asset: asset_address.into(),
					})
					.for_each(|fetch| ethereum_fetch_batch.push(fetch));

				ScheduledEgress::<T>::take(foreign_asset)
					.iter()
					.filter_map(|(amount, address)| match address {
						ForeignChainAddress::Eth(eth_address) => Some(TransferAssetParams {
							asset: asset_address.into(),
							to: eth_address.into(),
							amount: *amount,
						}),
						_ => None,
					})
					.for_each(|transfer| ethereum_egress_batch.push(transfer));
			}
		});

		if !ethereum_egress_batch.is_empty() || !ethereum_fetch_batch.is_empty() {
			Self::deposit_event(Event::<T>::EgressBroadcasted {
				foreign_assets,
				egress_batch_size: ethereum_egress_batch.len() as u32,
				fetch_batch_size: ethereum_fetch_batch.len() as u32,
			});
			let egress_transaction = T::EthereumEgressTransaction::new_unsigned(
				T::EthereumReplayProtection::replay_protection(),
				ethereum_fetch_batch,
				ethereum_egress_batch,
			);
			T::EthereumBroadcaster::threshold_sign_and_broadcast(egress_transaction);
		}
	}

	fn get_asset_ethereum_address(asset: Asset) -> Option<EthereumAddress> {
		if asset == Asset::Eth {
			Some(ETHEREUM_ETH_ADDRESS)
		} else {
			T::EthereumAssetsAddressProvider::try_get_asset_address(asset)
		}
	}
}

impl<T: Config> EgressApi for Pallet<T> {
	fn schedule_egress(
		foreign_asset: ForeignChainAsset,
		amount: AssetAmount,
		egress_address: ForeignChainAddress,
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
		egress_address: &ForeignChainAddress,
	) -> bool {
		match foreign_asset.chain {
			ForeignChain::Ethereum =>
				matches!(egress_address, ForeignChainAddress::Eth(..)) &&
					Self::get_asset_ethereum_address(foreign_asset.asset).is_some(),
			ForeignChain::Polkadot => matches!(egress_address, ForeignChainAddress::Dot(..)),
		}
	}
}

impl<T: Config> IngressFetchApi for Pallet<T> {
	fn schedule_ingress_fetch(fetch_details: Vec<(Asset, FetchParameter)>) {
		Self::deposit_event(Event::<T>::EthereumIngressFetchesScheduled {
			fetches_added: fetch_details.len() as u32,
		});

		for (asset, fetch_param) in fetch_details {
			let FetchParameter::Eth(intent_id) = fetch_param;
			EthereumScheduledIngressFetch::<T>::append(&asset, intent_id);
		}
	}
}
