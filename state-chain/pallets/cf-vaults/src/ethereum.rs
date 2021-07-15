#![cfg_attr(not(feature = "std"), no_std)]
use frame_support::pallet_prelude::*;
pub use pallet::*;
use crate::rotation::*;
use crate::rotation::ChainParams::Ethereum;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + ChainFlip {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		type Vaults: ConstructionManager<RequestIndex>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		Nothing(),
	}

	#[pallet::error]
	pub enum Error<T> {
		Invalid,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub(super) fn call_me(
			origin: OriginFor<T>
		) -> DispatchResultWithPostInfo {
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
		}
	}
}

impl<T: Config> Construct<RequestIndex, T::ValidatorId> for Pallet<T> {

	type Manager = T::Vaults;

	fn start_construction_phase(index: RequestIndex, response: KeygenResponse<T::ValidatorId>) {
		// We would complete the construction and then notify the completion
		Self::Manager::on_completion(index, Ok(
			ValidatorRotationRequest::new(Ethereum(vec![]))
		));
	}
}