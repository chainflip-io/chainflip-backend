#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip governance
//!
//! TODO: write some nice explanation here
//!

use codec::Decode;
use frame_support::dispatch::GetDispatchInfo;
use frame_support::dispatch::UnfilteredDispatchable;
use frame_support::traits::EnsureOrigin;
use frame_support::traits::UnixTime;
pub use pallet::*;
use sp_runtime::DispatchError;
use sp_std::vec;
use sp_std::vec::Vec;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
#[frame_support::pallet]
pub mod pallet {

	/// Time span a proposal expires in seconds (currently  5 days)
	const EXPIRY_SPAN: u64 = 7200;

	use frame_support::{
		dispatch::GetDispatchInfo,
		pallet_prelude::*,
		traits::{UnfilteredDispatchable, UnixTime},
	};

	use codec::{Encode, FullCodec};
	use frame_system::{pallet, pallet_prelude::*};
	use sp_std::boxed::Box;
	use sp_std::vec;
	use sp_std::vec::Vec;
	/// Proposal struct
	#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
	pub struct Proposal<AccountId> {
		/// Id - key in the proposal map
		pub id: u32,
		/// Encoded representation of a extrinsic
		pub call: OpaqueCall,
		/// Expiry date (in secondes)
		pub expiry: u64,
		/// Numbers of votes for a proposal
		pub votes: u32,
		/// Array of accounts which already approved the proposal
		pub voted: Vec<AccountId>,
		/// Boolean value if the extrinsic was executed
		pub executed: bool,
	}

	type AccountId<T> = <T as frame_system::Config>::AccountId;
	type OpaqueCall = Vec<u8>;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// The outer Origin needs to be compatible with this pallet's Origin
		type Origin: From<RawOrigin>;
		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<<Self as pallet::Config>::Origin>;
		/// The overarching call type.
		type Call: Member
			+ FullCodec
			+ UnfilteredDispatchable<Origin = <Self as Config>::Origin>
			+ GetDispatchInfo;
		/// UnixTime implementation for TimeSource
		type TimeSource: UnixTime;
	}
	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	/// Map of proposals
	#[pallet::storage]
	#[pallet::getter(fn proposals)]
	pub(super) type Proposals<T: Config> =
		StorageMap<_, Blake2_128Concat, u32, Proposal<T::AccountId>, ValueQuery>;

	/// Array of ongoing proposal ids
	#[pallet::storage]
	#[pallet::getter(fn ongoing_proposals)]
	pub type OnGoingProposals<T> = StorageValue<_, Vec<u32>, ValueQuery>;

	/// Total number of submitted proposals
	#[pallet::storage]
	#[pallet::getter(fn number_of_proposals)]
	pub type NumberOfProposals<T> = StorageValue<_, u32>;

	/// Array of accounts which are included in the current governance
	#[pallet::storage]
	#[pallet::getter(fn members)]
	pub(super) type Members<T> = StorageValue<_, Vec<AccountId<T>>, ValueQuery>;

	/// on_initialize hook - check and execute before every block all ongoing proposals
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			let result = Self::process_proposals();
			Self::cleanup(result.0);
			result.1
		}
	}

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new proposal was submitted
		Proposed(u32),
		/// A proposal was executed
		Executed(u32),
		/// A proposal is expired
		Expired(u32),
		/// The execution of a proposal failed
		ExecutionFailed(u32),
		/// The decode of the a proposal failed
		DecodeFailed(u32),
		/// A proposal was approved
		Voted(u32),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An account already voted on a proposal
		AlreadyVoted,
		/// A proposal was already executed
		AlreadyExecuted,
		/// A proposal is already expired
		AlreadyExpired,
		/// The signer of an extrinsic is no member of the current governance
		NoMember,
		/// The proposal was not found in the the proposal map
		NotFound,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Propose a governance ensured extrinsic
		#[pallet::weight(10_000)]
		pub fn propose_governance_extrinsic(
			origin: OriginFor<T>,
			call: Box<<T as Config>::Call>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Ensure origin is part of the governance
			Self::ensure_member(&who)?;
			// Generate the next proposal id
			let proposal_id = Self::next_proposal_id();
			// Insert the proposal
			<Proposals<T>>::insert(
				proposal_id,
				Proposal {
					id: proposal_id,
					call: call.encode(),
					expiry: T::TimeSource::now().as_secs() + EXPIRY_SPAN,
					executed: false,
					votes: 0,
					voted: vec![],
				},
			);
			// Add the proposal to the ongoing proposals
			<OnGoingProposals<T>>::mutate(|proposals| {
				proposals.push(proposal_id);
			});
			Self::deposit_event(Event::Proposed(proposal_id.clone()));
			Ok(().into())
		}
		/// Sets a new set of governance members
		#[pallet::weight(10_000)]
		pub fn new_membership_set(
			origin: OriginFor<T>,
			accounts: Vec<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			// Ensure the extrinsic was executed by the governance
			T::EnsureGovernance::ensure_origin(origin)?;
			<Members<T>>::put(accounts);
			Ok(().into())
		}
		/// Approve a proposal by a given proposal id
		#[pallet::weight(10_000)]
		pub fn approve(origin: OriginFor<T>, id: u32) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::ensure_member(&who)?;
			// Try to approve the proposal
			Self::try_vote(who, id)?;
			Ok(().into())
		}
	}

	/// Genesis definition
	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub members: Vec<AccountId<T>>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				members: Default::default(),
			}
		}
	}

	/// Sets the genesis governance
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			Members::<T>::set(self.members.clone());
		}
	}

	#[pallet::origin]
	pub type Origin = RawOrigin;

	/// The raw origin enum for this pallet.
	#[derive(PartialEq, Eq, Clone, RuntimeDebug, Encode, Decode)]
	pub enum RawOrigin {
		GovernanceThreshold,
	}
}

