#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub use pallet::*;
use sp_runtime::traits::{Convert, OpaqueKeys, AtLeast32BitUnsigned};
use sp_std::prelude::*;
use frame_support::sp_runtime::traits::{Saturating, Zero};
use log::{debug};
use frame_support::pallet_prelude::*;
use cf_traits::EpochInfo;
use serde::{Serialize, Deserialize};

pub trait WeightInfo {
	fn set_blocks_for_epoch() -> Weight;
	fn set_validator_target_size() -> Weight;
	fn force_auction() -> Weight;
	fn confirm_auction() -> Weight;
}

pub type ValidatorSize = u32;
type SessionIndex = u32;

#[derive(Encode, Decode, Clone, Copy, RuntimeDebug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochIndex(SessionIndex);

impl From<SessionIndex> for EpochIndex {
	fn from(i: SessionIndex) -> Self {
		EpochIndex(i/2)
	}
}

/// This handler can be implemented in order to hook into Epoch lifecycle events.
pub trait EpochTransitionHandler {
	/// The id type used for the validators. 
	type ValidatorId;

	/// Triggered at the start of a new Epoch.
	fn on_new_epoch(_new_validators: Vec<Self::ValidatorId>) {}

	/// Triggered at the start of the auction phase.
	fn on_new_auction(_outgoing_validators: Vec<Self::ValidatorId>) {}

	/// Triggered before the end of the trading phase and the start of the auction.
	fn on_before_auction() {}

	/// Triggered after the end of the auction, before a new Epoch.
	fn on_before_epoch_ending() {}
}

impl<T: pallet_session::Config> EpochTransitionHandler for PhantomData<T> {
	type ValidatorId = T::ValidatorId;
}

/// Something that can provide us a list of candidates with their corresponding stakes
pub trait CandidateProvider {
	type ValidatorId: Eq + Ord + Clone;
	type Stake: Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;

	fn get_candidates() -> Vec<(Self::ValidatorId, Self::Stake)>;
}

