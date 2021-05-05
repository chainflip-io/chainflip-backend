// Code mostly taken from here: https://github.com/gautamdhameja/substrate-validator-set
// modifications to it, such as validation (since we're not using sudo to add validators)
// will come later

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
use sp_runtime::traits::{Convert, OpaqueKeys};
use sp_std::prelude::*;
use frame_support::sp_runtime::traits::{Saturating, Zero};
use log::{debug};

type ValidatorSize = u32;
type EpochIndex = u32;

pub trait ValidatorHandler<ValidatorId> {
	fn on_new_session(
		changed: bool,
		current_validators: Vec<ValidatorId>,
		next_validators: Vec<ValidatorId>
	);
	fn on_before_session_ending();
}

impl<ValidatorId> ValidatorHandler<ValidatorId> for () {
	fn on_new_session(
		_changed: bool,
		_current_validators: Vec<ValidatorId>,
		_next_validators: Vec<ValidatorId>
	) {}
	fn on_before_session_ending() {}
}

pub trait CandidateProvider {
	type ValidatorId: Eq + Ord + Clone;
	type Stake: Eq + Ord + Copy;

	fn get_candidates() -> Vec<(Self::ValidatorId, Self::Stake)>;
}

impl CandidateProvider for () {
	type ValidatorId = u32;
	type Stake = u32;

	fn get_candidates() -> Vec<(Self::ValidatorId, Self::Stake)> {
		vec![]
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use frame_support::sp_runtime::SaturatedConversion;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		type ValidatorId: Eq + Ord + Clone;
		// type Stake: Eq + Ord + Copy;
		/// A provider for our validators
		type CandidateProvider: CandidateProvider<ValidatorId=Self::ValidatorId>;
		/// A handler for callbacks
		type ValidatorHandler: ValidatorHandler<Self::ValidatorId>;

		#[pallet::constant]
		type MinEpoch: Get<<Self as frame_system::Config>::BlockNumber>;

		#[pallet::constant]
		type MinValidatorSetSize: Get<u64>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		AuctionStarted(EpochIndex),
		AuctionEnded(EpochIndex),
		EpochChanged(T::BlockNumber, T::BlockNumber),
		MaximumValidatorsChanged(ValidatorSize, ValidatorSize),
		ForceRotationRequested(),
	}

	#[pallet::error]
	pub enum Error<T> {
		NoValidators,
		InvalidEpoch,
		InvalidValidatorSetSize,
		InvalidAuction,
	}

	// Pallet implements [`Hooks`] trait to define some logic to execute in some context.
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub(super) fn set_epoch(
			origin: OriginFor<T>,
			number_of_blocks: T::BlockNumber,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(number_of_blocks >= T::MinEpoch::get(), Error::<T>::InvalidEpoch);
			let old_epoch = BlocksPerEpoch::<T>::get();
			ensure!(old_epoch != number_of_blocks, Error::<T>::InvalidEpoch);
			BlocksPerEpoch::<T>::set(number_of_blocks);
			Self::deposit_event(Event::EpochChanged(old_epoch, number_of_blocks));
			Ok(().into())
		}

		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub(super) fn set_validator_target_size(
			origin: OriginFor<T>,
			size: ValidatorSize,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(size >= T::MinValidatorSetSize::get().saturated_into(), Error::<T>::InvalidValidatorSetSize);
			let old_size = SizeValidatorSet::<T>::get();
			ensure!(old_size != size, Error::<T>::InvalidValidatorSetSize);
			SizeValidatorSet::<T>::set(size);
			Self::deposit_event(Event::MaximumValidatorsChanged(old_size, size));
			Ok(().into())
		}

		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub(super) fn force_rotation(
			origin: OriginFor<T>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			Force::<T>::set(true);
			Self::deposit_event(Event::ForceRotationRequested());
			Ok(().into())
		}

		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub(super) fn confirm_auction(
			origin: OriginFor<T>,
			index: EpochIndex,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			ensure!(Some(index) == AuctionToConfirm::<T>::get(), Error::<T>::InvalidAuction);
			Ok(().into())
		}
	}

	#[pallet::storage]
	#[pallet::getter(fn force)]
	pub(super) type Force<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn last_block_number)]
	pub(super) type LastBlockNumber<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn epoch_number_of_blocks)]
	pub(super) type BlocksPerEpoch<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn max_validators)]
	pub(super) type SizeValidatorSet<T: Config> = StorageValue<_, ValidatorSize, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn is_auction_phase)]
	pub(super) type IsAuctionPhase<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn auction_confirmed)]
	pub(super) type AuctionToConfirm<T: Config> = StorageValue<_, EpochIndex, OptionQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub size_validator_set: ValidatorSize,
		pub epoch_number_of_blocks: T::BlockNumber,

	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				size_validator_set: Zero::zero(),
				epoch_number_of_blocks: Zero::zero(),
			}
		}
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {}
	}
}

impl<T: Config> pallet_session::SessionHandler<T::ValidatorId> for Pallet<T> {
	const KEY_TYPE_IDS: &'static [sp_runtime::KeyTypeId] = &[];
	fn on_genesis_session<Ks: OpaqueKeys>(_validators: &[(T::ValidatorId, Ks)]) {}

	fn on_new_session<Ks: OpaqueKeys>(
		changed: bool,
		validators: &[(T::ValidatorId, Ks)],
		queued_validators: &[(T::ValidatorId, Ks)],
	) {
		let current_validators = validators.iter()
			.map(|(id, _)| id.clone())
			.collect::<Vec<T::ValidatorId>>();

		let next_validators = queued_validators.iter()
			.map(|(id, _)| id.clone())
			.collect::<Vec<T::ValidatorId>>();

		T::ValidatorHandler::on_new_session(changed, current_validators, next_validators);
	}

