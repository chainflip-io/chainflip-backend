#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

use bitvec::prelude::*;
use codec::FullCodec;
use frame_support::{
	dispatch::{DispatchResultWithPostInfo, Dispatchable},
	pallet_prelude::Member,
	traits::EnsureOrigin,
	Hashable,
};
use pallet_session::SessionHandler;
use sp_runtime::traits::{AtLeast32BitUnsigned, Zero};
use sp_std::{ops::AddAssign, prelude::*};

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

		type Epoch: Member + FullCodec + AtLeast32BitUnsigned + Default;

		type ValidatorId: Member
			+ FullCodec
			+ From<<Self as frame_system::Config>::AccountId>
			+ Into<<Self as frame_system::Config>::AccountId>;
	}

	/// Alias for the `Epoch` type defined by the `ValidatorProvider`.
	pub(super) type Epoch<T> = <T as Config>::Epoch;

	/// A hash to index the call by.
	pub(super) type CallHash = [u8; 32];

	/// Convenience alias for a collection of bits representing the votes of each validator.
	pub(super) type VoteMask = BitSlice<Msb0, u8>;

	/// The type used for tallying votes.
	pub(super) type VoteCount = u32;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type Calls<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, Epoch<T>, Identity, CallHash, Vec<u8>>;

	#[pallet::storage]
	pub(super) type ValidatorIndex<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		Epoch<T>,
		Identity,
		<T as frame_system::Config>::AccountId,
		u16,
	>;

	// TODO: This param should probably be managed in the sessions pallet. (The *active* validator set and
	// therefore the threshold might change due to unavailable nodes, slashing etc.)
	#[pallet::storage]
	pub(super) type ConsensusThreshold<T> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub(super) type NumValidators<T> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub(super) type CurrentEpoch<T: Config> = StorageValue<_, Epoch<T>, ValueQuery>;

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
		/// Called as a witness of some external event. The call parameter is the resultant extrinsic. This can be
		/// thought of as a vote for the encoded [`Call`](crate::Pallet::Call) value.
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
		let epoch: Epoch<T> = CurrentEpoch::<T>::get();
		let num_validators = NumValidators::<T>::get() as usize;

		// Look up the signer in the list of validators
		let index =
			ValidatorIndex::<T>::get(&epoch, &who).ok_or(Error::<T>::UnauthorizedWitness)? as usize;

		// Register the vote
		let call_hash = Hashable::blake2_256(&call);
		let num_votes = Calls::<T>::try_mutate::<_, _, _, Error<T>, _>(
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

		let threshold = ConsensusThreshold::<T>::get() as usize;

		Self::deposit_event(Event::<T>::WitnessReceived(
			call_hash,
			who.into(),
			num_votes as VoteCount,
		));

		// Check if threshold is reached and, if so, apply the voted-on Call.
		if num_votes == threshold {
			Self::deposit_event(Event::<T>::ThresholdReached(
				call_hash,
				num_votes as VoteCount,
			));
			let result = call.dispatch((RawOrigin::WitnessThreshold).into());
			Self::deposit_event(Event::<T>::WitnessExecuted(
				call_hash,
				result.map(|_| ()).map_err(|e| e.error),
			));
		}

		Ok(().into())
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
}

/// Implementation of [SessionHandler](pallet_session::SessionHandler) to update
/// the current list of validators and the current epoch.
impl<T, ValidatorId> SessionHandler<ValidatorId> for Pallet<T>
where
	T: Config,
	ValidatorId: Clone + Into<<T as frame_system::Config>::AccountId>,
{
	const KEY_TYPE_IDS: &'static [sp_runtime::KeyTypeId] = &[];

	fn on_genesis_session<Ks: sp_runtime::traits::OpaqueKeys>(validators: &[(ValidatorId, Ks)]) {
		for (i, (v, _k)) in validators.iter().enumerate() {
			ValidatorIndex::<T>::insert(<T as Config>::Epoch::zero(), (*v).clone().into(), i as u16)
		}
	}

	fn on_new_session<Ks: sp_runtime::traits::OpaqueKeys>(
		_changed: bool,
		_validators: &[(ValidatorId, Ks)],
		queued_validators: &[(ValidatorId, Ks)],
	) {
		CurrentEpoch::<T>::mutate(|e| e.add_assign(1u32.into()));

		for (i, (v, _k)) in queued_validators.iter().enumerate() {
			ValidatorIndex::<T>::insert(<T as Config>::Epoch::zero(), (*v).clone().into(), i as u16)
		}
	}

	fn on_disabled(_validator_index: usize) {
		// Reduce threshold?
		todo!()
	}
}
