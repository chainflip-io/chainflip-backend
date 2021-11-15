#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use frame_support::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::Zero;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{Chainflip, EpochInfo, Heartbeat, IsOnline, NetworkState};
	use frame_support::sp_runtime::traits::BlockNumberProvider;
	use frame_system::pallet_prelude::*;

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

		/// Epoch info
		type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// We check network liveness on every heartbeat interval and feed back the state of the
		/// network as `NetworkState`
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			if current_block % T::HeartbeatBlockInterval::get() == Zero::zero() {
				let (network_weight, network_state) = Self::check_network_liveness(current_block);
				// Provide feedback via the `Heartbeat` trait on each interval
				T::Heartbeat::on_heartbeat_interval(network_state);

				return network_weight
			}

			Zero::zero()
		}
	}

	impl<T: Config> IsOnline for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn is_online(validator_id: &Self::ValidatorId) -> bool {
			match Nodes::<T>::get(validator_id) {
				None => false,
				Some(block_number) => {
					let current_block_number = frame_system::Pallet::<T>::current_block_number();
					Self::has_submitted_this_interval(block_number, current_block_number)
				},
			}
		}
	}

	/// The nodes in the network.  We are assuming here that an account staked with FLIP is
	/// equivalent to an operational node and would appear in this map once they have submitted
	/// a heartbeat.
	#[pallet::storage]
	#[pallet::getter(fn nodes)]
	pub(super) type Nodes<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, T::BlockNumber>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A heartbeat is used to measure the liveness of a node. It is measured in blocks.
		/// For every interval we expect at least one heartbeat from all nodes of the network.
		/// Failing this they would be considered offline.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(10_000)]
		pub fn heartbeat(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let validator_id: T::ValidatorId = ensure_signed(origin)?.into();
			let current_block_number = frame_system::Pallet::<T>::current_block_number();
			Nodes::<T>::insert(&validator_id, current_block_number);

			T::Heartbeat::heartbeat_submitted(&validator_id, current_block_number);
			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		fn has_submitted_this_interval(
			reported_block_number: BlockNumberFor<T>,
			current_block_number: BlockNumberFor<T>,
		) -> bool {
			(current_block_number - reported_block_number) < T::HeartbeatBlockInterval::get()
		}
		/// Check liveness of our nodes for this heartbeat interval and create a map of the state
		/// of the network for those nodes that are validators.
		fn check_network_liveness(
			current_block_number: BlockNumberFor<T>,
		) -> (Weight, NetworkState<T::ValidatorId>) {
			let mut network_state = NetworkState::default();

			for (validator_id, block_number) in Nodes::<T>::iter() {
				if T::EpochInfo::is_validator(&validator_id) {
					if Self::has_submitted_this_interval(block_number, current_block_number) {
						network_state.online.push(validator_id);
					} else {
						network_state.offline.push(validator_id);
					}
					network_state.number_of_nodes += 1;
				}
			}

			// Weight will be treated when we have benchmarks
			(Zero::zero(), network_state)
		}
	}
}
