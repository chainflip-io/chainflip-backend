use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
	}

	#[pallet::error]
	pub enum Error<T> {
		AnError,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AnEvent {},
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub fn an_extrinsic(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			Ok(().into())
		}
	}
}