/// Custom governance origin
pub struct EnsureGovernance;

/// Implementation for EnsureOrigin trait for custom EnsureGovernance struct.
/// We use this to execute extrinsic by a governance origin.
impl<OuterOrigin> EnsureOrigin<OuterOrigin> for EnsureGovernance
where
	OuterOrigin: Into<Result<RawOrigin, OuterOrigin>> + From<RawOrigin>,
{
	type Success = ();

	fn try_origin(o: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
		match o.into() {
			Ok(o) => match o {
				RawOrigin::GovernanceThreshold => Ok(()),
			},
			Err(o) => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn successful_origin() -> OuterOrigin {
		RawOrigin::GovernanceThreshold.into()
	}
}

impl<T: Config> Pallet<T> {
	/// Check if a proposal fits all requirements to get executed
	fn is_proposal_executable(proposal: &Proposal<T::AccountId>) -> bool {
		// majority + not executed + not expired
		Self::majority_reached(proposal.votes)
			&& !proposal.executed
			&& proposal.expiry >= T::TimeSource::now().as_secs()
	}
	/// Processes all ongoing proposals
	fn process_proposals() -> (Vec<usize>, u64) {
		let ongoing_proposals = <OnGoingProposals<T>>::get();
		let mut executed_or_expired: Vec<usize> = vec![];
		let mut weight: u64 = 0;
		// Iterate over all ongoing proposals
		for (index, proposal_id) in ongoing_proposals.iter().enumerate() {
			let proposal = <Proposals<T>>::get(proposal_id);
			// Execute proposal if valid
			if Self::is_proposal_executable(&proposal) {
				// Decode the saved extrinsic
				if let Some(call) = Self::decode_call(proposal.call) {
					// Sum up the extrinsic weight to the block weight
					weight = weight.checked_add(call.get_dispatch_info().weight).unwrap();
					let result =
						call.dispatch_bypass_filter((RawOrigin::GovernanceThreshold).into());
					// Mark the proposal as executed - doesn't matter if successful or not
					<Proposals<T>>::mutate(proposal_id, |proposal| {
						proposal.executed = true;
					});
					executed_or_expired.push(index);
					if result.is_ok() {
						Self::deposit_event(Event::Executed(proposal_id.clone()));
					} else {
						Self::deposit_event(Event::ExecutionFailed(proposal_id.clone()));
					}
				}
				continue;
			}
			// Check if proposal is expired
			if proposal.expiry < T::TimeSource::now().as_secs() {
				executed_or_expired.push(index);
				Self::deposit_event(Event::Expired(proposal_id.clone()));
			}
		}
		(executed_or_expired, weight)
	}
	/// Removes ongoing proposals
	fn cleanup(proposals: Vec<usize>) {
		<OnGoingProposals<T>>::mutate(|ongoing_proposals| {
			for i in proposals {
				ongoing_proposals.remove(i);
			}
		});
	}
	/// Calcs the threshold based on the total amount of governance members (current threshold is 2/3 + 1)
	fn calc_threshold(total: u32) -> u32 {
		let doubled = total * 2;
		if doubled % 3 == 0 {
			doubled / 3
		} else {
			doubled / 3 + 1
		}
	}
	/// Checks if the majority for a proposal is reached
	fn majority_reached(votes: u32) -> bool {
		let total_number_of_voters = <Members<T>>::get().len() as u32;
		let threshold = Self::calc_threshold(total_number_of_voters);
		votes >= threshold
	}
	/// Ensures that the account is a member of the governance
	fn ensure_member(account: &T::AccountId) -> Result<(), DispatchError> {
		if !<Members<T>>::get().contains(account) {
			Err(Error::<T>::NoMember.into())
		} else {
			Ok(())
		}
	}
	/// Generates the next proposal id
	fn next_proposal_id() -> u32 {
		//TODO: refactor needed here
		if let Some(number_of_proposals) = <NumberOfProposals<T>>::get() {
			let next_id = number_of_proposals + 1;
			<NumberOfProposals<T>>::put(next_id);
			next_id
		} else {
			<NumberOfProposals<T>>::put(0);
			0
		}
	}
	/// Tries to vote a proposal
	fn try_vote(account: T::AccountId, proposal_id: u32) -> Result<(), DispatchError> {
		// Check if proposal exist
		if !<Proposals<T>>::contains_key(proposal_id) {
			return Err(Error::<T>::NotFound.into());
		}
		let proposal = <Proposals<T>>::get(proposal_id);
		// Check if already executed
		if proposal.executed {
			return Err(Error::<T>::AlreadyExecuted.into());
		}
		// Check expiry
		if proposal.expiry < T::TimeSource::now().as_secs() {
			return Err(Error::<T>::AlreadyExpired.into());
		}
		// Check already voted
		if proposal.voted.contains(&account) {
			return Err(Error::<T>::AlreadyVoted.into());
		}
		<Proposals<T>>::mutate(proposal_id, |proposal| {
			proposal.voted.push(account);
			proposal.votes = proposal.votes.checked_add(1).unwrap();
		});
		Self::deposit_event(Event::Voted(proposal_id.clone()));
		Ok(())
	}
	/// Decodes a encoded representation of a Call
	fn decode_call(call: Vec<u8>) -> Option<<T as Config>::Call> {
		Decode::decode(&mut &call[..]).ok()
	}
}
