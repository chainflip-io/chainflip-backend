#![cfg_attr(not(feature = "std"), no_std)]
use cf_primitives::{Asset, AssetAmount, ForeignChain, ForeignChainAddress};
use cf_traits::{liquidity::SwappingApi, IngressApi};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::{cmp, vec::Vec};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub struct Swap<AccountId> {
	pub from: Asset,
	pub to: Asset,
	pub amount: AssetAmount,
	pub egress_address: ForeignChainAddress,
	pub relayer_id: AccountId,
	pub relayer_commission_bps: u16,
}

#[frame_support::pallet]
pub mod pallet {

	use cf_chains::{eth::assets, Ethereum};
	use cf_primitives::{Asset, AssetAmount, EthereumAddress, IntentId};
	use cf_traits::{AccountRoleRegistry, Chainflip, EgressApi, SwapIntentHandler};

	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;
		/// An interface to the ingress api implementation.
		type Ingress: IngressApi<AccountId = <Self as frame_system::Config>::AccountId, Ethereum>;
		/// An interface to the egress api implementation.
		type Egress: EgressApi<Ethereum>;
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
	pub(super) type SwapQueue<T: Config> = StorageValue<_, Vec<Swap<T::AccountId>>, ValueQuery>;

	/// Earned Fees by Relayers
	#[pallet::storage]
	pub(super) type EarnedRelayerFees<T: Config> =
		StorageDoubleMap<_, Identity, T::AccountId, Twox64Concat, Asset, AssetAmount>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An new swap intent has been registered.
		NewSwapIntent { intent_id: IntentId, ingress_address: ForeignChainAddress },
	}
	#[pallet::error]
	pub enum Error<T> {
		InvalidAsset,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Do swapping with remaining weight in this block
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			// The computational cost for a swap.
			let swap_weight = T::WeightInfo::execute_swap();
			let mut swaps = SwapQueue::<T>::get();
			// We split the array in what we can process during this block and the rest. If we could
			// do more we just process all. We calculate the index based on the available weight and
			// the weight we need for performing a single swap.
			let remaining_swaps = swaps.split_off(cmp::min(
				swaps.len(),
				(remaining_weight.saturating_div(swap_weight)) as usize,
			));
			let swaps_executed = swaps.len();
			for swap in swaps {
				Self::execute_swap(swap);
			}
			// Write the rest back (potentially an empty vector).
			SwapQueue::<T>::put(remaining_swaps);
			// return the weight we used during the execution of this function.
			swap_weight * swaps_executed as u64 + T::WeightInfo::on_idle()
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
			relayer_commission_bps: u16,
		) -> DispatchResultWithPostInfo {
			let relayer = T::AccountRoleRegistry::ensure_relayer(origin)?;

			let (intent_id, ingress_address) = match ingress_asset.into() {
				ForeignChain::Ethereum => T::Ingress::register_swap_intent(
					ingress_asset.try_into().unwrap(),
					egress_asset,
					egress_address,
					relayer_commission_bps,
					relayer,
				),
				_ => todo!(),
			}?;
			Self::deposit_event(Event::<T>::NewSwapIntent { intent_id, ingress_address });

			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Executes a swap. This includes the whole process of:
		///
		/// - Doing the Swap inside the AMM
		/// - Doing the egress
		///
		/// We are going to benchmark this function individually to have a approximation of
		/// how 'expensive' a swap is.
		pub fn execute_swap(swap: Swap<T::AccountId>) {
			let (swap_output, (asset, fee)) =
				T::SwappingApi::swap(swap.from, swap.to, swap.amount, swap.relayer_commission_bps);
			EarnedRelayerFees::<T>::mutate(&swap.relayer_id, asset, |maybe_fees| {
				if let Some(fees) = maybe_fees {
					*maybe_fees = Some(fees.saturating_add(fee))
				} else {
					*maybe_fees = Some(fee)
				}
			});
			// TODO: remove the expects by using AnyChain.
			T::Egress::schedule_egress(
				assets::eth::Asset::try_from(swap.to).expect("Only eth assets supported"),
				swap_output,
				EthereumAddress::try_from(swap.egress_address)
					.expect("On eth assets supported")
					.into(),
			);
		}
	}

	impl<T: Config> SwapIntentHandler for Pallet<T> {
		type AccountId = T::AccountId;
		/// Callback function to kick of the swapping process after a successful ingress.
		fn schedule_swap(
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			egress_address: ForeignChainAddress,
			relayer_id: Self::AccountId,
			relayer_commission_bps: u16,
		) {
			SwapQueue::<T>::append(Swap {
				from,
				to,
				amount,
				egress_address,
				relayer_id,
				relayer_commission_bps,
			});
		}
	}
}
