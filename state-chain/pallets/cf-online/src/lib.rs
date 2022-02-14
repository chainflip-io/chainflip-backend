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
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{
		offline_conditions::Banned, Chainflip, EpochInfo, Heartbeat, IsOnline, KeygenExclusionSet,
		NetworkState,
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
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// We check network liveness on every heartbeat interval and feed back the state of the
		/// network as `NetworkState`.
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			if current_block % T::HeartbeatBlockInterval::get() == Zero::zero() {
				let network_state = Self::check_network_liveness(current_block);
				// Provide feedback via the `Heartbeat` trait on each interval
				T::Heartbeat::on_heartbeat_interval(network_state);

				return T::WeightInfo::submit_network_state()
			}

			T::WeightInfo::on_initialize_no_action()
		}
	}

	impl<T: Config> IsOnline for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		/// We verify if the node is online checking first if they are banned and if they are not
		/// running a check against when they last submitted a heartbeat
		fn is_online(validator_id: &Self::ValidatorId) -> bool {
			match Nodes::<T>::try_get(validator_id) {
				Ok(node) => {
					let current_block_number = frame_system::Pallet::<T>::current_block_number();
					!node.is_banned(current_block_number) &&
						node.has_submitted_this_interval(current_block_number)
				},
				Err(_) => false,
			}
		}
	}

	// Data for tracking a node's liveness state.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct Liveness<T: Config> {
		/// The last heartbeat received from this node
		pub last_heartbeat: T::BlockNumber,
		/// The block number this node is banned until
		pub banned_until: T::BlockNumber,
	}

	impl<T: Config> Default for Liveness<T> {
		fn default() -> Self {
			Liveness { last_heartbeat: Zero::zero(), banned_until: Zero::zero() }
		}
	}

	impl<T: Config> Liveness<T> {
		pub fn has_submitted_this_interval(&self, current_block_number: T::BlockNumber) -> bool {
			(current_block_number - self.last_heartbeat) < T::HeartbeatBlockInterval::get()
		}

		pub fn is_banned(&self, current_block_number: T::BlockNumber) -> bool {
			self.banned_until > current_block_number
		}
	}

	/// A map linking a node's validator id with the last block number at which they submitted a
	/// heartbeat and if they are banned until which block they are banned.
	#[pallet::storage]
	#[pallet::getter(fn nodes)]
	pub(super) type Nodes<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, Liveness<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn excluded_from_keygen)]
	pub(super) type ExcludedFromKeygen<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, (), ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A heartbeat is used to measure the liveness of a node. It is measured in blocks.
		/// For every interval we expect at least one heartbeat from all nodes of the network.
		/// Failing this they would be considered offline.  Banned validators can continue to submit
		/// heartbeats so that when their ban has expired they would be considered online again.
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

			Nodes::<T>::mutate(&validator_id, |node| {
				(*node).last_heartbeat = current_block_number;
			});

			T::Heartbeat::heartbeat_submitted(&validator_id, current_block_number);
			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Check liveness of our nodes for this heartbeat interval and create a map of the state
		/// of the network for those nodes that are validators.  Whether a validator is banned or
		/// not has no bearing on this metric.
		fn check_network_liveness(
			current_block_number: BlockNumberFor<T>,
		) -> NetworkState<T::ValidatorId> {
			let (online, offline) =
				T::EpochInfo::current_validators().into_iter().partition(|validator_id| {
					match Nodes::<T>::try_get(validator_id) {
						Ok(node) => node.has_submitted_this_interval(current_block_number),
						Err(_) => false,
					}
				});

			NetworkState { online, offline }
		}

		/// Get all of the Validators which match the condition of having submitted their
		/// heartbeat this interval, and are not currently banned.
		pub fn online_validators() -> Vec<T::ValidatorId> {
			T::EpochInfo::current_validators()
				.into_iter()
				.filter(|validator_id| Self::is_online(validator_id))
				.collect()
		}
	}

	impl<T: Config> Banned for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn ban(validator_id: &Self::ValidatorId) {
			let current_block_number = frame_system::Pallet::<T>::current_block_number();
			// Ban is one heartbeat interval from now
			let ban = current_block_number.saturating_add(T::HeartbeatBlockInterval::get());
			Nodes::<T>::mutate(validator_id, |node| {
				(*node).banned_until = ban;
			});
		}
	}

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
