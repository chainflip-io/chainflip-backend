#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

pub mod migrations;

use cf_traits::{AuthoritiesCfeVersions, CompatibleCfeVersions};
use codec::{Codec, Decode, Encode};
use frame_support::{
	dispatch::GetDispatchInfo,
	ensure,
	pallet_prelude::{DispatchResultWithPostInfo, Weight},
	sp_runtime::{DispatchError, Percent, TransactionOutcome},
	storage::with_transaction,
	traits::{EnsureOrigin, Get, StorageVersion, UnfilteredDispatchable, UnixTime},
};
pub use pallet::*;
use sp_std::{boxed::Box, ops::Add, vec::Vec};

mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

/// Hash over (call, nonce, runtime_version)
pub type GovCallHash = [u8; 32];

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(2);

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

macro_rules! ensure_governance_member {
	($origin:ident) => {{
		let account_id = ensure_signed($origin)?;
		ensure!(Members::<T>::get().contains(&account_id), Error::<T>::NotMember);
		account_id
	}};
}

pub type ProposalId = u32;
/// Implements the functionality of the Chainflip governance.
#[frame_support::pallet]
pub mod pallet {

	use super::*;

	use cf_primitives::SemVer;
	use cf_traits::{Chainflip, ExecutionCondition, RuntimeUpgrade};
	use codec::Encode;
	use frame_support::{
		dispatch::GetDispatchInfo,
		error::BadOrigin,
		pallet_prelude::*,
		traits::{UnfilteredDispatchable, UnixTime},
	};
	use frame_system::pallet_prelude::*;
	use sp_std::{boxed::Box, collections::btree_set::BTreeSet, vec::Vec};

	use super::{GovCallHash, WeightInfo};