/// Empty impl of [`CandidateProvider`]
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
	use frame_system::pallet_prelude::*;
	use frame_support::sp_runtime::SaturatedConversion;
	use cf_traits::Permissions;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_session::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// A provider for our candidates
		type CandidateProvider: CandidateProvider<ValidatorId=Self::ValidatorId>;
		
		/// A handler for epoch lifecycle events
		type EpochTransitionHandler: EpochTransitionHandler<ValidatorId=Self::ValidatorId>;

		/// Minimum amount of blocks an epoch can run for
		#[pallet::constant]
		type MinEpoch: Get<<Self as frame_system::Config>::BlockNumber>;

		/// Minimum amount of validators we will want in a set
		#[pallet::constant]
		type MinValidatorSetSize: Get<u64>;

		type ValidatorWeightInfo: WeightInfo;

		type Permissions: Permissions<AccountId=Self::AccountId, Scope=u64>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An auction phase has started \[epoch_index\]
		AuctionStarted(EpochIndex),
		/// A new epoch has started \[epoch_index\]
		NewEpoch(EpochIndex),
		/// The number of blocks has changed for our epoch \[from, to\]
		EpochDurationChanged(T::BlockNumber, T::BlockNumber),
		/// The number of validators in a set has been changed \[from, to\]
		MaximumValidatorsChanged(ValidatorSize, ValidatorSize),
		/// The auction has been confirmed off-chain \[epoch_index\]
		AuctionConfirmed(EpochIndex),
		/// A new auction has been forced
		ForceAuctionRequested(),
	}

	#[pallet::error]
	pub enum Error<T> {
		// TODO We need to handle condition when we have no candidates
		NoValidators,
		/// Epoch block number supplied is invalid
		InvalidEpoch,
		/// Validator set size provided is invalid
		InvalidValidatorSetSize,
		/// Invalid auction index used in confirmation
		InvalidAuction,
		/// Permission required
		PermissionRequired,
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets the number of blocks an epoch should run for
		/// The dispatch origin of this function must be root.
		#[pallet::weight(
			T::ValidatorWeightInfo::set_blocks_for_epoch()
		)]
		pub(super) fn set_blocks_for_epoch(
			origin: OriginFor<T>,
			number_of_blocks: T::BlockNumber,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(number_of_blocks >= T::MinEpoch::get(), Error::<T>::InvalidEpoch);
			let old_epoch = BlocksPerEpoch::<T>::get();
			ensure!(old_epoch != number_of_blocks, Error::<T>::InvalidEpoch);
			BlocksPerEpoch::<T>::set(number_of_blocks);
			Self::deposit_event(Event::EpochDurationChanged(old_epoch, number_of_blocks));
			Ok(().into())
		}

		/// Sets the size of our validate set size
		/// The dispatch origin of this function must be root.
		#[pallet::weight(
			T::ValidatorWeightInfo::set_validator_target_size()
		)]
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

		/// Force an auction phase.  The next block will run an auction.
		/// The dispatch origin of this function must be root.
		#[pallet::weight(
			T::ValidatorWeightInfo::force_auction()
		)]
		pub(super) fn force_auction(
			origin: OriginFor<T>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			Force::<T>::set(true);
			Self::deposit_event(Event::ForceAuctionRequested());
			Ok(().into())
		}

		/// When we are in an auction phase we will need to wait for off-chain confirmation
		/// of the epoch index already emitted with [AuctionStarted]
		/// The dispatch origin of this function must be signed.
		#[pallet::weight(
			T::ValidatorWeightInfo::confirm_auction()
		)]
		pub(super) fn confirm_auction(
			origin: OriginFor<T>,
			index: EpochIndex,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			T::Permissions::has_scope(who, 1);
			ensure!(Some(index) == AuctionToConfirm::<T>::get(), Error::<T>::InvalidAuction);
			AuctionToConfirm::<T>::set(None);
			Self::deposit_event(Event::AuctionConfirmed(index));
			Ok(().into())
		}
	}

	/// Force auction on next block
	#[pallet::storage]
	#[pallet::getter(fn force)]
	pub(super) type Force<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// The starting block number for the current epoch
	#[pallet::storage]
	#[pallet::getter(fn last_block_number)]
	pub(super) type LastBlockNumber<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// The number of blocks an epoch runs for
	#[pallet::storage]
	#[pallet::getter(fn epoch_number_of_blocks)]
	pub(super) type BlocksPerEpoch<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// The maximum number of validators we want
	#[pallet::storage]
	#[pallet::getter(fn max_validators)]
	pub(super) type SizeValidatorSet<T: Config> = StorageValue<_, ValidatorSize, ValueQuery>;

	/// Whether we are in an auction
	#[pallet::storage]
	#[pallet::getter(fn is_auction_phase)]
	pub(super) type IsAuctionPhase<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// Epoch index of auction we are waiting for confirmation for
	#[pallet::storage]
	#[pallet::getter(fn auction_confirmed)]
	pub(super) type AuctionToConfirm<T: Config> = StorageValue<_, EpochIndex, OptionQuery>;

	/// Current epoch index
	#[pallet::storage]
	#[pallet::getter(fn current_epoch)]
	pub(super) type CurrentEpoch<T: Config> = StorageValue<_, EpochIndex, ValueQuery>;

	/// Current bond value
	#[pallet::storage]
	#[pallet::getter(fn current_bond)]
	pub(super) type CurrentBond<T: Config> = StorageValue<_, Stake<T>, ValueQuery>;

	/// Validator lookup
	#[pallet::storage]
	pub(super) type ValidatorLookup<T: Config> = StorageMap< _, Identity, T::ValidatorId, ()>;

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

impl<T:Config> EpochInfo for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = <<T as Config>::CandidateProvider as CandidateProvider>::Stake;
	type EpochIndex = EpochIndex;

	fn current_validators() -> Vec<Self::ValidatorId> {
		<pallet_session::Module<T>>::validators()
	}

	fn next_validators() -> Vec<Self::ValidatorId> {
		if Self::is_auction_phase() {
			return <pallet_session::Module<T>>::queued_keys()
				.into_iter()
				.map(|(k, _)| k)
				.collect()
		}
		vec![]
	}

	fn bond() -> Self::Amount {
		CurrentBond::<T>::get()
	}

	fn epoch_index() -> EpochIndex {
		CurrentEpoch::<T>::get()
	}

	fn is_validator(account: &Self::ValidatorId) -> bool {
		ValidatorLookup::<T>::contains_key(account)
	}
}

