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

use cf_chains::{AllBatch, Chain, ChainAbi, FetchAssetParams, TransferAssetParams};
use cf_primitives::{AssetAmount, ForeignChain, IntentId};
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

	use cf_traits::Chainflip;
	use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};

	pub(crate) type TargetChainAsset<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAsset;
	pub(crate) type TargetChainAccount<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::ChainAccount;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// Marks which chain this pallet is interacting with.
		type TargetChain: Chain + ChainAbi;

		/// Replay protection.
		type ReplayProtection: ReplayProtectionProvider<Self::TargetChain>;

		/// The type of the chain-native transaction.
		type AllBatch: AllBatch<Self::TargetChain>;

		/// A broadcaster instance.
		type Broadcaster: Broadcaster<Self::TargetChain, ApiCall = Self::AllBatch>;

		/// Governance origin to manage allowed assets
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

	/// Scheduled fetch and egress for the Ethereum chain.
	#[pallet::storage]
	pub(crate) type ScheduledRequests<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<FetchOrTransfer<T::TargetChain>>, ValueQuery>;

	/// Stores the list of assets that are not allowed to be egressed.
	#[pallet::storage]
	pub(crate) type DisabledEgressAssets<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, ()>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		AssetEgressDisabled {
			asset: TargetChainAsset<T, I>,
			disabled: bool,
		},
		EgressScheduled {
			asset: TargetChainAsset<T, I>,
			amount: AssetAmount,
			egress_address: TargetChainAccount<T, I>,
		},
		IngressFetchesScheduled {
			fetches_added: u32,
		},
		BatchBroadcastRequested {
			fetch_batch_size: u32,
			egress_batch_size: u32,
		},
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// Take a batch of scheduled Egress and send them out
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			// Ensure we have enough weight to send an non-empty batch, and request queue isn't
			// empty.
			if remaining_weight <= T::WeightInfo::send_batch(1u32) ||
				ScheduledRequests::<T, I>::decode_len() == Some(0)
			{
				return T::WeightInfo::on_idle_with_nothing_to_send()
			}

			// Calculate the number of requests that the weight allows.
			let single_request_cost =
				T::WeightInfo::send_batch(1u32).saturating_sub(T::WeightInfo::send_batch(0u32));
			let request_count = remaining_weight
				.saturating_sub(T::WeightInfo::send_batch(0u32))
				.saturating_div(single_request_cost) as u32;

			let actual_requests_sent = Self::send_scheduled_batch(Some(request_count));

			T::WeightInfo::send_batch(actual_requests_sent)
		}

		fn integrity_test() {
			// Ensures the weights are benchmarked correctly.
			assert!(T::WeightInfo::send_batch(1) > T::WeightInfo::send_batch(0));
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Sets if an asset is not allowed to be sent out of the chain via Egress.
		/// Requires Governance
		///
		/// ## Events
		///
		/// - [On update](Event::AssetEgressDisabled)
		#[pallet::weight(T::WeightInfo::disable_asset_egress())]
		pub fn disable_asset_egress(
			origin: OriginFor<T>,
			asset: TargetChainAsset<T, I>,
			disabled: bool,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			if disabled {
				DisabledEgressAssets::<T, I>::insert(asset, ());
			} else {
				DisabledEgressAssets::<T, I>::remove(asset);
			}

			Self::deposit_event(Event::<T, I>::AssetEgressDisabled { asset, disabled });

			Ok(())
		}

		/// Send up to `maybe_size` number of scheduled transactions out for a specific chain.
		/// If None is set for `maybe_size`, send all scheduled transactions.
		/// Requires governance
		///
		/// ## Events
		///
		/// - [on_success](Event::BatchBroadcastRequested)
		#[pallet::weight(0)]
		pub fn send_scheduled_batch_for_chain(
			origin: OriginFor<T>,
			chain: ForeignChain,
			maybe_size: Option<u32>,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			// Currently we only support the Ethereum Chain
			if chain == ForeignChain::Ethereum {
				Self::send_scheduled_batch(maybe_size);
			}

			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Take up to `maybe_size` number of scheduled requests for the Ethereum chain and send them
	/// out in an `AllBatch` call. If `maybe_size` is `None`, send all scheduled transactions.
	///
	/// Returns the actual number of transactions sent.
	///
	/// Egress transactions with Blacklisted assets are not sent, and kept in storage.
	fn send_scheduled_batch(maybe_size: Option<u32>) -> u32 {
		if maybe_size == Some(0) {
			return 0
		}

		let batch_to_send: Vec<_> = ScheduledRequests::<T, I>::mutate(|requests: &mut Vec<_>| {
			// Take up to batch_size requests to be sent
			let mut available_batch_size = maybe_size.unwrap_or(requests.len() as u32);

			// Filter out disabled assets
			requests
				.drain_filter(|request| {
					if available_batch_size > 0 &&
						!DisabledEgressAssets::<T, I>::contains_key(request.asset())
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
				FetchOrTransfer::<T::TargetChain>::Fetch { intent_id, asset } => {
					fetch_params.push(FetchAssetParams { intent_id, asset });
				},
				FetchOrTransfer::<T::TargetChain>::Transfer { asset, to, amount } => {
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
		Self::deposit_event(Event::<T, I>::BatchBroadcastRequested {
			fetch_batch_size,
			egress_batch_size,
		});
		fetch_batch_size.saturating_add(egress_batch_size)
	}
}

impl<T: Config<I>, I: 'static> EgressApi<T::TargetChain> for Pallet<T, I> {
	fn schedule_egress(
		asset: TargetChainAsset<T, I>,
		amount: AssetAmount,
		egress_address: TargetChainAccount<T, I>,
	) {
		ScheduledRequests::<T, I>::append(FetchOrTransfer::<T::TargetChain>::Transfer {
			asset,
			to: egress_address.clone(),
			amount,
		});

		Self::deposit_event(Event::<T, I>::EgressScheduled { asset, amount, egress_address });
	}
}

impl<T: Config<I>, I: 'static> IngressFetchApi<T::TargetChain> for Pallet<T, I> {
	fn schedule_ingress_fetch(fetch_details: Vec<(TargetChainAsset<T, I>, IntentId)>) {
		let fetches_added = fetch_details.len() as u32;
		for (asset, intent_id) in fetch_details {
			ScheduledRequests::<T, I>::append(FetchOrTransfer::<T::TargetChain>::Fetch {
				intent_id,
				asset,
			});
		}
		Self::deposit_event(Event::<T, I>::IngressFetchesScheduled { fetches_added });
	}
}
