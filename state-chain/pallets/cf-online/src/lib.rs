#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod migrations;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, StorageVersion},
};
pub use pallet::*;
use sp_runtime::traits::Zero;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{Chainflip, EpochInfo, Heartbeat, IsOnline, NetworkState, QualifyNode};
	use frame_support::sp_runtime::traits::BlockNumberProvider;
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::Saturating;

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
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

		fn on_runtime_upgrade() -> Weight {
			migrations::PalletMigration::<T>::on_runtime_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::pre_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::post_upgrade()
		}
	}

	/// A node is considered online if fewer than [T::HeartbeatBlockInterval] blocks
	/// have elapsed since their last heartbeat submission.
	impl<T: Config> IsOnline for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn is_online(validator_id: &Self::ValidatorId) -> bool {
			if let Some(last_heartbeat) = LastHeartbeat::<T>::get(validator_id) {
				frame_system::Pallet::<T>::current_block_number().saturating_sub(last_heartbeat) <
					T::HeartbeatBlockInterval::get()
			} else {
				false
			}
		}
	}

	/// The last block numbers at which validators submitted a heartbeat.
	#[pallet::storage]
	#[pallet::getter(fn last_heartbeat)]
	pub type LastHeartbeat<T: Config> =
		StorageMap<_, Twox64Concat, T::ValidatorId, T::BlockNumber, OptionQuery>;

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
				T::EpochInfo::current_authorities().into_iter().partition(Self::is_online);

			NetworkState { online, offline }
		}
	}

	impl<T: Config> QualifyNode for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
			Self::is_online(validator_id)
		}
	}
}
