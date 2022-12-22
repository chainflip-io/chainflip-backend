#![cfg_attr(not(feature = "std"), no_std)]
use cf_chains::ChainCrypto;
use codec::{Decode, Encode};
use frame_support::{dispatch::Weight, pallet_prelude::*, RuntimeDebugNoBound};
use sp_std::{cmp::PartialEq, vec};

pub use pallet::*;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, TypeInfo, RuntimeDebugNoBound)]
#[scale_info(skip_type_params(T))]
pub enum Proposal<T: Config> {
	SetGovernanceKey(<<T as Config>::Chain as ChainCrypto>::GovKey),
	SetCommunityKey(<<T as Config>::Chain as ChainCrypto>::GovKey),
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::ChainAbi;
	use cf_traits::{Broadcaster, Chainflip, FeePayment, StakingInfo};

	use cf_chains::{
		SetCommKeyWithAggKey as SetCommunityKeyApiCall, SetGovKeyWithAggKey as SetGovKeyApiCall,
	};

	use crate::pallet::Proposal::{SetCommunityKey, SetGovernanceKey};
	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	use frame_system::pallet_prelude::*;
	use sp_std::vec::Vec;
	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Burns the proposal fee from the accounts.
		type FeePayment: FeePayment<Amount = Self::Amount, AccountId = Self::AccountId>;
		/// The chain instance.
		type Chain: ChainAbi;
		/// Smart contract calls.
		type ApiCalls: SetGovKeyApiCall<Self::Chain> + SetCommunityKeyApiCall<Self::Chain>;
		/// Provides information about the current distribution of on-chain stake.
		type StakingInfo: StakingInfo<
			AccountId = <Self as frame_system::Config>::AccountId,
			Balance = Self::Amount,
		>;
		/// Transaction broadcaster for configured destination chain.
		type Broadcaster: Broadcaster<Self::Chain, ApiCall = Self::ApiCalls>;
		/// Benchmarking weights.
		type WeightInfo: WeightInfo;
		/// Voting period of a proposal in blocks.
		#[pallet::constant]
		type VotingPeriod: Get<BlockNumberFor<Self>>;
		/// The cost of a proposal in FLIPPERINOS.
		#[pallet::constant]
		type ProposalFee: Get<Self::Amount>;
		/// Delay in blocks after a successfully backed proposal gets executed.
		#[pallet::constant]
		type EnactmentDelay: Get<BlockNumberFor<Self>>;
	}

	/// All unresolved proposals that are open for backing, indexed by the block at which the vote
	/// will be resolved.
	#[pallet::storage]
	#[pallet::getter(fn proposals)]
	pub type Proposals<T: Config> = StorageMap<_, Twox64Concat, BlockNumberFor<T>, Proposal<T>>;

	/// The accounts currently backing each proposal.
	#[pallet::storage]
	#[pallet::getter(fn backers)]
	pub type Backers<T: Config> =
		StorageMap<_, Twox64Concat, Proposal<T>, Vec<T::AccountId>, ValueQuery>;

	/// The Government key proposal currently awaiting enactment, if any. Indexed by the block
	/// number we will attempt to enact this update.
	#[pallet::storage]
	#[pallet::getter(fn gov_enactment)]
	pub type GovKeyUpdateAwaitingEnactment<T> = StorageValue<
		_,
		(BlockNumberFor<T>, <<T as Config>::Chain as ChainCrypto>::GovKey),
		OptionQuery,
	>;

	/// The Community key proposal currently awaiting enactment, if any. Indexed by the block number
	/// we will attempt to enact this update.
	#[pallet::storage]
	#[pallet::getter(fn community_enactment)]
	pub type CommKeyUpdateAwaitingEnactment<T> = StorageValue<
		_,
		(BlockNumberFor<T>, <<T as Config>::Chain as ChainCrypto>::GovKey),
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A proposal has been submitted.
		ProposalSubmitted { proposal: Proposal<T> },
		/// A proposal has passed.
		ProposalPassed { proposal: Proposal<T> },
		/// A proposal was rejected.
		ProposalRejected { proposal: Proposal<T> },
		/// A proposal was enacted.
		ProposalEnacted { proposal: Proposal<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Proposal is already backed by the same account.
		AlreadyBacked,
		/// Proposal doesn't exist.
		ProposalDoesntExist,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			let mut weight = Weight::zero();
			if let Some(proposal) = Proposals::<T>::take(current_block) {
				weight = T::WeightInfo::on_initialize_resolve_votes(
					Self::resolve_vote(proposal).try_into().unwrap(),
				);
			}
			if let Some((enactment_block, gov_key)) = GovKeyUpdateAwaitingEnactment::<T>::get() {
				if enactment_block == current_block {
					T::Broadcaster::threshold_sign_and_broadcast(
						<T::ApiCalls as SetGovKeyApiCall<T::Chain>>::new_unsigned(gov_key),
					);
					Self::deposit_event(Event::<T>::ProposalEnacted {
						proposal: Proposal::<T>::SetGovernanceKey(gov_key),
					});
					GovKeyUpdateAwaitingEnactment::<T>::kill();
					weight.saturating_accrue(T::WeightInfo::on_initialize_execute_proposal());
				}
			}
			if let Some((enactment_block, comm_key)) = CommKeyUpdateAwaitingEnactment::<T>::get() {
				if enactment_block == current_block {
					T::Broadcaster::threshold_sign_and_broadcast(
						<T::ApiCalls as SetCommunityKeyApiCall<T::Chain>>::new_unsigned(comm_key),
					);
					Self::deposit_event(Event::<T>::ProposalEnacted {
						proposal: Proposal::<T>::SetCommunityKey(comm_key),
					});
					CommKeyUpdateAwaitingEnactment::<T>::kill();
					weight.saturating_accrue(T::WeightInfo::on_initialize_execute_proposal());
				}
			}
			weight
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a proposal. The caller will be charged a proposal fee equal to
		/// [Config::ProposalFee].
		///
		/// ## Events
		///
		/// - [ProposalSubmitted](Event::ProposalSubmitted)
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [InsufficientLiquidity](pallet_cf_flip::Error::InsufficientLiquidity)
		#[pallet::weight(T::WeightInfo::submit_proposal())]
		pub fn submit_proposal(
			origin: OriginFor<T>,
			proposal: Proposal<T>,
		) -> DispatchResultWithPostInfo {
			let proposer = ensure_signed(origin)?;
			T::FeePayment::try_burn_fee(&proposer, T::ProposalFee::get())?;
			Proposals::<T>::insert(
				<frame_system::Pallet<T>>::block_number() + T::VotingPeriod::get(),
				proposal.clone(),
			);
			Backers::<T>::insert(proposal.clone(), vec![proposer]);
			Self::deposit_event(Event::<T>::ProposalSubmitted { proposal });
			Ok(().into())
		}

		/// Backs a proposal. The caller signals their support for a proposal.
		///
		/// ## Errors
		///
		/// - [BadOrigin](frame_system::BadOrigin)
		/// - [ProposalDoesntExist](Error::ProposalDoesntExist)
		/// - [AlreadyBacked](Error::AlreadyBacked)
		#[pallet::weight(T::WeightInfo::back_proposal(Backers::<T>::decode_len(proposal).unwrap_or_default() as u32))]
		pub fn back_proposal(
			origin: OriginFor<T>,
			proposal: Proposal<T>,
		) -> DispatchResultWithPostInfo {
			let backer = ensure_signed(origin)?;
			Backers::<T>::try_mutate_exists(proposal, |maybe_backers| match maybe_backers {
				Some(backers) => {
					if backers.contains(&backer) {
						return Err(Error::<T>::AlreadyBacked)
					}
					backers.push(backer);
					Ok(())
				},
				None => Err(Error::<T>::ProposalDoesntExist),
			})?;
			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn resolve_vote(proposal: Proposal<T>) -> usize {
			let backers = Backers::<T>::take(&proposal);
			Self::deposit_event(
				if backers.iter().map(T::StakingInfo::total_stake_of).sum::<T::Amount>() >
					(T::StakingInfo::total_onchain_stake() / 3u32.into()) * 2u32.into()
				{
					let enactment_block =
						<frame_system::Pallet<T>>::block_number() + T::EnactmentDelay::get();
					match proposal {
						SetGovernanceKey(key) => {
							GovKeyUpdateAwaitingEnactment::<T>::put((enactment_block, key));
						},
						SetCommunityKey(key) => {
							CommKeyUpdateAwaitingEnactment::<T>::put((enactment_block, key));
						},
					}
					Event::<T>::ProposalPassed { proposal }
				} else {
					Event::<T>::ProposalRejected { proposal }
				},
			);
			backers.len()
		}
	}
}
