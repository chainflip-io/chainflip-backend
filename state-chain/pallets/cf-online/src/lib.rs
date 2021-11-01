#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extended_key_value_attributes)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

pub mod liveness;

use core::convert::TryInto;
use frame_support::pallet_prelude::*;
use liveness::*;
pub use pallet::*;
use sp_runtime::traits::Zero;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{Chainflip, EpochInfo, Heartbeat, IsOnline, NetworkState};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + Chainflip {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The number of blocks for the time frame we would test liveliness within
		#[pallet::constant]
		type HeartbeatBlockInterval: Get<<Self as frame_system::Config>::BlockNumber>;

		/// A Heartbeat
		type Heartbeat: Heartbeat<ValidatorId = Self::ValidatorId>;

		/// Epoch info
		type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// On initializing each block we check network liveness on every heartbeat interval and
		/// feedback the state of the network as `NetworkState`
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			let block = current_block.try_into().unwrap_or_default();
			if current_block % T::HeartbeatBlockInterval::get() == Zero::zero() {
				let network_state = Self::check_network_liveness();
				// Provide feedback via the `Heartbeat` trait on each interval
				T::Heartbeat::on_heartbeat_interval(network_state);

				return T::WeightInfo::submit_network_state(block);
			}

			T::WeightInfo::on_initialize_no_action(block)
		}
	}

	impl<T: Config> IsOnline for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn is_online(validator_id: &Self::ValidatorId) -> bool {
			match Nodes::<T>::get(validator_id) {
				None => false,
				Some(node) => node.is_online(),
			}
		}
	}

	/// The nodes in the network.  We are assuming here that an account staked with FLIP
	/// is equivalent to an operational node and would appear in this map once they have submitted
	/// a heartbeat.
	///
	#[pallet::storage]
	#[pallet::getter(fn nodes)]
	pub(super) type Nodes<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, Liveness, OptionQuery>;

	#[pallet::event]
	pub enum Event<T: Config> {}

	#[pallet::error]
	pub enum Error<T> {
		/// A heartbeat has already been submitted for the current heartbeat interval for this node
		AlreadySubmittedHeartbeat,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A heartbeat that is used to measure the liveness of a node.
		/// For every interval we expect a heartbeat from all nodes in the network.  Only one
		/// heartbeat for each node is accepted per heartbeat interval.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin): This is not a staked node.
		/// - [AlreadySubmittedHeartbeat](Error::AlreadySubmittedHeartbeat): This node has already
		///   submitted the heartbeat for this interval.
		#[pallet::weight(T::WeightInfo::heartbeat())]
		pub(super) fn heartbeat(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let validator_id: T::ValidatorId = ensure_signed(origin)?.into();

			match Nodes::<T>::get(&validator_id) {
				None => {
					Nodes::<T>::insert(&validator_id, SUBMITTED);
				}
				Some(mut node) => {
					ensure!(!node.has_submitted(), Error::<T>::AlreadySubmittedHeartbeat);
					// Update this node
					node.update_current_interval(true);
					Nodes::<T>::insert(&validator_id, node);
				}
			}

			T::Heartbeat::heartbeat_submitted(&validator_id);

			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Check liveness of our nodes for this heartbeat interval and create a map of the state
		/// of the network for those nodes that are validators.  All nodes are then marked as having
		/// not submitted a heartbeat for the next upcoming heartbeat interval.
		fn check_network_liveness() -> NetworkState<T::ValidatorId> {
			let mut network_state = NetworkState::default();

			Nodes::<T>::translate(|validator_id, mut node: Liveness| {
				if T::EpochInfo::is_validator(&validator_id) {
					// Has the node submitted if not mark them as awaiting a heartbeat
					if !node.has_submitted() {
						network_state.awaiting.push(validator_id.clone());
					}
					// If the node is online
					if node.is_online() {
						network_state.online.push(validator_id);
					}
					network_state.number_of_nodes += 1;
				}
				// Reset the states for all nodes for this interval
				Some(node.update_current_interval(false))
			});

			// Weight will be treated when we have benchmarks
			network_state
		}
	}
}
