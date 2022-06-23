#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use codec::{Decode, Encode};
use frame_support::{
	dispatch::{GetDispatchInfo, UnfilteredDispatchable, Weight},
	traits::{EnsureOrigin, UnixTime},
};
pub use pallet::*;
use sp_runtime::DispatchError;
use sp_std::{boxed::Box, ops::Add, vec, vec::Vec};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

mod old_storage {
	frame_support::generate_storage_alias!(Governance, ProposalCount => Value<u32>);

	pub fn take_old_id() -> Option<u32> {
		ProposalCount::take()
	}
}

/// Hash over (call, nonce, runtime_version)
pub type GovCallHash = [u8; 32];

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
/// Implements the functionality of the Chainflip governance.
#[frame_support::pallet]
pub mod pallet {
	use cf_traits::{Chainflip, ExecutionCondition, RuntimeUpgrade};
	use frame_support::{
		dispatch::GetDispatchInfo,
		error::BadOrigin,
		pallet_prelude::*,
		traits::{UnfilteredDispatchable, UnixTime},
	};

	use codec::Encode;
	use frame_system::{pallet, pallet_prelude::*};
	use sp_std::{boxed::Box, vec::Vec};

	use crate::{GovCallHash, WeightInfo};

	pub type ActiveProposal = (ProposalId, Timestamp);
	/// Proposal struct
	#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct Proposal<AccountId> {
		/// Encoded representation of a extrinsic
		pub call: OpaqueCall,
		/// Array of accounts which already approved the proposal
		pub approved: Vec<AccountId>,
	}

	impl<T> Default for Proposal<T> {
		fn default() -> Self {
			Self { call: Default::default(), approved: Default::default() }
		}
	}

	type AccountId<T> = <T as frame_system::Config>::AccountId;
	type OpaqueCall = Vec<u8>;
	type Timestamp = u64;
	pub type ProposalId = u32;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// The outer Origin needs to be compatible with this pallet's Origin
		type Origin: From<RawOrigin>
			+ From<frame_system::RawOrigin<<Self as frame_system::Config>::AccountId>>;
		/// Implementation of EnsureOrigin trait for governance
		type EnsureGovernance: EnsureOrigin<<Self as pallet::Config>::Origin>;
		/// The overarching call type.
		type Call: Member
			+ Parameter
			+ UnfilteredDispatchable<Origin = <Self as Config>::Origin>
			+ From<frame_system::Call<Self>>
			+ From<Call<Self>>
			+ GetDispatchInfo;
		/// UnixTime implementation for TimeSource
		type TimeSource: UnixTime;
		/// Benchmark weights
		type WeightInfo: WeightInfo;
		/// Provides the logic if a runtime can be performed
		type UpgradeCondition: ExecutionCondition;
		/// Provides to implementation for a runtime upgrade
		type RuntimeUpgrade: RuntimeUpgrade;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	/// Proposals.
	#[pallet::storage]
	#[pallet::getter(fn proposals)]
	pub(super) type Proposals<T: Config> =
		StorageMap<_, Blake2_128Concat, ProposalId, Proposal<T::AccountId>, ValueQuery>;

	/// Active proposals.
	#[pallet::storage]
	#[pallet::getter(fn active_proposals)]
	pub(super) type ActiveProposals<T> = StorageValue<_, Vec<ActiveProposal>, ValueQuery>;

	/// Call hash that has been committed to by the Governance Key.
	#[pallet::storage]
	#[pallet::getter(fn gov_key_whitelisted_call_hash)]
	pub(super) type GovKeyWhitelistedCallHash<T> = StorageValue<_, GovCallHash, OptionQuery>;

	/// Any nonces before this have been consumed.
	#[pallet::storage]
	#[pallet::getter(fn next_gov_key_call_hash_nonce)]
	pub(super) type NextGovKeyCallHashNonce<T> = StorageValue<_, u32, ValueQuery>;

	/// Number of proposals that have been submitted.
	#[pallet::storage]
	#[pallet::getter(fn proposal_id_counter)]
	pub(super) type ProposalIdCounter<T> = StorageValue<_, u32, ValueQuery>;

	/// Pipeline of proposals which will get executed in the next block.
	#[pallet::storage]
	#[pallet::getter(fn execution_pipeline)]
	pub(super) type ExecutionPipeline<T> =
		StorageValue<_, Vec<(OpaqueCall, ProposalId)>, ValueQuery>;

	/// Time in seconds until a proposal expires.
	#[pallet::storage]
	#[pallet::getter(fn expiry_span)]
	pub(super) type ExpiryTime<T> = StorageValue<_, Timestamp, ValueQuery>;

	/// Array of accounts which are included in the current governance.
	#[pallet::storage]
	#[pallet::getter(fn members)]
	pub(super) type Members<T> = StorageValue<_, Vec<AccountId<T>>, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// on_initialize hook - check the ActiveProposals
		/// and remove the expired ones for house keeping
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// Check expiry and expire the proposals if needed
			let active_proposal_weight = Self::check_expiry();
			// Execute all proposals which reached threshold in the last block
			let execution_weight = Self::execute_proposals();
			active_proposal_weight + execution_weight
		}

		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			if let Some(old) = crate::old_storage::take_old_id() {
				ProposalIdCounter::<T>::put(old);
			}
			0
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
		/// The execution of a proposal failed \[dispatch_error\]
		FailedExecution(DispatchError),
		/// The decode of call failed \[proposal_id\]
		DecodeOfCallFailed(ProposalId),
		/// The upgrade conditions for a runtime upgrade were satisfied
		UpgradeConditionsSatisfied,
		/// Call executed by GovKey
		GovKeyCallExecuted { call_hash: GovCallHash },
		/// CallHash whitelisted by the GovKey
		GovKeyCallHashWhitelisted { call_hash: GovCallHash },
		/// Failed GovKey call
		GovKeyCallExecutionFailed { call_hash: GovCallHash },
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
		/// A runtime upgrade has failed because the upgrade conditions were not satisfied
		UpgradeConditionsNotMet,
		/// A runtime upgrade was not successful
		UpgradeHasFailed,
		/// The call hash was not whitelisted
		CallHashNotWhitelisted,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Propose a governance ensured extrinsic
		///
		/// ## Events
		///
		/// - [Proposed](Event::Proposed)
		///
		/// ## Errors
		///
		/// - [NotMember](Error::NotMember)
		#[pallet::weight(T::WeightInfo::propose_governance_extrinsic())]
		pub fn propose_governance_extrinsic(
			origin: OriginFor<T>,
			call: Box<<T as Config>::Call>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Ensure origin is part of the governance
			ensure!(Members::<T>::get().contains(&who), Error::<T>::NotMember);
			// Push proposal
			let id = Self::push_proposal(call);
			Self::deposit_event(Event::Proposed(id));
			// Governance member don't pay fees
			Ok(Pays::No.into())
		}

		/// Sets a new set of governance members
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
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::new_membership_set())]
		pub fn new_membership_set(
			origin: OriginFor<T>,
			accounts: Vec<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			// Ensure the extrinsic was executed by the governance
			T::EnsureGovernance::ensure_origin(origin)?;
			// Set the new members of the governance
			Members::<T>::put(accounts);
			Ok(().into())
		}

		/// Performs a runtime upgrade of the Chainflip runtime
		/// **Can only be called via the Governance Origin**
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		/// - [UpgradeConditionsNotMet](Error::UpgradeConditionsNotMet)
		#[pallet::weight((T::BlockWeights::get().max_block, DispatchClass::Operational))]
		pub fn chainflip_runtime_upgrade(
			origin: OriginFor<T>,
			code: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			// Ensure the extrinsic was executed by the governance
			T::EnsureGovernance::ensure_origin(origin)?;
			// Ensure execution conditions
			ensure!(T::UpgradeCondition::is_satisfied(), Error::<T>::UpgradeConditionsNotMet);
			// Emit an additional event
			Self::deposit_event(Event::UpgradeConditionsSatisfied);
			// Do the runtime upgrade
			T::RuntimeUpgrade::do_upgrade(code)
		}

		/// Approve a proposal by a given proposal id
		/// Approve a Proposal.
		///
		/// ## Events
		///
		/// - [Approved](Event::Approved)
		///
		/// ## Errors
		///
		/// - [NotMember](Error::NotMember)
		/// - [ProposalNotFound](Error::ProposalNotFound)
		/// - [AlreadyApproved](Error::AlreadyApproved)
		#[pallet::weight(T::WeightInfo::approve())]
		pub fn approve(origin: OriginFor<T>, id: ProposalId) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Ensure origin is part of the governance
			ensure!(Members::<T>::get().contains(&who), Error::<T>::NotMember);
			// Ensure that the proposal exists
			ensure!(Proposals::<T>::contains_key(id), Error::<T>::ProposalNotFound);
			// Try to approve the proposal
			Self::try_approve(who, id)?;
			// Governance members don't pay transaction fees
			Ok(Pays::No.into())
		}

		/// **Can only be called via the Governance Origin**
		///
		/// Execute an extrinsic as root
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::call_as_sudo().saturating_add(call.get_dispatch_info().weight))]
		pub fn call_as_sudo(
			origin: OriginFor<T>,
			// TODO: Not possible to fix the clippy warning here. At the moment we
			// need to ignore it on a global level.
			call: Box<<T as Config>::Call>,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			// Execute the root call
			call.dispatch_bypass_filter(frame_system::RawOrigin::Root.into())
		}

		/// **Can only be called via the Witnesser Origin**
		///
		/// Set a whitelisted call hash, to be executed when someone submits a call
		/// via `submit_govkey_call` that matches the hash whitelisted here.
		///
		/// ## Events
		///
		/// - [GovKeyCallHashWhitelisted](Event::GovKeyCallHashWhitelisted)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		#[pallet::weight(T::WeightInfo::set_whitelisted_call_hash())]
		pub fn set_whitelisted_call_hash(
			origin: OriginFor<T>,
			call_hash: GovCallHash,
		) -> DispatchResult {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;
			GovKeyWhitelistedCallHash::<T>::put(call_hash);
			Self::deposit_event(Event::GovKeyCallHashWhitelisted { call_hash });
			Ok(())
		}

		/// **Can only be called via the Governance Origin or a Staked Party**
		///
		/// Submit a call to be executed if the gov key has already committed to it.
		///
		/// ## Events
		///
		/// - GovKeyCallDispatched
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_support::error::BadOrigin)
		/// - [CallHashNotWhitelisted](Error::CallHashNotWhitelisted)
		#[pallet::weight(T::WeightInfo::submit_govkey_call().saturating_add(call.get_dispatch_info().weight))]
		pub fn submit_govkey_call(
			origin: OriginFor<T>,
			call: Box<<T as Config>::Call>,
		) -> DispatchResultWithPostInfo {
			ensure!(
				(ensure_signed(origin.clone()).is_ok() ||
					T::EnsureGovernance::ensure_origin(origin).is_ok()),
				BadOrigin,
			);
			let next_nonce = NextGovKeyCallHashNonce::<T>::get();
			let call_hash =
				frame_support::Hashable::blake2_256(&(call.clone(), next_nonce, T::Version::get()));
			match GovKeyWhitelistedCallHash::<T>::get() {
				Some(whitelisted_call_hash) if whitelisted_call_hash == call_hash => {
					Self::deposit_event(
						match call.dispatch_bypass_filter(RawOrigin::GovernanceApproval.into()) {
							Ok(_) => Event::GovKeyCallExecuted { call_hash },
							Err(_) => Event::GovKeyCallExecutionFailed { call_hash },
						},
					);
					NextGovKeyCallHashNonce::<T>::put(next_nonce + 1);
					GovKeyWhitelistedCallHash::<T>::kill();

					Ok(Pays::No.into())
				},
				_ => Err(Error::<T>::CallHashNotWhitelisted.into()),
			}
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
			const FIVE_DAYS_IN_SECONDS: u64 = 5 * 24 * 60 * 60;
			Self { members: Default::default(), expiry_span: FIVE_DAYS_IN_SECONDS }
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
	#[derive(PartialEq, Eq, Clone, RuntimeDebug, Encode, Decode, TypeInfo)]
	pub enum RawOrigin {
		GovernanceApproval,
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
				RawOrigin::GovernanceApproval => Ok(()),
			},
			Err(o) => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn successful_origin() -> OuterOrigin {
		RawOrigin::GovernanceApproval.into()
	}
}

