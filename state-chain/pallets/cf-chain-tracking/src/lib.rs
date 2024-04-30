#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

mod benchmarking;
pub mod migrations;
mod mock;
mod tests;

pub mod weights;
pub use weights::WeightInfo;

use cf_chains::{Chain, ChainState, FeeEstimationApi};
use cf_traits::{AdjustedFeeEstimationApi, Chainflip, GetBlockHeight};
use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::{FixedPointNumber, FixedU128};
use sp_std::marker::PhantomData;

const NO_CHAIN_STATE: &str = "Chain state should be set at genesis and never removed.";

pub struct GetOne;

impl Get<FixedU128> for GetOne {
	fn get() -> FixedU128 {
		FixedU128::from_u32(1)
	}
}

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(3);

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
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	/// The tracked state of the external chain.
	/// It is safe to unwrap() this value. We set it at genesis and it is only ever updated
	/// by chain tracking, never removed. We use OptionQuery here so we don't need to
	/// impl Default for ChainState.
	#[pallet::storage]
	#[pallet::getter(fn chain_state)]
	#[allow(clippy::type_complexity)]
	pub type CurrentChainState<T: Config<I>, I: 'static = ()> =
		StorageValue<_, ChainState<T::TargetChain>>;

	/// The fee multiplier value used when estimating ingress/egree fees
	#[pallet::storage]
	#[pallet::getter(fn fee_multiplier)]
	pub type FeeMultiplier<T: Config<I>, I: 'static = ()> =
		StorageValue<_, FixedU128, ValueQuery, GetOne>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// The tracked state of this chain has been updated.
		ChainStateUpdated { new_chain_state: ChainState<T::TargetChain> },
		/// The fee multiplier for this chain has been updated.
		FeeMultiplierUpdated { new_fee_multiplier: FixedU128 },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The submitted data is too old.
		StaleDataSubmitted,
		/// Block height must be a multiple of the witness period
		InvalidBlockHeight,
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		pub init_chain_state: ChainState<T::TargetChain>,
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			CurrentChainState::<T, I>::put(ChainState {
				block_height: self.init_chain_state.block_height -
					<T::TargetChain as Chain>::block_phase(self.init_chain_state.block_height),
				tracked_data: self.init_chain_state.tracked_data.clone(),
			});
		}
	}

	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			use frame_support::sp_runtime::traits::Zero;
			Self {
				init_chain_state: ChainState {
					block_height: <T::TargetChain as Chain>::ChainBlockNumber::zero(),
					tracked_data: <T::TargetChain as Chain>::TrackedData::default(),
				},
			}
		}
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
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::update_chain_state())]
		pub fn update_chain_state(
			origin: OriginFor<T>,
			new_chain_state: ChainState<T::TargetChain>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			ensure!(
				<T::TargetChain as Chain>::block_phase(new_chain_state.block_height) ==
					Default::default(),
				Error::<T, I>::InvalidBlockHeight
			);
			CurrentChainState::<T, I>::try_mutate::<_, Error<T, I>, _>(|previous_chain_state| {
				ensure!(
					new_chain_state.block_height >
						previous_chain_state.as_ref().expect(NO_CHAIN_STATE).block_height,
					Error::<T, I>::StaleDataSubmitted
				);
				*previous_chain_state = Some(new_chain_state.clone());

				Ok(())
			})?;
			Self::deposit_event(Event::<T, I>::ChainStateUpdated { new_chain_state });

			Ok(().into())
		}

		/// Update the fee multiplier with the provided value
		///
		/// Requires Governance.
		#[pallet::call_index(1)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn update_fee_multiplier(
			origin: OriginFor<T>,
			new_fee_multiplier: FixedU128,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			FeeMultiplier::<T, I>::put(new_fee_multiplier);
			Self::deposit_event(Event::<T, I>::FeeMultiplierUpdated { new_fee_multiplier });

			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> GetBlockHeight<T::TargetChain> for Pallet<T, I> {
	fn get_block_height() -> <T::TargetChain as Chain>::ChainBlockNumber {
		CurrentChainState::<T, I>::get().expect(NO_CHAIN_STATE).block_height
	}
}

impl<T: Config<I>, I: 'static> AdjustedFeeEstimationApi<T::TargetChain> for Pallet<T, I> {
	fn estimate_ingress_fee(
		asset: <T::TargetChain as Chain>::ChainAsset,
	) -> <T::TargetChain as Chain>::ChainAmount {
		FeeMultiplier::<T, I>::get().saturating_mul_int(
			CurrentChainState::<T, I>::get()
				.expect(NO_CHAIN_STATE)
				.tracked_data
				.estimate_ingress_fee(asset),
		)
	}

	fn estimate_egress_fee(
		asset: <T::TargetChain as Chain>::ChainAsset,
	) -> <T::TargetChain as Chain>::ChainAmount {
		FeeMultiplier::<T, I>::get().saturating_mul_int(
			CurrentChainState::<T, I>::get()
				.expect(NO_CHAIN_STATE)
				.tracked_data
				.estimate_egress_fee(asset),
		)
	}
}
