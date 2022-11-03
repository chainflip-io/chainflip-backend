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

use cf_chains::{
	AllBatch, Chain RuntimeFetchAssetParams as FetchAssetParams,
	RuntimeTransferAssetParams as TransferAssetParams,
};
use cf_primitives::{chains::{assets, Ethereum}, AssetAmount, ForeignChain, IntentId};
use cf_traits::{Broadcaster, EgressApi, IngressFetchApi, ReplayProtectionProvider};
use frame_support::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::Saturating;
pub use sp_std::{vec, vec::Vec};
pub use weights::WeightInfo;

/// Enum wrapper for fetch and egress requests.
#[derive(RuntimeDebug, Eq, PartialEq, Copy, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum FetchOrTransfer<C: Chain> {
	Fetch { intent_id: IntentId, asset: C::ChainAsset },
	Transfer { asset: C::ChainAsset, to: C::ChainAccount, amount: AssetAmount },
}

impl<C: Chain> FetchOrTransfer<C> {
	fn asset(&self) -> &C::ChainAsset {
		match self {
			FetchOrTransfer::Fetch { asset, .. } => asset,
			FetchOrTransfer::Transfer { asset, .. } => asset,
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	use cf_primitives::chains::assets;
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
		type ReplayProtection: ReplayProtectionProvider<Ethereum>;

		/// The type of the chain-native transaction.
		type AllBatch: AllBatch<Ethereum>;

		/// A broadcaster instance.
		type Broadcaster: Broadcaster<Ethereum, ApiCall = Self::AllBatch>;

		/// Governance origin to manage allowed assets
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

	/// Scheduled fetch and egress for the Ethereum chain.
	#[pallet::storage]
	pub(crate) type EthereumScheduledRequests<T: Config> =
		StorageValue<_, Vec<FetchOrTransfer<Ethereum>>, ValueQuery>;

	/// Stores the list of assets that are not allowed to be egressed.
	#[pallet::storage]
	pub(crate) type EthereumDisabledEgressAssets<T: Config> =
		StorageMap<_, Twox64Concat, <Ethereum as Chain>::ChainAsset, ()>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AssetEgressDisabled {
			asset: <Ethereum as Chain>::ChainAsset,
			disabled: bool,
		},
		EgressScheduled {
			asset: <Ethereum as Chain>::ChainAsset,
			amount: AssetAmount,
			egress_address: <Ethereum as Chain>::ChainAccount,
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
			asset: <Ethereum as Chain>::ChainAsset,
			disabled: bool,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			if disabled {
				EthereumDisabledEgressAssets::<T>::insert(asset, ());
			} else {
				EthereumDisabledEgressAssets::<T>::remove(asset);
			}

			Self::deposit_event(Event::<T>::AssetEgressDisabled { asset, disabled });

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
			EthereumScheduledRequests::<T>::mutate(|requests: &mut Vec<_>| {
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

		let mut fetch_params = vec![];
		let mut egress_params = vec![];
		for request in batch_to_send {
			match request {
				FetchOrTransfer::<Ethereum>::Fetch { intent_id, asset } => {
					fetch_params.push(FetchAssetParams { intent_id, asset });
				},
				FetchOrTransfer::<Ethereum>::Transfer { asset, to, amount } => {
					egress_params.push(TransferAssetParams { asset, to, amount });
				},
			}
		}
		let fetch_batch_size = fetch_params.len() as u32;
		let egress_batch_size = egress_params.len() as u32;

		// Construct and send the transaction.
		#[allow(clippy::unit_arg)]
		let egress_transaction = T::AllBatch::new_unsigned(
			T::ReplayProtection::replay_protection(),
			fetch_params,
			egress_params,
		);
		T::Broadcaster::threshold_sign_and_broadcast(egress_transaction);
		Self::deposit_event(Event::<T>::EthereumBatchBroadcastRequested {
			fetch_batch_size,
			egress_batch_size,
		});
		fetch_batch_size.saturating_add(egress_batch_size)
	}
}

impl<T: Config> EgressApi<Ethereum> for Pallet<T> {
	fn schedule_egress(
		asset: <Ethereum as Chain>::ChainAsset,
		amount: AssetAmount,
		egress_address: <Ethereum as Chain>::ChainAccount,
	) {
		EthereumScheduledRequests::<T>::append(FetchOrTransfer::<Ethereum>::Transfer {
			asset,
			to: egress_address,
			amount,
		});

		Self::deposit_event(Event::<T>::EgressScheduled { asset, amount, egress_address });
	}
}

impl<T: Config> IngressFetchApi<Ethereum> for Pallet<T> {
	fn schedule_ingress_fetch(fetch_details: Vec<(<Ethereum as Chain>::ChainAsset, IntentId)>) {
		let fetches_added = fetch_details.len() as u32;
		for (asset, intent_id) in fetch_details {
			EthereumScheduledRequests::<T>::append(FetchOrTransfer::<Ethereum>::Fetch {
				intent_id,
				asset,
			});
		}
		Self::deposit_event(Event::<T>::IngressFetchesScheduled { fetches_added });
	}
}
