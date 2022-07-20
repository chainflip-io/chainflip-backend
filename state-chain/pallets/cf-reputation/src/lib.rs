#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(2);

use cf_traits::{
	offence_reporting::*, Chainflip, Heartbeat, NetworkState, ReputationResetter, Slashing,
};

pub mod weights;
pub use weights::WeightInfo;

use frame_support::{
	pallet_prelude::*,
	traits::{Get, OnRuntimeUpgrade, StorageVersion},
};
pub use pallet::*;
use sp_runtime::traits::Zero;
use sp_std::{
	collections::{btree_set::BTreeSet, vec_deque::VecDeque},
	iter::Iterator,
	prelude::*,
};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod migrations;

mod reporting_adapter;
mod reputation;
mod suspensions;

pub use reporting_adapter::*;
pub use reputation::*;
pub use suspensions::*;

type RuntimeSuspensionTracker<T> = SuspensionTracker<
	<T as Chainflip>::ValidatorId,
	<T as frame_system::Config>::BlockNumber,
	<T as Config>::Offence,
>;

impl<T: Config> ReputationParameters for T {
	type OnlineCredits = T::BlockNumber;

	fn bounds() -> (ReputationPoints, ReputationPoints) {
		T::ReputationPointFloorAndCeiling::get()
	}

	fn accrual_rate() -> (ReputationPoints, Self::OnlineCredits) {
		AccrualRatio::<T>::get()
	}
}

type RuntimeReputationTracker<T> = reputation::ReputationTracker<T>;

/// A penalty comprises the reputation that will be deducted and the number of blocks suspension
/// that are imposed.
#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
#[codec(mel_bound(T: Config))]
pub struct Penalty<T: Config> {
	pub reputation: ReputationPoints,
	pub suspension: T::BlockNumber,
}

impl<T: Config> sp_std::fmt::Debug for Penalty<T> {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		f.debug_struct("Penalty")
			.field("reputation", &self.reputation)
			.field("suspension", &self.suspension)
			.finish()
	}
}

impl<T: Config> Default for Penalty<T> {
	fn default() -> Self {
		Self { reputation: Default::default(), suspension: Default::default() }
	}
}

