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
	Broadcaster, ChainTrackedDataProvider, EgressAbiBuilder, EgressApi, EthExchangeRateProvider,
	FlipBalance, ReplayProtectionProvider, SupportedEthAssetsAddressProvider,
};
use frame_support::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::{traits::Zero, FixedPointNumber};
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

		/// Benchmark weights
		type WeightInfo: WeightInfo;

		/// Replay protection.
		type ReplayProtection: ReplayProtectionProvider<Ethereum>;

		/// The type of the chain-native transaction.
		type EgressTransaction: AllBatch<Ethereum>;

		/// A broadcaster instance.
		type Broadcaster: Broadcaster<Ethereum, ApiCall = Self::EgressTransaction>;

		/// Governance origin to manage allowed assets
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// API for getting Eth related parameters.
		type ChainTrackedDataProvider: ChainTrackedDataProvider<Ethereum>;

		/// An API for getting Ethereum related parameters
		type SupportedEthAssetsAddressProvider: SupportedEthAssetsAddressProvider;

		/// Price feeder that provides the exchange rate for Eth to other assets.
		type EthExchangeRateProvider: EthExchangeRateProvider;
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
			asset: ForeignChainAsset,
			amount: FlipBalance,
			egress_address: ForeignChainAddress,
		},
		EgressBroadcasted {
			asset: ForeignChainAsset,
			num_tx: u32,
			gas_fee: FlipBalance,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		// The given asset is not allowed to be Egressed
		AssetNotAllowedToEgress,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Take a batch of scheduled Egress and send them out
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut weights_left = remaining_weight;

			AllowedEgressAssets::<T>::iter().for_each(|(asset, ())| {
				if weights_left > T::WeightInfo::send_batch_egress(1) {
					let tx_count = Self::send_scheduled_batch_transaction(asset, None);
					weights_left =
						weights_left.saturating_sub(T::WeightInfo::send_batch_egress(tx_count));
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
	fn send_scheduled_batch_transaction(asset: ForeignChainAsset, maybe_count: Option<u32>) -> u32 {
		// Take the scheduled Egress calls to be sent out of storage.
		let mut all_scheduled = ScheduledEgressBatches::<T>::take(asset);
		let split_point: usize = match maybe_count {
			Some(count) => all_scheduled.len().saturating_sub(count as usize),
			None => 0,
		};
		let mut batch = all_scheduled.split_off(split_point);
		if batch.is_empty() {
			return 0
		}

		let batch_size = batch.len() as u32;
		if !all_scheduled.is_empty() {
			ScheduledEgressBatches::<T>::insert(asset, all_scheduled);
		}

		// Construct the Egress Tx and send it out.
		if asset.chain == ForeignChain::Ethereum {
			if let Some((egress_transaction, fee)) =
				Pallet::<T>::construct_batched_transaction(asset, &mut batch)
			{
				T::Broadcaster::threshold_sign_and_broadcast(egress_transaction);
				Self::deposit_event(Event::<T>::EgressBroadcasted {
					asset,
					num_tx: batch_size,
					gas_fee: fee,
				});
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
		ensure!(
			AllowedEgressAssets::<T>::get(asset).is_some(),
			Error::<T>::AssetNotAllowedToEgress
		);

		ScheduledEgressBatches::<T>::mutate(&asset, |batch| {
			batch.push((amount, egress_address));
		});
		Self::deposit_event(Event::<T>::EgressScheduled { asset, amount, egress_address });

		Ok(())
	}
}

impl<T: Config> EgressAbiBuilder for Pallet<T> {
	type Amount = FlipBalance;
	type EgressAddress = ForeignChainAddress;
	type EgressTransaction = T::EgressTransaction;

	// Take in a batch of transactions and construct the Transaction appropriate for the chain.
	fn construct_batched_transaction(
		asset: ForeignChainAsset,
		batch: &mut EgressBatch<Self::Amount, Self::EgressAddress>,
	) -> Option<(Self::EgressTransaction, FlipBalance)> {
		if asset.chain != ForeignChain::Ethereum {
			return None
		}
		if let Some(asset_address) =
			T::SupportedEthAssetsAddressProvider::try_get_asset_address(asset.asset)
		{
			// Take the transaction fee by skimming from the batch.
			let total_fee = Self::skim_transaction_fee(asset, batch);

			// Take only transactions going into Ethereum and construct transaction as a batch.
			let asset_params = batch
				.iter_mut()
				.filter_map(|(amount, address)| match address {
					ForeignChainAddress::Eth(eth_address) => Some(TransferAssetParams {
						asset: asset_address.into(),
						account: eth_address.into(),
						amount: *amount,
					}),
					_ => None,
				})
				.collect();

			Some((
				T::EgressTransaction::new_unsigned(
					T::ReplayProtection::replay_protection(),
					vec![], // TODO: fetch assets
					asset_params,
				),
				total_fee,
			))
		} else {
			None
		}
	}

	/// Obtains the total transaction fee by deducting an equal amount from each transaction in the
	/// batch.
	///
	/// Returns the total fee.
	fn skim_transaction_fee(
		asset: ForeignChainAsset,
		batch: &mut EgressBatch<Self::Amount, Self::EgressAddress>,
	) -> Self::Amount {
		let fee_each = Self::estimate_cost(asset, batch).saturating_div(batch.len() as u128);
		let mut total_fee: Self::Amount = 0;

		if !fee_each.is_zero() {
			batch.iter_mut().for_each(|(amount, _)| {
				*amount = amount.saturating_sub(fee_each);
				total_fee = total_fee.saturating_add(fee_each);
			});
		}

		total_fee
	}

	/// Estimates the total transaction cost for the given batch.
	fn estimate_cost(
		asset: ForeignChainAsset,
		_batch: &EgressBatch<Self::Amount, Self::EgressAddress>,
	) -> Self::Amount {
		// TODO: Gets the gas fee cost in Eth and convert it to the given asset
		match asset.chain {
			ForeignChain::Ethereum => {
				let tracked_data =
					T::ChainTrackedDataProvider::get_tracked_data().unwrap_or_default();

				// Convert the gas fee (in target chain's native currency) into target currency.
				T::EthExchangeRateProvider::get_eth_exchange_rate(asset.asset)
					.checked_mul_int(tracked_data.base_fee)
					.unwrap_or_default()
			},
			ForeignChain::Polkadot => 0,
		}
	}
}
