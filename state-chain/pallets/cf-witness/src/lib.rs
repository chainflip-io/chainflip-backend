#![cfg_attr(not(feature = "std"), no_std)]

//! A pallet that abstracts the notion of witnessing an external event.
//!
//! Based loosely on parity's own [`pallet_multisig`](https://github.com/paritytech/substrate/tree/master/frame/multisig).
//!
//! ## Usage
//!
//! ### Witnessing a an event.
//!
//! Witnessing can be thought of as voting on an action (represented by a `call`) triggered by some external event.
//!
//! Witnessing happens via the signed [`witness`](pallet::Pallet::witness) extrinsic including the encoded `call` to be
//! dispatched.
//!
//! If the encoded call is not already stored, it is stored against its hash. A vote is then counted on behalf of the
//! signing validator account. When a configured threshold is reached, the previously-stored `call` is dispatched.
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
use codec::FullCodec;
use frame_support::{
	dispatch::{
		DispatchResult, DispatchResultWithPostInfo, GetDispatchInfo, UnfilteredDispatchable,
	},
	pallet_prelude::Member,
	traits::EnsureOrigin,
	Hashable,
};
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
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
			+ UnfilteredDispatchable<Origin = <Self as Config>::Origin>
			+ GetDispatchInfo;

		type Epoch: Member + FullCodec + Copy + AtLeast32BitUnsigned + Default;

		type ValidatorId: Member
			+ FullCodec
			+ From<<Self as frame_system::Config>::AccountId>
			+ Into<<Self as frame_system::Config>::AccountId>;

		type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId, EpochIndex = Self::Epoch>;

		type Amount: Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;
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

	/// A lookup mapping (epoch, call_hash) to a bitmask representing the votes for each validator.
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

	/// The number of active validators.
	#[pallet::storage]
	pub(super) type NumValidators<T> = StorageValue<_, u32, ValueQuery>;

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
	}

	#[pallet::error]
	pub enum Error<T> {
		/// CRITICAL: The validator index is out of bounds. This should never happen.
		ValidatorIndexOutOfBounds,

		/// Witness is not a validator.
		UnauthorizedWitness,

		/// A witness vote was cast twice by the same validator.
		DuplicateWitness,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Called as a witness of some external event.
		///
		/// The provided `call` will be dispatched when the configured threshold number of validtors have submitted an
		/// identical transaction. This can be thought of as a vote for the encoded [`Call`](crate::Pallet::Call) value.
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn witness(
			origin: OriginFor<T>,
			call: Box<<T as Config>::Call>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			Self::do_witness(who, *call)
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
	/// Think of this a vote for some action (represented by a runtime `call`) to be taken. At a high level:
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
		call: <T as Config>::Call,
	) -> DispatchResultWithPostInfo {
		let epoch: Epoch<T> = T::EpochInfo::epoch_index();
		let num_validators = NumValidators::<T>::get() as usize;

		// Look up the signer in the list of validators
		let index =
			ValidatorIndex::<T>::get(&epoch, &who).ok_or(Error::<T>::UnauthorizedWitness)? as usize;

		// Register the vote
		let call_hash = Hashable::blake2_256(&call);
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

				let mut vote_count = bits.count_ones();

				// Get a reference to the existing vote.
				let mut vote = bits
					.get_mut(index)
					.ok_or(Error::<T>::ValidatorIndexOutOfBounds)?;

				// Return an error if already voted, otherwise set the indexed bit to `true` to indicate a vote.
				if *vote {
					Err(Error::<T>::DuplicateWitness)?
				}

				vote_count += 1;
				*vote = true;

				Ok(vote_count)
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
			let result = call.dispatch_bypass_filter((RawOrigin::WitnessThreshold).into());
			Self::deposit_event(Event::<T>::WitnessExecuted(
				call_hash,
				result.map(|_| ()).map_err(|e| e.error),
			));
			result.unwrap_or_else(|err| err.post_info)
		} else {
			().into()
		};

		Ok(post_dispatch_info)
	}
}

impl<T: pallet::Config> cf_traits::Witnesser for Pallet<T> {
	type AccountId = T::ValidatorId;
	type Call = <T as pallet::Config>::Call;

	fn witness(who: Self::AccountId, call: Self::Call) -> DispatchResultWithPostInfo {
		Self::do_witness(who.into(), call)
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

	#[cfg(feature = "runtime-benchmarks")]
	fn successful_origin() -> OuterOrigin {
		RawOrigin::WitnessThreshold.into()
	}
}

impl<T: Config> pallet_cf_validator::EpochTransitionHandler for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;

	fn on_new_epoch(new_validators: &Vec<Self::ValidatorId>, _new_bond: Self::Amount) {
		let epoch = T::EpochInfo::epoch_index();

		let mut total = 0;
		for (i, v) in new_validators.iter().enumerate() {
			ValidatorIndex::<T>::insert(&epoch, (*v).clone().into(), i as u16);
			total += 1;
		}
		NumValidators::<T>::set(total);

		let calc_threshold = |total: u32| -> u32 {
			let doubled = total * 2;
			if doubled % 3 == 0 {
				doubled / 3
			} else {
				doubled / 3 + 1
			}
		};

		// Assume all validators are live at the start of an Epoch.
		ConsensusThreshold::<T>::mutate(|thresh| *thresh = calc_threshold(total))
	}
}
