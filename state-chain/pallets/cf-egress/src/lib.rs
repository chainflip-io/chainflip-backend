#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

// #[cfg(test)]
// mod mock;
// #[cfg(test)]
// mod tests;

use cf_chains::{AllBatch, TransferAssetParams};
use cf_primitives::{
	try_convert_foreign_chain_asset_to_ethereum_address, ForeignChain, ForeignChainAddress,
	ForeignChainAsset,
};
use cf_traits::{Broadcaster, EgressApi, FlipBalance, ReplayProtectionProvider};
use frame_support::pallet_prelude::*;
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	use cf_chains::Ethereum;
	use cf_primitives::{EgressBatch, ForeignChainAddress};
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
		type EgressTransaction: AllBatch<Ethereum>;

		/// A broadcaster instance.
		type Broadcaster: Broadcaster<Ethereum, ApiCall = Self::EgressTransaction>;

		/// Governance origin to manage allowed assets
		type EnsureGovernance: EnsureOrigin<Self::Origin>;
	}

	#[pallet::storage]
	pub(crate) type ScheduledEgressBatches<T: Config> = StorageMap<
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
			account_id: T::AccountId,
			asset: ForeignChainAsset,
			amount: FlipBalance,
			egress_address: ForeignChainAddress,
		},
	}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Take a batch of scheduled Egress and send them out
		fn on_idle(_block_number: BlockNumberFor<T>, _remaining_weight: Weight) -> Weight {
			// Estimate number of Egress Tx using weight

			AllowedEgressAssets::<T>::iter().for_each(|(asset, ())| {
				Self::send_scheduled_batch_transaction(asset, None);
			});

			// Send the Egress out

			0
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
		#[pallet::weight(0)]
		pub fn set_asset_egress_permission(
			origin: OriginFor<T>,
			asset: ForeignChainAsset,
			allowed: bool,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			match allowed {
				true =>
					if !AllowedEgressAssets::<T>::contains_key(asset) {
						AllowedEgressAssets::<T>::insert(asset, ());
					},
				false =>
					if AllowedEgressAssets::<T>::contains_key(asset) {
						AllowedEgressAssets::<T>::remove(asset);
					},
			}

			Self::deposit_event(Event::<T>::AssetPermissionSet { asset, allowed });

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	// Take Some(number of) or all scheduled batch Egress and send send it out.
	// Returns the actual number of Egress sent
	fn send_scheduled_batch_transaction(
		asset: ForeignChainAsset,
		maybe_count: Option<usize>,
	) -> usize {
		// Take the scheduled Egress calls to be sent out of storage.
		let mut all_scheduled = ScheduledEgressBatches::<T>::take(asset);
		let split_point = match maybe_count {
			Some(count) => all_scheduled.len().saturating_sub(count),
			None => 0,
		};
		let batch = all_scheduled.split_off(split_point);
		let batch_size = batch.len();
		if !all_scheduled.is_empty() {
			ScheduledEgressBatches::<T>::insert(asset, all_scheduled);
		}

		// Construct the Egress Tx and send it out.
		if asset.chain == ForeignChain::Ethereum {
			if let Some(asset_adress) = try_convert_foreign_chain_asset_to_ethereum_address(asset) {
				let asset_params = batch
					.into_iter()
					.filter_map(|(amount, address)| match address {
						ForeignChainAddress::Eth(eth_address) => Some(TransferAssetParams {
							asset: asset_adress.into(),
							account: eth_address.into(),
							amount,
						}),
						_ => None,
					})
					.collect();

				T::Broadcaster::threshold_sign_and_broadcast(T::EgressTransaction::new_unsigned(
					T::ReplayProtection::replay_protection(),
					vec![], // TODO: fetch assets
					asset_params,
				));
			};
		}
		batch_size
	}
}

impl<T: Config> EgressApi for Pallet<T> {
	type Amount = FlipBalance;
	type EgressAddress = ForeignChainAddress;

	fn add_to_egress_batch(
		asset: ForeignChainAsset,
		amount: Self::Amount,
		egress_address: Self::EgressAddress,
	) -> DispatchResult {
		ScheduledEgressBatches::<T>::mutate(&asset, |batch| {
			batch.push((amount, egress_address));
		});

		Ok(())
	}
}