	#[derive(Default, Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub enum ExecutionMode {
		#[default]
		Automatic,
		Manual,
	}

	#[derive(Encode, Decode, TypeInfo, Clone, Copy, RuntimeDebug, PartialEq, Eq)]
	pub struct ActiveProposal {
		pub proposal_id: ProposalId,
		pub expiry_time: Timestamp,
	}

	/// Proposal struct
	#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct Proposal<AccountId> {
		/// Encoded representation of a extrinsic.
		pub call: OpaqueCall,
		/// Accounts who have already approved the proposal.
		pub approved: BTreeSet<AccountId>,
		/// Proposal is pre authorised.
		pub execution: ExecutionMode,
	}

	type AccountId<T> = <T as frame_system::Config>::AccountId;
	type OpaqueCall = Vec<u8>;
	type Timestamp = u64;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Standard Event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The outer Origin needs to be compatible with this pallet's Origin
		type RuntimeOrigin: From<RawOrigin>
			+ From<frame_system::RawOrigin<<Self as frame_system::Config>::AccountId>>;
		/// The overarching call type.
		type RuntimeCall: Member
			+ Parameter
			+ UnfilteredDispatchable<RuntimeOrigin = <Self as Config>::RuntimeOrigin>
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
		/// For getting required CFE version for a runtime upgrade.
		type CompatibleCfeVersions: CompatibleCfeVersions;
		/// For getting current authorities' CFE versions.
		type AuthoritiesCfeVersions: AuthoritiesCfeVersions;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// Proposals.
	#[pallet::storage]
	#[pallet::getter(fn proposals)]
	pub(super) type Proposals<T: Config> =
		StorageMap<_, Blake2_128Concat, ProposalId, Proposal<T::AccountId>>;

	/// Active proposals.
	#[pallet::storage]
	#[pallet::getter(fn active_proposals)]
	pub(super) type ActiveProposals<T> = StorageValue<_, Vec<ActiveProposal>, ValueQuery>;

	/// Call hash that has been committed to by the Governance Key.
	#[pallet::storage]
	#[pallet::getter(fn gov_key_whitelisted_call_hash)]
	pub(super) type GovKeyWhitelistedCallHash<T> = StorageValue<_, GovCallHash, OptionQuery>;

	/// Pre authorised governance calls.
	#[pallet::storage]
	pub(super) type PreAuthorisedGovCalls<T> =
		StorageMap<_, Twox64Concat, u32, OpaqueCall, OptionQuery>;

	/// Any nonces before this have been consumed.
	#[pallet::storage]
	#[pallet::getter(fn next_gov_key_call_hash_nonce)]
	pub type NextGovKeyCallHashNonce<T> = StorageValue<_, u32, ValueQuery>;

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

	/// Accounts in the current governance set.
	#[pallet::storage]
	#[pallet::getter(fn members)]
	pub(super) type Members<T> = StorageValue<_, BTreeSet<AccountId<T>>, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// on_initialize hook - check the ActiveProposals
		/// and remove the expired ones for house keeping
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// Check expiry and expire the proposals if needed
			let active_proposal_weight = Self::check_expiry();
			let execution_weight = Self::execute_pending_proposals();
			active_proposal_weight + execution_weight
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
		/// Call executed by GovKey
		GovKeyCallExecuted { call_hash: GovCallHash },
		/// CallHash whitelisted by the GovKey
		GovKeyCallHashWhitelisted { call_hash: GovCallHash },
		/// Failed GovKey call
		GovKeyCallExecutionFailed { call_hash: GovCallHash, error: DispatchError },
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
		/// Decoding Members len failed.
		DecodeMembersLenFailed,
		/// A runtime upgrade has failed because the upgrade conditions were not satisfied
		UpgradeConditionsNotMet,
		/// The call hash was not whitelisted
		CallHashNotWhitelisted,
		/// Insufficient number of CFEs are at the target version to receive the runtime upgrade.
		NotEnoughAuthoritiesCfesAtTargetVersion,
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
		#[pallet::call_index(0)]
		#[pallet::weight((T::WeightInfo::propose_governance_extrinsic(), DispatchClass::Operational))]
		pub fn propose_governance_extrinsic(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
			execution: ExecutionMode,
		) -> DispatchResultWithPostInfo {
			let account_id = ensure_governance_member!(origin);

			let id = Self::push_proposal(call, execution);
			Self::deposit_event(Event::Proposed(id));

			Self::inner_approve(account_id, id)?;

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
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::new_membership_set())]
		pub fn new_membership_set(
			origin: OriginFor<T>,
			new_members: BTreeSet<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			Members::<T>::mutate(|old_members| {
				for member in old_members.difference(&new_members) {
					<frame_system::Pallet<T>>::dec_sufficients(member);
				}
				for member in new_members.difference(old_members) {
					<frame_system::Pallet<T>>::inc_sufficients(member);
				}
				*old_members = new_members;
			});
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
		#[pallet::call_index(2)]
		#[pallet::weight((T::BlockWeights::get().max_block, DispatchClass::Operational))]
		pub fn chainflip_runtime_upgrade(
			origin: OriginFor<T>,
			// The version that the percent of nodes need to be compatible with in order for the
			// runtime upgrade to go through.
			cfe_version_restriction: Option<(SemVer, Percent)>,
			code: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			ensure!(T::UpgradeCondition::is_satisfied(), Error::<T>::UpgradeConditionsNotMet);

			if let Some((required_version, percent)) = cfe_version_restriction {
				ensure!(
					T::AuthoritiesCfeVersions::percent_authorities_compatible_with_version(
						required_version
					) >= percent,
					Error::<T>::NotEnoughAuthoritiesCfesAtTargetVersion,
				);
			}

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
		#[pallet::call_index(3)]
		#[pallet::weight((T::WeightInfo::approve(), DispatchClass::Operational))]
		pub fn approve(
			origin: OriginFor<T>,
			approved_id: ProposalId,
		) -> DispatchResultWithPostInfo {
			let account_id = ensure_governance_member!(origin);
			Self::inner_approve(account_id, approved_id)?;
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
		#[allow(clippy::boxed_local)]
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::call_as_sudo().saturating_add(call.get_dispatch_info().weight))]
		pub fn call_as_sudo(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
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
		#[pallet::call_index(5)]
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

		/// **Can only be called via the Governance Origin or a Funded Party**
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
		#[pallet::call_index(6)]
		#[pallet::weight((T::WeightInfo::submit_govkey_call().saturating_add(call.get_dispatch_info().weight), DispatchClass::Operational))]
		pub fn submit_govkey_call(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResultWithPostInfo {
			ensure!(
				(ensure_signed(origin.clone()).is_ok() ||
					T::EnsureGovernance::ensure_origin(origin).is_ok()),
				BadOrigin,
			);
			let (call_hash, nonce) = Self::compute_gov_key_call_hash::<_>(call.clone());
			match GovKeyWhitelistedCallHash::<T>::get() {
				Some(whitelisted_call_hash) if whitelisted_call_hash == call_hash => {
					Self::deposit_event(match Self::dispatch_governance_call(*call) {
						Ok(_) => Event::GovKeyCallExecuted { call_hash },
						Err(err) =>
							Event::GovKeyCallExecutionFailed { call_hash, error: err.error },
					});
					NextGovKeyCallHashNonce::<T>::put(nonce.wrapping_add(1));
					GovKeyWhitelistedCallHash::<T>::kill();

					Ok(Pays::No.into())
				},
				_ => Err(Error::<T>::CallHashNotWhitelisted.into()),
			}
		}

		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::dispatch_whitelisted_call())]
		pub fn dispatch_whitelisted_call(
			origin: OriginFor<T>,
			approved_id: ProposalId,
		) -> DispatchResult {
			ensure_governance_member!(origin);
			if let Some(call) = PreAuthorisedGovCalls::<T>::take(approved_id) {
				if let Ok(call) = <T as Config>::RuntimeCall::decode(&mut &(*call)) {
					Self::deposit_event(match Self::dispatch_governance_call(call) {
						Ok(_) => Event::Executed(approved_id),
						Err(err) => Event::FailedExecution(err.error),
					});
					Ok(())
				} else {
					Err(Error::<T>::DecodeOfCallFailed.into())
				}
			} else {
				Err(Error::<T>::ProposalNotFound.into())
			}
		}
	}

	/// Genesis definition
	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub members: BTreeSet<AccountId<T>>,
		pub expiry_span: u64,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			const FIVE_DAYS_IN_SECONDS: u64 = 5 * 24 * 60 * 60;
			Self { members: Default::default(), expiry_span: FIVE_DAYS_IN_SECONDS }
		}
	}

	/// Sets the genesis governance
	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for member in &self.members {
				<frame_system::Pallet<T>>::inc_sufficients(member);
			}
			Members::<T>::set(self.members.clone());
			ExpiryTime::<T>::set(self.expiry_span);
		}
	}

	#[pallet::origin]
	pub type Origin = RawOrigin;

	/// The raw origin enum for this pallet.
	#[derive(PartialEq, Eq, Clone, RuntimeDebug, Encode, Decode, TypeInfo, MaxEncodedLen)]
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
	fn try_successful_origin() -> Result<OuterOrigin, ()> {
		Ok(RawOrigin::GovernanceApproval.into())
	}
}

