#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

mod benchmarking;
mod migrations;
mod mock;
mod tests;

pub mod weights;
pub use weights::WeightInfo;

use cf_chains::Chain;
use cf_traits::{Chainflip, GetBlockHeight};
use frame_support::{
	dispatch::DispatchResultWithPostInfo, pallet_prelude::*, sp_runtime::traits::Zero,
	traits::OnRuntimeUpgrade,
};
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::marker::PhantomData;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// A marker trait identifying the chain whose state we are tracking.
		type TargetChain: Chain;

		/// The weights for the pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<T::BlockNumber> for Pallet<T, I> {
		fn on_runtime_upgrade() -> Weight {
			migrations::PalletMigration::<T, I>::on_runtime_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, &'static str> {
			migrations::PalletMigration::<T, I>::pre_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: sp_std::vec::Vec<u8>) -> Result<(), &'static str> {
			migrations::PalletMigration::<T, I>::post_upgrade(state)
		}
	}

	#[derive(
		PartialEqNoBound,
		EqNoBound,
		CloneNoBound,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		DebugNoBound,
	)]
	#[scale_info(skip_type_params(T, I))]
	pub struct ChainState<C: Chain> {
		pub block_height: C::ChainBlockNumber,
		pub tracked_data: C::TrackedData,
	}

	/// The tracked state of the external chain.
	#[pallet::storage]
	#[pallet::getter(fn chain_state)]
	#[allow(clippy::type_complexity)]
	pub type CurrentChainState<T: Config<I>, I: 'static = ()> =
		StorageValue<_, ChainState<T::TargetChain>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// The tracked state of this chain has been updated.
		ChainStateUpdated { new_chain_state: ChainState<T::TargetChain> },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The submitted data is too old.
		StaleDataSubmitted,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Logs the latest known state of the external chain defined by [Config::TargetChain].
		///
		/// ## Events
		///
		/// - [Event::ChainStateUpdated]
		///
		/// ## Errors
		///
		/// - [Error::StaleDataSubmitted]
		#[pallet::weight(T::WeightInfo::update_chain_state())]
		pub fn update_chain_state(
			origin: OriginFor<T>,
			new_chain_state: ChainState<T::TargetChain>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			CurrentChainState::<T, I>::try_mutate::<_, Error<T, I>, _>(|maybe_previous| {
				if let Some(previous_chain_state) = maybe_previous {
					ensure!(
						new_chain_state.block_height > previous_chain_state.block_height,
						Error::<T, I>::StaleDataSubmitted
					)
				}

				*maybe_previous = Some(new_chain_state.clone());

				Ok(())
			})?;
			Self::deposit_event(Event::<T, I>::ChainStateUpdated { new_chain_state });

			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> GetBlockHeight<T::TargetChain> for Pallet<T, I> {
	fn get_block_height() -> <T::TargetChain as Chain>::ChainBlockNumber {
		CurrentChainState::<T, I>::get()
			.map(|state| state.block_height)
			.unwrap_or(Zero::zero())
	}
}
