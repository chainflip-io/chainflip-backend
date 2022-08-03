#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod tests;

use bitvec::prelude::*;
use cf_traits::{EpochIndex, EpochInfo};
use codec::FullCodec;
use frame_support::{
	dispatch::{DispatchResultWithPostInfo, GetDispatchInfo, UnfilteredDispatchable},
	ensure,
	pallet_prelude::Member,
	traits::EnsureOrigin,
	Hashable,
};
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::prelude::*;
use utilities::success_threshold_from_share_count;

pub trait WitnessDataExtraction {
	/// Extracts some data from a call and encodes it so it can be stored for later.
	fn extract(&mut self) -> Option<Vec<u8>>;
	/// Takes all of the previously extracted data, combines it, and injects it back into the call.
	///
	/// The combination method should be resistant to minority attacks / outliers. For example,
	/// medians are resistant to outliers, but means are not.
	fn combine_and_inject(&mut self, data: &mut [Vec<u8>]);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::EpochIndex;
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
			+ Parameter
			+ From<frame_system::Call<Self>>
			+ UnfilteredDispatchable<Origin = <Self as Config>::Origin>
			+ GetDispatchInfo
			+ WitnessDataExtraction;

		type ValidatorId: Member
			+ FullCodec
			+ From<<Self as frame_system::Config>::AccountId>
			+ Into<<Self as frame_system::Config>::AccountId>;

		type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId>;

		type Amount: Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	/// A hash to index the call by.
	#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub struct CallHash(pub [u8; 32]);
	impl sp_std::fmt::Debug for CallHash {
		fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
			write!(f, "0x{}", hex::encode(self.0))
		}
	}

	/// Convenience alias for a collection of bits representing the votes of each authority.
	pub(super) type VoteMask = BitSlice<Msb0, u8>;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// A lookup mapping (epoch, call_hash) to a bitmask representing the votes for each authority.
	#[pallet::storage]
	pub type Votes<T: Config> =
		StorageDoubleMap<_, Twox64Concat, EpochIndex, Identity, CallHash, Vec<u8>>;

	/// Stores extra call data for later recomposition.
	#[pallet::storage]
	pub type ExtraCallData<T: Config> =
		StorageDoubleMap<_, Twox64Concat, EpochIndex, Identity, CallHash, Vec<Vec<u8>>>;

	/// A flag indicating that the CallHash has been executed.
	#[pallet::storage]
	pub type CallHashExecuted<T: Config> = StorageMap<_, Identity, CallHash, ()>;

	/// No hooks are implemented for this pallet.
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A witness call has failed.
		WitnessExecutionFailed { call_hash: CallHash, error: DispatchError },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// CRITICAL: The authority index is out of bounds. This should never happen.
		AuthorityIndexOutOfBounds,

		/// Witness is not an authority.
		UnauthorisedWitness,

		/// A witness vote was cast twice by the same authority.
		DuplicateWitness,

		/// The epoch has expired
		EpochExpired,

		/// Invalid epoch
		InvalidEpoch,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Called as a witness of some external event.
		///
		/// The provided `call` will be dispatched when the configured threshold number of validtors
		/// have submitted an identical transaction. This can be thought of as a vote for the
		/// encoded [Call](Config::Call) value.
		///
		/// ## Events
		///
		/// - [WitnessExecutionFailed](Event::WitnessExecutionFailed)
		///
		/// ## Errors
		///
		/// - [UnauthorisedWitness](Error::UnauthorisedWitness)
		/// - [AuthorityIndexOutOfBounds](Error::AuthorityIndexOutOfBounds)
		/// - [DuplicateWitness](Error::DuplicateWitness)
		#[pallet::weight(
			T::WeightInfo::witness().saturating_add(call.get_dispatch_info().weight /
				T::EpochInfo::authority_count_at_epoch(T::EpochInfo::epoch_index()).unwrap_or(1u32) as u64)
		)]
		pub fn witness(
			origin: OriginFor<T>,
			// TODO: Not possible to fix the clippy warning here. At the moment we
			// need to ignore it on a global level.
			call: Box<<T as Config>::Call>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::do_witness(who, *call)
		}

		/// Called as a witness of some external event.
		///
		/// The provided `call` will be dispatched when the configured threshold number of
		/// authorities have submitted an identical transaction. This can be thought of as a vote
		/// for the encoded [Call](Config::Call) value.
		///
		/// ## Events
		///
		/// - [WitnessExecutionFailed](Event::WitnessExecutionFailed)
		///
		/// ## Errors
		///
		/// - [UnauthorisedWitness](Error::UnauthorisedWitness)
		/// - [AuthorityIndexOutOfBounds](Error::AuthorityIndexOutOfBounds)
		/// - [DuplicateWitness](Error::DuplicateWitness)
		#[pallet::weight(
			T::WeightInfo::witness().saturating_add(call.get_dispatch_info().weight /
				T::EpochInfo::authority_count_at_epoch(*epoch_index).unwrap_or(1u32) as u64)
		)]
		pub fn witness_at_epoch(
			origin: OriginFor<T>,
			call: Box<<T as Config>::Call>,
			epoch_index: EpochIndex,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::do_witness_at_epoch(who, *call, epoch_index)
		}
	}

	/// Witness pallet origin
	#[pallet::origin]
	pub type Origin = RawOrigin;

	/// The raw origin enum for this pallet.
	#[derive(PartialEq, Eq, Clone, RuntimeDebug, Encode, Decode, TypeInfo)]
	pub enum RawOrigin {
		HistoricalActiveEpochWitnessThreshold,
		CurrentEpochWitnessThreshold,
	}
}

