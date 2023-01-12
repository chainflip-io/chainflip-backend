#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

use cf_chains::{Age, Chain};
use cf_traits::Chainflip;
use frame_support::dispatch::DispatchResultWithPostInfo;
use frame_system::pallet_prelude::OriginFor;
pub use pallet::*;
use sp_std::marker::PhantomData;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

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

		/// Determines the maximum age of tracked data submissions.
		#[pallet::constant]
		type AgeLimit: Get<<Self::TargetChain as Chain>::ChainBlockNumber>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	/// The tracked state of the external chain.
	#[pallet::storage]
	pub type ChainState<T: Config<I>, I: 'static = ()> =
		StorageValue<_, <T::TargetChain as Chain>::TrackedData>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// The tracked state of this chain has been updated.
		ChainStateUpdated { state: <T::TargetChain as Chain>::TrackedData },
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
			state: <T::TargetChain as Chain>::TrackedData,
		) -> DispatchResultWithPostInfo {
			let _ok = T::EnsureWitnessed::ensure_origin(origin)?;

			ChainState::<T, I>::try_mutate::<_, Error<T, I>, _>(|maybe_previous| {
				if let Some(previous) = maybe_previous.replace(state.clone()) {
					ensure!(
						sp_runtime::traits::Saturating::saturating_sub(
							previous.birth_block(),
							state.birth_block()
						) < T::AgeLimit::get(),
						Error::<T, I>::StaleDataSubmitted
					)
				};
				Ok(())
			})?;

			Self::deposit_event(Event::<T, I>::ChainStateUpdated { state });

			Ok(().into())
		}
	}
}
