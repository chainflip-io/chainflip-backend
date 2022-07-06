#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode};
use frame_support::{
	dispatch::{Weight},
};
pub use frame_system::pallet::*;

pub use pallet::*;
#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::SetGovKey;
use cf_chains::{ChainAbi, SetAggKeyWithAggKey};
	use cf_traits::{Broadcaster, Chainflip, FeePayment, StakingInfo};
	use frame_support::{
		pallet_prelude::*,
	};

	use crate::Proposal::SetGovernanceKey;
	use crate::Proposal::SetCommunityKey;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	use codec::Encode;
	use frame_system::{pallet_prelude::*};
	use sp_runtime::traits::AtLeast32BitUnsigned;
	use sp_std::{vec::Vec};

	pub type ProposalId = u32;
	pub type PublicKey = [u8; 32];

	#[derive(Encode, Decode, TypeInfo, Copy, Clone, RuntimeDebug, PartialEq, Eq)]
	pub enum Proposal {
		SetGovernanceKey(PublicKey),
		SetCommunityKey(PublicKey),
	}

	#[derive(Encode, Decode, TypeInfo, Copy, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct ProposalInner {
		proposal: Proposal,
		id: ProposalId,
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ Into<u128>
			+ From<u128>;

		type FeePayment: FeePayment<Amount = Self::Balance, AccountId = Self::AccountId>;

		type Chain: ChainAbi;

		type SetGovKeyWithAggKeyApiCall: SetGovKey<Self::Chain>;

		type SetCommKeyWithAggKeyApiCall: SetAggKeyWithAggKey<Self::Chain>;

		type BroadcasterSetCommKeyWithAggKey: Broadcaster<
			Self::Chain,
			ApiCall = Self::SetCommKeyWithAggKeyApiCall,
		>;

		type BroadcasterSetGovKeyWithAggKey: Broadcaster<
			Self::Chain,
			ApiCall = Self::SetGovKeyWithAggKeyApiCall,
		>;

		/// The Flip token implementation.
		type Flip: StakingInfo<
			AccountId = <Self as frame_system::Config>::AccountId,
			Balance = Self::Balance,
		>;
	}

	#[pallet::storage]
	#[pallet::getter(fn voting_period)]
	pub(super) type VotingPeriod<T> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn proposal_count)]
	pub(super) type ProposalCount<T> = StorageValue<_, ProposalId, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn enactment_delay)]
	pub(super) type EnactmentDelay<T> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn proposal_fee)]
	pub(super) type ProposalFee<T> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn proposals)]
	pub(super) type Proposals<T> = StorageMap<_, Twox64Concat, BlockNumberFor<T>, ProposalInner>;

	#[pallet::storage]
	#[pallet::getter(fn execution_pipeline)]
	pub(super) type ExecutionPipeline<T> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, Proposal, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn bakers)]
	pub(super) type Bakers<T: Config> =
		StorageMap<_, Twox64Concat, ProposalId, Vec<T::AccountId>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		ProposalSubmitted(ProposalId, Proposal),
		ProposalPassed(ProposalId),
		ProposalRejected(ProposalId),
		ProposalEnacted(ProposalId),
	}

	#[pallet::error]
	pub enum Error<T> {
		AlreadyBacked,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			if let Some(proposal) = ExecutionPipeline::<T>::take(n) {
				match proposal {
					SetGovernanceKey(_) => {
						// TODO: Broadcast new Governance-Key
					},
					SetCommunityKey(_) => {
						// TODO: Broadcast new Community-Key
					},
				}
			}
			0
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub fn submit_proposal(
			origin: OriginFor<T>,
			proposal: Proposal,
		) -> DispatchResultWithPostInfo {
			let _ = ensure_signed(origin)?;
			let proposal_id =  Self::generate_next_id();
			Proposals::<T>::insert(
				<frame_system::Pallet<T>>::block_number() + VotingPeriod::<T>::get(),
				ProposalInner { proposal, id: proposal_id },
			);
			Self::deposit_event(Event::<T>::ProposalSubmitted(proposal_id, proposal));
			Ok(().into())
		}

		#[pallet::weight(10_000)]
		pub fn back_proposal(
			origin: OriginFor<T>,
			proposal_id: ProposalId,
		) -> DispatchResultWithPostInfo {
			let baker = ensure_signed(origin)?;
			Bakers::<T>::mutate(proposal_id, |bakers| {
				if bakers.contains(&baker) {
					return Err(Error::<T>::AlreadyBacked)
				}
				bakers.push(baker);
				Ok(())
			})?;
			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn resolve_vote(proposal: ProposalInner) {
			let total_baked: u128 = Bakers::<T>::take(proposal.id)
				.iter()
				.map(|baker| {
					// TODO: Call into the staking pallet
					T::Flip::total_balance_of(baker).into()
				})
				.sum::<u128>();
			let total_stake: u128 = T::Flip::onchain_funds().into();
			if total_baked > total_stake / 2 {
				ExecutionPipeline::<T>::insert(<frame_system::Pallet<T>>::block_number() + EnactmentDelay::<T>::get(), proposal.proposal);
				Self::deposit_event(Event::<T>::ProposalPassed(proposal.id));
			} else {
				Self::deposit_event(Event::<T>::ProposalRejected(proposal.id));
			}
		}
		fn generate_next_id() -> ProposalId {
			let next_id = ProposalCount::<T>::get().saturating_add(1);
			ProposalCount::<T>::set(next_id);
			next_id
		}
	}
}
