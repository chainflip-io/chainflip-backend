#![cfg_attr(not(feature = "std"), no_std)]
use cf_primitives::{Asset, AssetAmount, ForeignChain, ForeignChainAddress};
use cf_traits::{liquidity::SwappingApi, IngressApi, SystemStateInfo};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{traits::Saturating, Permill},
};
use frame_system::pallet_prelude::*;
use sp_arithmetic::{helpers_128bit::multiply_by_rational_with_rounding, Rounding};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

const BASIS_POINTS_PER_MILLION: u32 = 100;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub struct Swap {
	pub swap_id: u64,
	pub from: Asset,
	pub to: Asset,
	pub amount: AssetAmount,
	pub egress_address: ForeignChainAddress,
}

#[frame_support::pallet]
pub mod pallet {

	use cf_chains::AnyChain;
	use cf_primitives::{Asset, AssetAmount, BasisPoints, EgressId};
	use cf_traits::{AccountRoleRegistry, Chainflip, EgressApi, SwapIntentHandler};

	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;
		/// API for handling asset ingress.
		type IngressHandler: IngressApi<
			AnyChain,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;
		/// API for handling asset egress.
		type EgressHandler: EgressApi<AnyChain>;
		/// An interface to the AMM api implementation.
		type SwappingApi: SwappingApi;
		/// The Weight information.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	/// Scheduled Swaps
	#[pallet::storage]
	pub(super) type SwapQueue<T: Config> = StorageValue<_, Vec<Swap>, ValueQuery>;

