#![cfg_attr(not(feature = "std"), no_std)]

//! A pallet that abstracts the notion of witnessing an external event.
//!
//! Based loosely on parity's own [`pallet_multisig`](https://github.com/paritytech/substrate/tree/master/frame/multisig).
//!
//! ## Usage
//!
//! ### Witnessing a an event.
//!
//! Witnessing can be thought of as voting on an action (represented by a `call`) triggered by some external event. It
//! is a two-step process:
//!
//! 1. Submit a [`register`](pallet::Pallet::register) extrinsic as an *unsigned* transaction.
//! 2. Submit a [`witness`](pallet::Pallet::witness) extrinsic as a *signed* transaction.
//!
//! The first step registers the actual `call` to be voted on, and the latter references the `call` via its `blake2_256`
//! hash. When a configured threshold is reached, the previously-stored `call` is dispatched.
//!
//! Note that calls *must* have a unique hash so that the votes don't clash.
//!
//! ### Restricting target calls
//!
//! This crate also provides [`EnsureWitnessed`](EnsureWitnessed), an implementation of [`EnsureOrigin`](EnsureOrigin)
//! that can be used to restrict an extrinsic so that it can only be dispatched via witness consensus.
//!
//! Note again that each call that is voted on should have a unique hash, and therefore the call arguments should have
//! some form of entropy to ensure that each the call is idempotent.
//!
//! See the README for instructions on how to integrate this pallet with the runtime.
//!

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use bitvec::prelude::*;
use cf_traits::EpochInfo;
use codec::{Decode, Encode, FullCodec};
use frame_support::{
	dispatch::{DispatchResultWithPostInfo, Dispatchable},
	ensure,
	pallet_prelude::Member,
	traits::EnsureOrigin,
	Hashable,
};
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		dispatch::{DispatchResult, Dispatchable, GetDispatchInfo, PostDispatchInfo},
		pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The outer Origin needs to be compatible with this pallet's Origin
		type Origin: From<RawOrigin>;

		/// The overarching call type.
		type Call: Member
			+ FullCodec
			+ Dispatchable<Origin = <Self as Config>::Origin, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo
			+ From<frame_system::Call<Self>>;

		type Epoch: Member + Copy + FullCodec + AtLeast32BitUnsigned + Default;

		type ValidatorId: Member
			+ FullCodec
			+ From<<Self as frame_system::Config>::AccountId>
			+ Into<<Self as frame_system::Config>::AccountId>;

		type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId, EpochIndex = Self::Epoch>;
	}

	/// Alias for the `Epoch` configuration type.
	pub(super) type Epoch<T> = <T as Config>::Epoch;

	/// A hash to index the call by.
	pub(super) type CallHash = [u8; 32];

	/// Convenience alias for a collection of bits representing the votes of each validator.
	pub(super) type VoteMask = BitSlice<Msb0, u8>;

	/// The type used for tallying votes.
	pub(super) type VoteCount = u32;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// `Calls` contains the call to be dispatched.
	#[pallet::storage]
	pub type Calls<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		Epoch<T>,
		Identity,
		CallHash,
		<T as Config>::Call,
		OptionQuery,
	>;

	/// `Votes` is a tally of votes for each registered call.
	#[pallet::storage]
	pub type Votes<T: Config> =
		StorageDoubleMap<_, Twox64Concat, Epoch<T>, Identity, CallHash, Vec<u8>>;

	/// Defines a unique index for each validator for every epoch.
	#[pallet::storage]
	pub(super) type ValidatorIndex<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		Epoch<T>,
		Blake2_128Concat,
		<T as frame_system::Config>::AccountId,
		u16,
	>;

	/// The current threshold for reaching consensus.
	/// TODO: This param should probably be managed in the sessions pallet. (The *active* validator set and
	/// therefore the threshold might change due to unavailable nodes, slashing etc.)
	#[pallet::storage]
	pub(super) type ConsensusThreshold<T> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub(super) type NumValidators<T> = StorageValue<_, u32, ValueQuery>;

	/// The current epoch index.
	#[pallet::storage]
	pub(super) type CurrentEpoch<T: Config> = StorageValue<_, Epoch<T>, ValueQuery>;

	/// No hooks are implemented for this pallet.
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Some external event has been witnessed [call_sig, who, num_votes]
		WitnessReceived(CallHash, <T as Config>::ValidatorId, VoteCount),

		/// The witness threshold has been reached [call_sig, num_votes]
		ThresholdReached(CallHash, VoteCount),

		/// A witness call has been executed [call_sig, result].
		WitnessExecuted(CallHash, DispatchResult),

		/// Some external event has been witnessed [call_sig, who, num_votes]
		CallRegistered(CallHash),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// CRITICAL: The validator index is out of bounds. This should never happen.
		ValidatorIndexOutOfBounds,

		/// Witness is not a validator.
		UnauthorizedWitness,

		/// A witness vote was cast twice by the same validator.
		DuplicateWitness,

		/// An attempt has been made to register a vote for an already-registered call.
		DuplicateRegistration,

		/// A an attempt has been made to dispatch or vote for a call that has not been registered.
		UnregisteredCall,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Vote for the hash of a call. The corresponding call must be registered prior to being witnessed.
		///
		/// If the witness threshold is passed, dispatches the call.
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn witness(origin: OriginFor<T>, call_hash: CallHash) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			Self::do_witness(who, call_hash)
		}

		/// Called to register an dispatchable `call` to vote on. The `call` will be dispatched when the configured
		/// voting threshold is reached.
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn register(
			origin: OriginFor<T>,
			call: Box<<T as Config>::Call>,
		) -> DispatchResultWithPostInfo {
			let _ = ensure_none(origin)?;

			let _call_hash = Self::try_register(*call)?;

			Ok(().into())
		}
	}

	/// Witness pallet origin
	#[pallet::origin]
	pub type Origin = RawOrigin;

	/// The raw origin enum for this pallet.
	#[derive(PartialEq, Eq, Clone, RuntimeDebug, Encode, Decode)]
	pub enum RawOrigin {
		WitnessThreshold,
	}
}

