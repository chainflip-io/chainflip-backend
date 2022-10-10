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

use cf_chains::{AllBatch, Ethereum, FetchAssetParams, TransferAssetParams};
use cf_primitives::{
	Asset, AssetAmount, EthereumAddress, ForeignChain, ForeignChainAddress, ForeignChainAsset,
	IntentId, ETHEREUM_ETH_ADDRESS,
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
	#[pallet::storage]
	pub(crate) type EthereumScheduledEgress<T: Config> =
		StorageValue<_, Vec<TransferAssetParams<Ethereum>>, ValueQuery>;

	/// Scheduled fetch requests for the Ethereum chain.
	#[pallet::storage]
	pub(crate) type EthereumScheduledIngressFetch<T: Config> =
		StorageValue<_, Vec<FetchAssetParams<Ethereum>>, ValueQuery>;

	/// Stores the list of assets that are not allowed to be egressed.
	#[pallet::storage]
	pub(crate) type EthereumDisabledEgressAssets<T: Config> =
		StorageMap<_, Twox64Concat, EthereumAddress, ()>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		EthereumAssetEgressDisabled {
			asset: Asset,
			disabled: bool,
		},
		EgressScheduled {
			foreign_asset: ForeignChainAsset,
			amount: AssetAmount,
			egress_address: ForeignChainAddress,
		},
		IngressFetchesScheduled {
			fetches_added: u32,
		},
		EthereumBatchBroadcastRequested {
			fetch_batch_size: u32,
			egress_batch_size: u32,
		},
	}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Take a batch of scheduled Fetch and Egress for each chain and send them out.
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut weights_to_spend = remaining_weight;

			// Send batch for Ethereum chain
			let ethereum_fetch_batch_size = EthereumScheduledIngressFetch::<T>::get().len() as u32;
			let etheruem_egress_batch_size = EthereumScheduledEgress::<T>::get().len() as u32;

			let ethereum_weight = T::WeightInfo::send_ethereum_batch(
				ethereum_fetch_batch_size,
				etheruem_egress_batch_size,
			);
			if remaining_weight >= ethereum_weight {
				Self::send_ethereum_batch();
				weights_to_spend = remaining_weight.saturating_sub(ethereum_weight);
			}

			remaining_weight.saturating_sub(weights_to_spend)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets if an asset is not allowed to be sent out into the Ethereum chain via Egress.
		/// Requires Governance
		///
		/// ## Events
		///
		/// - [On update](Event::AssetEgressDisabled)
		#[pallet::weight(T::WeightInfo::disable_ethereum_asset_egress())]
		pub fn disable_ethereum_asset_egress(
			origin: OriginFor<T>,
			asset: Asset,
			disabled: bool,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
			if let Some(asset_eth_address) = Self::get_asset_ethereum_address(asset) {
				let asset_is_disabled =
					EthereumDisabledEgressAssets::<T>::contains_key(asset_eth_address);
				if disabled && !asset_is_disabled {
					EthereumDisabledEgressAssets::<T>::insert(asset_eth_address, ());
				} else if !disabled && asset_is_disabled {
					EthereumDisabledEgressAssets::<T>::remove(asset_eth_address);
				}

				Self::deposit_event(Event::<T>::EthereumAssetEgressDisabled { asset, disabled });
			}

			Ok(())
		}

		/// Send all scheduled fetch and egress out for a specific chain.
		/// Requires governance
		///
		/// ## Events
		///
		/// - [on_success](Event::EthereumBatchBroadcastRequested)
		#[pallet::weight(T::WeightInfo::send_ethereum_batch(0, 0))]
		pub fn send_scheduled_batch_for_chain(
			origin: OriginFor<T>,
			chain: ForeignChain,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			// Currently we only support Ethereum
			if chain == ForeignChain::Ethereum {
				Self::send_ethereum_batch();
			}

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	// Take the scheduled fetch and egress batch and send them out to the Ethereum chain as a Batch
	// transaction.
	fn send_ethereum_batch() {
		// Get egresses from storage, filter out disallowed assets.
		let mut ethereum_egress_batch = EthereumScheduledEgress::<T>::get();
		ethereum_egress_batch.retain(|param| {
			let address: [u8; 20] = param.asset.into();
			!EthereumDisabledEgressAssets::<T>::contains_key(address)
		});

		EthereumScheduledEgress::<T>::mutate(|batch| {
			batch.retain(|param| {
				let address: [u8; 20] = param.asset.into();
				EthereumDisabledEgressAssets::<T>::contains_key(address)
			})
		});

		// Get fetches from storage.
		let ethereum_fetch_batch = EthereumScheduledIngressFetch::<T>::take();

		// Build the egress and fetch batch
		if !ethereum_egress_batch.is_empty() || !ethereum_fetch_batch.is_empty() {
			let egress_batch_size = ethereum_egress_batch.len();
			let fetch_batch_size = ethereum_fetch_batch.len();
			let egress_transaction = T::EthereumEgressTransaction::new_unsigned(
				T::EthereumReplayProtection::replay_protection(),
				ethereum_fetch_batch,
				ethereum_egress_batch,
			);
			T::EthereumBroadcaster::threshold_sign_and_broadcast(egress_transaction);
			Self::deposit_event(Event::<T>::EthereumBatchBroadcastRequested {
				fetch_batch_size: fetch_batch_size as u32,
				egress_batch_size: egress_batch_size as u32,
			});
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
	) {
		debug_assert!(
			Self::is_egress_valid(&foreign_asset, &egress_address),
			"Egress validity is checked by calling functions."
		);

		// Only Ethereum is currently supported.
		if let (ForeignChain::Ethereum, ForeignChainAddress::Eth(eth_address)) =
			(foreign_asset.chain, egress_address)
		{
			EthereumScheduledEgress::<T>::append(TransferAssetParams {
				asset: Self::get_asset_ethereum_address(foreign_asset.asset)
					.expect("Asset ensured to be valid.")
					.into(),
				to: eth_address.into(),
				amount,
			});
			Self::deposit_event(Event::<T>::EgressScheduled {
				foreign_asset,
				amount,
				egress_address,
			});
		}
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
	fn schedule_ethereum_ingress_fetch(fetch_details: Vec<(Asset, IntentId)>) {
		let fetches_added = fetch_details.len() as u32;
		for (asset, intent_id) in fetch_details {
			if let Some(asset_address) = Self::get_asset_ethereum_address(asset) {
				EthereumScheduledIngressFetch::<T>::append(FetchAssetParams {
					swap_id: intent_id,
					asset: asset_address.into(),
				});
			}
		}
		Self::deposit_event(Event::<T>::IngressFetchesScheduled { fetches_added });
	}
}
