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
use frame_support::sp_std::convert::TryInto;
pub use pallet::*;
use pallet_cf_validator::EpochTransitionHandler;
use sp_runtime::traits::Zero;
use sp_std::vec::Vec;

pub trait Slashing {
	type ValidatorId;
	fn slash(validator_id: &Self::ValidatorId) -> Weight;
}

pub enum OfflineCondition {
	BroadcastOutputFailed(ReputationPoints),
	ParticipateSigningFailed(ReputationPoints),
}

#[derive(Debug, PartialEq)]
pub enum ReportError {
	// Validator doesn't exist
	UnknownValidator,
}

pub trait OfflineConditions {
	type ValidatorId;
	fn report(
		condition: OfflineCondition,
		validator_id: &Self::ValidatorId,
	) -> Result<Weight, ReportError>;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::EpochInfo;
	use frame_support::sp_runtime::offchain::storage_lock::BlockNumberProvider;
	use frame_system::pallet_prelude::*;
	use sp_std::ops::Neg;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	pub type ReputationPoints = i32;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// A stable ID for a validator.
		type ValidatorId: Member + Parameter + From<<Self as frame_system::Config>::AccountId>;

		type Amount: Copy;

		/// The number of blocks for the time frame we would test liveliness within
		#[pallet::constant]
		type HeartbeatBlockInterval: Get<<Self as frame_system::Config>::BlockNumber>;

		/// The number of reputation points we lose for every x blocks offline
		#[pallet::constant]
		type ReputationPointPenalty: Get<(u32, u32)>;

		/// The floor and ceiling values for a reputation score
		#[pallet::constant]
		type ReputationPointFloorAndCeiling: Get<(ReputationPoints, ReputationPoints)>;

		/// When we have to, we slash
		type Slasher: Slashing<ValidatorId = Self::ValidatorId>;

		// Information about the current epoch.
		type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			// Read heartbeat interval to see if we need to check liveness
			if current_block % T::HeartbeatBlockInterval::get() == Zero::zero() {
				return Self::check_liveness(current_block);
			}

