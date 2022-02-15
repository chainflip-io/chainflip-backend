#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod releases {
	use frame_support::traits::StorageVersion;
	// Genesis version
	pub const V0: StorageVersion = StorageVersion::new(0);
	// Version 1 - adds MintInterval storage items
	pub const V1: StorageVersion = StorageVersion::new(1);
}

pub mod weights;
pub use weights::WeightInfo;

use frame_support::{pallet_prelude::*, sp_std::convert::TryInto};
pub use pallet::*;
use sp_runtime::traits::Zero;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod migrations;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{offline_conditions::*, Chainflip, Heartbeat, NetworkState, Slashing};
	use frame_system::pallet_prelude::*;
	use sp_std::ops::Neg;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::storage_version(releases::V1)]
	pub struct Pallet<T>(_);

	/// The credits one earns being online, equivalent to a blocktime online
	pub type OnlineCreditsFor<T> = <T as frame_system::Config>::BlockNumber;
	/// Reputation of a validator
	#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
	pub struct Reputation<OnlineCredits> {
		online_credits: OnlineCredits,
		pub reputation_points: ReputationPoints,
	}

	/// A reputation penalty as a ratio of points penalised over number of blocks
	#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
	pub struct ReputationPenalty<BlockNumber> {
		pub points: ReputationPoints,
		pub blocks: BlockNumber,
	}

	type ReputationOf<T> = Reputation<<T as frame_system::Config>::BlockNumber>;

	#[pallet::config]
	pub trait Config: frame_system::Config + Chainflip {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The number of blocks for the time frame we would test liveliness within
		#[pallet::constant]
		type HeartbeatBlockInterval: Get<<Self as frame_system::Config>::BlockNumber>;

		/// The floor and ceiling values for a reputation score
		#[pallet::constant]
		type ReputationPointFloorAndCeiling: Get<(ReputationPoints, ReputationPoints)>;

		/// When we have to, we slash
		type Slasher: Slashing<
			AccountId = Self::ValidatorId,
			BlockNumber = <Self as frame_system::Config>::BlockNumber,
		>;

		/// Penalise
		type Penalty: OfflinePenalty;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;

		/// Ban validators
		type Banned: Banned<ValidatorId = Self::ValidatorId>;
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			if releases::V0 == <Pallet<T> as GetStorageVersion>::on_chain_storage_version() {
				releases::V1.put::<Pallet<T>>();
				migrations::v1::migrate::<T>();
				return T::WeightInfo::on_runtime_upgrade_v1()
			}
			T::WeightInfo::on_runtime_upgrade()
		}
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<(), &'static str> {
			if releases::V0 == <Pallet<T> as GetStorageVersion>::on_chain_storage_version() {
				migrations::v1::pre_migrate::<T, Self>()
			} else {
				Ok(())
			}
		}
		#[cfg(feature = "try-runtime")]
		fn post_upgrade() -> Result<(), &'static str> {
			if releases::V1 == <Pallet<T> as GetStorageVersion>::on_chain_storage_version() {
				migrations::v1::post_migrate::<T, Self>()
			} else {
				Ok(())
			}
		}
	}

	/// The ratio at which one accrues Reputation points in exchange for online credits
	#[pallet::storage]
	#[pallet::getter(fn accrual_ratio)]
	pub(super) type AccrualRatio<T: Config> =
		StorageValue<_, (ReputationPoints, OnlineCreditsFor<T>), ValueQuery>;

	/// A map tracking our validators.  We record the number of reputation points that they may
	/// have.
	#[pallet::storage]
	#[pallet::getter(fn reputation)]
	pub type Reputations<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, ReputationOf<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn reputation_point_penalty)]
	/// The number of reputation points we lose for every x blocks offline
	pub(super) type ReputationPointPenalty<T: Config> =
		StorageValue<_, ReputationPenalty<BlockNumberFor<T>>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An offline condition has been met
		OfflineConditionPenalty(T::ValidatorId, OfflineCondition, ReputationPoints),
		/// The accrual rate for our reputation points has been updated \[points, online credits\]
		AccrualRateUpdated(ReputationPoints, OnlineCreditsFor<T>),
		/// The value for ReputationPointPenalty has been updated
		ReputationPointPenaltyUpdated(ReputationPenalty<BlockNumberFor<T>>),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An invalid amount of reputation points set for the accrual ratio
		InvalidAccrualReputationPoints,
		/// An invalid amount of online credits for the accrual ratio
		InvalidAccrualOnlineCredits,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// The accrual ratio can be updated and would come into play in the current heartbeat
		/// interval This is only available to sudo
		///
		/// ## Events
		///
		/// - [AccrualRateUpdated](Event::AccrualRateUpdated)
		///
		/// ## Errors
		///
		/// - [InvalidAccrualReputationPoints](Error::InvalidAccrualReputationPoints)
		/// - [InvalidAcctualOnlineCredits](Error::InvalidAccrualOnlineCredits)
		#[pallet::weight(T::WeightInfo::update_accrual_ratio())]
		pub fn update_accrual_ratio(
			origin: OriginFor<T>,
			points: ReputationPoints,
			online_credits: OnlineCreditsFor<T>,
		) -> DispatchResultWithPostInfo {
			// Ensure we are root when setting this
			ensure_root(origin)?;
			// Some very basic validation here.  Should be improved in subsequent PR based on
			// further definition of limits
			ensure!(points > Zero::zero(), Error::<T>::InvalidAccrualReputationPoints);
			ensure!(online_credits > Zero::zero(), Error::<T>::InvalidAccrualOnlineCredits);
			// Online credits are equivalent to block time and hence should be less than our
			// heartbeat interval
			ensure!(
				online_credits > T::HeartbeatBlockInterval::get(),
				Error::<T>::InvalidAccrualOnlineCredits
			);

			AccrualRatio::<T>::set((points, online_credits));
			Self::deposit_event(Event::AccrualRateUpdated(points, online_credits));

			Ok(().into())
		}
		/// Updates the value for the ReputationPointPenalty storage item
		///
		/// ## Events
		///
		/// - [ReputationPointPenaltyUpdated](Event::ReputationPointPenaltyUpdated)
		#[pallet::weight(T::WeightInfo::update_reputation_point_penalty())]
		pub fn update_reputation_point_penalty(
			origin: OriginFor<T>,
			value: ReputationPenalty<BlockNumberFor<T>>,
		) -> DispatchResultWithPostInfo {
			// Ensure we are root when setting this
			ensure_root(origin)?;
			ReputationPointPenalty::<T>::put(value.clone());
			Self::deposit_event(Event::ReputationPointPenaltyUpdated(value));
			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub accrual_ratio: (ReputationPoints, OnlineCreditsFor<T>),
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { accrual_ratio: (Zero::zero(), Zero::zero()) }
		}
	}

	/// On genesis, we are initializing the accrual ratio confirming that it is greater than the
	/// heartbeat interval.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			assert!(
				self.accrual_ratio.1 > T::HeartbeatBlockInterval::get(),
				"Heartbeat interval needs to be less than block duration reward"
			);
			AccrualRatio::<T>::set(self.accrual_ratio);
			ReputationPointPenalty::<T>::put(ReputationPenalty { points: 1, blocks: 10u32.into() });
		}
	}

	/// Implementation of `OfflineReporter` reporting on `OfflineCondition` with specified number
	/// of reputation points
	impl<T: Config> OfflineReporter for Pallet<T> {
		type ValidatorId = T::ValidatorId;
		type Penalty = T::Penalty;

		fn report(condition: OfflineCondition, validator_id: &Self::ValidatorId) {
			// Confirm validator is present
			if !Reputations::<T>::contains_key(validator_id) {
				log::error!(
					target: "cf-reputation",
					"Cannot find Validator {:?} to report.",
					validator_id
				);
				return
			}

			let (penalty, to_ban) = Self::Penalty::penalty(&condition);

			if to_ban {
				T::Banned::ban(validator_id);
			}

			Self::deposit_event(Event::OfflineConditionPenalty(
				(*validator_id).clone(),
				condition,
				penalty,
			));

			Self::update_reputation(validator_id, penalty.neg());
		}
	}

	impl<T: Config> Heartbeat for Pallet<T> {
		type ValidatorId = T::ValidatorId;
		type BlockNumber = T::BlockNumber;

		/// A heartbeat is submitted and in doing so the validator is credited the blocks for this
		/// heartbeat interval.  These block credits are transformed to reputation points based on
		/// the accrual ratio.
		fn heartbeat_submitted(
			validator_id: &Self::ValidatorId,
			_block_number: Self::BlockNumber,
		) -> Weight {
			// Check if this validator has reputation
			if !Reputations::<T>::contains_key(&validator_id) {
				// Credit this validator with the blocks for this interval and set 0 reputation
				// points
				Reputations::<T>::insert(
					validator_id,
					Reputation {
						online_credits: Self::online_credit_reward(),
						reputation_points: 0,
					},
				);

				T::DbWeight::get().reads_writes(1, 1)
			} else {
				// Update reputation points for this validator
				Reputations::<T>::mutate(
					validator_id,
					|Reputation { online_credits, reputation_points }| {
						*online_credits += Self::online_credit_reward();
						let (rewarded_points, credits) = AccrualRatio::<T>::get();
						// If we have hit a number of credits to earn reputation points
						if *online_credits >= credits {
							// Swap these credits for reputation
							*online_credits -= credits;
							// Update reputation
							*reputation_points += rewarded_points;
						}
					},
				);

				T::DbWeight::get().reads_writes(2, 1)
			}
		}

		/// For those that we are still *awaiting* on will be penalised reputation points and any
		/// online credits earned will be set to zero.  In other words we expect continued liveness
		/// before we earn points.
		/// Once the reputation points fall below zero slashing comes into play and is delegated to
		/// the `Slashing` trait.
		fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>) -> Weight {
			// Penalise those that are missing this heartbeat
			let mut weight = 0;
			for validator_id in network_state.offline {
				let reputation_points = Reputations::<T>::mutate(
					&validator_id,
					|Reputation { online_credits, reputation_points }| {
						if T::ReputationPointFloorAndCeiling::get().0 < *reputation_points {
							// Update reputation points
							// TODO: refactor to make it not panic!
							let ReputationPenalty { points, blocks } =
								ReputationPointPenalty::<T>::get();
							let interval: u32 =
								T::HeartbeatBlockInterval::get().try_into().unwrap_or(0);
							let blocks: u32 = blocks.try_into().unwrap_or(0);

							let penalty =
								(points.saturating_mul(interval as i32).checked_div(blocks as i32))
									.expect("calculating offline penalty shouldn't fail");

							*reputation_points = Pallet::<T>::clamp_reputation_points(
								(*reputation_points).saturating_sub(penalty),
							);
							// Reset the credits earned as being online consecutively
							*online_credits = Zero::zero();
						}
						weight += T::DbWeight::get().reads_writes(1, 1);

						*reputation_points
					},
				);

				if reputation_points < Zero::zero() ||
					Reputations::<T>::get(&validator_id).reputation_points < Zero::zero()
				{
					// At this point we slash the validator by the amount of blocks offline
					weight += T::Slasher::slash(&validator_id, T::HeartbeatBlockInterval::get());
				}
				weight += T::DbWeight::get().reads(1);
			}
			weight
		}
	}

	impl<T: Config> Pallet<T> {
		/// Return number of online credits for reward
		fn online_credit_reward() -> OnlineCreditsFor<T> {
			// Equivalent to the number of blocks used for the heartbeat
			T::HeartbeatBlockInterval::get()
		}

		/// Update reputation for validator.  Points are clamped to `ReputationPointFloorAndCeiling`
		fn update_reputation(validator_id: &T::ValidatorId, points: ReputationPoints) -> Weight {
			Reputations::<T>::mutate(validator_id, |Reputation { reputation_points, .. }| {
				*reputation_points =
					Pallet::<T>::clamp_reputation_points(*reputation_points + points);
				T::DbWeight::get().reads_writes(1, 1)
			})
		}

		/// Clamp reputation points to bounds defined in the pallet
		fn clamp_reputation_points(reputation_points: i32) -> i32 {
			let (floor, ceiling) = T::ReputationPointFloorAndCeiling::get();
			reputation_points.clamp(floor, ceiling)
		}
	}
}
