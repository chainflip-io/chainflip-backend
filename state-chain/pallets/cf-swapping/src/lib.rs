#![cfg_attr(not(feature = "std"), no_std)]
use cf_chains::address::ForeignChainAddress;
use cf_primitives::{Asset, AssetAmount, ForeignChain};
use cf_traits::{liquidity::SwappingApi, IngressApi, SystemStateInfo};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{traits::Saturating, Permill},
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_arithmetic::{helpers_128bit::multiply_by_rational_with_rounding, Rounding};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

const BASIS_POINTS_PER_MILLION: u32 = 100;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
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
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
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
		NewSwapIntent {
			ingress_address: ForeignChainAddress,
		},
		/// The swap ingress was received.
		SwapIngressReceived {
			swap_id: u64,
			ingress_address: ForeignChainAddress,
			ingress_amount: AssetAmount,
		},
		SwapScheduledByWitnesser {
			swap_id: u64,
			ingress_amount: AssetAmount,
			egress_address: ForeignChainAddress,
		},
		/// A swap was executed.
		SwapExecuted {
			swap_id: u64,
		},
		/// A swap egress was scheduled.
		SwapEgressScheduled {
			swap_id: u64,
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
		},
		/// A withdrawal was requested.
		WithdrawalRequested {
			amount: AssetAmount,
			address: ForeignChainAddress,
			egress_id: EgressId,
		},
		BatchSwapFailed {
			asset_pair: (Asset, Asset),
		},
	}
	#[pallet::error]
	pub enum Error<T> {
		/// The provided asset and withdrawal address are incompatible.
		IncompatibleAssetAndAddress,
		/// The Asset cannot be egressed to the destination chain.
		InvalidEgressAddress,
		/// The withdrawal is not possible because not enough funds are available.
		NoFundsAvailable,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Do swapping with remaining weight in this block
		fn on_idle(_block_number: BlockNumberFor<T>, available_weight: Weight) -> Weight {
			let swaps = SwapQueue::<T>::take();
			let mut used_weight = T::DbWeight::get().reads(1);

			let swap_groups = Self::group_swaps_by_asset_pair(swaps);
			let mut unexecuted = vec![];

			for (asset_pair, swaps) in swap_groups {
				let swap_group_weight = T::WeightInfo::execute_group_of_swaps(swaps.len() as u32);
				if used_weight.saturating_add(swap_group_weight).all_gt(available_weight) {
					// Add un-excecuted swaps back to storage
					unexecuted.extend(swaps)
				} else {
					// Execute the swaps and add the weights.
					used_weight.saturating_accrue(swap_group_weight);
					if Self::execute_group_of_swaps(&swaps[..], asset_pair.0, asset_pair.1).is_err()
					{
						// If the swaps failed to execute, add them back into the queue.
						Self::deposit_event(Event::<T>::BatchSwapFailed { asset_pair });
						unexecuted.extend(swaps)
					}
				}
			}

			if !unexecuted.is_empty() {
				SwapQueue::<T>::put(unexecuted);
				used_weight.saturating_accrue(T::DbWeight::get().writes(1));
			}
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
				ForeignChain::from(egress_address.clone()) == ForeignChain::from(egress_asset),
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

		/// Relayers can withdraw their collected fees.
		///
		/// ## Events
		///
		/// - [WithdrawalRequested](Event::WithdrawalRequested)
		#[pallet::weight(T::WeightInfo::withdraw())]
		pub fn withdraw(
			origin: OriginFor<T>,
			asset: Asset,
			egress_address: ForeignChainAddress,
		) -> DispatchResult {
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_relayer(origin)?;

			ensure!(
				ForeignChain::from(egress_address.clone()) == ForeignChain::from(asset),
				Error::<T>::InvalidEgressAddress
			);

			let amount = EarnedRelayerFees::<T>::take(account_id, asset);
			ensure!(amount != 0, Error::<T>::NoFundsAvailable);

			Self::deposit_event(Event::<T>::WithdrawalRequested {
				amount,
				address: egress_address.clone(),
				egress_id: T::EgressHandler::schedule_egress(asset, amount, egress_address),
			});

			Ok(())
		}

		/// Allow Witnessers to submit a Swap request on the behalf of someone else.
		/// Requires Witnesser origin.
		///
		/// ## Events
		///
		/// - [SwapScheduled](Event::SwapIngressReceived)
		#[pallet::weight(0)]
		pub fn schedule_swap_by_witnesser(
			origin: OriginFor<T>,
			from: Asset,
			to: Asset,
			ingress_amount: AssetAmount,
			egress_address: ForeignChainAddress,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let swap_id = Self::schedule_swap(from, to, ingress_amount, egress_address.clone());

			Self::deposit_event(Event::<T>::SwapScheduledByWitnesser {
				swap_id,
				ingress_amount,
				egress_address,
			});

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn execute_group_of_swaps(swaps: &[Swap], from: Asset, to: Asset) -> DispatchResult {
			debug_assert!(
				!swaps.is_empty(),
				"The implementation of grouped_swaps ensures that the swap groups are non-empty."
			);

			let bundle_input: AssetAmount = swaps
				.iter()
				.map(|swap| {
					debug_assert_eq!((swap.from, swap.to), (from, to));
					swap.amount
				})
				.sum();

			debug_assert!(bundle_input > 0, "Swap input of zero is invalid.");

			let bundle_output = T::SwappingApi::swap(from, to, bundle_input)?;
			for swap in swaps {
				Self::deposit_event(Event::<T>::SwapExecuted { swap_id: swap.swap_id });
				let swap_output = multiply_by_rational_with_rounding(
					swap.amount,
					bundle_output,
					bundle_input,
					Rounding::Down,
				)
				.expect("bundle_input >= swap_amount ∴ result can't overflow");

				if swap_output > 0 {
					let egress_id = T::EgressHandler::schedule_egress(
						swap.to,
						swap_output,
						swap.clone().egress_address,
					);

					Self::deposit_event(Event::<T>::SwapEgressScheduled {
						swap_id: swap.swap_id,
						egress_id,
						asset: to,
						amount: swap_output,
					});
				} else {
					// This is unlikely but theoretically possible if, for example, the initial swap
					// input is so small compared to the total bundle size that it rounds down to
					// zero when we do the division.
					log::warn!(
						"Swap {:?} in bundle {{ input: {bundle_input}, output: {bundle_output} }} resulted in swap output of zero.",
						swap
					);
				}
			}
			Ok(())
		}

		fn group_swaps_by_asset_pair(swaps: Vec<Swap>) -> BTreeMap<(Asset, Asset), Vec<Swap>> {
			let mut grouped_swaps = BTreeMap::new();
			for swap in swaps {
				grouped_swaps.entry((swap.from, swap.to)).or_insert(vec![]).push(swap)
			}
			grouped_swaps
		}

		fn schedule_swap(
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			egress_address: ForeignChainAddress,
		) -> u64 {
			// The caller should ensure that the egress details are consistent.
			debug_assert_eq!(ForeignChain::from(egress_address.clone()), ForeignChain::from(to));

			let swap_id = SwapIdCounter::<T>::mutate(|id| {
				id.saturating_accrue(1);
				*id
			});

			SwapQueue::<T>::append(Swap { swap_id, from, to, amount, egress_address });

			swap_id
		}
	}

	impl<T: Config> SwapIntentHandler for Pallet<T> {
		type AccountId = T::AccountId;

		/// Callback function to kick off the swapping process after a successful ingress.
		fn on_swap_ingress(
			ingress_address: ForeignChainAddress,
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			egress_address: ForeignChainAddress,
			relayer_id: Self::AccountId,
			relayer_commission_bps: BasisPoints,
		) {
			let fee = Permill::from_parts(relayer_commission_bps as u32 * BASIS_POINTS_PER_MILLION) *
				amount;

			EarnedRelayerFees::<T>::mutate(&relayer_id, from, |earned_fees| {
				earned_fees.saturating_accrue(fee)
			});

			let swap_id = Self::schedule_swap(from, to, amount.saturating_sub(fee), egress_address);

			Self::deposit_event(Event::<T>::SwapIngressReceived {
				swap_id,
				ingress_address,
				ingress_amount: amount,
			});
		}
	}
}
