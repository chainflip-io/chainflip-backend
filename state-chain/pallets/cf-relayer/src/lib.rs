#![cfg_attr(not(feature = "std"), no_std)]
use cf_primitives::{ForeignChainAddress, ForeignChainAsset};
use cf_traits::IngressApi;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;

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

	use cf_primitives::IntentId;

	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// An interface to the ingress api implementation.
		type Ingress: IngressApi<AccountId = <Self as frame_system::Config>::AccountId>;
		/// Weight information
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An new swap intent has been registered.
		NewSwapIntent { intent_id: IntentId, ingress_address: ForeignChainAddress },
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register a new swap intent.
		///
		/// ## Events
		///
		/// - [NewSwapIntent](Event::NewSwapIntent)
		#[pallet::weight(T::WeightInfo::request_swap_intent())]
		pub fn register_swap_intent(
			origin: OriginFor<T>,
			ingress_asset: ForeignChainAsset,
			egress_asset: ForeignChainAsset,
			egress_address: ForeignChainAddress,
			relayer_commission_bps: u16,
		) -> DispatchResultWithPostInfo {
			// TODO: should be ensure_relayer.
			let _ = ensure_signed(origin)?;

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
}