	/// Triggered before [`SessionManager::end_session`] handler
	fn on_before_session_ending() {
		T::ValidatorHandler::on_before_session_ending();
	}

	fn on_disabled(_validator_index: usize) {
		// TBD
	}
}

/// Indicates to the session module if the session should be rotated.
impl<T: Config> pallet_session::ShouldEndSession<T::BlockNumber> for Pallet<T> {
	fn should_end_session(now: T::BlockNumber) -> bool {
		Self::should_end_session(now)
	}
}

/// Provides the new set of validators to the session module when session is being rotated.
impl<T: Config> pallet_session::SessionManager<T::ValidatorId> for Pallet<T> {
	// On rotation this is called #3
	fn new_session(new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		debug!("planning new_session({})", new_index);
		Self::new_session(new_index)
	}

	// On rotation this is called #1
	fn end_session(end_index: SessionIndex) {
		debug!("starting start_session({})", end_index);
		Self::end_session(end_index)
	}

	// On rotation this is called #2
	fn start_session(start_index: SessionIndex) {
		debug!("ending end_session({})", start_index);
		Self::start_session(start_index);
	}
}

impl<T: Config> frame_support::traits::EstimateNextSessionRotation<T::BlockNumber> for Pallet<T> {
	fn estimate_next_session_rotation(now: T::BlockNumber) -> Option<T::BlockNumber> {
		Self::estimate_next_session_rotation(now)
	}

	// The validity of this weight depends on the implementation of `estimate_next_session_rotation`
	fn weight(_now: T::BlockNumber) -> u64 {
		0
	}
}

/// In this module, for simplicity, we just return the same AccountId.
pub struct ValidatorOf<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> Convert<T::AccountId, Option<T::AccountId>> for ValidatorOf<T> {
	fn convert(account: T::AccountId) -> Option<T::AccountId> {
		Some(account)
	}
}

type Stake<T> = <<T as Config>::CandidateProvider as CandidateProvider>::Stake;

type SessionIndex = u32; 

impl<T: Config> Pallet<T> {

	/// This returns validators for the *next* session and is called at the *beginning* of the current session.
	///
	/// If we are at the beginning of a non-auction session, the next session will be an auction session, so we return
	/// `None` to indicate that the validator set remains unchanged. Otherwise, the set is considered changed even if 
	/// the new set of validators is the same as the old one.  
	///
	/// If we are the beginning of an auction session, we need to run the auction to set the validators for the upcoming
	/// Epoch.
	///
	/// `AuctionStarted` is emitted and the rotation from auction to trading phases will wait on a
	/// confirmation via the `auction_confirmed` extrinsic
	fn new_session(new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		if !Self::is_auction_phase() {
			if let Some(index) = AuctionToConfirm::<T>::get() {
				Self::deposit_event(Event::AuctionEnded(index));
			}
			return None
		}

		debug!("Creating a new auction-phase session {}", new_index);
		Self::deposit_event(Event::AuctionStarted(new_index));
		AuctionToConfirm::<T>::set(Some(new_index));
		let candidates = T::CandidateProvider::get_candidates();
		let new_validators = Self::run_auction(candidates);
		new_validators
	}

	/// The end of the session is triggered, we alternate between regular trading sessions and auction sessions. 
	fn end_session(end_index: SessionIndex) {
		IsAuctionPhase::<T>::mutate(|is_auction| {
			if *is_auction {
				debug!("Ending the auction session {}", end_index);
			} else {
				debug!("Ending the trading session {}", end_index);
			}
			*is_auction = !*is_auction;
		});
	}

	fn start_session(start_index: SessionIndex) {
		debug!("Starting a new session {}", start_index);
	}

	pub fn run_auction(mut candidates: Vec<(T::ValidatorId, Stake<T>)>) -> Option<Vec<T::ValidatorId>> {
		// A basic auction algorithm.  We sort by stake amount and take the top of the validator
		// set size and let session pallet do the rest
		// Space here to add other prioritisation parameters
		if !candidates.is_empty() {
			candidates.sort_unstable_by_key(|k| k.1);
			candidates.reverse();
			let max_size = SizeValidatorSet::<T>::get();
			let candidates = candidates.get(0..max_size as usize);
			if let Some(candidates) = candidates {
				let candidates: Vec<T::ValidatorId> = candidates.iter().map(|i| i.0.clone()).collect();
				return Some(candidates);
			}
		}
		Some(vec![])
	}

	/// Check if we have a forced session for this block.  If not, if we are in the "auction" phase
	/// then we would rotate only with a confirmation of that auction else we would count blocks to
	/// see if the epoch has come to end
	pub fn should_end_session(now: T::BlockNumber) -> bool {
		if Force::<T>::get() {
			Force::<T>::set(false);
			return true
		}

		if Self::is_auction_phase() {
			Self::auction_confirmed().is_none()
		} else {
			let epoch_blocks = BlocksPerEpoch::<T>::get();
			if epoch_blocks == Zero::zero() {
				return false;
			}
			let last_block_number = LastBlockNumber::<T>::get();
			let diff = now.saturating_sub(last_block_number);
			let end = diff >= epoch_blocks;
			if end { LastBlockNumber::<T>::set(now); }
			end
		}
	}

	pub fn estimate_next_session_rotation(now: T::BlockNumber) -> Option<T::BlockNumber> {
		let epoch_blocks = BlocksPerEpoch::<T>::get();
		Some(now + epoch_blocks)
	}
}