impl<T: Config> Pallet<T> {
	/// Do the actual witnessing.
	///
	/// Think of this a vote for some action (represented by a runtime `call`) to be taken. At a
	/// high level:
	///
	/// 1. Ensure we are not submitting a witness for an expired epoch
	/// 2. Look up the account id in the list of authorities.
	/// 3. Get the list of votes for the epoch and call, or an empty list if this is the first vote.
	/// 4. Add the account's vote to the list.
	/// 5. Check the number of votes against the required threshold.
	/// 6. If the threshold is exceeded, execute the voted-on `call`.
	///
	/// This implementation uses a bitmask whereby each index to the bitmask represents an authority
	/// account ID in the current Epoch.
	///
	/// **Note:**
	/// This implementation currently allows voting to continue even after the vote threshold is
	/// reached.
	fn do_witness_at_epoch(
		who: <T as frame_system::Config>::AccountId,
		mut call: <T as Config>::Call,
		epoch_index: EpochIndex,
	) -> DispatchResultWithPostInfo {
		// Ensure the epoch has not yet expired
		ensure!(epoch_index > T::EpochInfo::last_expired_epoch(), Error::<T>::EpochExpired);

		// The number of authorities for the epoch
		// This value is updated alongside ValidatorIndex, so if we have a authority, we have an
		// authority count.
		let num_authorities =
			T::EpochInfo::authority_count_at_epoch(epoch_index).ok_or(Error::<T>::InvalidEpoch)?;

		let index = T::EpochInfo::authority_index(epoch_index, &who.into())
			.ok_or(Error::<T>::UnauthorisedWitness)? as usize;

		// Register the vote
		// `extract()` modifies the call, so we need to calculate the call hash *after* this.
		let extra_data = call.extract();
		let call_hash = CallHash(call.blake2_256());
		let num_votes = Votes::<T>::try_mutate::<_, _, _, Error<T>, _>(
			&epoch_index,
			&call_hash,
			|buffer| {
				// If there is no storage item, create an empty one.
				if buffer.is_none() {
					let empty_mask = BitVec::<Msb0, u8>::repeat(false, num_authorities as usize);
					*buffer = Some(empty_mask.into_vec())
				}

				let bytes = buffer
					.as_mut()
					.expect("Checked for none condition above, this will never panic;");

				// Convert to an addressable bitmask
				let bits = VoteMask::from_slice_mut(bytes)
				.expect("Only panics if the slice size exceeds the max; The number of authorities should never exceed this;");

				let mut vote_count = bits.count_ones();

				// Get a reference to the existing vote.
				let mut vote = bits.get_mut(index).ok_or(Error::<T>::AuthorityIndexOutOfBounds)?;

				// Return an error if already voted, otherwise set the indexed bit to `true` to
				// indicate a vote.
				if *vote {
					return Err(Error::<T>::DuplicateWitness)
				}

				vote_count += 1;
				*vote = true;

				if let Some(extra_data) = extra_data {
					ExtraCallData::<T>::append(epoch_index, &call_hash, extra_data);
				}

				Ok(vote_count)
			},
		)?;

		// Check if threshold is reached and, if so, apply the voted-on Call.
		if num_votes == success_threshold_from_share_count(num_authorities) as usize &&
			CallHashExecuted::<T>::get(&call_hash).is_none()
		{
			if let Some(mut extra_data) = ExtraCallData::<T>::get(epoch_index, &call_hash) {
				call.combine_and_inject(&mut extra_data)
			}
			let _result = call
				.dispatch_bypass_filter(
					(if epoch_index == T::EpochInfo::epoch_index() {
						RawOrigin::CurrentEpochWitnessThreshold
					} else {
						RawOrigin::HistoricalActiveEpochWitnessThreshold
					})
					.into(),
				)
				.map_err(|e| {
					Self::deposit_event(Event::<T>::WitnessExecutionFailed {
						call_hash,
						error: e.error,
					});
				});
			CallHashExecuted::<T>::insert(&call_hash, ());
		}

		Ok(().into())
	}

