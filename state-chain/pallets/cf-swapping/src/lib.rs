#![cfg_attr(not(feature = "std"), no_std)]
use cf_primitives::{ForeignChainAddress, ForeignChainAsset};
use cf_traits::{liquidity::AmmPoolApi, IngressApi};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::vec::Vec;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {

	use cf_primitives::{Asset, AssetAmount, IntentId, Swap};
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
		type Ingress: IngressApi<AccountId = <Self as frame_system::Config>::AccountId>;
		/// An interface to the egress api implementation.
		type Egress: EgressApi;
		/// The AMM
		type AmmPoolApi: AmmPoolApi<Balance = AssetAmount>;
		/// Weight information
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	/// Scheduled Swaps
	#[pallet::storage]
	pub(super) type SwapQueue<T: Config> = StorageValue<_, Vec<Swap>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An new swap intent has been registered.
		NewSwapIntent { intent_id: IntentId, ingress_address: ForeignChainAddress },
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Do swapping with remaining weight in this block
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			// The computational cost for a swap - TODO: Add the real weight values here
			let swap_weight = T::WeightInfo::execute_swap();
			let mut swaps = SwapQueue::<T>::get();
			// Calculate the capacities we have left for this block
			let capacity = (remaining_weight / swap_weight) as usize;
			// Split the array in what we can process during this block and the rest. If we could do
			// more we just process all.
			let cut_off = if (swaps.len() as usize) < capacity { swaps.len() } else { capacity };
			let swaps_that_fit = swaps.split_off(cut_off);
			for swap in swaps_that_fit.iter() {
				Self::execute_swap(swap.clone());
			}
			// Write the rest back (potentially and empty vector).
			SwapQueue::<T>::put(swaps);
			// return the weight we used during the execution of this function
			swap_weight * capacity as u64 + T::WeightInfo::on_idle()
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
			ingress_asset: ForeignChainAsset,
			egress_asset: ForeignChainAsset,
			egress_address: ForeignChainAddress,
			relayer_commission_bps: u16,
		) -> DispatchResultWithPostInfo {
			T::AccountRoleRegistry::ensure_relayer(origin)?;

			// TODO: ensure egress address chain matches egress asset chain
			//   (or consider if we can merge both into one struct / derive one from the other)
			let (intent_id, ingress_address) = T::Ingress::register_swap_intent(
				ingress_asset,
				egress_asset,
				egress_address,
				relayer_commission_bps,
			)?;

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
		pub fn execute_swap(swap: Swap) {
			let amount = T::AmmPoolApi::swap(swap.from, swap.to, swap.amount);
			// Send the assets off-chain.
			// TODO: If this is falling we have to reschedule it. Not sure if this is the right
			// place for it though. I would expect the Egress pallet is responsible for that?
			if let Err(_) = T::Egress::schedule_egress(swap.to, amount, swap.egress_address) {
				log::warn!("Failed to egress swap.");
			}
		}
	}

	impl<T: Config> SwapIntentHandler for Pallet<T> {
		/// Callback function to kick of the swapping process after a successful ingress.
		fn schedule_swap(
			from: Asset,
			to: ForeignChainAsset,
			amount: AssetAmount,
			ingress_address: ForeignChainAddress,
			egress_address: ForeignChainAddress,
		) {
			SwapQueue::<T>::mutate(|swaps| {
				swaps.push(Swap { from, to, amount, ingress_address, egress_address })
			});
		}
	}
}
