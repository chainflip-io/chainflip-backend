#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip governance
//!
//! ## Purpose
//!
//! This pallet implements the current Chainflip governance functionality. The purpose of this pallet is primarily
//! to provide the following capabilities:
//!
//! - Handle the set of governance members
//! - Handle submitting proposals
//! - Handle approving proposals
//! - Provide tools to implement governance secured extrinsic in other pallets
//!
//! ## Governance model
//!
//! The governance model is a simple approved system. Every member can propose an extrinsic, which is secured by
//! the EnsureGovernance implementation of the EnsureOrigin trait. Apart from that, every member is allowed to
//! approve a proposed governance extrinsic. If a proposal can raise 2/3 + 1 approvals, it's getting executed by
//! the system automatically. Moreover, every proposal has an expiry date. If a proposal is not able to raise
//! enough approvals in time, it gets dropped and won't be executed.
//!
//! note: For implementation details pls see the readme.

use std::convert::TryInto;

use codec::Decode;
use frame_support::dispatch::GetDispatchInfo;
use frame_support::traits::EnsureOrigin;
use frame_support::traits::UnfilteredDispatchable;
use frame_support::traits::UnixTime;
pub use pallet::*;
use sp_runtime::DispatchError;
use sp_std::vec::Vec;

/// Time span a proposal expires in seconds (currently  5 days)
const EXPIRY_SPAN: u64 = 7200;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
/// Implements the functionality of the Chainflip governance.
#[frame_support::pallet]
pub mod pallet {

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
		/// Encoded representation of a extrinsic
		pub call: OpaqueCall,
		/// Date of creation
		pub created: u64,
		/// Array of accounts which already approved the proposal
		pub approved: Vec<AccountId>,
		/// Boolean value if the extrinsic was executed
		pub executed: bool,
		/// Boolean if the proposal is expired
		pub expired: bool,
	}

	type AccountId<T> = <T as frame_system::Config>::AccountId;
	type OpaqueCall = Vec<u8>;
	type ProposalId = u32;

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
	pub(super) type Proposals<T: Config> = StorageValue<_, Vec<Proposal<T::AccountId>>, ValueQuery>;

	/// Array of accounts which are included in the current governance
	#[pallet::storage]
	#[pallet::getter(fn members)]
	pub(super) type Members<T> = StorageValue<_, Vec<AccountId<T>>, ValueQuery>;

	/// on_initialize hook - check and execute before every block all ongoing proposals
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			let weight = Self::process_proposals();
			weight
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new proposal was submitted [proposal_id]
		Proposed(ProposalId),
		/// A proposal was executed [proposal_id]
		Executed(ProposalId),
		/// A proposal is expired [proposal_id]
		Expired(ProposalId),
		/// The execution of a proposal failed [proposal_id]
		ExecutionFailed(ProposalId),
		/// The decode of the a proposal failed [proposal_id]
		DecodeFailed(ProposalId),
		/// A proposal was approved [proposal_id]
		Approved(ProposalId),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An account already approved a proposal
		AlreadyApproved,
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
			ensure!(<Members<T>>::get().contains(&who), Error::<T>::NoMember);
			// Generate the next proposal id
			let proposal_id = <Proposals<T>>::get().len() as u32;
			// Insert the proposal
			<Proposals<T>>::append(Proposal {
				call: call.encode(),
				created: T::TimeSource::now().as_secs(),
				executed: false,
				expired: false,
				approved: vec![],
			});
			Self::deposit_event(Event::Proposed(proposal_id));
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
			// Ensure origin is part of the governance
			ensure!(<Members<T>>::get().contains(&who), Error::<T>::NoMember);
			// Try to approve the proposal
			Self::try_approve(who, id as usize)?;
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
	/// Processes all ongoing proposals.
	/// Iterates over all proposals and checks if the proposal is ready to execute or expired.
	fn process_proposals() -> u64 {
		let mut weight: u64 = 0;
		let proposals = <Proposals<T>>::get();

		// Iterate over all proposals
		for (index, proposal) in proposals.iter().enumerate() {
			// Check if the proposal was executed or is already expired
			if proposal.executed || proposal.expired {
				continue;
			}
			// Check if the proposal is expired
			if proposal.created + EXPIRY_SPAN < T::TimeSource::now().as_secs() {
				<Proposals<T>>::mutate(|p| {
					p.get_mut(index).unwrap().expired = true;
				});
				Self::deposit_event(Event::Expired(index.try_into().unwrap()));
				continue;
			}
			// Check if the majority is reached
			if !Self::majority_reached(proposal.approved.len()) {
				continue;
			}
			// Execute the proposal
			if let Some(call) = Self::decode_call(&proposal.call) {
				// Sum up the extrinsic weight to the next block weight
				weight = weight.checked_add(call.get_dispatch_info().weight).unwrap();
				let result = call.dispatch_bypass_filter((RawOrigin::GovernanceThreshold).into());
				// Mark the proposal as executed - doesn't matter if successful or not
				<Proposals<T>>::mutate(|p| {
					p.get_mut(index).unwrap().executed = true;
				});
				if result.is_ok() {
					Self::deposit_event(Event::Executed(index.try_into().unwrap()));
				} else {
					Self::deposit_event(Event::ExecutionFailed(index.try_into().unwrap()));
				}
			}
		}
		weight
	}
	/// Calcs the threshold based on the total amount of governance members (current threshold is 1/2 + 1)
	fn calc_threshold(total: u32) -> u32 {
		if total % 2 == 0 {
			total / 2
		} else {
			total / 2 + 1
		}
	}
	/// Checks if the majority for a proposal is reached
	fn majority_reached(approvals: usize) -> bool {
		let total_number_of_voters = <Members<T>>::get().len() as u32;
		let threshold = Self::calc_threshold(total_number_of_voters);
		approvals as u32 >= threshold
	}
	/// Tries to approve a proposal
	fn try_approve(account: T::AccountId, proposal_id: usize) -> Result<(), DispatchError> {
		// Check if proposal exist
		if let Some(proposal) = <Proposals<T>>::get().get(proposal_id) {
			// Check expiry
			if proposal.created + EXPIRY_SPAN < T::TimeSource::now().as_secs() {
				return Err(Error::<T>::AlreadyExpired.into());
			}
			// Check already approved
			if proposal.approved.contains(&account) {
				return Err(Error::<T>::AlreadyApproved.into());
			}
			Self::deposit_event(Event::Approved(proposal_id.try_into().unwrap()));
			// Add the approval
			<Proposals<T>>::mutate(|p| {
				p.get_mut(proposal_id).unwrap().approved.push(account);
			});
			Ok(())
		} else {
			Err(Error::<T>::NotFound.into())
		}
	}
	/// Decodes a encoded representation of a Call
	fn decode_call(call: &Vec<u8>) -> Option<<T as Config>::Call> {
		Decode::decode(&mut &call[..]).ok()
	}
}
