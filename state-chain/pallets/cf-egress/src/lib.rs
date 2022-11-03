#![cfg_attr(not(feature = "std"), no_std)]
#![feature(drain_filter)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
mod weights;

use cf_chains::{AllBatch, FetchAssetParams, TransferAssetParams};
use cf_primitives::{
	Asset, AssetAmount, EthereumAddress, ForeignChain, ForeignChainAddress, ForeignChainAsset,
	IntentId, ETHEREUM_ETH_ADDRESS,
};
use cf_traits::{
	ApiCallDataProvider, Broadcaster, EgressApi, EthereumAssetsAddressProvider, IngressFetchApi,
	ReplayProtectionProvider,
};
use frame_support::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::Saturating;
pub use sp_std::{vec, vec::Vec};
pub use weights::WeightInfo;

/// Enum wrapper for fetch and egress requests.
#[derive(RuntimeDebug, Eq, PartialEq, Copy, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum EthereumRequest {
	Fetch { intent_id: IntentId, asset: Asset },
	Transfer { asset: Asset, to: EthereumAddress, amount: AssetAmount },
}

impl EthereumRequest {
	fn asset(&self) -> &Asset {
		match self {
			EthereumRequest::Fetch { asset, .. } => asset,
			EthereumRequest::Transfer { asset, .. } => asset,
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	use cf_chains::eth::Ethereum;
	use cf_traits::{ApiCallDataProvider, Chainflip};
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
		type EthereumReplayProtection: ReplayProtectionProvider<Ethereum>
			+ ApiCallDataProvider<Ethereum>;

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

	/// Scheduled fetch and egress for the Ethereum chain.
	#[pallet::storage]
	pub(crate) type EthereumScheduledRequests<T: Config> =
		StorageValue<_, Vec<EthereumRequest>, ValueQuery>;

	/// Stores the list of assets that are not allowed to be egressed.
	#[pallet::storage]
	pub(crate) type EthereumDisabledEgressAssets<T: Config> =
		StorageMap<_, Twox64Concat, Asset, ()>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AssetEgressDisabled {
			foreign_asset: ForeignChainAsset,
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
		/// Take a batch of scheduled Egress and send them out
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			// Ensure we have enough weight to send an non-empty batch, and request queue isn't
			// empty.
			if remaining_weight <= T::WeightInfo::send_ethereum_batch(1u32) ||
				EthereumScheduledRequests::<T>::decode_len() == Some(0)
			{
				return T::WeightInfo::on_idle_with_nothing_to_send()
			}

			// Calculate the number of requests that the weight allows.
			let single_request_cost = T::WeightInfo::send_ethereum_batch(1u32)
				.saturating_sub(T::WeightInfo::send_ethereum_batch(0u32));
			let request_count = remaining_weight
				.saturating_sub(T::WeightInfo::send_ethereum_batch(0u32))
				.saturating_div(single_request_cost) as u32;

			let actual_requests_sent = Self::send_ethereum_scheduled_batch(Some(request_count));

			T::WeightInfo::send_ethereum_batch(actual_requests_sent)
		}

		fn integrity_test() {
			// Ensures the weights are benchmarked correctly.
			assert!(T::WeightInfo::send_ethereum_batch(1) > T::WeightInfo::send_ethereum_batch(0));
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
			foreign_asset: ForeignChainAsset,
			disabled: bool,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			if foreign_asset.chain == ForeignChain::Ethereum {
				if disabled {
					EthereumDisabledEgressAssets::<T>::insert(foreign_asset.asset, ());
				} else {
					EthereumDisabledEgressAssets::<T>::remove(foreign_asset.asset);
				}
			}

			Self::deposit_event(Event::<T>::AssetEgressDisabled { foreign_asset, disabled });

			Ok(())
		}

		/// Send up to `maybe_size` number of scheduled transactions out for a specific chain.
		/// If None is set for `maybe_size`, send all scheduled transactions.
		/// Requires governance
		///
		/// ## Events
		///
		/// - [on_success](Event::EthereumBatchBroadcastRequested)
		#[pallet::weight(0)]
		pub fn send_scheduled_batch_for_chain(
			origin: OriginFor<T>,
			chain: ForeignChain,
			maybe_size: Option<u32>,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			// Currently we only support the Ethereum Chain
			if chain == ForeignChain::Ethereum {
				Self::send_ethereum_scheduled_batch(maybe_size);
			}

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Take up to `maybe_size` number of scheduled requests for the Ethereum chain and send them
	/// out in an `AllBatch` call. If `maybe_size` is `None`, send all scheduled transactions.
	///
	/// Returns the actual number of transactions sent.
	///
	/// Egress transactions with Blacklisted assets are not sent, and kept in storage.
	fn send_ethereum_scheduled_batch(maybe_size: Option<u32>) -> u32 {
		if maybe_size == Some(0) {
			return 0
		}

		let batch_to_send: Vec<_> =
			EthereumScheduledRequests::<T>::mutate(|requests: &mut Vec<EthereumRequest>| {
				// Take up to batch_size requests to be sent
				let mut available_batch_size = maybe_size.unwrap_or(requests.len() as u32);

				// Filter out disabled assets
				requests
					.drain_filter(|request| {
						if available_batch_size > 0 &&
							!EthereumDisabledEgressAssets::<T>::contains_key(request.asset())
						{
							available_batch_size.saturating_reduce(1);
							true
						} else {
							false
						}
					})
					.collect()
			});

		if batch_to_send.is_empty() {
			return 0
		}

		// Construct the Params required for Ethereum AllBatch all.
		let mut fetch_params = vec![];
		let mut egress_params = vec![];
		for request in batch_to_send {
			match request {
				EthereumRequest::Fetch { intent_id, asset } => {
					// Asset should always have a valid Ethereum address
					if let Some(asset_address) = Self::get_ethereum_asset_identifier(asset) {
						fetch_params
							.push(FetchAssetParams { intent_id, asset: asset_address.into() });
					}
				},
				EthereumRequest::Transfer { asset, to, amount } => {
					// Asset should always have a valid Ethereum address
					if let Some(asset_address) = Self::get_ethereum_asset_identifier(asset) {
						egress_params.push(TransferAssetParams {
							asset: asset_address.into(),
							to: to.into(),
							amount,
						});
					}
				},
			}
		}
		let fetch_batch_size = fetch_params.len() as u32;
		let egress_batch_size = egress_params.len() as u32;

		// Construct and send the transaction.
		#[allow(clippy::unit_arg)]
		let egress_transaction = T::EthereumEgressTransaction::new_unsigned(
			T::EthereumReplayProtection::replay_protection(),
			T::EthereumReplayProtection::chain_extra_data(),
			fetch_params,
			egress_params,
		);
		T::EthereumBroadcaster::threshold_sign_and_broadcast(egress_transaction);
		Self::deposit_event(Event::<T>::EthereumBatchBroadcastRequested {
			fetch_batch_size,
			egress_batch_size,
		});
		fetch_batch_size.saturating_add(egress_batch_size)
	}

	fn get_ethereum_asset_identifier(asset: Asset) -> Option<EthereumAddress> {
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

		// Currently only Ethereum chain is supported
		if let (ForeignChain::Ethereum, ForeignChainAddress::Eth(eth_address)) =
			(foreign_asset.chain, egress_address)
		{
			EthereumScheduledRequests::<T>::append(EthereumRequest::Transfer {
				asset: foreign_asset.asset,
				to: eth_address,
				amount,
			});
		}

		Self::deposit_event(Event::<T>::EgressScheduled { foreign_asset, amount, egress_address });
	}

	fn is_egress_valid(
		foreign_asset: &ForeignChainAsset,
		egress_address: &ForeignChainAddress,
	) -> bool {
		match foreign_asset.chain {
			ForeignChain::Ethereum =>
				matches!(egress_address, ForeignChainAddress::Eth(..)) &&
					Self::get_ethereum_asset_identifier(foreign_asset.asset).is_some(),
			ForeignChain::Polkadot => matches!(egress_address, ForeignChainAddress::Dot(..)),
		}
	}
}

impl<T: Config> IngressFetchApi for Pallet<T> {
	fn schedule_ethereum_ingress_fetch(fetch_details: Vec<(Asset, IntentId)>) {
		let fetches_added = fetch_details.len() as u32;
		for (asset, intent_id) in fetch_details {
			debug_assert!(
				Self::get_ethereum_asset_identifier(asset).is_some(),
				"Asset validity is checked by calling functions."
			);
			EthereumScheduledRequests::<T>::append(EthereumRequest::Fetch { intent_id, asset });
		}
		Self::deposit_event(Event::<T>::IngressFetchesScheduled { fetches_added });
	}
}
