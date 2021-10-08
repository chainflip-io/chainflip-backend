#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extended_key_value_attributes)]

#[doc = include_str!("../README.md")]
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use cf_traits::EpochTransitionHandler;
use frame_support::pallet_prelude::*;
use frame_support::sp_std::convert::TryInto;
pub use pallet::*;
use sp_runtime::traits::Zero;

/// Conditions as judged as offline
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum OfflineCondition {
	/// A broadcast of an output has failed
	BroadcastOutputFailed,
	/// There was a failure in participation during a signing
	ParticipateSigningFailed,
	/// Not Enough Performance Credits
	NotEnoughPerformanceCredits,
	/// Contradicting Self During a Signing Ceremony
	ContradictingSelfDuringSigningCeremony,
}

/// Error on reporting an offline condition
#[derive(Debug, PartialEq)]
pub enum ReportError {
	// Validator doesn't exist
	UnknownValidator,
}

/// Offline conditions are reported on
pub trait OfflineConditions {
	type ValidatorId;
	/// Report the condition for validator
	/// Returns `Ok(Weight)` else an error if the validator isn't valid
	fn report(
		condition: OfflineCondition,
		penalty: ReputationPoints,
		validator_id: &Self::ValidatorId,
	) -> Result<Weight, ReportError>;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{EmergencyRotation, EpochInfo, NetworkState, Online, Slashing};
	use frame_system::pallet_prelude::*;
	use sp_std::ops::Neg;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	/// Reputation points type as signed integer
	pub type ReputationPoints = i32;
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
	pub trait Config: frame_system::Config {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// A stable ID for a validator.
		type ValidatorId: Member + Parameter + From<<Self as frame_system::Config>::AccountId>;

		// An amount of a bid
		type Amount: Copy;

		/// The number of blocks for the time frame we would test liveliness within
		#[pallet::constant]
		type HeartbeatBlockInterval: Get<<Self as frame_system::Config>::BlockNumber>;

		/// The number of reputation points we lose for every x blocks offline
		#[pallet::constant]
		type ReputationPointPenalty: Get<ReputationPenalty<Self::BlockNumber>>;

		/// The floor and ceiling values for a reputation score
		#[pallet::constant]
		type ReputationPointFloorAndCeiling: Get<(ReputationPoints, ReputationPoints)>;

		/// Trigger an emergency rotation on falling below the percentage of online validators
		#[pallet::constant]
		type EmergencyRotationPercentageTrigger: Get<u8>;

		/// When we have to, we slash
		type Slasher: Slashing<
			AccountId = Self::ValidatorId,
			BlockNumber = <Self as frame_system::Config>::BlockNumber,
		>;

		/// Information about the current epoch.
		type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId, Amount = Self::Amount>;

		/// Request an emergency rotation
		type EmergencyRotation: EmergencyRotation;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// On initializing each block we check liveness and network liveness on every heartbeat interval
		/// A request for an emergency rotation is made if needed
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			if current_block % T::HeartbeatBlockInterval::get() == Zero::zero() {
				let liveness_weight = Self::check_liveness();
				let (network_weight, network_state) = Self::check_network_liveness();

				if network_state.percentage_online()
					< T::EmergencyRotationPercentageTrigger::get() as u32
				{
					Self::deposit_event(Event::EmergencyRotationRequested(network_state));
					T::EmergencyRotation::request_emergency_rotation();
				}

				return liveness_weight + network_weight;
			}