#[derive(Copy, Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	MissedHeartbeat,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_traits::{EpochInfo, QualifyNode};
	use frame_support::sp_runtime::traits::BlockNumberProvider;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::generate_store(pub (super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The runtime offence type must be compatible with this pallet's offence type.
		type Offence: From<PalletOffence>
			+ Member
			+ Parameter
			+ MaxEncodedLen
			+ Copy
			+ MaybeSerializeDeserialize;

		/// When we have to, we slash
		type Slasher: Slashing<
			AccountId = Self::ValidatorId,
			BlockNumber = <Self as frame_system::Config>::BlockNumber,
		>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;

		/// Handle to allow us to trigger across any pallet on a heartbeat interval
		type Heartbeat: Heartbeat<ValidatorId = Self::ValidatorId, BlockNumber = Self::BlockNumber>;

		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// The number of blocks for the time frame we would test liveliness within
		#[pallet::constant]
		type HeartbeatBlockInterval: Get<<Self as frame_system::Config>::BlockNumber>;

		/// The floor and ceiling values for a reputation score
		#[pallet::constant]
		type ReputationPointFloorAndCeiling: Get<(ReputationPoints, ReputationPoints)>;

		/// The maximum number of reputation points that can be accrued
		#[pallet::constant]
		type MaximumAccruableReputation: Get<ReputationPoints>;
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: T::BlockNumber) -> Weight {
			if Self::blocks_since_new_interval(current_block) == Zero::zero() {
				// Provide feedback via the `Heartbeat` trait on each interval
				T::Heartbeat::on_heartbeat_interval(Self::current_network_state());

				return T::WeightInfo::submit_network_state()
			}
			T::WeightInfo::on_initialize_no_action()
		}

		fn on_runtime_upgrade() -> Weight {
			migrations::PalletMigration::<T>::on_runtime_upgrade();
			T::WeightInfo::on_runtime_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::pre_upgrade()
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade() -> Result<(), &'static str> {
			migrations::PalletMigration::<T>::post_upgrade()
		}
	}

	/// The ratio at which one accrues Reputation points in exchange for online credits
	#[pallet::storage]
	#[pallet::getter(fn accrual_ratio)]
	pub type AccrualRatio<T: Config> =
		StorageValue<_, (ReputationPoints, T::BlockNumber), ValueQuery>;

	/// Reputation trackers for each node
	#[pallet::storage]
	#[pallet::getter(fn reputation)]
	pub type Reputations<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ValidatorId, RuntimeReputationTracker<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn suspensions)]
	/// Suspension tracking storage for each offence.
	pub type Suspensions<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::Offence,
		VecDeque<(T::BlockNumber, T::ValidatorId)>,
		ValueQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn penalties)]
	/// The penalty to be applied for each offence.
	pub type Penalties<T: Config> = StorageMap<_, Twox64Concat, T::Offence, Penalty<T>>;

	#[pallet::storage]
	#[pallet::getter(fn offence_time_slot_tracker)]
	/// The penalty to be applied for each offence.
	pub type OffenceTimeSlotTracker<T: Config> = StorageMap<_, Identity, ReportId, OpaqueTimeSlot>;

	/// The last block numbers at which validators submitted a heartbeat.
	#[pallet::storage]
	#[pallet::getter(fn last_heartbeat)]
	pub type LastHeartbeat<T: Config> =
		StorageMap<_, Twox64Concat, T::ValidatorId, T::BlockNumber, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An offence has been penalised. \[offender, offence, penalty\]
		OffencePenalty(T::ValidatorId, T::Offence, ReputationPoints),
		/// The accrual rate for our reputation points has been updated \[points, online_credits\]
		AccrualRateUpdated(ReputationPoints, T::BlockNumber),
		/// The penalty for missing a heartbeat has been updated. \[points\]
		MissedHeartbeatPenaltyUpdated(ReputationPoints),
		/// The penalty for some offence has been updated \[offence, old_penalty, new_penalty\]
		PenaltyUpdated(T::Offence, Penalty<T>, Penalty<T>),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Tried to set the accrual ration to something invalid.
		InvalidAccrualRatio,
		/// The block in a reputation point penalty must be non-zero.
		InvalidReputationPenaltyRate,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// The accrual ratio can be updated and would come into play in the current heartbeat
		/// interval. This is gated with governance.
		///
		/// ## Events
		///
		/// - [AccrualRateUpdated](Event::AccrualRateUpdated)
		///
		/// ## Errors
		///
		/// - [InvalidAccrualReputationPoints](Error::InvalidAccrualReputationPoints)
		#[pallet::weight(T::WeightInfo::update_accrual_ratio())]
		pub fn update_accrual_ratio(
			origin: OriginFor<T>,
			reputation_points: ReputationPoints,
			online_credits: T::BlockNumber,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			ensure!(
				reputation_points <= T::MaximumAccruableReputation::get() &&
					online_credits > Zero::zero(),
				Error::<T>::InvalidAccrualRatio
			);

			AccrualRatio::<T>::set((reputation_points, online_credits));
			Self::deposit_event(Event::AccrualRateUpdated(reputation_points, online_credits));

			Ok(().into())
		}

		/// Updates the penalty for missing a heartbeat.
		///
		/// ## Events
		///
		/// - [MissedHeartbeatPenaltyUpdated](Event::MissedHeartbeatPenaltyUpdated)
		#[pallet::weight(T::WeightInfo::update_missed_heartbeat_penalty())]
		pub fn update_missed_heartbeat_penalty(
			origin: OriginFor<T>,
			reputation: ReputationPoints,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			Penalties::<T>::insert(
				T::Offence::from(PalletOffence::MissedHeartbeat),
				Penalty::<T> { reputation, suspension: T::HeartbeatBlockInterval::get() },
			);

			Self::deposit_event(Event::MissedHeartbeatPenaltyUpdated(reputation));
			Ok(().into())
		}

		/// Set the [Penalty] for an [Offence].
		#[pallet::weight(T::WeightInfo::set_penalty())]
		pub fn set_penalty(
			origin: OriginFor<T>,
			offence: T::Offence,
			penalty: Penalty<T>,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;

			let old = Penalties::<T>::mutate(&offence, |maybe_penalty| {
				let old = maybe_penalty.clone().unwrap_or_default();
				*maybe_penalty = Some(penalty.clone());
				old
			});

			Self::deposit_event(Event::<T>::PenaltyUpdated(offence, old, penalty));

			Ok(().into())
		}

		/// A heartbeat is used to measure the liveness of a node. It is measured in blocks.
		/// For every interval we expect at least one heartbeat from all nodes of the network.
		/// Failing this they would be considered offline. Suspended validators can continue to
		/// submit heartbeats so that when their suspension has expired they would be considered
		/// online again.
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

			let start_of_this_interval =
				current_block_number - Self::blocks_since_new_interval(current_block_number);

			// Heartbeat intervals range is (start, end]
			match LastHeartbeat::<T>::get(&validator_id) {
				Some(last_heartbeat) if last_heartbeat > start_of_this_interval => {
					// we have already submitted a heartbeat for this interval
				},
				_ => {
					LastHeartbeat::<T>::insert(&validator_id, current_block_number);
					Reputations::<T>::mutate(&validator_id, |rep| {
						rep.boost_reputation(Self::online_credit_reward());
					});
				},
			};

			Ok(().into())
		}
	}

	impl<T: Config> QualifyNode for Pallet<T> {
		type ValidatorId = T::ValidatorId;

		/// A node is considered online, and therefore qualified if fewer than
		/// [T::HeartbeatBlockInterval] blocks have elapsed since their last heartbeat submission.
		fn is_qualified(validator_id: &Self::ValidatorId) -> bool {
			use sp_runtime::traits::Saturating;
			if let Some(last_heartbeat) = LastHeartbeat::<T>::get(validator_id) {
				frame_system::Pallet::<T>::current_block_number().saturating_sub(last_heartbeat) <
					T::HeartbeatBlockInterval::get()
			} else {
				false
			}
		}
	}

	impl<T: Config> Pallet<T> {
		/// Returns the number of blocks that have elapsed since the new HeartbeatBlockInterval
		pub fn blocks_since_new_interval(block_number: T::BlockNumber) -> T::BlockNumber {
			block_number % T::HeartbeatBlockInterval::get()
		}

		/// Partitions the authorities based on whether they are considered online or offline.
		pub fn current_network_state() -> NetworkState<T::ValidatorId> {
			let (online, offline) =
				T::EpochInfo::current_authorities().into_iter().partition(Self::is_qualified);

			NetworkState { online, offline }
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub accrual_ratio: (ReputationPoints, T::BlockNumber),
		#[allow(clippy::type_complexity)]
		pub penalties: Vec<(T::Offence, (ReputationPoints, T::BlockNumber))>,
		pub genesis_nodes: Vec<T::ValidatorId>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				accrual_ratio: (Zero::zero(), Zero::zero()),
				penalties: Default::default(),
				genesis_nodes: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			AccrualRatio::<T>::set(self.accrual_ratio);
			for (offence, (reputation, suspension)) in self.penalties.iter() {
				Penalties::<T>::insert(
					offence,
					Penalty::<T> { reputation: *reputation, suspension: *suspension },
				);
			}
			let current_block_number = frame_system::Pallet::<T>::current_block_number();
			for node in &self.genesis_nodes {
				LastHeartbeat::<T>::insert(node, current_block_number);
			}
		}
	}
}

