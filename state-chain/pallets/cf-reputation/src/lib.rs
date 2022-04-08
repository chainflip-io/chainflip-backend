#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(2);

use cf_traits::{
	offence_reporting::*, Chainflip, EpochTransitionHandler, Heartbeat, NetworkState, Slashing,
};

pub mod weights;
pub use weights::WeightInfo;

use frame_support::{
	pallet_prelude::*,
	traits::{Get, OnRuntimeUpgrade, StorageVersion},
};
pub use pallet::*;
use sp_runtime::traits::{UniqueSaturatedInto, Zero};
use sp_std::{
	collections::{btree_set::BTreeSet, vec_deque::VecDeque},
	iter::Iterator,
	prelude::*,
};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod migrations;

mod reputation;
mod suspensions;

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

/// A reputation penalty as a ratio of points penalised over number of blocks
#[derive(Clone, PartialEq, Eq, RuntimeDebug, Encode, Decode)]
pub struct ReputationPenaltyRate<BlockNumber> {
	pub points: ReputationPoints,
	pub per_blocks: BlockNumber,
}

/// A penalty comprises the reputation that will be deducted and the number of blocks suspension
/// that are imposed.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub struct Penalty<T: Config> {
	reputation: ReputationPoints,
	suspension: T::BlockNumber,
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
		Self { reputation: 15, suspension: T::HeartbeatBlockInterval::get() }
	}
}

#[derive(Copy, Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
pub enum PalletOffence {
	MissedHeartbeat,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + Chainflip {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The runtime offence type must be compatible with this pallet's offence type.
		type Offence: From<PalletOffence> + Member + Parameter + Copy + MaybeSerializeDeserialize;

		/// When we have to, we slash
		type Slasher: Slashing<
			AccountId = Self::ValidatorId,
			BlockNumber = <Self as frame_system::Config>::BlockNumber,
		>;

		/// Benchmark stuff
		type WeightInfo: WeightInfo;

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
		type MaximumReputationPointAccrued: Get<ReputationPoints>;
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
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

	/// Reputation trackers for each validator.
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
	#[pallet::getter(fn keygen_exclusion_set)]
	pub type KeygenExclusionSet<T: Config> = StorageValue<_, Vec<T::ValidatorId>, ValueQuery>;

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
		/// - [InvalidAcctualOnlineCredits](Error::InvalidAccrualOnlineCredits)
		#[pallet::weight(T::WeightInfo::update_accrual_ratio())]
		pub fn update_accrual_ratio(
			origin: OriginFor<T>,
			points: ReputationPoints,
			online_credits: T::BlockNumber,
		) -> DispatchResultWithPostInfo {
			let _success = T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(
				points <= T::MaximumReputationPointAccrued::get() && online_credits > Zero::zero(),
				Error::<T>::InvalidAccrualRatio
			);

			AccrualRatio::<T>::set((points, online_credits));
			Self::deposit_event(Event::AccrualRateUpdated(points, online_credits));

			Ok(().into())
		}

		/// Updates the penalty for missing a heartbeat.
		///
		/// ## Events
		///
		/// - [MissedHeartbeatPenaltyUpdated](Event::MissedHeartbeatPenaltyUpdated)
		#[pallet::weight(T::WeightInfo::update_reputation_point_penalty())]
		pub fn update_missed_heartbeat_penalty(
			origin: OriginFor<T>,
			value: ReputationPenaltyRate<BlockNumberFor<T>>,
		) -> DispatchResultWithPostInfo {
			let _success = T::EnsureGovernance::ensure_origin(origin)?;

			let ReputationPenaltyRate { points, per_blocks } = value;
			let interval: u16 = T::HeartbeatBlockInterval::get().unique_saturated_into();
			let per_blocks: u16 = per_blocks.unique_saturated_into();

			let reputation =
				(points.saturating_mul(interval as i32).checked_div(per_blocks as i32))
					.ok_or(Error::<T>::InvalidReputationPenaltyRate)?;

			Penalties::<T>::insert(
				T::Offence::from(PalletOffence::MissedHeartbeat),
				Penalty::<T> { reputation, suspension: Zero::zero() },
			);

			Self::deposit_event(Event::MissedHeartbeatPenaltyUpdated(reputation));
			Ok(().into())
		}

		// #[pallet::weight(T::WeightInfo::set_penalty())]
		#[pallet::weight(10_000_000)]
		pub fn set_penalty(
			origin: OriginFor<T>,
			offence: T::Offence,
			penalty: Penalty<T>,
		) -> DispatchResultWithPostInfo {
			let _success = T::EnsureGovernance::ensure_origin(origin)?;

			let old = Penalties::<T>::mutate(&offence, |maybe_penalty| {
				let old = maybe_penalty.clone().unwrap_or_default();
				*maybe_penalty = Some(penalty.clone());
				old
			});

			Self::deposit_event(Event::<T>::PenaltyUpdated(offence, old, penalty));

			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub accrual_ratio: (ReputationPoints, T::BlockNumber),
		#[allow(clippy::type_complexity)]
		pub penalties: Vec<(T::Offence, (ReputationPoints, T::BlockNumber))>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { accrual_ratio: (Zero::zero(), Zero::zero()), penalties: Default::default() }
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
		}
	}
}

impl<T: Config> OffenceReporter for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Offence = T::Offence;

	fn report_many<'a>(offence: impl Into<Self::Offence>, validators: &[Self::ValidatorId]) {
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
}

impl<T: Config> Heartbeat for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type BlockNumber = T::BlockNumber;

	fn heartbeat_submitted(validator_id: &Self::ValidatorId, _block_number: Self::BlockNumber) {
		Reputations::<T>::mutate(&validator_id, |rep| {
			rep.boost_reputation(Self::online_credit_reward());
		});
	}

	fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>) {
		<Self as OffenceReporter>::report_many(
			PalletOffence::MissedHeartbeat,
			network_state.offline.as_slice(),
		);
		for validator_id in network_state.offline {
			let reputation_points = Reputations::<T>::mutate(&validator_id, |rep| {
				rep.reset_online_credits();
				rep.reputation_points
			});

			if reputation_points < 0 {
				// At this point we slash the validator by the amount of blocks offline
				T::Slasher::slash(&validator_id, T::HeartbeatBlockInterval::get());
			}
		}
	}
}

impl<T: Config> EpochTransitionHandler for Pallet<T> {
	type ValidatorId = T::ValidatorId;

	fn on_new_epoch(_epoch_validators: &[Self::ValidatorId]) {
		KeygenExclusionSet::<T>::kill();
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