			Zero::zero()
		}
	}

	#[pallet::storage]
	#[pallet::getter(fn accrual_ratio)]
	pub(super) type AccrualRatio<T: Config> =
		StorageValue<_, (ReputationPoints, T::BlockNumber), ValueQuery>;

	#[pallet::storage]
	pub(super) type AwaitingHeartbeats<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, bool, OptionQuery>;

	/// A map tracking our validators.  We record the number of blocks they have been alive
	/// according to the heartbeats submitted.  We are assuming that during a `HeartbeatInterval`
	/// if a `heartbeat()` transaction is submitted that they are alive during the entire
	/// `HeartbeatInterval` of blocks.
	#[pallet::storage]
	#[pallet::getter(fn reputation)]
	pub type Reputation<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::ValidatorId,
		(T::BlockNumber, ReputationPoints),
		ValueQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Broadcast of an output has failed for validator
		BroadcastOutputFailed(T::ValidatorId, ReputationPoints),
		/// Validator has failed to participate in a signing ceremony
		ParticipateSigningFailed(T::ValidatorId, ReputationPoints),
		/// The accrual rate for our reputation poins has been updated \[points, blocks\]
		AccrualRateUpdated(ReputationPoints, T::BlockNumber),
	}

	#[pallet::error]
	pub enum Error<T> {
		Invalid,
		AlreadySubmittedHeartbeat,
		InvalidReputationPoints,
		InvalidReputationBlocks,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub(super) fn heartbeat(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let validator_id: T::ValidatorId = ensure_signed(origin)?.into();
			// Ensure we haven't had a heartbeat for this interval yet for this validator
			ensure!(
				AwaitingHeartbeats::<T>::get(&validator_id).unwrap_or(false),
				Error::<T>::AlreadySubmittedHeartbeat
			);
			// Update this validator from the hot list
			AwaitingHeartbeats::<T>::mutate(&validator_id, |awaiting| *awaiting = Some(false));
			// Check if this validator has reputation
			if !Reputation::<T>::contains_key(&validator_id) {
				// Track current block number and set 0 reputation points for the validator
				Reputation::<T>::insert(
					validator_id,
					(frame_system::Pallet::<T>::current_block_number(), 0),
				);
			} else {
				// Update reputation points for this validator
				Reputation::<T>::mutate(validator_id, |(block_number, points)| {
					// Accrue some blocks of `HeartbeatInterval` size
					*block_number = *block_number + T::HeartbeatBlockInterval::get();
					let (reputation_points, reputation_blocks) = AccrualRatio::<T>::get();
					// If we have hit a number of blocks to earn reputation points
					if *block_number >= reputation_blocks {
						// Swap these blocks for reputation, probably better after the try_mutate here
						*block_number = *block_number - reputation_blocks;
						// Update reputation
						*points = *points + reputation_points;
					}
				});
			}

			Ok(().into())
		}

		#[pallet::weight(10_000)]
		pub(super) fn update_accrual_ratio(
			origin: OriginFor<T>,
			points: ReputationPoints,
			blocks: BlockNumberFor<T>,
		) -> DispatchResultWithPostInfo {
			let _ = ensure_root(origin)?;
			// Some very basic validation here.  Should be improved in subsequent PR
			ensure!(points > Zero::zero(), Error::<T>::InvalidReputationPoints);
			ensure!(blocks > Zero::zero(), Error::<T>::InvalidReputationBlocks);
			AccrualRatio::<T>::set((points, blocks));
			Self::deposit_event(Event::AccrualRateUpdated(points, blocks));
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub accrual_ratio: (ReputationPoints, T::BlockNumber),
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				accrual_ratio: (1, 10u32.into()),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			AccrualRatio::<T>::set(self.accrual_ratio);
			// A list of those we expect to be online, which are our set of validators
			for validator_id in T::EpochInfo::current_validators().iter() {
				AwaitingHeartbeats::<T>::insert(validator_id, true);
			}
		}
	}

	impl<T: Config> EpochTransitionHandler for Pallet<T> {
		type ValidatorId = T::ValidatorId;
		type Amount = T::Amount;

		fn on_new_epoch(new_validators: &Vec<Self::ValidatorId>, _new_bond: Self::Amount) {
			for validator_id in new_validators.iter() {
				AwaitingHeartbeats::<T>::insert(validator_id, true);
			}
		}
	}

	impl<T: Config> OfflineConditions for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn report(
			condition: OfflineCondition,
			validator_id: &Self::ValidatorId,
		) -> Result<Weight, ReportError> {
			// Confirm validator is present
			ensure!(
				Reputation::<T>::contains_key(validator_id),
				ReportError::UnknownValidator
			);

			// Handle offline conditions
			match condition {
				OfflineCondition::BroadcastOutputFailed(penalty) => {
					Self::deposit_event(Event::BroadcastOutputFailed(
						(*validator_id).clone(),
						penalty,
					));
					Ok(Self::update_reputation(validator_id, penalty.neg()))
				}
				OfflineCondition::ParticipateSigningFailed(penalty) => {
					Self::deposit_event(Event::ParticipateSigningFailed(
						(*validator_id).clone(),
						penalty,
					));
					Ok(Self::update_reputation(validator_id, penalty.neg()))
				}
			}
		}
	}

	impl<T: Config> Pallet<T> {
		fn update_reputation(validator_id: &T::ValidatorId, points: ReputationPoints) -> Weight {
			Reputation::<T>::mutate(validator_id, |(_, current_points)| {
				*current_points = *current_points + points;
				let (mut floor, mut ceiling) = T::ReputationPointFloorAndCeiling::get();
				*current_points = *current_points.clamp(&mut floor, &mut ceiling);

				T::DbWeight::get().reads_writes(1, 1)
			})
		}

		fn calculate_offline_penalty(
			current_block: T::BlockNumber,
			last_block: T::BlockNumber,
		) -> ReputationPoints {
			// What points for what blocks we penalise by
			let (penalty_points, penalty_blocks) = T::ReputationPointPenalty::get();
			// The blocks we have missed
			let dead_blocks = TryInto::<u32>::try_into(current_block - last_block).unwrap_or(0);
			// The points to be penalised
			((penalty_points * dead_blocks / penalty_blocks) as ReputationPoints).neg()
		}

		fn check_liveness(current_block: T::BlockNumber) -> Weight {
			// Let's run through those that haven't come back to us and those that have
			let mut weight = 0;
			AwaitingHeartbeats::<T>::translate(|validator_id, awaiting| {
				// Still waiting on these, penalise and those that are in reputation debt will be
				// slashed
				if awaiting
					&& Reputation::<T>::mutate(
						&validator_id,
						|(last_block_number_alive, reputation_points)| {
							if T::ReputationPointFloorAndCeiling::get().0 < *reputation_points {
								// Update reputation points
								*reputation_points = *reputation_points
									+ Self::calculate_offline_penalty(
										current_block,
										*last_block_number_alive,
									);
								// Set their block time to current as they have paid their debt in reputation
								*last_block_number_alive = current_block;
							}
							weight += T::DbWeight::get().reads_writes(1, 1);

							*reputation_points
						},
					) < Zero::zero() || Reputation::<T>::get(&validator_id).1 < Zero::zero()
				{
					weight += T::Slasher::slash(&validator_id);
				}

				weight += T::DbWeight::get().reads(1);
				Some(true)
			});

			weight
		}
	}
}

impl<T: Config> Slashing for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	fn slash(_validator_id: &Self::ValidatorId) -> Weight {
		0
	}
}
