#![cfg_attr(not(feature = "std"), no_std)]

use codec::Decode;
use frame_support::traits::EnsureOrigin;
pub use pallet::*;
use sp_runtime::DispatchError;
use sp_std::vec::Vec;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
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

	#[derive(Encode, Decode, Clone, RuntimeDebug, Default, PartialEq, Eq)]
	pub struct Proposal<AccountId> {
		pub id: u32,
		pub call: OpaqueCall,
		pub expiry: u64,
		pub votes: u32,
		pub voted: Vec<AccountId>,
		pub executed: bool,
	}

	type AccountId<T> = <T as frame_system::Config>::AccountId;
	type OpaqueCall = Vec<u8>;
	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		type Origin: From<RawOrigin>;
		type EnsureGovernance: EnsureOrigin<<Self as pallet::Config>::Origin>;
		type Call: Member
			+ FullCodec
			+ UnfilteredDispatchable<Origin = <Self as Config>::Origin>
			+ GetDispatchInfo;
		type TimeSource: UnixTime;
	}
	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn proposals)]
	pub(super) type Proposals<T: Config> =
		StorageMap<_, Blake2_128Concat, u32, Proposal<T::AccountId>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn ongoing_proposals)]
	pub type OnGoingProposals<T> = StorageValue<_, Vec<u32>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn number_of_proposals)]
	pub type NumberOfProposals<T> = StorageValue<_, u32>;

	#[pallet::storage]
	#[pallet::getter(fn members)]
	pub(super) type Members<T> = StorageValue<_, Vec<AccountId<T>>, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			let ongoing_proposals = <OnGoingProposals<T>>::get();
			let mut executed: Vec<usize> = vec![];
			for (index, proposal_id) in ongoing_proposals.iter().enumerate() {
				let proposal = <Proposals<T>>::get(proposal_id);
				if Self::majority_reached(proposal.votes)
					&& !proposal.executed
					&& proposal.expiry >= T::TimeSource::now().as_secs()
				{
					if let Some(call) = Self::decode_call(proposal.call) {
						let result =
							call.dispatch_bypass_filter((RawOrigin::GovernanceThreshold).into());
						<Proposals<T>>::mutate(proposal_id, |proposal| {
							proposal.executed = true;
						});
						if result.is_ok() {
							executed.push(index);
							Self::deposit_event(Event::GovernanceExtrinsicExecuted(
								proposal_id.clone(),
							));
						}
					}
				}
			}
			<OnGoingProposals<T>>::mutate(|ongoing_proposals| {
				for i in executed {
					ongoing_proposals.remove(i);
				}
			});
			0
		}
	}

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		ProposedGovernanceExtrinsic(u32, Vec<u8>),
		GovernanceExtrinsicExecuted(u32),
		Voted,
	}

	#[pallet::error]
	pub enum Error<T> {
		AlreadyVoted,
		AlreadyExecuted,
		AlreadyExpired,
		NoMember,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub fn propose_governance_extrinsic(
			origin: OriginFor<T>,
			call: Box<<T as Config>::Call>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::ensure_member(&who)?;
			let proposal_id = Self::next_proposal_id();
			<Proposals<T>>::insert(
				proposal_id,
				Proposal {
					id: proposal_id,
					call: call.encode(),
					expiry: T::TimeSource::now().as_secs() + 180,
					executed: false,
					votes: 0,
					voted: vec![],
				},
			);
			<OnGoingProposals<T>>::mutate(|proposals| {
				proposals.push(proposal_id);
			});
			Ok(().into())
		}

		#[pallet::weight(10_000)]
		pub fn new_membership_set(
			origin: OriginFor<T>,
			accounts: Vec<T::AccountId>,
		) -> DispatchResultWithPostInfo {
			T::EnsureGovernance::ensure_origin(origin)?;
			<Members<T>>::put(accounts);
			Ok(().into())
		}

		#[pallet::weight(10_000)]
		pub fn approve(origin: OriginFor<T>, id: u32) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::ensure_member(&who)?;
			Self::try_vote(who, id)?;
			Ok(().into())
		}
	}

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

	// The build of genesis for the pallet.
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

pub struct EnsureGovernance;

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
	fn calc_threshold(total: u32) -> u32 {
		let doubled = total * 2;
		if doubled % 3 == 0 {
			doubled / 3
		} else {
			doubled / 3 + 1
		}
	}
	fn majority_reached(votes: u32) -> bool {
		let total_number_of_voters = <Members<T>>::get().len() as u32;
		let threshold = Self::calc_threshold(total_number_of_voters);
		if votes >= threshold {
			true
		} else {
			false
		}
	}
	fn ensure_member(account: &T::AccountId) -> Result<(), DispatchError> {
		if !<Members<T>>::get().contains(account) {
			Err(Error::<T>::NoMember.into())
		} else {
			Ok(())
		}
	}
	fn next_proposal_id() -> u32 {
		if let Some(number_of_proposals) = <NumberOfProposals<T>>::get() {
			let next_id = number_of_proposals + 1;
			<NumberOfProposals<T>>::put(next_id);
			next_id
		} else {
			<NumberOfProposals<T>>::put(0);
			0
		}
	}
	fn calc_block_weight() -> u64 {
		// TODO: figure out what makes sense here
		0
	}
	fn try_vote(account: T::AccountId, proposal_id: u32) -> Result<(), DispatchError> {
		let proposal = <Proposals<T>>::get(proposal_id);
		if proposal.executed {
			return Err(Error::<T>::AlreadyExecuted.into());
		}
		//TODO: Check expiry
		//TODO: Check existing
		//TODO: Check already voted
		<Proposals<T>>::mutate(proposal_id, |proposal| {
			proposal.voted.push(account);
			proposal.votes = proposal.votes.checked_add(1).unwrap();
		});
		Ok(())
	}
	fn decode_call(call: Vec<u8>) -> Option<<T as Config>::Call> {
		Decode::decode(&mut &call[..]).ok()
	}
}
