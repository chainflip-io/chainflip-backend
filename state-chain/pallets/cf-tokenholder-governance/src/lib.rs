#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
	dispatch::{GetDispatchInfo, UnfilteredDispatchable, Weight},
	traits::{EnsureOrigin, UnixTime},
};
pub use pallet::*;
use sp_runtime::DispatchError;
use sp_std::{boxed::Box, ops::Add, vec, vec::Vec};

/// Implements the functionality of the Chainflip governance.
#[frame_support::pallet]
pub mod pallet {
	use cf_traits::{Chainflip, ExecutionCondition, RuntimeUpgrade};
	use frame_support::{
		dispatch::GetDispatchInfo,
		error::BadOrigin,
		pallet_prelude::*,
		traits::{UnfilteredDispatchable, UnixTime},
	};

	use codec::Encode;
	use frame_system::{pallet, pallet_prelude::*};
	use sp_std::{boxed::Box, vec::Vec};

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}
}