impl<T: Config> Pallet<T> {
	pub fn inner_approve(who: T::AccountId, approved_id: ProposalId) -> Result<(), DispatchError> {
		ensure!(Proposals::<T>::contains_key(approved_id), Error::<T>::ProposalNotFound);

		// Try to approve the proposal
		let proposal = Proposals::<T>::try_mutate(approved_id, |proposal| {
			let proposal = proposal.as_mut().ok_or(Error::<T>::ProposalNotFound)?;

			if !proposal.approved.insert(who) {
				return Err(Error::<T>::AlreadyApproved)
			}
			Self::deposit_event(Event::Approved(approved_id));
			Ok(proposal.clone())
		})?;

		if proposal.approved.len() >
			(Members::<T>::decode_non_dedup_len().ok_or(Error::<T>::DecodeMembersLenFailed)? / 2)
		{
			if proposal.execution == ExecutionMode::Manual {
				PreAuthorisedGovCalls::<T>::insert(approved_id, proposal.call);
			} else {
				ExecutionPipeline::<T>::append((proposal.call, approved_id));
			}
			Proposals::<T>::remove(approved_id);
			ActiveProposals::<T>::mutate(|proposals| {
				proposals.retain(|ActiveProposal { proposal_id, .. }| *proposal_id != approved_id)
			});
		}
		Ok(())
	}

