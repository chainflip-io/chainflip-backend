#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[frame_support::pallet]
pub mod pallet {
	use core::marker::PhantomData;

	use frame_support::{pallet_prelude::DispatchResultWithPostInfo, traits::IsType};
	use frame_system::{ensure_signed, pallet_prelude::OriginFor};

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: cf_traits::Chainflip {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		IngressConfirmed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
		pub fn emit_ingress_confirmed(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			Self::deposit_event(Event::IngressConfirmed);
			Ok(().into())
		}
	}
}