	/// SwapId Counter
	#[pallet::storage]
	pub type SwapIdCounter<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Earned Fees by Relayers
	#[pallet::storage]
	pub(super) type EarnedRelayerFees<T: Config> =
		StorageDoubleMap<_, Identity, T::AccountId, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An new swap intent has been registered.
		NewSwapIntent { ingress_address: ForeignChainAddress },
		/// The swap ingress was received.
		SwapIngressReceived {
			ingress_address: ForeignChainAddress,
			swap_id: u64,
			ingress_amount: AssetAmount,
		},
		/// A swap was executed.
		SwapExecuted { swap_id: u64 },
		/// A swap egress was scheduled.
		SwapEgressScheduled { swap_id: u64, egress_id: EgressId, egress_amount: AssetAmount },
	}
	#[pallet::error]
	pub enum Error<T> {
		/// The provided asset and withdrawal address are incompatible.
		IncompatibleAssetAndAddress,
		// The Asset cannot be egressed to the destination chain.
		InvalidEgressAddress,
		// The withdrawal is not possible because not enough funds are available.
		NoFundsAvailable,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Do swapping with remaining weight in this block
		fn on_idle(_block_number: BlockNumberFor<T>, available_weight: Weight) -> Weight {
			let swaps = SwapQueue::<T>::get();
			let mut used_weight =
				T::DbWeight::get().reads(1 as Weight) + T::DbWeight::get().writes(1 as Weight);

			let swap_groups = Self::group_swaps_by_asset_pair(swaps);
			let mut unexecuted = vec![];

			for (asset_pair, swaps) in swap_groups {
				let swap_group_weight = T::WeightInfo::execute_group_of_swaps(swaps.len() as u32);
				if used_weight.saturating_add(swap_group_weight) > available_weight {
					// Add un-excecuted swaps back to storage
					unexecuted.extend(swaps)
				} else {
					// Execute the swaps and add the weights.
					used_weight.saturating_accrue(swap_group_weight);
					Self::execute_group_of_swaps(swaps, asset_pair.0, asset_pair.1);
				}
			}

			SwapQueue::<T>::put(unexecuted);
			used_weight
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register a new swap intent.
		///
		/// ## Events
		///
		/// - [NewSwapIntent](Event::NewSwapIntent)
		#[pallet::weight(T::WeightInfo::register_swap_intent())]
		pub fn register_swap_intent(
			origin: OriginFor<T>,
			ingress_asset: Asset,
			egress_asset: Asset,
			egress_address: ForeignChainAddress,
			relayer_commission_bps: BasisPoints,
		) -> DispatchResult {
			let relayer = T::AccountRoleRegistry::ensure_relayer(origin)?;

			ensure!(
				ForeignChain::from(egress_address) == ForeignChain::from(egress_asset),
				Error::<T>::IncompatibleAssetAndAddress
			);

			let (_, ingress_address) = T::IngressHandler::register_swap_intent(
				ingress_asset,
				egress_asset,
				egress_address,
				relayer_commission_bps,
				relayer,
			)?;

			Self::deposit_event(Event::<T>::NewSwapIntent { ingress_address });

			Ok(())
		}

		#[pallet::weight(0)]
		pub fn withdrawal(
			origin: OriginFor<T>,
			asset: Asset,
			egress_address: ForeignChainAddress,
		) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_relayer(origin)?;

			// Check validity of Chain and Asset
			ensure!(
				ForeignChain::from(egress_address) == ForeignChain::from(asset),
				Error::<T>::InvalidEgressAddress
			);

			let amount = EarnedRelayerFees::<T>::take(account_id, asset);
			ensure!(amount != 0, Error::<T>::NoFundsAvailable);
			T::EgressHandler::schedule_egress(asset, amount, egress_address);
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn execute_group_of_swaps(swaps: Vec<Swap>, from: Asset, to: Asset) {
			let mut bundle_input = 0;
			let mut bundle_inputs = vec![];

			for swap in &swaps {
				debug_assert_eq!((swap.from, swap.to), (from, to));
				// TODO: use a struct instead of tuple.
				bundle_inputs.push((swap.amount, swap.to, swap.egress_address, swap.swap_id));
				bundle_input.saturating_accrue(swap.amount);
			}

			let (bundle_output, _) = T::SwappingApi::swap(from, to, bundle_input, 1);

			for swap in &swaps {
				Self::deposit_event(Event::<T>::SwapExecuted { swap_id: swap.swap_id });
			}

			for (input_amount, egress_asset, egress_address, id) in bundle_inputs {
				if let Some(swap_output) = multiply_by_rational_with_rounding(
					input_amount,
					bundle_output,
					bundle_input,
					Rounding::Down,
				) {
					let egress_id = T::EgressHandler::schedule_egress(
						egress_asset,
						swap_output,
						egress_address,
					);
					Self::deposit_event(Event::<T>::SwapEgressScheduled {
						swap_id: id,
						egress_id,
						egress_amount: swap_output,
					});
				} else {
					log::error!(
						"Unable to calculate valid swap output for swap {:?}!",
						&(input_amount, bundle_input, bundle_output)
					);
				}
			}
		}

		fn group_swaps_by_asset_pair(swaps: Vec<Swap>) -> BTreeMap<(Asset, Asset), Vec<Swap>> {
			let mut grouped_swaps = BTreeMap::new();
			for swap in swaps {
				grouped_swaps.entry((swap.from, swap.to)).or_insert(vec![]).push(swap)
			}
			grouped_swaps
		}
	}

	impl<T: Config> SwapIntentHandler for Pallet<T> {
		type AccountId = T::AccountId;

		/// Callback function to kick off the swapping process after a successful ingress.
		fn schedule_swap(
			ingress_address: ForeignChainAddress,
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			egress_address: ForeignChainAddress,
			relayer_id: Self::AccountId,
			relayer_commission_bps: BasisPoints,
		) -> DispatchResult {
			// The caller should ensure that the egress details are consistent.
			debug_assert_eq!(ForeignChain::from(egress_address), ForeignChain::from(to));
			let swap_id = SwapIdCounter::<T>::get().saturating_add(1);

			let fee = Permill::from_parts(relayer_commission_bps as u32 * BASIS_POINTS_PER_MILLION) *
				amount;

			EarnedRelayerFees::<T>::mutate(&relayer_id, from, |earned_fees| {
				earned_fees.saturating_accrue(fee)
			});

			SwapQueue::<T>::append(Swap {
				swap_id,
				from,
				to,
				amount: amount.saturating_sub(fee),
				egress_address,
			});

			Self::deposit_event(Event::<T>::SwapIngressReceived {
				ingress_address,
				swap_id,
				ingress_amount: amount,
			});
			SwapIdCounter::<T>::put(swap_id);
			Ok(())
		}
	}
}
