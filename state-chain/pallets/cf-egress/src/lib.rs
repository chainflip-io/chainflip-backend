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
use cf_primitives::{EgressBatch, ForeignChain, ForeignChainAddress, ForeignChainAsset};
use cf_traits::{
	Broadcaster, EgressAbiBuilder, EgressApi, FlipBalance, ReplayProtectionProvider,
	SupportedEthAssetsAddressProvider,
};
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
		type ReplayProtection: ReplayProtectionProvider<Ethereum>;

		/// The type of the chain-native transaction.
		type EthereumEgressTransaction: AllBatch<Ethereum>;

		/// A broadcaster instance.
		type EthereumBroadcaster: Broadcaster<Ethereum, ApiCall = Self::EthereumEgressTransaction>;

		/// Governance origin to manage allowed assets
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// An API for getting Ethereum related parameters
		type SupportedEthAssetsAddressProvider: SupportedEthAssetsAddressProvider;

		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

	#[pallet::storage]
	pub(crate) type ScheduledEgress<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ForeignChainAsset,
		EgressBatch<FlipBalance, ForeignChainAddress>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub(crate) type AllowedEgressAssets<T: Config> =
		StorageMap<_, Twox64Concat, ForeignChainAsset, (), OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AssetPermissionSet {
			asset: ForeignChainAsset,
			allowed: bool,
		},
		EgressScheduled {
			foreign_asset: ForeignChainAsset,
			amount: FlipBalance,
			egress_address: ForeignChainAddress,
		},
		EgressBroadcasted {
			asset: ForeignChainAsset,
			batch_size: u32,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		// The given asset is not allowed to be Egressed
		AssetEgressDisallowed,
		// The Asset cannot be egressed to the destination chain.
		InvalidEgressDestination,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Take a batch of scheduled Egress and send them out
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut weights_left = remaining_weight;

			AllowedEgressAssets::<T>::iter().for_each(|(asset, ())| {
				let egress_weight = T::WeightInfo::send_batch_egress(
					ScheduledEgress::<T>::get(&asset).len() as u32,
				);
				if weights_left >= egress_weight {
					Self::send_scheduled_batch_transaction(asset);
					weights_left = weights_left.saturating_sub(egress_weight);
				}
			});

			remaining_weight.saturating_sub(weights_left)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets if an asset is allowed to be sent out of the chain via Egress.
		/// Requires Governance
		///
		/// ## Events
		///
		/// - [On update](Event::AssetPermissionSet)
		#[pallet::weight(T::WeightInfo::set_asset_egress_permission())]
		pub fn set_asset_egress_permission(
			origin: OriginFor<T>,
			asset: ForeignChainAsset,
			allowed: bool,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			let asset_is_allowed = AllowedEgressAssets::<T>::contains_key(asset);
			if allowed && !asset_is_allowed {
				AllowedEgressAssets::<T>::insert(asset, ());
			} else if !allowed && asset_is_allowed {
				AllowedEgressAssets::<T>::remove(asset);
			}

			Self::deposit_event(Event::<T>::AssetPermissionSet { asset, allowed });

			Ok(())
		}

		/// Send all scheduled egress out for an asset, ignoring weight constraint.
		/// Requires governance
		///
		/// ## Events
		///
		/// - [on_success](Event::EgressBroadcasted)
		#[pallet::weight(T::WeightInfo::set_asset_egress_permission())]
		pub fn send_scheduled_egress_for_asset(
			origin: OriginFor<T>,
			asset: ForeignChainAsset,
		) -> DispatchResultWithPostInfo {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			let egress_weight =
				T::WeightInfo::send_batch_egress(ScheduledEgress::<T>::get(&asset).len() as u32);

			Self::send_scheduled_batch_transaction(asset);

			Ok(Some(egress_weight).into())
		}
	}
}

impl<T: Config> Pallet<T> {
	// Take all scheduled batch Egress for an asset and send them out as a batch.
	fn send_scheduled_batch_transaction(asset: ForeignChainAsset) {
		// Take the scheduled Egress calls to be sent out of storage.
		let scheduled = ScheduledEgress::<T>::take(asset);

		let batch_size = scheduled.len() as u32;

		// Construct the Egress Tx and send it out.
		// NOTE: currently, we only support Ethereum chain.
		if asset.chain == ForeignChain::Ethereum {
			if let Some(egress_transaction) =
				Pallet::<T>::construct_batched_transaction(asset, scheduled)
			{
				T::EthereumBroadcaster::threshold_sign_and_broadcast(egress_transaction);
				Self::deposit_event(Event::<T>::EgressBroadcasted { asset, batch_size });
			};
		}
	}
}

impl<T: Config> EgressApi for Pallet<T> {
	type Amount = FlipBalance;
	type EgressAddress = ForeignChainAddress;

	fn egress_asset(
		foreign_asset: ForeignChainAsset,
		amount: Self::Amount,
		egress_address: Self::EgressAddress,
	) -> DispatchResult {
		ensure!(
			AllowedEgressAssets::<T>::get(foreign_asset).is_some(),
			Error::<T>::AssetEgressDisallowed
		);

		match egress_address {
			ForeignChainAddress::Eth(_) => ensure!(
				foreign_asset.chain == ForeignChain::Ethereum,
				Error::<T>::InvalidEgressDestination
			),
			ForeignChainAddress::Dot(_) => ensure!(
				foreign_asset.chain == ForeignChain::Polkadot,
				Error::<T>::InvalidEgressDestination
			),
		};

		ScheduledEgress::<T>::mutate(&foreign_asset, |batch| {
			batch.push((amount, egress_address));
		});
		Self::deposit_event(Event::<T>::EgressScheduled { foreign_asset, amount, egress_address });

		Ok(())
	}
}

impl<T: Config> EgressAbiBuilder for Pallet<T> {
	type Amount = FlipBalance;
	type EgressAddress = ForeignChainAddress;
	type EgressTransaction = T::EthereumEgressTransaction;

	// Take in a batch of transactions and construct the Transaction appropriate for the chain.
	fn construct_batched_transaction(
		foreign_asset: ForeignChainAsset,
		batch: EgressBatch<Self::Amount, Self::EgressAddress>,
	) -> Option<Self::EgressTransaction> {
		// NOTE: We currently only support Ethereum chain.
		if foreign_asset.chain != ForeignChain::Ethereum {
			return None
		}
		let asset_address =
			T::SupportedEthAssetsAddressProvider::try_get_asset_address(foreign_asset.asset)?;

		Some(T::EthereumEgressTransaction::new_unsigned(
			T::ReplayProtection::replay_protection(),
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
		))
	}
}