impl<T: Config> OffenceReporter for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Offence = T::Offence;

	fn report_many(offence: impl Into<Self::Offence>, validators: &[Self::ValidatorId]) {
		let offence = offence.into();
		let penalty = Self::resolve_penalty_for(offence);

		if penalty.reputation > 0 {
			for validator_id in validators {
				Reputations::<T>::mutate(&validator_id, |rep| {
					rep.deduct_reputation(penalty.reputation);
				});
				Self::deposit_event(Event::OffencePenalty(
					validator_id.clone(),
					offence,
					penalty.reputation,
				));
			}
		}

		if penalty.suspension > Zero::zero() {
			Self::suspend_all(validators, &offence, penalty.suspension);
		}
	}

	fn forgive_all(offence: impl Into<Self::Offence>) {
		Suspensions::<T>::remove(&offence.into());
	}
}

pub trait OffenceList<T: Config> {
	const OFFENCES: &'static [T::Offence];
}

pub struct GetValidatorsExcludedFor<T: Config, L: OffenceList<T>>(
	sp_std::marker::PhantomData<(T, L)>,
);

impl<T: Config, L: OffenceList<T>> Get<BTreeSet<T::ValidatorId>>
	for GetValidatorsExcludedFor<T, L>
{
	fn get() -> BTreeSet<T::ValidatorId> {
		Pallet::<T>::validators_suspended_for(L::OFFENCES)
	}
}

