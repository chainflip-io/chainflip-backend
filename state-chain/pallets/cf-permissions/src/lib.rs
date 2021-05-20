#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
use sp_runtime::traits::{Convert, OpaqueKeys, AtLeast32BitUnsigned};
use sp_std::prelude::*;
use frame_support::sp_runtime::traits::{Saturating, Zero};
use log::{debug};
use frame_support::pallet_prelude::*;
use serde::{Serialize, Deserialize};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;
	use frame_support::sp_runtime::SaturatedConversion;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		AnEvent(),
	}

	#[pallet::error]
	pub enum Error<T> {
		AnError,
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {

		#[pallet::weight(10_000)]
		pub(super) fn something(
			origin: OriginFor<T>
		) -> DispatchResultWithPostInfo {
			Ok(().into())
		}

	}

	#[pallet::storage]
	#[pallet::getter(fn value)]
	pub(super) type Value<T: Config> = StorageValue<_, bool, ValueQuery>;

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
		fn build(&self) {}
	}
}

impl<T: Config> Pallet<T> {
}