			Zero::zero()
		}
	}

	type Liveness = u8;
	const NOT_SUBMITTED: u8 = 0;
	const SUBMITTED: u8 = 1;

	/// Liveness bitmap tracking intervals
	trait LivenessTracker {
		/// Online status
		fn is_online(self) -> bool;
		/// Update state of current interval
		fn update_current_interval(&mut self, online: bool) -> Self;
		/// State of submission for the current interval
		fn has_submitted(self) -> bool;
	}

	impl LivenessTracker for Liveness {
		fn is_online(self) -> bool {
			// Online for 2 * `HeartbeatBlockInterval` or 2 lsb
			self & 0x3 != 0
		}

		fn update_current_interval(&mut self, online: bool) -> Self {
			*self <<= 1;
			*self |= online as u8;
			*self
		}

		fn has_submitted(self) -> bool {
			self & 0x1 == 0x1
		}
	}

	impl<T: Config> Online for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn is_online(validator_id: &Self::ValidatorId) -> bool {
			ValidatorsLiveness::<T>::get(validator_id)
				.unwrap_or_default()
				.is_online()
		}
	}
	/// The ratio at which one accrues Reputation points in exchange for online credits
	///
	#[pallet::storage]
	#[pallet::getter(fn accrual_ratio)]
	pub(super) type AccrualRatio<T: Config> =
		StorageValue<_, (ReputationPoints, OnlineCreditsFor<T>), ValueQuery>;

	/// The liveness of our validators
	///
	#[pallet::storage]
	#[pallet::getter(fn validator_liveness)]
	pub(super) type ValidatorsLiveness<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, Liveness, OptionQuery>;

	/// A map tracking our validators.  We record the number of blocks they have been alive
	/// according to the heartbeats submitted.  We are assuming that during a `HeartbeatInterval`
	/// if a `heartbeat()` transaction is submitted that they are alive during the entire
	/// `HeartbeatInterval` of blocks.
	///
	#[pallet::storage]
	#[pallet::getter(fn reputation)]
	pub type Reputations<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, ReputationOf<T>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An offline condition has been met
		OfflineConditionPenalty(T::ValidatorId, OfflineCondition, ReputationPoints),
		/// The accrual rate for our reputation poins has been updated \[points, online credits\]
		AccrualRateUpdated(ReputationPoints, OnlineCreditsFor<T>),
		/// An emergency rotation has been requested \[network state\]
		EmergencyRotationRequested(NetworkState),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// A heartbeat has already been submitted for this validator
		AlreadySubmittedHeartbeat,
		/// An invalid amount of reputation points set for the accrual ratio
		InvalidAccrualReputationPoints,
		/// An invalid amount of online credits for the accrual ratio
		InvalidAccrualOnlineCredits,
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
		pub fn heartbeat(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			// for the validator
			let validator_id: T::ValidatorId = ensure_signed(origin)?.into();

			// If the validator doesn't exist we insert in the map and continue
			// If present we confirm they have already submitted or not
			// Ensure we haven't had a heartbeat for this interval yet for this validator
			ensure!(
				!ValidatorsLiveness::<T>::get(&validator_id)
					.unwrap_or_default()
					.has_submitted(),
				Error::<T>::AlreadySubmittedHeartbeat
			);

			// Update this validator from the hot list
			ValidatorsLiveness::<T>::mutate(&validator_id, |maybe_liveness| {
				if let Some(mut liveness) = *maybe_liveness {
					*maybe_liveness = Some(liveness.update_current_interval(true));
				}
			});
			// Check if this validator has reputation
			if !Reputations::<T>::contains_key(&validator_id) {
				// Credit this validator with the blocks for this interval and set 0 reputation points
				Reputations::<T>::insert(
					validator_id,
					Reputation {
						online_credits: Self::online_credit_reward(),
						reputation_points: 0,
					},
				);
			} else {
				// Update reputation points for this validator
				Reputations::<T>::mutate(
					validator_id,
					|Reputation {
					     online_credits,
					     reputation_points,
					 }| {
						// Accrue some online credits of `HeartbeatInterval` size
						*online_credits = *online_credits + Self::online_credit_reward();
						let (rewarded_points, credits) = AccrualRatio::<T>::get();
						// If we have hit a number of credits to earn reputation points
						if *online_credits >= credits {
							// Swap these credits for reputation
							*online_credits = *online_credits - credits;
							// Update reputation
							*reputation_points = *reputation_points + rewarded_points;
						}
					},
				);
			}

			Ok(().into())
		}

		/// The accrual ratio can be updated and would come into play in the current heartbeat interval
		/// This is only available to sudo
		///
		#[pallet::weight(10_000)]
		pub(super) fn update_accrual_ratio(
			origin: OriginFor<T>,
			points: ReputationPoints,
			online_credits: OnlineCreditsFor<T>,
		) -> DispatchResultWithPostInfo {
			// Ensure we are root when setting this
			let _ = ensure_root(origin)?;
			// Some very basic validation here.  Should be improved in subsequent PR based on
			// further definition of limits
			ensure!(
				points > Zero::zero(),
				Error::<T>::InvalidAccrualReputationPoints
			);
			ensure!(
				online_credits > Zero::zero(),
				Error::<T>::InvalidAccrualOnlineCredits
			);
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
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub accrual_ratio: (ReputationPoints, OnlineCreditsFor<T>),
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				accrual_ratio: (Zero::zero(), Zero::zero()),
			}
		}
	}

	/// On genesis we are initializing the accrual ratio confirming that it is greater than the
	/// heartbeat interval.  We also expect a set of validators to expect heartbeats from.
	///
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			assert!(
				self.accrual_ratio.1 > T::HeartbeatBlockInterval::get(),
				"Heartbeat interval needs to be less than block duration reward"
			);
			AccrualRatio::<T>::set(self.accrual_ratio);
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
				ValidatorsLiveness::<T>::insert(validator_id, NOT_SUBMITTED);
			}
		}
	}

	/// Implementation of `OfflineConditions` reporting on `OfflineCondition` with specified number
	/// of reputation points
	impl<T: Config> OfflineConditions for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		fn report(
			condition: OfflineCondition,
			penalty: ReputationPoints,
			validator_id: &Self::ValidatorId,
		) -> Result<Weight, ReportError> {
			// Confirm validator is present
			ensure!(
				Reputations::<T>::contains_key(validator_id),
				ReportError::UnknownValidator
			);

			Self::deposit_event(Event::OfflineConditionPenalty(
				(*validator_id).clone(),
				condition,
				penalty,
			));

			Ok(Self::update_reputation(validator_id, penalty.neg()))
		}
	}

	impl<T: Config> Pallet<T> {
		/// Return number of online credits for reward
		///
		fn online_credit_reward() -> OnlineCreditsFor<T> {
			// Equivalent to the number of blocks used for the heartbeat
			T::HeartbeatBlockInterval::get()
		}

		/// Update reputation for validator.  Points are clamped to `ReputationPointFloorAndCeiling`
		///
		fn update_reputation(validator_id: &T::ValidatorId, points: ReputationPoints) -> Weight {
			Reputations::<T>::mutate(
				validator_id,
				|Reputation {
				     reputation_points, ..
				 }| {
					*reputation_points =
						Pallet::<T>::clamp_reputation_points(*reputation_points + points);
					T::DbWeight::get().reads_writes(1, 1)
				},
			)
		}

		/// Clamp reputation points to bounds defined in the pallet
		///
		fn clamp_reputation_points(reputation_points: i32) -> i32 {
			let (floor, ceiling) = T::ReputationPointFloorAndCeiling::get();
			reputation_points.clamp(floor, ceiling)
		}

		fn check_network_liveness() -> (Weight, NetworkState) {
			let (mut online, mut offline) = (0u32, 0u32);
			let mut weight = 0;
			for (_, liveness) in ValidatorsLiveness::<T>::iter() {
				weight += T::DbWeight::get().reads(1);
				if liveness.is_online() {
					online += 1
				} else {
					offline += 1
				};
			}

			(weight, NetworkState { online, offline })
		}
		/// Check liveness of our expected list of validators at the current block.
		/// For those that we are still *awaiting* on will be penalised reputation points and any online
		/// credits earned will be set to zero.  In other words we expect continued liveness before we
		/// earn points.
		/// Once the reputation points fall below zero slashing comes into play and is delegated to the
		/// `Slashing` trait.
		fn check_liveness() -> Weight {
			let mut weight = 0;
			// Let's run through those that haven't come back to us and those that have
			ValidatorsLiveness::<T>::translate(|validator_id, mut liveness: Liveness| {
				// Still waiting on these, penalise and those that are in reputation debt will be
				// slashed
				let penalised = !liveness.has_submitted()
					&& Reputations::<T>::mutate(
						&validator_id,
						|Reputation {
						     online_credits,
						     reputation_points,
						 }| {
							if T::ReputationPointFloorAndCeiling::get().0 < *reputation_points {
								// Update reputation points
								let ReputationPenalty { points, blocks } =
									T::ReputationPointPenalty::get();
								let interval: u32 =
									T::HeartbeatBlockInterval::get().try_into().unwrap_or(0);
								let blocks: u32 = blocks.try_into().unwrap_or(0);

								let penalty = (points
									.saturating_mul(interval as i32)
									.checked_div(blocks as i32))
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
					) < Zero::zero();

				if penalised
					|| Reputations::<T>::get(&validator_id).reputation_points < Zero::zero()
				{
					// At this point we slash the validator by the amount of blocks offline
					weight += T::Slasher::slash(&validator_id, T::HeartbeatBlockInterval::get());
				}

				weight += T::DbWeight::get().reads(1);
				Some(liveness.update_current_interval(false))
			});

			weight
		}
	}
}
