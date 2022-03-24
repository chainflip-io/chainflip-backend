#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

use frame_support::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::Zero;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{
		Chainflip, EpochInfo, Heartbeat, IsOnline, KeygenExclusionSet, NetworkState,
		QualifyValidator,
	};
	use frame_support::sp_runtime::traits::BlockNumberProvider;
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::Saturating;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + Chainflip {
		/// The number of blocks for the time frame we would test liveliness within
		#[pallet::constant]
		type HeartbeatBlockInterval: Get<<Self as frame_system::Config>::BlockNumber>;

		/// A Heartbeat
		type Heartbeat: Heartbeat<ValidatorId = Self::ValidatorId, BlockNumber = Self::BlockNumber>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {
		/// We check network liveness on every heartbeat interval and feed back the state of the
		/// network as `NetworkState`.
		fn on_initialize(current_block: T::BlockNumber) -> Weight {
			if current_block % T::HeartbeatBlockInterval::get() == Zero::zero() {
				let network_state = Self::current_network_state();
				// Provide feedback via the `Heartbeat` trait on each interval
				T::Heartbeat::on_heartbeat_interval(network_state);

				return T::WeightInfo::submit_network_state()
			}

			T::WeightInfo::on_initialize_no_action()
		}
	}

	/// A validator is considered online if fewer than [T::HeartbeatBlockInterval] blocks
	/// have elapsed since their last heartbeat submission.
	impl<T: Config> IsOnline for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn is_online(validator_id: &Self::ValidatorId) -> bool {
			Self::is_online_at(frame_system::Pallet::<T>::current_block_number(), validator_id)
		}
	}

	/// The last block numbers at which validators submitted a heartbeat.
	#[pallet::storage]
	#[pallet::getter(fn nodes)]
	pub(super) type LastHeartbeat<T: Config> =
		StorageMap<_, Twox64Concat, T::ValidatorId, T::BlockNumber, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn excluded_from_keygen)]
	pub(super) type ExcludedFromKeygen<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, (), OptionQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A heartbeat is used to measure the liveness of a node. It is measured in blocks.
		/// For every interval we expect at least one heartbeat from all nodes of the network.
		/// Failing this they would be considered offline. Suspended validators can continue to
		/// submit heartbeats so that when their suspension has expired they would be considered
		/// online again.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::heartbeat())]
		pub fn heartbeat(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let validator_id: T::ValidatorId = ensure_signed(origin)?.into();
			let current_block_number = frame_system::Pallet::<T>::current_block_number();

			LastHeartbeat::<T>::insert(&validator_id, current_block_number);

			T::Heartbeat::heartbeat_submitted(&validator_id, current_block_number);
			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Partitions the validators based on whether they are considered online or offline.
		fn current_network_state() -> NetworkState<T::ValidatorId> {
			let (online, offline) =
				T::EpochInfo::current_validators().into_iter().partition(Self::is_online);

			NetworkState { online, offline }
		}

		fn is_online_at(block_number: T::BlockNumber, validator_id: &T::ValidatorId) -> bool {
			if let Some(last_heartbeat) = LastHeartbeat::<T>::get(validator_id) {
				block_number.saturating_sub(last_heartbeat) < T::HeartbeatBlockInterval::get()
			} else {
				false
			}
		}
	}

	impl<T: Config> QualifyValidator for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
			Self::is_online(validator_id)
		}
	}

	// TODO: move this to reputation pallet.
	impl<T: Config> KeygenExclusionSet for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn add_to_set(validator_id: T::ValidatorId) {
			ExcludedFromKeygen::<T>::insert(validator_id, ());
		}

		fn is_excluded(validator_id: &T::ValidatorId) -> bool {
			ExcludedFromKeygen::<T>::contains_key(validator_id)
		}

		fn forgive_all() {
			ExcludedFromKeygen::<T>::remove_all(None);
		}
	}
}
