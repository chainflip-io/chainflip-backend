#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip Validator Module
//!
//! A module to manage the validator set for the Chainflip State Chain
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Module`]
//!
//! ## Overview
//!
//! The module contains functionality to manage the validator set used to ensure the Chainflip
//! State Chain network.  It extends on the functionality offered by the `session` pallet provided by
//! Parity.  There are two types of sessions; an Epoch session in which we have a constant set of validators
//! and an Auction session in which we continue with our current validator set and request a set of
//! candidates for validation.  Once validated and confirmed become our new set of validators within the
//! Epoch session.
//!
//! ## Terminology
//!
//! - **Validator:** A node that has staked an amount of `FLIP` ERC20 token.
//!
//! - **Validator ID:** Equivalent to an Account ID
//!
//! - **Epoch:** A period in blocks in which a constant set of validators ensure the network.
//!
//! - **Auction** A non defined period of blocks in which we continue with the existing validators
//!   and assess the new candidate set of their validity as validators.  This period is closed when
//!   `confirm_auction` is called and the candidate set are now the new validating set.
//!
//! - **Session:** A session as defined by the `session` pallet. We have two sessions; Epoch which has
//!   a fixed number of blocks set with `set_blocks_for_epoch` and an Auction session which is of an
//!   undetermined number of blocks.
//!
//! - **Sudo:** A single account that is also called the "sudo key" which allows "privileged functions"
//!
//! ### Dispatchable Functions
//!
//! - `set_blocks_for_epoch` - Set the number of blocks an Epoch should run for.
//! - `set_validator_target_size` - Set the target size for a validator set.
//! - `force_auction` - Force an auction to start on the next block.
//! - `confirm_auction` - Confirm that any dependencies for the auction have been confirmed.
//!

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub use pallet::*;
use sp_runtime::traits::{Convert, OpaqueKeys, AtLeast32BitUnsigned, One};
use sp_std::prelude::*;
use frame_support::sp_runtime::traits::{Saturating, Zero};
use log::{debug};
use frame_support::pallet_prelude::*;
use cf_traits::{EpochInfo, Auction, CandidateProvider, ValidatorSet, ValidatorProposal, AuctionError};
use serde::{Serialize, Deserialize};
use frame_support::traits::ValidatorRegistration;
use sp_std::cmp::min;

pub trait WeightInfo {
	fn set_blocks_for_epoch() -> Weight;
	fn set_validator_target_size() -> Weight;
	fn force_auction() -> Weight;
	fn confirm_auction() -> Weight;
}

pub type ValidatorSize = u32;
type SessionIndex = u32;

/// Handler for Epoch life cycle events.
pub trait EpochTransitionHandler {
	/// The id type used for the validators.
	type ValidatorId;

	/// A new epoch has started
	///
	/// The new set of validator `new_validators` are now validating
	fn on_new_epoch(_new_validators: Vec<Self::ValidatorId>) {}

	/// We have entered an auction phase
	///
	/// The existing validators remain validating and these are shared as `outgoing_validators`
	/// Obviously the new set of candidates for the auction would be very similar if not
	/// the same as the outgoing set
	fn on_new_auction(_outgoing_validators: Vec<Self::ValidatorId>) {}

	/// Triggered before the end of the trading phase and the start of the auction.
	fn on_before_auction() {}

	/// Triggered after the end of the auction, before a new Epoch.
	fn on_before_epoch_ending() {}
}

impl<T: pallet_session::Config> EpochTransitionHandler for PhantomData<T> {
	type ValidatorId = T::ValidatorId;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;
	use frame_support::sp_runtime::SaturatedConversion;
	use pallet_session::WeightInfo as SessionWeightInfo;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_session::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// A provider for our candidates
		type CandidateProvider: CandidateProvider<ValidatorId=Self::ValidatorId, Amount=Self::Amount>;

		/// A handler for epoch lifecycle events
		type EpochTransitionHandler: EpochTransitionHandler<ValidatorId=Self::ValidatorId>;

		/// Minimum amount of blocks an epoch can run for
		#[pallet::constant]
		type MinEpoch: Get<<Self as frame_system::Config>::BlockNumber>;

		/// Minimum amount of validators we will want in a set
		#[pallet::constant]
		type MinValidatorSetSize: Get<u64>;

		type ValidatorWeightInfo: WeightInfo;

		type EpochIndex: Member 
			+ codec::FullCodec 
			+ Copy 
			+ AtLeast32BitUnsigned 
			+ Default;
		
		type Amount: Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;

		type Registrar: ValidatorRegistration<Self::ValidatorId>;

		type Auction: Auction<ValidatorId=Self::ValidatorId, Amount=Self::Amount, Registrar=Self::Registrar>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An auction phase has started \[epoch_index\]
		AuctionStarted(T::EpochIndex),
		/// A new epoch has started \[epoch_index\]
		NewEpoch(T::EpochIndex),
		/// The number of blocks has changed for our epoch \[from, to\]
		EpochDurationChanged(T::BlockNumber, T::BlockNumber),
		/// The number of validators in a set has been changed \[from, to\]
		MaximumValidatorsChanged(ValidatorSize, ValidatorSize),
		/// The auction has been confirmed off-chain \[epoch_index\]
		AuctionConfirmed(T::EpochIndex),
		/// A new auction has been forced
		ForceAuctionRequested(),
		/// An auction has not started
		AuctionNonStarter(T::EpochIndex),
		/// An auction has not completed
		AuctionNotCompleted(AuctionError)
	}

	#[pallet::error]
	pub enum Error<T> {
		NoValidators,
		/// Epoch block number supplied is invalid
		InvalidEpoch,
		/// Validator set size provided is invalid
		InvalidValidatorSetSize,
		/// Invalid auction index used in confirmation
		InvalidAuction,
		/// FailedForceAuction
		FailedForceAuction,
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {

		/// Sets the number of blocks an epoch should run for
		///
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
		///
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
		///
		/// The dispatch origin of this function must be root.
		#[pallet::weight(
			T::ValidatorWeightInfo::force_auction()
		)]
		pub(super) fn force_auction(
			origin: OriginFor<T>,
		) -> DispatchResultWithPostInfo {
			ensure!(!Self::is_auction_phase(), Error::<T>::FailedForceAuction);
			ensure_root(origin)?;
			Force::<T>::set(true);
			Self::deposit_event(Event::ForceAuctionRequested());
			Ok(().into())
		}

		/// When we are in an auction phase we will need to wait for a confirmation
		/// of the epoch index already emitted with [AuctionStarted]
		///
		/// The dispatch origin of this function must be signed.
		#[pallet::weight(
			T::ValidatorWeightInfo::confirm_auction()
		)]
		pub(super) fn confirm_auction(
			origin: OriginFor<T>,
			index: T::EpochIndex,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			ensure!(Some(index) == AuctionToConfirm::<T>::get(), Error::<T>::InvalidAuction);
			AuctionToConfirm::<T>::set(None);
			Self::deposit_event(Event::AuctionConfirmed(index));
			Ok(().into())
		}

		/// Allow a validator to set their keys for upcoming sessions
		///
		/// The dispatch origin of this function must be signed.
		#[pallet::weight(<T as pallet_session::Config>::WeightInfo::set_keys())]
		pub(super) fn set_keys(origin: OriginFor<T>, keys: T::Keys, proof: Vec<u8>) -> DispatchResultWithPostInfo {
			<pallet_session::Module<T>>::set_keys(origin, keys, proof)?;
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
	#[pallet::getter(fn auction_to_confirm)]
	pub(super) type AuctionToConfirm<T: Config> = StorageValue<_, T::EpochIndex, OptionQuery>;

	/// Current epoch index
	#[pallet::storage]
	#[pallet::getter(fn current_epoch)]
	pub(super) type CurrentEpoch<T: Config> = StorageValue<_, T::EpochIndex, ValueQuery>;

	/// Current bond value
	#[pallet::storage]
	#[pallet::getter(fn current_bond)]
	pub(super) type CurrentBond<T: Config> = StorageValue<_, T::Amount, ValueQuery>;

	/// Validator lookup
	#[pallet::storage]
	pub(super) type ValidatorLookup<T: Config> = StorageMap<_, Identity, T::ValidatorId, ()>;

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

impl<T: Config> EpochInfo for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;
	type EpochIndex = T::EpochIndex;

	fn current_validators() -> Vec<Self::ValidatorId> {
		<pallet_session::Module<T>>::validators()
	}

	fn next_validators() -> Vec<Self::ValidatorId> {
		if Self::is_auction_phase() {
			return <pallet_session::Module<T>>::queued_keys()
				.into_iter()
				.map(|(k, _)| k)
				.collect();
		}

		vec![]
	}

	fn bond() -> Self::Amount {
		CurrentBond::<T>::get()
	}

	fn epoch_index() -> Self::EpochIndex {
		CurrentEpoch::<T>::get()
	}

	fn is_validator(account: &Self::ValidatorId) -> bool {
		ValidatorLookup::<T>::contains_key(account)
	}

	fn is_auction_phase() -> bool {
		Self::is_auction_phase()
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
		Self::new_session()
	}

	/// The current session is ending
	fn end_session(end_index: SessionIndex) {
		debug!("ending end_session({})", end_index);
		Self::end_session()
	}

	/// The session is starting
	fn start_session(start_index: SessionIndex) {
		debug!("starting start_session({})", start_index);
		Self::start_session();
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

impl<T: Config> Pallet<T> {
	/// This returns validators for the *next* session and is called at the *beginning* of the current session.
	///
	/// If we are at the beginning of an epoch session, the next session will be an auction session, so we return
	/// `None` to indicate that the validator set remains unchanged. Otherwise, the set would be considered changed even 
	/// if the new set of validators matches the old one.
	///
	/// If we are the beginning of an auction session, we need to run the auction to set the validators for the upcoming
	/// Epoch.
	///
	/// `AuctionStarted` is emitted and the rotation from auction to trading phases will wait on a
	/// confirmation via the `auction_to_confirm` extrinsic
	fn new_session() -> Option<Vec<T::ValidatorId>> {
		let epoch_index = CurrentEpoch::<T>::get();
		if !Self::is_auction_phase() {
			Self::deposit_event(Event::NewEpoch(epoch_index));
			ValidatorLookup::<T>::remove_all();
			for validator in <pallet_session::Module<T>>::validators() {
				ValidatorLookup::<T>::insert(validator, ());
			}
			return None;
		}

		match T::Auction::validate_auction(T::CandidateProvider::get_candidates())
			.and_then(T::Auction::run_auction)
			.and_then(|proposal| T::Auction::complete_auction(proposal)) {
			Ok(proposal) => {
				Self::deposit_event(Event::AuctionStarted(epoch_index));
				AuctionToConfirm::<T>::set(Some(epoch_index));
				CurrentBond::<T>::set(proposal.1);
				Some(proposal.0)
			},
			Err(e) => {
				debug!("AuctionError: {:?}", e);
				Self::deposit_event(Event::AuctionNotCompleted(e));
				None
			},
		}
	}

	/// The end of the session is triggered, we alternate between epoch sessions and auction sessions.
	fn end_session() {
		let epoch_index = CurrentEpoch::<T>::get();
		IsAuctionPhase::<T>::mutate(|is_auction| {
			if *is_auction {
				debug!("Ending the auction session {:?}", epoch_index);
				CurrentEpoch::<T>::set(epoch_index + One::one());
			} else {
				debug!("Ending the trading session {:?}", epoch_index);
			}
			*is_auction = !*is_auction;
		});
	}

	fn start_session() {
		debug!("Starting a new session");
	}

	/// Check if we have a forced session for this block.  If not, if we are in the "auction" phase
	/// then we would rotate only with a confirmation of that auction else we would count blocks to
	/// see if the epoch has come to end
	pub fn should_end_session(now: T::BlockNumber) -> bool {
		if Force::<T>::get() {
			Force::<T>::set(false);
			return true;
		}

		if Self::is_auction_phase() {
			Self::auction_to_confirm().is_none()
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

	/// As we don't know when we will get confirmation on an auction we will need to return `None`
	pub fn estimate_next_session_rotation(_now: T::BlockNumber) -> Option<T::BlockNumber> {
		None
	}
}

impl<T: Config> Auction for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type Amount = T::Amount;
	type Registrar = T::Registrar;

	fn validate_auction(mut candidates: ValidatorSet<Self>) -> Result<ValidatorSet<Self>, AuctionError> {
		// Set of rules to validate validators
		// Rule #1 - If we have a stake at 0 then please leave
		candidates.retain(|(_, amount)| !amount.is_zero());
		// Rule #2 - They are registered
		candidates.retain(|(id, _)| Self::Registrar::is_registered(id));
		// Rule #3 - If we have less than our min set size we return an empty vector
		if (candidates.len() as u64) < T::MinValidatorSetSize::get() { return Err(AuctionError::MinValidatorSize) };

		Ok(candidates)
	}

	fn run_auction(mut candidates: ValidatorSet<Self>) -> Result<ValidatorProposal<Self>, AuctionError> {
		// A basic auction algorithm.  We sort by stake amount and take the top of the validator
		// set size and let session pallet do the rest
		// On completing the auction our list of validators and the bond returned
		// Space here to add other prioritisation parameters
		if !candidates.is_empty() {
			candidates.sort_unstable_by_key(|k| k.1);
			candidates.reverse();
			let max_size = min(SizeValidatorSet::<T>::get(), candidates.len() as u32);
			let candidates = candidates.get(0..max_size as usize);
			if let Some(candidates) = candidates {
				if let Some((_, bond)) = candidates.last() {
					let candidates: Vec<T::ValidatorId> = candidates.iter().map(|i| i.0.clone()).collect();
					return Ok((candidates, bond.clone()));
				}
			}
		}

		Err(AuctionError::Empty)
	}

	fn complete_auction(proposal: ValidatorProposal<Self>) -> Result<ValidatorProposal<Self>, AuctionError> {
		// Rule #1 - we end up with a bond of 0 so we abort
		if proposal.1.is_zero() {
			return Err(AuctionError::BondIsZero);
		}

		// Rule #... more rules here

		Ok(proposal)
	}
}
