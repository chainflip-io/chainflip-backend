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
//! period set by the *heartbeat interval*.  By continuing to submit heartbeats the validator will
//! earn *online credits*.  These *online credits* are exchanged for *reputation points*
//! when they have been *online* for a specified period.  *Reputation points* buffer the validator
//! from being slashed when they go offline for a period of time.
//!
//! Penalties in terms of reputation points are incurred when any one of the *offline conditions* are
//! met.  Falling into negative reputation leads to the eventual slashing of FLIP.  As soon as reputation
//! is positive slashing stops.
//!
//! ## Terminology
//! - **Validator:** A node in our network that is producing blocks.
//! - **Heartbeat:** A term used to measure the liveness of a validator.
//! - **Heartbeat interval:** The duration in time, measured in blocks we would expect to receive a
//!   *heartbeat* from a validator.
//! - **Online:** A node that is online has successfully submitted a heartbeat during the current
//!   heartbeat interval.
//! - **Offline:** A node that is considered offline when they have *not* submitted a heartbeat during
//!   the last heartbeat interval.
//! - **Online credits:** A credit accrued by being continuously online which inturn is used to earn.
//!   *reputation points*.  Failing to stay *online* results in losing all of their *online credits*.
//! - **Reputation points:** A point system which allows validators to earn reputation by being *online*.
//!   They lose reputation points by being meeting one of the *offline conditions*.
//! - **Offline conditions:** One of the following conditions: *missed heartbeat*, *failed to broadcast
//!   an output*, *failed to participate in a signing ceremony*, *not enough performance credits* and
//!   *contradicting self during signing ceremony*.  Each condition has its associated penalty in
//!   reputation points.
//! - **Slashing:** The process of debiting FLIP tokens from a validator.  Slashing only occurs in this
//!   pallet when a validator's reputation points fall below zero *and* they are *offline*.
//! - **Accrual Ratio:** A ratio of reputation points earned per number of offline credits
//!

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

mod liveness;

use frame_support::pallet_prelude::*;
pub use pallet::*;
use pallet_cf_validator::EpochTransitionHandler;
use sp_runtime::traits::Zero;
use sp_std::vec::Vec;
use liveness::*;

/// Error on reporting an offline condition
#[derive(Debug, PartialEq)]
pub enum ReportError {
	// Validator doesn't exist
	UnknownValidator,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{EpochInfo, Heartbeat, NetworkState, IsOnline};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// A stable ID for a validator.
		type ValidatorId: Member + Parameter + From<<Self as frame_system::Config>::AccountId>;

		/// The number of blocks for the time frame we would test liveliness within
		#[pallet::constant]
		type HeartbeatBlockInterval: Get<<Self as frame_system::Config>::BlockNumber>;

		/// Information about the current epoch.
		type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;

		/// A Heartbeat
		type Heartbeat: Heartbeat<ValidatorId = Self::ValidatorId>;

		// An amount of a bid
		type Amount: Copy;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// On initializing each block we check liveness and network liveness on every heartbeat interval
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			if current_block % T::HeartbeatBlockInterval::get() == Zero::zero() {
				let (network_weight, network_state) = Self::check_network_liveness();
				// Provide feedback via the `Heartbeat` trait on each interval
				T::Heartbeat::on_heartbeat_interval(network_state.clone());

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
		/// mark off when we have received a heartbeat.  In doing so the validator is credited
		/// the blocks for this heartbeat interval.  Once the block credits have surpassed the accrual
		/// block number they will earn reputation points based on the accrual ratio.
		///
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
			// Update this validator from the hot list
			ValidatorsLiveness::<T>::mutate(&validator_id, |maybe_liveness| {
				if let Some(mut liveness) = *maybe_liveness {
					*maybe_liveness = Some(liveness.update_current_interval(true));
				}
			});

			T::Heartbeat::heartbeat_submitted(validator_id);

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

	/// On genesis we expect a set of validators to expect heartbeats from.
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

		fn on_new_epoch(new_validators: &Vec<Self::ValidatorId>, _new_bond: Self::Amount) {
			// Clear our expectations
			ValidatorsLiveness::<T>::remove_all();
			// Set the new list of validators we expect a heartbeat from
			for validator_id in new_validators.iter() {
				ValidatorsLiveness::<T>::insert(validator_id, 0);
			}
		}
	}

	impl<T: Config> Pallet<T> {

		/// Check liveness of our expected list of validators at the current block.
		fn check_network_liveness() -> (Weight, NetworkState<T::ValidatorId>) {
			let mut weight = 0;
			// TODO better model for this
			let mut online: Vec<T::ValidatorId> = Vec::new();
			let mut offline: Vec<T::ValidatorId> = Vec::new();
			let mut missing: Vec<T::ValidatorId> = Vec::new();

			ValidatorsLiveness::<T>::translate(|validator_id, mut liveness: Liveness| {
				weight += T::DbWeight::get().reads_writes(1, 1);
				if liveness.is_online() {
					if !liveness.has_submitted() { missing.push(validator_id.clone()); }
					online.push(validator_id);
				} else {
					offline.push(validator_id);
				};

				Some(liveness.update_current_interval(false))
			});

			(weight, NetworkState { online, offline, missing })
		}
	}
}