impl<T: Config> Pallet<T> {
	/// Checks the expiry state of the proposals
	fn check_expiry() -> Weight {
		match ActiveProposals::<T>::decode_len() {
			Some(proposal_len) if proposal_len > 0 => {
				// Separate the proposals into expired an active by partitioning
				let (expired, active): (Vec<ActiveProposal>, Vec<ActiveProposal>) =
					ActiveProposals::<T>::get()
						.iter()
						.partition(|p| p.1 <= T::TimeSource::now().as_secs());
				// Remove expired proposals
				let number_expired_proposals = expired.len() as u32;
				Self::expire_proposals(expired);
				ActiveProposals::<T>::set(active);
				T::WeightInfo::on_initialize(proposal_len as u32) +
					T::WeightInfo::expire_proposals(number_expired_proposals)
			},
			_ => T::WeightInfo::on_initialize_best_case(),
		}
	}
	/// Executes all proposals in the pipeline
	fn execute_proposals() -> Weight {
		let mut execution_weight = 0;
		// If there is something in the pipeline execute it
		if ExecutionPipeline::<T>::decode_len() > Some(0) {
			for (call, id) in ExecutionPipeline::<T>::get() {
				Self::deposit_event(if let Some(call) = Self::decode_call(&call) {
					execution_weight += call.get_dispatch_info().weight;
					match call.dispatch_bypass_filter((RawOrigin::GovernanceApproval).into()) {
						Ok(_) => Event::Executed(id),
						Err(err) => Event::FailedExecution(err.error),
					}
				} else {
					// Emit an error if the decode of a call failed
					Event::DecodeOfCallFailed(id)
				})
			}
			// Clean up execution pipeline
			ExecutionPipeline::<T>::set(vec![]);
		}
		execution_weight
	}
	/// Expire proposals
	fn expire_proposals(expired: Vec<ActiveProposal>) {
		for expired_proposal in expired {
			Proposals::<T>::remove(expired_proposal.0);
			Self::deposit_event(Event::Expired(expired_proposal.0));
		}
	}
	/// Push a proposal
	fn push_proposal(call: Box<<T as Config>::Call>) -> u32 {
		// Generate the next proposal id
		let id = Self::get_next_id();
		// Insert a new proposal
		Proposals::<T>::insert(id, Proposal { call: call.encode(), approved: vec![] });
		// Update the proposal counter
		ProposalIdCounter::<T>::put(id);
		// Add the proposal to the active proposals array
		ActiveProposals::<T>::append((id, T::TimeSource::now().as_secs() + ExpiryTime::<T>::get()));
		id
	}
	/// Returns the next proposal id
	fn get_next_id() -> ProposalId {
		ProposalIdCounter::<T>::get().add(1)
	}
	/// Executes an proposal if the majority is reached
	fn schedule_proposal_execution(id: ProposalId) -> Result<(), DispatchError> {
		let proposal = Proposals::<T>::get(id);
		// Try to decode the stored extrinsic
		// Push the proposal to the execution pipeline
		ExecutionPipeline::<T>::append((proposal.call, id));
		// Remove the proposal from storage
		Proposals::<T>::remove(id);
		// Remove the proposal from active proposals
		let mut active_proposals = ActiveProposals::<T>::get();
		active_proposals.retain(|x| x.0 != id);
		// Set the new active proposals
		ActiveProposals::<T>::set(active_proposals);
		Ok(())
	}
	/// Checks if the majority for a proposal is reached
	fn majority_reached(approvals: usize) -> bool {
		approvals > Members::<T>::decode_len().unwrap_or_default() / 2
	}
	/// Tries to approve a proposal
	fn try_approve(account: T::AccountId, id: u32) -> Result<(), DispatchError> {
		let mut votes = 0;
		Proposals::<T>::mutate(id, |proposal| {
			// Check already approved
			if proposal.approved.contains(&account) {
				return Err(Error::<T>::AlreadyApproved)
			}
			// Add account to approved array
			proposal.approved.push(account);
			votes = proposal.approved.len();
			Self::deposit_event(Event::Approved(id));
			Ok(())
		})?;
		// Check if the majority is reached
		if Self::majority_reached(votes) {
			// Schedule execution
			Self::schedule_proposal_execution(id)?;
		}
		Ok(())
	}
	/// Decodes a encoded representation of a Call
	/// Returns None if the encode of the extrinsic has failed
	fn decode_call(call: &[u8]) -> Option<<T as Config>::Call> {
		Decode::decode(&mut &(*call)).ok()
	}
}