impl<T: Config> pallet_session::SessionHandler<T::ValidatorId> for Pallet<T> {

	/// TODO look at the key management
	const KEY_TYPE_IDS: &'static [sp_runtime::KeyTypeId] = &[];
	fn on_genesis_session<Ks: OpaqueKeys>(_validators: &[(T::ValidatorId, Ks)]) {}

	/// A new session has started.  As we are either one of the two states, auction or trading,
	/// we forward the validator set to [EpochTransitionHandler::on_new_auction] or
	/// [EpochTransitionHandler::on_new_epoch]
	fn on_new_session<Ks: OpaqueKeys>(
		_changed: bool,
		validators: &[(T::ValidatorId, Ks)],
		_queued_validators: &[(T::ValidatorId, Ks)],
	) {
		let current_validators = validators.iter()
			.map(|(id, _)| id.clone())
			.collect::<Vec<T::ValidatorId>>();

		if Self::is_auction_phase() {
			T::EpochTransitionHandler::on_new_auction(current_validators);
		} else {
			T::EpochTransitionHandler::on_new_epoch(current_validators);
		}
	}

	/// Triggered before \[`SessionManager::end_session`\] handler
	fn on_before_session_ending() {
		if Self::is_auction_phase() {
			T::EpochTransitionHandler::on_before_epoch_ending();
		} else {
			T::EpochTransitionHandler::on_before_auction();
		}
	}

	/// TODO handle this at some point
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
	/// Prepare candidates for a new session
	fn new_session(new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		debug!("planning new_session({})", new_index);
		Self::new_session(new_index)
	}

	/// The current session is ending
	fn end_session(end_index: SessionIndex) {
		debug!("starting start_session({})", end_index);
		Self::end_session(end_index)
	}

	/// The session is starting
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

impl<T: Config> Pallet<T> {
	/// This returns validators for the *next* session and is called at the *beginning* of the current session.
	///
	/// If we are at the beginning of a non-auction session, the next session will be an auction session, so we return
	/// `None` to indicate that the validator set remains unchanged. Otherwise, the set would be considered changed even 
	/// if the new set of validators matches the old one.  
	///
	/// If we are the beginning of an auction session, we need to run the auction to set the validators for the upcoming
	/// Epoch.
	///
	/// `AuctionStarted` is emitted and the rotation from auction to trading phases will wait on a
	/// confirmation via the `auction_confirmed` extrinsic
	fn new_session(new_index: SessionIndex) -> Option<Vec<T::ValidatorId>> {
		if !Self::is_auction_phase() {
			Self::deposit_event(Event::NewEpoch(new_index.into()));
			CurrentEpoch::<T>::set(new_index.into());
			ValidatorLookup::<T>::remove_all();
			for validator in <pallet_session::Module<T>>::validators() {
				ValidatorLookup::<T>::insert(validator, ());
			}
			return None
		}

		debug!("Creating a new auction-phase session {}", new_index);
		Self::deposit_event(Event::AuctionStarted(new_index.into()));
		AuctionToConfirm::<T>::set(Some(new_index.into()));
		let candidates = T::CandidateProvider::get_candidates();
		let (new_validators, bond) = Self::run_auction(candidates);
		CurrentBond::<T>::set(bond);
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

	pub fn run_auction(mut candidates: Vec<(T::ValidatorId, Stake<T>)>) -> (Option<Vec<T::ValidatorId>>, Stake<T>) {
		// A basic auction algorithm.  We sort by stake amount and take the top of the validator
		// set size and let session pallet do the rest
		// On completing the auction our list of validators and the bond returned
		// Space here to add other prioritisation parameters
		if !candidates.is_empty() {
			candidates.sort_unstable_by_key(|k| k.1);
			candidates.reverse();
			let max_size = SizeValidatorSet::<T>::get();
			let candidates = candidates.get(0..max_size as usize);
			if let Some(candidates) = candidates {
				if let Some((_, bond)) = candidates.last() {
					let candidates: Vec<T::ValidatorId> = candidates.iter().map(|i| i.0.clone()).collect();
					return (Some(candidates), bond.clone());
				}
			}
		}

		(Some(vec![]), Zero::zero())
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