	fn do_witness(
		who: <T as frame_system::Config>::AccountId,
		call: <T as Config>::Call,
	) -> DispatchResultWithPostInfo {
		Self::do_witness_at_epoch(who, call, T::EpochInfo::epoch_index())
	}
}

impl<T: pallet::Config> cf_traits::Witnesser for Pallet<T> {
	type AccountId = T::ValidatorId;
	type Call = <T as pallet::Config>::Call;
	type BlockNumber = T::BlockNumber;

	fn witness(who: Self::AccountId, call: Self::Call) -> DispatchResultWithPostInfo {
		Self::do_witness(who.into(), call)
	}

	fn witness_at_epoch(
		who: Self::AccountId,
		call: Self::Call,
		epoch: EpochIndex,
	) -> DispatchResultWithPostInfo {
		Self::do_witness_at_epoch(who.into(), call, epoch)
	}
}

/// Simple struct on which to implement EnsureOrigin for our pallet's custom origin type.
///
/// # Example:
///
/// ```ignore
/// if let Ok(()) = EnsureWitnessed::ensure_origin(origin) {
///     log::debug!("This extrinsic was called as a result of witness threshold consensus.");
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
			Ok(raw_origin) => match raw_origin {
				RawOrigin::HistoricalActiveEpochWitnessThreshold |
				RawOrigin::CurrentEpochWitnessThreshold => Ok(()),
			},
			Err(o) => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn successful_origin() -> OuterOrigin {
		RawOrigin::HistoricalActiveEpochWitnessThreshold.into()
	}
}

/// Simple struct on which to implement EnsureOrigin for our pallet's custom origin type.
///
/// # Example:
///
/// ```ignore
/// if let Ok(()) = EnsureWitnessedAtCurrentEpoch::ensure_origin(origin) {
///     log::debug!("This extrinsic was called as a result of witness threshold consensus.");
/// }
/// ```
pub struct EnsureWitnessedAtCurrentEpoch;

impl<OuterOrigin> EnsureOrigin<OuterOrigin> for EnsureWitnessedAtCurrentEpoch
where
	OuterOrigin: Into<Result<RawOrigin, OuterOrigin>> + From<RawOrigin>,
{
	type Success = ();

	fn try_origin(o: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
		match o.into() {
			Ok(raw_origin) => match raw_origin {
				RawOrigin::CurrentEpochWitnessThreshold => Ok(()),
				_ => Err(raw_origin.into()),
			},
			Err(o) => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn successful_origin() -> OuterOrigin {
		RawOrigin::CurrentEpochWitnessThreshold.into()
	}
}
