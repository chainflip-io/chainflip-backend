#![cfg_attr(not(feature = "std"), no_std)]
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use codec::FullCodec;
use frame_support::pallet_prelude::Member;
use sp_runtime::traits::AtLeast32BitUnsigned;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use bitvec::prelude::*;
    use frame_support::{dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo}, pallet_prelude::*};
	use frame_system::pallet_prelude::*;
    use sp_core::blake2_256;

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

		// TODO: Investigate frame_support::ValidatorSet incase this already provides the required functionality.
		type ValidatorProvider: ValidatorProvider<Self>;

		type ValidatorId: Member + FullCodec + From<<Self as frame_system::Config>::AccountId>;
	}

	/// Alias for the `Epoch` type defined by the `ValidatorProvider`.
	type Epoch<T> = <<T as Config>::ValidatorProvider as ValidatorProvider<T>>::Epoch;

	/// Just a bunch of bytes, but they should decode to a valid `Call`.
	type OpaqueCall = Vec<u8>;

	/// A hash to index the call by.
	type CallHash = [u8; 32];

	/// Convenience alias for a collection of bits representing the votes of each validator.
	type VoteMask = BitSlice<Msb0, u8>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type Calls<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		Epoch<T>,
		Blake2_128Concat,
		<T as Config>::Call, 
		Vec<u8>
	>;

	#[pallet::storage]
	pub(super) type ValidatorIndex<T: Config> = StorageDoubleMap<
		_, 
		Blake2_128Concat,
		Epoch<T>,
		Identity,
		<T as frame_system::Config>::AccountId,
		u64
	>;

	// QUESTION: Should this be managed here or in the sessions pallet, for example? (The *active* validator set and 
	// therefore the threshold might change due to unavailable nodes, slashing etc.)
	#[pallet::storage]
	pub(super) type ConsensusThreshold<T> = StorageValue<_, u32, ValueQuery>;
	
	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new call has been registered for voting [call, call_sig]
		NewEvent(OpaqueCall, CallHash),

		/// Some external event has been witnessed [call_sig, who]
		WitnessReceived(CallHash, <T as Config>::ValidatorId),

		/// The witness threshold has been reached [call_sig, num_votes]
		ThresholdReached(CallHash),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// CRITICAL: The validator index is out of bounds. This should never happen. 
		ValidatorIndexOutOfBounds,
		/// Witness is not a validator.
		UnauthorizedWitness,
		/// A witness vote was cast twice by the same validator.
		DuplicateWitness
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		// TODO: 
		//   - think about using a hook to apply voted-on extrinsics in batches instead of inline in the witness fn.
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
			call: <T as Config>::Call) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			let epoch: Epoch<T> = T::ValidatorProvider::current_epoch();
			let num_validators = T::ValidatorProvider::num_validators() as usize;
			
			// Look up the signer in the list of validators
			let index = ValidatorIndex::<T>::get(&epoch, &who)
				.ok_or(Error::<T>::UnauthorizedWitness)? as usize;
			
			// Register the vote
			let num_votes = Calls::<T>::try_mutate::<_, _, _, Error::<T>, _>(&epoch, &call, |buffer| {
				// If there is no storage item, create an empty one.
				if buffer.is_none() {
					let empty_mask = bits![Msb0, u8; 0].repeat(num_validators);
					*buffer = Some(empty_mask.into_vec())
				}

				let bytes = buffer.as_mut().expect("Checked for none condition above, this will never panic;");

				// Convert to an addressable bitmask
				let bits = VoteMask::from_slice_mut(bytes)
					.expect("Only panics if the slice size exceeds the max; The number of validators should never exceed this;");

				// Return an error if already voted, otherwise set the indexed bit to `true` to indicate a vote.
				if bits[index] {
					Err(Error::<T>::DuplicateWitness)?
				} else {
					let mut vote = bits.get_mut(index).ok_or(Error::<T>::ValidatorIndexOutOfBounds)?;
					*vote = true;
				}

				Ok(bits.count_ones())
			})?;

			let call_hash = call.using_encoded(|bytes| blake2_256(bytes));
			let threshold = ConsensusThreshold::<T>::get() as usize;

			Self::deposit_event(Event::<T>::WitnessReceived(call_hash, who.into()));

			// Check if threshold is reached and, if so, apply the voted-on Call.
			if num_votes == threshold {
				Self::deposit_event(Event::<T>::ThresholdReached(call_hash));
			}

			// QUESTION: Do we want to allow voting to continue *after* the call has been made? Might be useful for 
			// slashing?

			Ok(().into())
		}
	}
}

pub trait ValidatorProvider<T: Config> {
	type Epoch: Member + FullCodec + AtLeast32BitUnsigned;
	type Validatorid: From<<T as frame_system::Config>::AccountId>;

	fn num_validators() -> u32;

	fn validators() -> Vec<Self::Validatorid>;

	fn current_epoch() -> Self::Epoch;
}