impl<T: Config> Pallet<T> {
	/// Do the actual witnessing.
	///
	/// Think of this a vote for some action (represented by a `call_hash` that maps to a runtime `call`) to be taken.
	/// The action must be registered before it can be voted on (see [register](Call::register)).
	///
	/// At a high level:
	///
	/// 1. Look up the account id in the list of validators.
	/// 2. Get the list of votes for the call, or an empty list if this is the first vote.
	/// 3. Add the account's vote to the list.
	/// 4. Check the number of votes against the reuquired threshold.
	/// 5. If the threshold is exceeded, execute the voted-on `call`.
	///
	/// This implementation uses a bitmask whereby each index to the bitmask represents a validator account ID in the
	/// current Epoch.
	///
	/// **Note:**
	/// This implementation currently allows voting to continue even after the vote threshold is reached.
	///
	fn do_witness(
		who: <T as frame_system::Config>::AccountId,
		call_hash: CallHash,
	) -> DispatchResultWithPostInfo {
		let epoch: Epoch<T> = CurrentEpoch::<T>::get();
		let num_validators = NumValidators::<T>::get() as usize;

		// Make sure the call has been registered.
		ensure!(
			Calls::<T>::contains_key(&epoch, &call_hash),
			Error::<T>::UnregisteredCall
		);

		// Look up the signer in the list of validators
		let index =
			ValidatorIndex::<T>::get(&epoch, &who).ok_or(Error::<T>::UnauthorizedWitness)? as usize;

		// Register the vote
		let num_votes = Votes::<T>::try_mutate::<_, _, _, Error<T>, _>(
			&epoch,
			&call_hash,
			|buffer| {
				// If there is no storage item, create an empty one.
				if buffer.is_none() {
					let empty_mask = BitVec::<Msb0, u8>::repeat(false, num_validators);
					*buffer = Some(empty_mask.into_vec())
				}

				let bytes = buffer
					.as_mut()
					.expect("Checked for none condition above, this will never panic;");

				// Convert to an addressable bitmask
				let bits = VoteMask::from_slice_mut(bytes)
				.expect("Only panics if the slice size exceeds the max; The number of validators should never exceed this;");

				// Return an error if already voted, otherwise set the indexed bit to `true` to indicate a vote.
				if bits[index] {
					Err(Error::<T>::DuplicateWitness)?
				} else {
					let mut vote = bits
						.get_mut(index)
						.ok_or(Error::<T>::ValidatorIndexOutOfBounds)?;
					*vote = true;
				}

				Ok(bits.count_ones())
			},
		)?;

		Self::deposit_event(Event::<T>::WitnessReceived(
			call_hash,
			who.into(),
			num_votes as VoteCount,
		));

		// Check if threshold is reached and, if so, apply the voted-on Call.
		let threshold = ConsensusThreshold::<T>::get() as usize;

		let post_dispatch_info = if num_votes == threshold {
			Self::deposit_event(Event::<T>::ThresholdReached(
				call_hash,
				num_votes as VoteCount,
			));
			Self::maybe_dispatch_call(&call_hash)?
		} else {
			().into()
		};

		Ok(post_dispatch_info)
	}

