#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip Reputation Module
//!
//! A module to manage the reputation of our validators for the Chainflip State Chain
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Module`]
//!
//! ## Overview
//! The module contains functionality
//!
//! ## Terminology
//! - **Offline:**

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use frame_support::pallet_prelude::*;
pub use pallet::*;

pub trait Slashing {}
trait OfflineConditions {
	type ValidatorId;
	fn broadcast_output_failed(validator: Self::ValidatorId);
	fn participate_signing_failed(validator: Self::ValidatorId);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::AtLeast32BitUnsigned;
	use cf_traits::EpochInfo;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// A stable ID for a validator.
		type ValidatorId: Member + Parameter;

		type Amount;

		/// The number of blocks for the time frame we would test liveliness within
		#[pallet::constant]
		type HeartbeatBlockInterval: Get<<Self as frame_system::Config>::BlockNumber>;

		/// Online credit
		type ReputationPoints: Default + Member + Parameter + AtLeast32BitUnsigned;

		/// When we have to, we slash
		type Slasher: Slashing;

		// Information about the current epoch.
		type EpochInfo: EpochInfo<
			ValidatorId = Self::ValidatorId,
			Amount = Self::Amount,
		>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::storage]
	#[pallet::getter(fn liveliness)]
	pub type Liveliness<T: Config> = StorageMap<_, Blake2_128Concat, T::ValidatorId, T::BlockNumber, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn reputation_points)]
	pub type ReputationPoints<T: Config> = StorageMap<_, Blake2_128Concat, T::ValidatorId, T::ReputationPoints, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Broadcast of an output has failed for validator
		BroadcastOutputFailed(T::ValidatorId),
		/// Validator has failed to participate in a signing ceremony
		ParticipateSigningFailed(T::ValidatorId),
		/// Validators that are \[offline, online\]
		ValidatorsOnlineCheck(Vec<T::ValidatorId>, Vec<T::ValidatorId>),
	}

	#[pallet::error]
	pub enum Error<T> {
		Invalid,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {

		#[pallet::weight(10_000)]
		pub(super) fn heartbeat(
			origin: OriginFor<T>
		) -> DispatchResultWithPostInfo {
			Ok(().into())
		}

	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
		}
	}

	impl<T: Config> OfflineConditions for Pallet<T> {
		type ValidatorId = T::ValidatorId;
		fn broadcast_output_failed(validator: Self::ValidatorId) {
			todo!("implement")
		}
		fn participate_signing_failed(validator: Self::ValidatorId) {todo!("implement")}
	}

	impl<T: Config> Pallet<T> {
		fn check_liveliness() {
			todo!("implement")
		}

		fn slash(validator: T::ValidatorId) {
			todo!("implement")
		}

		fn calculate_reputation(validator: T::ValidatorId) {
			todo!("implement")
		}
	}
}