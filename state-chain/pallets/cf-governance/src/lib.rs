#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extended_key_value_attributes)]
#![doc = include_str!("../README.md")]

use codec::Decode;
use frame_support::traits::EnsureOrigin;
use frame_support::traits::UnfilteredDispatchable;
pub use pallet::*;
use sp_runtime::DispatchError;
use sp_std::ops::Add;
use sp_std::vec::Vec;

const FIVE_DAYS_IN_SECONDS: u64 = 432000;

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

	use crate::FIVE_DAYS_IN_SECONDS;

	pub type ActiveProposal = (ProposalId, Timestamp);
	/// Proposal struct
	#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
	pub struct Proposal<AccountId> {
		/// Encoded representation of a extrinsic
		pub call: OpaqueCall,
		/// Array of accounts which already approved the proposal
		pub approved: Vec<AccountId>,
	}

	type AccountId<T> = <T as frame_system::Config>::AccountId;
	type OpaqueCall = Vec<u8>;
	type Timestamp = u64;
	pub type ProposalId = u32;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// The outer Origin needs to be compatible with this pallet's Origin
		type Origin: From<RawOrigin>
			+ From<frame_system::RawOrigin<<Self as frame_system::Config>::AccountId>>;
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

	/// Proposals
	#[pallet::storage]
	#[pallet::getter(fn proposals)]
	pub(super) type Proposals<T: Config> =
		StorageMap<_, Blake2_128Concat, ProposalId, Proposal<T::AccountId>, ValueQuery>;

	/// Active proposals
	#[pallet::storage]
	#[pallet::getter(fn active_proposals)]
	pub(super) type ActiveProposals<T> = StorageValue<_, Vec<ActiveProposal>, ValueQuery>;

	/// Count how many proposals ever has been submitted
	#[pallet::storage]
	#[pallet::getter(fn number_of_proposals)]
	pub(super) type ProposalCount<T> = StorageValue<_, u32, ValueQuery>;

	/// Time in seconds after a proposal expires
	#[pallet::storage]
	#[pallet::getter(fn expiry_span)]
	pub(super) type ExpiryTime<T> = StorageValue<_, Timestamp, ValueQuery>;

	/// Array of accounts which are included in the current governance
	#[pallet::storage]
	#[pallet::getter(fn members)]
	pub(super) type Members<T> = StorageValue<_, Vec<AccountId<T>>, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// on_initialize hook - check the ActiveProposals
		/// and remove the expired ones for house keeping
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// Check if their are any ongoing proposals
			match <ActiveProposals<T>>::decode_len() {
				Some(proposal_len) if proposal_len > 0 => {
					// Separate the proposals into expired an active by partitioning
					let (expired, active): (Vec<ActiveProposal>, Vec<ActiveProposal>) =
						<ActiveProposals<T>>::get()
							.iter()
							.partition(|p| p.1 <= T::TimeSource::now().as_secs());
					let number_of_expired_proposals = expired.len();
					// Remove expired proposals
					for expired_proposal in expired {
						<Proposals<T>>::remove(expired_proposal.0);
						Self::deposit_event(Event::Expired(expired_proposal.0));
					}
					<ActiveProposals<T>>::set(active);
					// Weight is 1 reads + (n + 1) * writes
					T::DbWeight::get().reads(1)
						+ T::DbWeight::get().writes(number_of_expired_proposals as u64 + 1)
				}
				_ => T::DbWeight::get().reads(1),
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new proposal was submitted \[proposal_id\]
		Proposed(ProposalId),
		/// A proposal was executed \[proposal_id\]
		Executed(ProposalId),
		/// A proposal is expired \[proposal_id\]
		Expired(ProposalId),
		/// A proposal was approved \[proposal_id\]
		Approved(ProposalId),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An account already approved a proposal
		AlreadyApproved,
		/// The signer of an extrinsic is no member of the current governance
		NotMember,
		/// The proposal was not found - it may have expired or it may already be executed
		ProposalNotFound,
		/// Decode of call failed
		DecodeOfCallFailed,
		/// The majority was not reached when the execution was triggered
		MajorityNotReached,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Propose a governance ensured extrinsic.
		///
		/// ## Events
		///
		/// - [Proposed](Event::Proposed): Successfully proposed the extrinsic to Governance Members.
		///
		/// ## Errors
		///
		/// - [NotMember](Error::NotMember): The caller is not a Governance Member.
		#[pallet::weight(10_000)]
		pub fn propose_governance_extrinsic(
			origin: OriginFor<T>,
			call: Box<<T as Config>::Call>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Ensure origin is part of the governance
			ensure!(<Members<T>>::get().contains(&who), Error::<T>::NotMember);
			// Generate the next proposal id
			let id = Self::get_next_id();
			// Insert a new proposal
			<Proposals<T>>::insert(
				id,
				Proposal {
					call: call.encode(),
					approved: vec![],
				},
			);
			// Update the proposal counter
			<ProposalCount<T>>::put(id);
			// Add the proposal to the active proposals array
			<ActiveProposals<T>>::append((
				id,
				T::TimeSource::now().as_secs() + <ExpiryTime<T>>::get(),
			));
			Self::deposit_event(Event::Proposed(id));
			// Governance member don't pay fees
			Ok(Pays::No.into())
		}

		/// **Can only be called via the Governance Origin**
		///
		/// Sets a new set of governance members. Note that this can be called with an empty vector
		/// to remove the possibility to govern the chain at all.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - [BadOrigin](Error::BadOrigin): The caller is not the Governance Origin.
		#[pallet::weight(10_000)]
		pub fn new_membership_set(
			origin: OriginFor<T>,
			accounts: Vec<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			// Ensure the extrinsic was executed by the governance
			T::EnsureGovernance::ensure_origin(origin)?;
			// Set the new members of the governance
			<Members<T>>::put(accounts);
			Ok(().into())
		}

		/// Approve a Proposal.
		///
		/// ## Events
		///
		/// - [Approved](Event::Approved): The Proposal was successfully approved.
		///
		/// ## Errors
		///
		/// - [NotMember](Error::NotMember): The caller is not a Governance Member.
		/// - [ProposalNotFound](Error::ProposalNotFound): There is no Proposal with this ID.
		/// - [AlreadyApproved](Error::AlreadyApproved): This Governance Member has already approved this Proposal.
		#[pallet::weight(10_000)]
		pub fn approve(origin: OriginFor<T>, id: ProposalId) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Ensure origin is part of the governance
			ensure!(<Members<T>>::get().contains(&who), Error::<T>::NotMember);
			// Ensure that the proposal exists
			ensure!(
				<Proposals<T>>::contains_key(id),
				Error::<T>::ProposalNotFound
			);
			// Try to approve the proposal
			Self::try_approve(who, id)?;
			// Governance members don't pay transaction fees
			Ok(Pays::No.into())
		}

		/// Execute a Proposal.
		///
		/// ## Events
		///
		/// - [Executed](Event::Executed): The Proposal was successfully executed.
		///
		/// ## Errors
		///
		/// - [NotMember](Error::NotMember): the caller is not a Governance Member.
		/// - [ProposalNotFound](Error::ProposalNotFound): there is no Proposal with this `id`.
		/// - [DecodeOfCallFailed](Error::DecodeOfCallFailed): the call is not a valid extrinsic submission.
		/// - [MajorityNotReached](Error::MajorityNotReached): the Proposal has not achieved Quorum.
		#[pallet::weight(10_000)]
		pub fn execute(origin: OriginFor<T>, id: ProposalId) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Ensure origin is part of the governance
			ensure!(<Members<T>>::get().contains(&who), Error::<T>::NotMember);
			// Ensure that the proposal exists
			ensure!(
				<Proposals<T>>::contains_key(id),
				Error::<T>::ProposalNotFound
			);
			// Try to execute the proposal
			Self::execute_proposal(id)?;
			// Governance member don't pay fees
			Ok(Pays::No.into())
		}

		/// **Can only be called via the Governance Origin**
		///
		/// Execute an extrinsic as root
		///
		/// ## Errors
		///
		/// - [BadOrigin](Error::BadOrigin): the caller is not the Governance Origin.
		#[pallet::weight(10_000)]
		pub fn call_as_sudo(
			origin: OriginFor<T>,
			call: Box<<T as Config>::Call>,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			// Execute the root call
			call.dispatch_bypass_filter(frame_system::RawOrigin::Root.into())
		}
	}

	/// Genesis definition
	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub members: Vec<AccountId<T>>,
		pub expiry_span: u64,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				members: Default::default(),
				expiry_span: FIVE_DAYS_IN_SECONDS,
			}
		}
	}

	/// Sets the genesis governance
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			Members::<T>::set(self.members.clone());
			ExpiryTime::<T>::set(self.expiry_span);
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
	/// Returns the next proposal id
	fn get_next_id() -> ProposalId {
		<ProposalCount<T>>::get().add(1)
	}
	/// Executes an proposal if the majority is reached
	fn execute_proposal(id: ProposalId) -> Result<(), DispatchError> {
		let proposal = <Proposals<T>>::get(id);
		if Self::majority_reached(proposal.approved.len()) {
			// Try to decode the stored extrinsic
			if let Some(call) = Self::decode_call(&proposal.call) {
				// Execute the extrinsic
				let result = call.dispatch_bypass_filter((RawOrigin::GovernanceThreshold).into());
				// Check the result and emit events
				if result.is_ok() {
					Self::deposit_event(Event::Executed(id));
				} else {
					// Get the error during the execution and return it
					return Err(result.unwrap_err().error);
				}
				// Remove the proposal from storage
				<Proposals<T>>::remove(id);
				// Remove the proposal from active proposals
				let active_proposals = <ActiveProposals<T>>::get();
				let new_active_proposals = active_proposals
					.iter()
					.filter(|x| x.0 != id)
					.cloned()
					.collect::<Vec<_>>();
				// Set the new active proposals
				<ActiveProposals<T>>::set(new_active_proposals);
				Ok(())
			} else {
				// Emit an event if the decode of a call failed
				return Err(Error::<T>::DecodeOfCallFailed.into());
			}
		} else {
			return Err(Error::<T>::MajorityNotReached.into());
		}
	}
	/// Checks if the majority for a proposal is reached
	fn majority_reached(approvals: usize) -> bool {
		approvals > <Members<T>>::decode_len().unwrap_or_default() / 2
	}
	/// Tries to approve a proposal
	fn try_approve(account: T::AccountId, id: u32) -> Result<(), DispatchError> {
		<Proposals<T>>::mutate(id, |proposal| {
			// Check already approved
			if proposal.approved.contains(&account) {
				return Err(Error::<T>::AlreadyApproved.into());
			}
			// Add account to approved array
			proposal.approved.push(account);
			Self::deposit_event(Event::Approved(id));
			Ok(())
		})
	}
	/// Decodes a encoded representation of a Call
	/// Returns None if the encode of the extrinsic has failed
	fn decode_call(call: &Vec<u8>) -> Option<<T as Config>::Call> {
		Decode::decode(&mut &call[..]).ok()
	}
}
