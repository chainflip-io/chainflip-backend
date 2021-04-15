#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://substrate.dev/docs/en/knowledgebase/runtime/frame>

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use codec::FullCodec;
    use frame_support::{dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo}, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The overarching call type.
		type Call: Member + FullCodec
			+ Dispatchable<Origin=Self::Origin, PostInfo=PostDispatchInfo>
			+ GetDispatchInfo 
			+ From<frame_system::Call<Self>>;

		type Validator: ProvideValidators<Self>;
	}

	/// Just a bunch of bytes, but they should decode to a valid `Call`.
	type OpaqueCall = Vec<u8>;

	/// A hash to index the call by.
	type CallHash = [u8; 20];

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn something)]
	pub type Calls<T> = StorageMap<
		_, 
		Identity,
		CallHash, 
		OpaqueCall, 
		OptionQuery>;

	// Pallets use events to inform users when important changes are made.
	// https://substrate.dev/docs/en/knowledgebase/runtime/events
	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Some external event has been witnessed [who, what]
		WitnessReceived(T::AccountId, OpaqueCall),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Error names should be descriptive.
		NoneValue,
		/// Errors should have helpful documentation associated with them.
		StorageOverflow,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		// TODO: 
		//   - think about using a hook to apply voted-on extrinsics.
		//   - check the era and maybe update the validator set: store validator set as an IndexSet
		//			(see: https://substrate.dev/rustdocs/v3.0.0/indexmap/set/struct.IndexSet.html)
		// 			This way, the set of approvals can be stored in a BitVec which should be the quite memory-efficient.
		
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Called as a witness of some external event. The call parameter is the resultant extrinsic. This can be 
		/// thought of as a vote for the encoded [`Call`](crate::Pallet::Call) value. 
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn witness(
			origin: OriginFor<T>, 
			event: <T as Config>::Call) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// TODO:
			// - look up the signer in the list of validators
			// - register the vote in storage
			// - Maybe (?) check if threshold is reached and apply the voted-on Call.

			// Return a successful DispatchResultWithPostInfo
			Ok(().into())
		}
	}
}

pub trait ProvideValidators<T: Config> {
	type Era;

	fn validators(&self) -> Vec<<T as frame_system::Config>::AccountId>;

	fn current_era(&self) -> Self::Era;
}