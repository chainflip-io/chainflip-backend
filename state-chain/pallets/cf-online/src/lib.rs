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

use frame_support::pallet_prelude::*;
use liveness::*;
pub use pallet::*;
use cf_traits::EpochTransitionHandler;
use sp_runtime::traits::Zero;
use sp_std::vec::Vec;

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

		/// Information about the current epoch.
		type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;

		/// A Heartbeat
		type Heartbeat: Heartbeat<ValidatorId = Self::ValidatorId>;
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
			ValidatorsLiveness::<T>::get(validator_id)
				.unwrap_or_default()
				.is_online()
		}
	}

	/// The liveness of our validators
	///
	#[pallet::storage]
	pub(super) type ValidatorsLiveness<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, Liveness, OptionQuery>;

	#[pallet::event]
	pub enum Event<T: Config> {}

	#[pallet::error]
	pub enum Error<T> {
		/// A heartbeat has already been submitted for this validator
		AlreadySubmittedHeartbeat,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A heartbeat that is used to measure the liveness of a validator
		/// Every interval we have a set of validators we expect a heartbeat from with which we
		/// mark off when we have received a heartbeat.
		#[pallet::weight(10_000)]
		pub(super) fn heartbeat(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			// for the validator
			let validator_id: T::ValidatorId = ensure_signed(origin)?.into();
			// Ensure we haven't had a heartbeat for this interval yet for this validator
			ensure!(
				!ValidatorsLiveness::<T>::get(&validator_id)
					.unwrap_or(SUBMITTED)
					.has_submitted(),
				Error::<T>::AlreadySubmittedHeartbeat
			);
			// Update this validator
			ValidatorsLiveness::<T>::mutate(&validator_id, |maybe_liveness| {
				if let Some(mut liveness) = *maybe_liveness {
					*maybe_liveness = Some(liveness.update_current_interval(true));
				}
			});

			T::Heartbeat::heartbeat_submitted(&validator_id);

			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {}
		}
	}

	/// On genesis, we expect a set of validators to expect heartbeats from.
	///
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			// A list of those we expect to be online, which are our set of validators
			for validator_id in T::EpochInfo::current_validators().iter() {
				ValidatorsLiveness::<T>::insert(validator_id, 1);
			}
		}
	}

	/// Implementation of the `EpochTransitionHandler` trait with which we populate are
	/// expected list of validators.
	///
	impl<T: Config> EpochTransitionHandler for Pallet<T> {
		type ValidatorId = T::ValidatorId;
		type Amount = T::Amount;

		fn on_new_epoch(new_validators: &[Self::ValidatorId], _new_bond: Self::Amount) {
			// Clear our expectations
			ValidatorsLiveness::<T>::remove_all();
			// Set the new list of validators we expect a heartbeat from
			for validator_id in new_validators.iter() {
				ValidatorsLiveness::<T>::insert(validator_id, 0);
			}
		}
	}

	impl<T: Config> Pallet<T> {
		/// Check liveness of our expected list of validators at the current block and
		/// create a map of the state of the network
		fn check_network_liveness() -> (Weight, NetworkState<T::ValidatorId>) {
			let mut weight = 0;
			let mut online: Vec<T::ValidatorId> = Vec::new();
			let mut offline: Vec<T::ValidatorId> = Vec::new();
			let mut missing: Vec<T::ValidatorId> = Vec::new();

			ValidatorsLiveness::<T>::translate(|validator_id, mut liveness: Liveness| {
				weight += T::DbWeight::get().reads_writes(1, 1);
				if liveness.is_online() {
					if !liveness.has_submitted() {
						missing.push(validator_id.clone());
					}
					online.push(validator_id);
				} else {
					offline.push(validator_id);
				};

				Some(liveness.update_current_interval(false))
			});

			(
				weight,
				NetworkState { missing, online, offline	},
			)
		}
	}
}
