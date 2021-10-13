#![cfg_attr(not(feature = "std"), no_std)]

//! # ChainFlip Online Module
//!
//! A module to manage the liveness of our validators for the ChainFlip State Chain
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Module`]
//!
//! ## Overview
//! The module contains functionality to measure the liveness of our validators.  This is measured
//! with a *heartbeat* which should be submitted via the extrinsic `heartbeat()` within the time
//! period set by the *heartbeat interval*.
//!
//! ## Terminology
//! - Validator: A node in our network that is producing blocks.
//! - Heartbeat: A term used to measure the liveness of a validator.
//! - Heartbeat interval: The duration in time, measured in blocks we would expect to receive a
//!   heartbeat from a validator.
//! - Online: A node that is online has successfully submitted a heartbeat during the current
//!   heartbeat interval.
//! - Offline: A node that is considered offline when they have *not* submitted a heartbeat during
//!   the last heartbeat interval.

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

mod liveness;

use cf_traits::EpochTransitionHandler;
use frame_support::pallet_prelude::*;
use liveness::*;
pub use pallet::*;
use sp_runtime::traits::Zero;
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{Chainflip, Heartbeat, IsOnline, NetworkState, StakeHandler, StakerProvider};
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

		/// Provide a list of stakers in the network
		type StakerProvider: StakerProvider<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// On initializing each block we check liveness and network liveness on every heartbeat interval
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			if current_block % T::HeartbeatBlockInterval::get() == Zero::zero() {
				let (network_weight, network_state) = Self::check_network_liveness();
				// Provide feedback via the `Heartbeat` trait on each interval
				T::Heartbeat::on_heartbeat_interval(network_state);

				return network_weight;
			}

			Zero::zero()
		}
	}

	impl<T: Config> IsOnline for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn is_online(validator_id: &Self::ValidatorId) -> bool {
			Nodes::<T>::get(validator_id)
				.unwrap_or_default()
				.is_online()
		}
	}

	/// The liveness of nodes in the network.  We are assuming here that an account staked with FLIP
	/// is equivalent to an operational node.  The definition of operational is that they have the
	/// software package installed, running and that they are submitting heartbeats
	///
	#[pallet::storage]
	#[pallet::getter(fn liveness)]
	pub(super) type Nodes<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, Node, OptionQuery>;

	#[pallet::event]
	pub enum Event<T: Config> {}

	#[pallet::error]
	pub enum Error<T> {
		/// A heartbeat has already been submitted for this node
		AlreadySubmittedHeartbeat,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A heartbeat that is used to measure the liveness of a node
		/// For every interval there are a set of nodes we expect a heartbeat from with which we
		/// mark off when we have received a heartbeat.
		#[pallet::weight(10_000)]
		pub(super) fn heartbeat(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let validator_id: T::ValidatorId = ensure_signed(origin)?.into();
			// Ensure we haven't had a heartbeat during this interval for this node
			ensure!(
				!Nodes::<T>::get(&validator_id)
					.unwrap_or_default()
					.has_submitted(),
				Error::<T>::AlreadySubmittedHeartbeat
			);
			// Update this node
			Nodes::<T>::mutate(&validator_id, |maybe_node| {
				if let Some(mut node) = maybe_node {
					node.update_current_interval(true);
				}
			});

			T::Heartbeat::heartbeat_submitted(&validator_id);

			Ok(().into())
		}
	}

	/// Implementation of the `EpochTransitionHandler` trait with which we clear stale stakers
	/// that may have dropped off during the last epoch.
	impl<T: Config> EpochTransitionHandler for Pallet<T> {
		type ValidatorId = T::ValidatorId;
		type Amount = T::Amount;

		fn on_new_epoch(
			_old_validators: &[Self::ValidatorId],
			new_validators: &[Self::ValidatorId],
			_new_bond: Self::Amount,
		) {
			Nodes::<T>::remove_all();
			for (validator_id, _) in T::StakerProvider::get_stakers().iter() {
				let is_validator = new_validators.contains(validator_id);
				Nodes::<T>::insert(
					validator_id,
					Node {
						is_validator,
						..Default::default()
					},
				);
			}
		}
	}

	/// Implementation of the `StakeHandler` trait with which we add any new stakers as live
	///
	impl<T: Config> StakeHandler for Pallet<T> {
		type ValidatorId = T::ValidatorId;
		type Amount = T::Amount;

		fn stake_updated(validator_id: &Self::ValidatorId, _new_total: Self::Amount) {
			if !Nodes::<T>::contains_key(validator_id) {
				Nodes::<T>::insert(
					validator_id,
					Node {
						is_validator: false,
						..Default::default()
					},
				);
			}
		}
	}

	impl<T: Config> Pallet<T> {
		/// Check liveness of our expected list of validators at the current block and
		/// create a map of the state of the network
		fn check_network_liveness() -> (Weight, NetworkState<T::ValidatorId>) {
			let mut weight = 0;
			let mut network_state = NetworkState::default();

			Nodes::<T>::translate(|validator_id, mut node: Node| {
				weight += T::DbWeight::get().reads_writes(1, 1);
				if node.is_validator {
					if node.is_online() {
						if !node.has_submitted() {
							network_state.missing.push(validator_id.clone());
						}
						network_state.online.push(validator_id);
					} else {
						network_state.offline.push(validator_id);
					};
				}
				Some(node.update_current_interval(false))
			});

			(weight, network_state)
		}
	}
}
