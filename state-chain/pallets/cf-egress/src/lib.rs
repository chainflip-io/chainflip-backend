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
	/// TODO: Enforce chain and address consistency via type.
	#[pallet::storage]
	pub(crate) type EthereumScheduledEgress<T: Config> =
		StorageMap<_, Twox64Concat, Asset, Vec<(AssetAmount, EthereumAddress)>, ValueQuery>;

	/// Scheduled fetch requests for the Ethereum chain.
	#[pallet::storage]
	pub(crate) type EthereumScheduledIngressFetch<T: Config> =
		StorageMap<_, Twox64Concat, Asset, Vec<IntentId>, ValueQuery>;

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
			// Ensure we have enough weight to send an non-empty batch
			if remaining_weight <= T::WeightInfo::send_ethereum_batch(1, 1) {
				return 0
			}

			// Construct assets to send for the Ethereum chain.
			let single_fetch_cost = T::WeightInfo::send_ethereum_batch(1, 0)
				.saturating_sub(T::WeightInfo::send_ethereum_batch(0, 0));

			let mut ethereum_fetch_batch: Vec<FetchAssetParams<Ethereum>> = vec![];
			EthereumScheduledIngressFetch::<T>::iter_keys().for_each(|asset| {
				EthereumScheduledIngressFetch::<T>::mutate(asset, |batch| {
					// Calculate how many fetches can still be sent with the weights.
					let fetches_left = remaining_weight
						.saturating_sub(T::WeightInfo::send_ethereum_batch(
							ethereum_fetch_batch.len() as u32,
							0u32, // Egress are filled after fetch.
						))
						.saturating_div(single_fetch_cost);
					if !batch.is_empty() && fetches_left > 0 {
						if let Some(asset_address) = Self::get_asset_identifier(asset) {
							// Take as many Fetch requests as the weight allows.
							batch
								.drain(..sp_std::cmp::min(batch.len(), fetches_left as usize))
								.for_each(|intent_id| {
									ethereum_fetch_batch.push(FetchAssetParams {
										intent_id,
										asset: asset_address.into(),
									})
								});
						}
					}
				});
			});

			let single_egress_cost = T::WeightInfo::send_ethereum_batch(0, 1)
				.saturating_sub(T::WeightInfo::send_ethereum_batch(0, 0));

			let mut ethereum_egress_batch: Vec<TransferAssetParams<Ethereum>> = vec![];
			EthereumScheduledEgress::<T>::iter_keys()
				.filter(|asset| !EthereumDisabledEgressAssets::<T>::contains_key(asset))
				.for_each(|asset| {
					EthereumScheduledEgress::<T>::mutate(asset, |batch| {
						// Calculate how many egress can still be sent with the weights.
						let egress_left = remaining_weight
							.saturating_sub(T::WeightInfo::send_ethereum_batch(
								ethereum_fetch_batch.len() as u32,
								ethereum_egress_batch.len() as u32,
							))
							.saturating_div(single_egress_cost);

						if !batch.is_empty() && egress_left > 0 {
							if let Some(asset_address) = Self::get_asset_identifier(asset) {
								// Take as many Egress as the weight allows.
								batch
									.drain(..sp_std::cmp::min(batch.len(), egress_left as usize))
									.for_each(|(amount, address)| {
										ethereum_egress_batch.push(TransferAssetParams {
											asset: asset_address.into(),
											to: address.into(),
											amount,
										})
									});
							}
						}
					});
				});

			let fetch_batch_size = ethereum_fetch_batch.len() as u32;
			let egress_batch_size = ethereum_egress_batch.len() as u32;
			Self::send_ethereum_scheduled_batch(ethereum_fetch_batch, ethereum_egress_batch);

			T::WeightInfo::send_ethereum_batch(fetch_batch_size, egress_batch_size)
		}

		fn integrity_test() {
			// Ensures the weights are benchmarked correctly.
			assert!(
				T::WeightInfo::send_ethereum_batch(0, 1) > T::WeightInfo::send_ethereum_batch(0, 0)
			);
			assert!(
				T::WeightInfo::send_ethereum_batch(1, 0) > T::WeightInfo::send_ethereum_batch(0, 0)
			);
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

		/// Send all scheduled egress out for an asset, ignoring weight constraint.
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

			// Currently we only support the Ethereum Chain
			if chain == ForeignChain::Ethereum {
				let mut ethereum_fetch_batch: Vec<FetchAssetParams<Ethereum>> = vec![];
				EthereumScheduledIngressFetch::<T>::iter_keys().for_each(|asset| {
					if let Some(asset_address) = Self::get_asset_identifier(asset) {
						EthereumScheduledIngressFetch::<T>::take(asset).iter().for_each(
							|intent_id| {
								ethereum_fetch_batch.push(FetchAssetParams {
									intent_id: *intent_id,
									asset: asset_address.into(),
								})
							},
						);
					}
				});

				let mut ethereum_egress_batch: Vec<TransferAssetParams<Ethereum>> = vec![];
				EthereumScheduledEgress::<T>::iter_keys()
					.filter(|asset| !EthereumDisabledEgressAssets::<T>::contains_key(asset))
					.for_each(|asset| {
						if let Some(asset_address) = Self::get_asset_identifier(asset) {
							EthereumScheduledEgress::<T>::take(asset).iter().for_each(
								|(amount, address)| {
									ethereum_egress_batch.push(TransferAssetParams {
										asset: asset_address.into(),
										to: address.into(),
										amount: *amount,
									})
								},
							);
						}
					});

				Self::send_ethereum_scheduled_batch(ethereum_fetch_batch, ethereum_egress_batch);
			}

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Take all scheduled Fetch and Transfer for the Ethereum chain and send them out as a batch.
	fn send_ethereum_scheduled_batch(
		ethereum_fetch_batch: Vec<FetchAssetParams<Ethereum>>,
		ethereum_egress_batch: Vec<TransferAssetParams<Ethereum>>,
	) {
		if !ethereum_fetch_batch.is_empty() || !ethereum_egress_batch.is_empty() {
			let fetch_batch_size = ethereum_fetch_batch.len() as u32;
			let egress_batch_size = ethereum_egress_batch.len() as u32;

			let egress_transaction = T::EthereumEgressTransaction::new_unsigned(
				T::EthereumReplayProtection::replay_protection(),
				ethereum_fetch_batch,
				ethereum_egress_batch,
			);
			T::EthereumBroadcaster::threshold_sign_and_broadcast(egress_transaction);
			Self::deposit_event(Event::<T>::EthereumBatchBroadcastRequested {
				fetch_batch_size,
				egress_batch_size,
			});
		}
	}

	fn get_asset_identifier(asset: Asset) -> Option<EthereumAddress> {
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
			EthereumScheduledEgress::<T>::append(&foreign_asset.asset, (amount, eth_address));
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
					Self::get_asset_identifier(foreign_asset.asset).is_some(),
			ForeignChain::Polkadot => matches!(egress_address, ForeignChainAddress::Dot(..)),
		}
	}
}

impl<T: Config> IngressFetchApi for Pallet<T> {
	fn schedule_ethereum_ingress_fetch(fetch_details: Vec<(Asset, IntentId)>) {
		let fetches_added = fetch_details.len() as u32;
		for (asset, intent_id) in fetch_details {
			EthereumScheduledIngressFetch::<T>::append(&asset, intent_id);
		}
		Self::deposit_event(Event::<T>::IngressFetchesScheduled { fetches_added });
	}
}