	pub fn compute_gov_key_call_hash<CallData>(data: CallData) -> (GovCallHash, u32)
	where
		CallData: Clone + Codec,
	{
		let nonce = NextGovKeyCallHashNonce::<T>::get();
		(frame_support::Hashable::blake2_256(&(data, nonce, T::Version::get())), nonce)
	}

	fn check_expiry() -> Weight {
		let active_proposals = ActiveProposals::<T>::get();
		let num_proposals = active_proposals.len();
		if num_proposals == 0 {
			return T::WeightInfo::on_initialize_best_case()
		}
		let (expired, active): (Vec<ActiveProposal>, Vec<ActiveProposal>) =
			active_proposals.iter().partition(|active_proposal| {
				active_proposal.expiry_time <= T::TimeSource::now().as_secs()
			});

		ActiveProposals::<T>::set(active);
		Self::expire_proposals(expired) + T::WeightInfo::on_initialize(num_proposals as u32)
	}

	fn execute_pending_proposals() -> Weight {
		let mut execution_weight = Weight::zero();
		for (call, id) in ExecutionPipeline::<T>::take() {
			Self::deposit_event(
				if let Ok(call) = <T as Config>::RuntimeCall::decode(&mut &(*call)) {
					execution_weight.saturating_accrue(call.get_dispatch_info().weight);
					match Self::dispatch_governance_call(call) {
						Ok(_) => Event::Executed(id),
						Err(err) => Event::FailedExecution(err.error),
					}
				} else {
					Event::DecodeOfCallFailed(id)
				},
			)
		}
		execution_weight
	}

	fn expire_proposals(expired: Vec<ActiveProposal>) -> Weight {
		for ActiveProposal { proposal_id, .. } in &expired {
			Proposals::<T>::remove(proposal_id);
			Self::deposit_event(Event::Expired(*proposal_id));
		}
		T::WeightInfo::expire_proposals(expired.len() as u32)
	}

	fn push_proposal(call: Box<<T as Config>::RuntimeCall>, execution: ExecutionMode) -> u32 {
		let proposal_id = ProposalIdCounter::<T>::get().add(1);
		Proposals::<T>::insert(
			proposal_id,
			Proposal { call: call.encode(), approved: Default::default(), execution },
		);
		ProposalIdCounter::<T>::put(proposal_id);
		ActiveProposals::<T>::append(ActiveProposal {
			proposal_id,
			expiry_time: T::TimeSource::now().as_secs() + ExpiryTime::<T>::get(),
		});
		proposal_id
	}

	/// Dispatches a call from the governance origin, with transactional semantics, ie. if the call
	/// dispatch returns `Err`, rolls back any storage updates.
	fn dispatch_governance_call(call: <T as Config>::RuntimeCall) -> DispatchResultWithPostInfo {
		with_transaction(move || {
			match call.dispatch_bypass_filter(RawOrigin::GovernanceApproval.into()) {
				r @ Ok(_) => TransactionOutcome::Commit(r),
				r @ Err(_) => TransactionOutcome::Rollback(r),
			}
		})
	}
}