	/// Registers a call for future dispatch.
	///
	/// If the call has already been registered, returns a `DuplicateRegistration` error.
	fn try_register(call: <T as Config>::Call) -> Result<CallHash, Error<T>> {
		let epoch: Epoch<T> = CurrentEpoch::<T>::get();
		let call_hash = Self::call_hash(&call);

		Calls::<T>::try_mutate(&epoch, &call_hash, |existing_call| {
			*existing_call = match existing_call {
				Some(_) => Err(Error::<T>::DuplicateRegistration),
				None => {
					Self::deposit_event(Event::CallRegistered(call_hash));
					Ok(Some(call))
				}
			}?;
			Ok(())
		})?;

		Ok(call_hash)
	}

	/// Registers a call for future dispatch.
	///
	/// Doesn't care if the call has already been registered.
	fn do_register(call: <T as Config>::Call) -> CallHash {
		Self::try_register(call.clone()).unwrap_or_else(|_| Self::call_hash(&call))
	}

	/// Dispatches a stored call.
	///
	/// If no call has been stored against the provided hash, returns an `UnregisteredCall` error.
	///
	/// Note the dispatch is made from this pallet's `WitnessThreshold` origin.
	fn maybe_dispatch_call(call_hash: &CallHash) -> DispatchResultWithPostInfo {
		let epoch = CurrentEpoch::<T>::get();

		let call = Calls::<T>::get(epoch, call_hash).ok_or(Error::<T>::UnregisteredCall)?;

		let dispatch_result = call.dispatch((RawOrigin::WitnessThreshold).into());
		Self::deposit_event(Event::<T>::WitnessExecuted(
			call_hash.clone(),
			dispatch_result.map(|_| ()).map_err(|e| e.error),
		));

		let post_dispatch_info = dispatch_result.unwrap_or_else(|err| err.post_info);

		Ok(post_dispatch_info)
	}

	/// Computes the hash of a call.
	pub fn call_hash(call: &<T as Config>::Call) -> CallHash {
		Hashable::blake2_256(call)
	}
}

impl<T: pallet::Config> cf_traits::Witnesser for Pallet<T> {
	type AccountId = T::ValidatorId;
	type Call = <T as pallet::Config>::Call;

	fn witness(who: Self::AccountId, call: Self::Call) -> DispatchResultWithPostInfo {
		// Ignore the duplicate registration attempt for internal calls.
		let call_hash = Self::do_register(call);
		Self::do_witness(who.into(), call_hash)?;
		Ok(().into())
	}
}

/// Simple struct on which to implement EnsureOrigin for our pallet's custom origin type.
///
/// # Example:
///
/// ```ignore
/// if let Ok(()) = EnsureWitnessed::ensure_origin(origin) {
/// 	log::debug!("This extrinsic was called as a result of witness threshold consensus.");
/// }
/// ```
pub struct EnsureWitnessed;

impl<OuterOrigin> EnsureOrigin<OuterOrigin> for EnsureWitnessed
where
	OuterOrigin: Into<Result<RawOrigin, OuterOrigin>> + From<RawOrigin>,
{
	type Success = ();

	fn try_origin(o: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
		match o.into() {
			Ok(o) => match o {
				RawOrigin::WitnessThreshold => Ok(()),
			},
			Err(o) => Err(o),
		}
	}
}

impl<T: Config> pallet_cf_validator::EpochTransitionHandler for Pallet<T> {
	type ValidatorId = T::ValidatorId;

	fn on_new_epoch(new_validators: Vec<Self::ValidatorId>) {
		let epoch = T::EpochInfo::current_epoch();

		CurrentEpoch::<T>::set(epoch);
		for (i, v) in new_validators.iter().enumerate() {
			ValidatorIndex::<T>::insert(&epoch, (*v).clone().into(), i as u16)
		}
	}
}