impl<T: Config> Pallet<T> {
	pub fn penalise_offline_authorities(offline_authorities: Vec<T::ValidatorId>) {
		<Self as OffenceReporter>::report_many(
			PalletOffence::MissedHeartbeat,
			offline_authorities.as_slice(),
		);
		for validator_id in offline_authorities {
			let reputation_points = Reputations::<T>::mutate(&validator_id, |rep| {
				rep.reset_online_credits();
				rep.reputation_points
			});

			if reputation_points < 0 {
				// At this point we slash the node by the amount of blocks offline
				T::Slasher::slash(&validator_id, T::HeartbeatBlockInterval::get());
			}
		}
	}

	/// Return number of online credits for reward
	fn online_credit_reward() -> T::BlockNumber {
		// Equivalent to the number of blocks used for the heartbeat
		T::HeartbeatBlockInterval::get()
	}

	pub fn suspend_all<'a>(
		validators: impl IntoIterator<Item = &'a T::ValidatorId>,
		offence: &T::Offence,
		suspension: T::BlockNumber,
	) {
		// Scoped::<T, RuntimeSuspensionTracker<T>>::scoped(offence)
		// 	.mutate(|tracker| tracker.suspend(validators.iter().cloned(), suspension));
		let mut tracker = <SuspensionTracker<_, _, _> as StorageLoadable<T>>::load(offence);
		tracker.suspend(validators.into_iter().cloned(), suspension);
		StorageLoadable::<T>::commit(&mut tracker);
	}

	/// Gets a list of validators that are suspended for committing any of a list of offences.
	pub fn validators_suspended_for(offences: &[T::Offence]) -> BTreeSet<T::ValidatorId> {
		offences
			.iter()
			.flat_map(|offence| {
				<RuntimeSuspensionTracker<T> as StorageLoadable<T>>::load(offence).get_suspended()
			})
			.collect()
	}

	/// Look up the penalty for the given offence. Uses the default value if no mapping is
	/// available.
	fn resolve_penalty_for<O: Into<T::Offence>>(offence: O) -> Penalty<T> {
		let offence: T::Offence = offence.into();
		Penalties::<T>::get(&offence).unwrap_or_else(|| {
			log::warn!("No penalty defined for offence {:?}, using default.", offence);
			Default::default()
		})
	}
}

impl<T: Config> ReputationResetter for Pallet<T> {
	type ValidatorId = T::ValidatorId;

	/// Reset both the online credits and the reputation points of a validator to zero.
	fn reset_reputation(validator: &Self::ValidatorId) {
		Reputations::<T>::mutate(validator, |rep| {
			rep.reset_reputation();
			rep.reset_online_credits();
		});
	}
}
