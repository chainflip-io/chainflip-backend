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
use cf_primitives::EpochIndex;
use cf_traits::EpochInfo;
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
	use cf_traits::AccountRoleRegistry;
	use frame_support::{pallet_prelude::*, storage::with_transaction};
	use frame_system::pallet_prelude::*;
	use sp_runtime::TransactionOutcome;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The outer Origin needs to be compatible with this pallet's Origin
		type RuntimeOrigin: From<RawOrigin>;

		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;

		/// The overarching call type.
		type RuntimeCall: Member
			+ Parameter
			+ From<frame_system::Call<Self>>
			+ UnfilteredDispatchable<RuntimeOrigin = <Self as Config>::RuntimeOrigin>
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
	pub(super) type VoteMask = BitSlice<u8, Msb0>;

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
	pub type CallHashExecuted<T: Config> =
		StorageDoubleMap<_, Twox64Concat, EpochIndex, Identity, CallHash, ()>;

	/// This stores (expired) epochs that needs to have its data culled.
	#[pallet::storage]
	pub type EpochsToCull<T: Config> = StorageValue<_, Vec<EpochIndex>, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Clear stale data from expired epochs
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut epochs_to_cull = EpochsToCull::<T>::get();
			let epoch = if let Some(epoch) = epochs_to_cull.pop() {
				epoch
			} else {
				return T::WeightInfo::on_idle_with_nothing_to_remove()
			};

			let max_deletions_count_remaining = remaining_weight
				.ref_time()
				.checked_div(T::WeightInfo::remove_storage_items(1).ref_time())
				.unwrap_or_default();

			if max_deletions_count_remaining == 0 {
				return T::WeightInfo::on_idle_with_nothing_to_remove()
			}

			let mut deletions_count_remaining = max_deletions_count_remaining;
			let mut used_weight: Weight = Weight::zero();
			let (mut cleared_votes, mut cleared_extra_call_data, mut cleared_call_hash) =
				(false, false, false);

			// Cull the Votes storage
			let remove_result =
				Votes::<T>::clear_prefix(epoch, deletions_count_remaining as u32, None);
			deletions_count_remaining =
				deletions_count_remaining.saturating_sub(remove_result.backend as u64);
			used_weight = used_weight
				.saturating_add(T::WeightInfo::remove_storage_items(remove_result.backend));
			if remove_result.maybe_cursor.is_none() {
				cleared_votes = true;
			}

			// Cull the `ExtraCallData` storage
			if deletions_count_remaining > 0 {
				let remove_result =
					ExtraCallData::<T>::clear_prefix(epoch, deletions_count_remaining as u32, None);
				deletions_count_remaining =
					deletions_count_remaining.saturating_sub(remove_result.backend as u64);
				used_weight = used_weight
					.saturating_add(T::WeightInfo::remove_storage_items(remove_result.backend));
				if remove_result.maybe_cursor.is_none() {
					cleared_extra_call_data = true;
				}
			}

			// Cull the `CallHashExecuted` storage
			if deletions_count_remaining > 0 {
				let remove_result = CallHashExecuted::<T>::clear_prefix(
					epoch,
					deletions_count_remaining as u32,
					None,
				);
				used_weight = used_weight
					.saturating_add(T::WeightInfo::remove_storage_items(remove_result.backend));
				if remove_result.maybe_cursor.is_none() {
					cleared_call_hash = true;
				}
			}

			// If all storages have been cleared, update storage.
			if cleared_votes && cleared_extra_call_data && cleared_call_hash {
				EpochsToCull::<T>::put(epochs_to_cull);
			}
			used_weight
		}
	}

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
		/// Think of this a vote for some action (represented by a runtime `call`) to be taken. At a
		/// high level:
		///
		/// 1. Ensure we are not submitting a witness for an expired epoch
		/// 2. Look up the account id in the list of authorities.
		/// 3. Get the list of votes for the epoch and call, or an empty list if this is the first
		/// vote.
		/// 4. Add the account's vote to the list.
		/// 5. Check the number of votes against the required threshold.
		/// 6. The provided `call` will be dispatched when the configured threshold number of
		/// authorities have submitted an identical transaction. This can be thought of as a vote
		/// for the encoded [Call](Config::Call) value.
		///
		/// This implementation uses a bitmask whereby each index to the bitmask represents an
		/// authority account ID in the current Epoch.
		///
		/// **Note:**
		/// This implementation currently allows voting to continue even after the vote threshold is
		/// reached.
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
		#[allow(clippy::boxed_local)]
		#[pallet::weight(
			T::WeightInfo::witness_at_epoch().saturating_add(call.get_dispatch_info().weight /
				T::EpochInfo::authority_count_at_epoch(*epoch_index).unwrap_or(1u32) as u64)
		)]
		pub fn witness_at_epoch(
			origin: OriginFor<T>,
			mut call: Box<<T as Config>::RuntimeCall>,
			epoch_index: EpochIndex,
		) -> DispatchResultWithPostInfo {
			let who = T::AccountRoleRegistry::ensure_validator(origin)?;

			let last_expired_epoch = T::EpochInfo::last_expired_epoch();
			let current_epoch = T::EpochInfo::epoch_index();
			// Ensure the epoch has not yet expired
			ensure!(epoch_index > last_expired_epoch, Error::<T>::EpochExpired);

			// The number of authorities for the epoch
			// This value is updated alongside ValidatorIndex, so if we have a authority, we have an
			// authority count.
			let num_authorities = T::EpochInfo::authority_count_at_epoch(epoch_index)
				.ok_or(Error::<T>::InvalidEpoch)?;

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
					let bytes = buffer.get_or_insert_with(|| {
						BitVec::<u8, Msb0>::repeat(false, num_authorities as usize).into_vec()
					});

					// Convert to an addressable bitmask
					let bits = VoteMask::from_slice_mut(bytes);

					let mut vote_count = bits.count_ones();

					// Get a reference to the existing vote.
					let mut vote =
						bits.get_mut(index).ok_or(Error::<T>::AuthorityIndexOutOfBounds)?;

					// Return an error if already voted, otherwise set the indexed bit to `true` to
					// indicate a vote.
					if *vote {
						return Err(Error::<T>::DuplicateWitness)
					}

					vote_count += 1;
					*vote = true;

					if let Some(extra_data) = extra_data {
						ExtraCallData::<T>::append(epoch_index, call_hash, extra_data);
					}

					Ok(vote_count)
				},
			)?;

			// Check if threshold is reached and, if so, apply the voted-on Call.
			// At the epoch boundary, asynchronicity can cause validators to witness events at a
			// earlier epoch than intended. We need to check that the same event has not already
			// been witnessed in the past.
			if num_votes == success_threshold_from_share_count(num_authorities) as usize &&
				(last_expired_epoch..=current_epoch)
					.all(|epoch| CallHashExecuted::<T>::get(epoch, call_hash).is_none())
			{
				if let Some(mut extra_data) = ExtraCallData::<T>::get(epoch_index, call_hash) {
					call.combine_and_inject(&mut extra_data)
				}
				let _result = with_transaction(move || {
					match call.dispatch_bypass_filter(
						(if epoch_index == current_epoch {
							RawOrigin::CurrentEpochWitnessThreshold
						} else {
							RawOrigin::HistoricalActiveEpochWitnessThreshold
						})
						.into(),
					) {
						r @ Ok(_) => TransactionOutcome::Commit(r),
						r @ Err(_) => TransactionOutcome::Rollback(r),
					}
				})
				.map_err(|e| {
					Self::deposit_event(Event::<T>::WitnessExecutionFailed {
						call_hash,
						error: e.error,
					});
				});
				CallHashExecuted::<T>::insert(epoch_index, call_hash, ());
			}
			Ok(().into())
		}
	}

	/// Witness pallet origin
	#[pallet::origin]
	pub type Origin = RawOrigin;

	/// The raw origin enum for this pallet.
	#[derive(PartialEq, Eq, Clone, RuntimeDebug, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub enum RawOrigin {
		HistoricalActiveEpochWitnessThreshold,
		CurrentEpochWitnessThreshold,
	}
}

impl<T: pallet::Config> cf_traits::EpochTransitionHandler for Pallet<T> {
	type ValidatorId = T::ValidatorId;

	/// Add the expired epoch to the queue to have its data culled. This is prevent the storage from
	/// growing indefinitely.
	fn on_expired_epoch(expired: EpochIndex) {
		EpochsToCull::<T>::append(expired);
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
