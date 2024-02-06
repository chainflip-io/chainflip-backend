#![cfg_attr(not(feature = "std"), no_std)]
use cf_chains::{eth::Address, ForeignChain};
use cf_traits::{BroadcastAnyChainGovKey, Chainflip, CommKeyBroadcaster, FeePayment, FundingInfo};
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::*, traits::StorageVersion, RuntimeDebugNoBound};
use sp_std::{cmp::PartialEq, vec, vec::Vec};

pub use pallet::*;

mod benchmarking;
pub mod migrations;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::WeightInfo;

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, RuntimeDebugNoBound)]
#[scale_info(skip_type_params(T))]
pub enum Proposal {
	SetGovernanceKey(ForeignChain, Vec<u8>),
	SetCommunityKey(Address),
}

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::storage_version(PALLET_VERSION)]
	pub struct Pallet<T>(_);

	use frame_system::pallet_prelude::*;
	use sp_std::collections::btree_set::BTreeSet;
	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Burns the proposal fee from the accounts.
		type FeePayment: FeePayment<Amount = Self::Amount, AccountId = Self::AccountId>;
		/// Broadcasts the community key.
		type CommKeyBroadcaster: CommKeyBroadcaster;
		/// Broadcasts the gov key to any supported chain.
		type AnyChainGovKeyBroadcaster: BroadcastAnyChainGovKey;
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
	pub type Proposals<T: Config> = StorageMap<_, Twox64Concat, BlockNumberFor<T>, Proposal>;

	/// The accounts currently backing each proposal.
	#[pallet::storage]
	#[pallet::getter(fn backers)]
	pub type Backers<T: Config> =
		StorageMap<_, Twox64Concat, Proposal, BTreeSet<T::AccountId>, ValueQuery>;

	/// The Government key proposal currently awaiting enactment, if any. Indexed by the block
	/// number we will attempt to enact this update.
	#[pallet::storage]
	pub type GovKeyUpdateAwaitingEnactment<T> =
		StorageValue<_, (BlockNumberFor<T>, (ForeignChain, Vec<u8>)), OptionQuery>;

	/// The Community key proposal currently awaiting enactment, if any. Indexed by the block number
	/// we will attempt to enact this update.
	#[pallet::storage]
	pub type CommKeyUpdateAwaitingEnactment<T> =
		StorageValue<_, (BlockNumberFor<T>, Address), OptionQuery>;

	/// Current Governance keys for foreign chains.
	#[pallet::storage]
	pub type GovKeys<T> = StorageMap<_, Twox64Concat, ForeignChain, Vec<u8>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A proposal has been submitted.
		ProposalSubmitted { proposal: Proposal },
		/// A proposal has passed.
		ProposalPassed { proposal: Proposal },
		/// A proposal was rejected.
		ProposalRejected { proposal: Proposal },
		/// A proposal was enacted.
		ProposalEnacted { proposal: Proposal },
		/// Update of GOV key has failed.
		GovKeyUpdatedHasFailed { chain: ForeignChain, key: Vec<u8> },
		/// Update of GOV key was successful.
		GovKeyUpdatedWasSuccessful { chain: ForeignChain, key: Vec<u8> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Proposal is already backed by the same account.
		AlreadyBacked,
		/// Proposal doesn't exist.
		ProposalDoesntExist,
		/// The proposed governance key is incompatible with the proposed chain.
		IncompatibleGovkey,
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
			if let Some((enactment_block, (chain, new_key))) =
				GovKeyUpdateAwaitingEnactment::<T>::get()
			{
				if enactment_block == current_block {
					if T::AnyChainGovKeyBroadcaster::broadcast_gov_key(
						chain,
						GovKeys::<T>::get(chain),
						new_key.clone(),
					)
					.is_ok()
					{
						GovKeys::<T>::insert(chain, &new_key);
						Self::deposit_event(Event::<T>::GovKeyUpdatedWasSuccessful {
							chain,
							key: new_key.clone(),
						});
					} else {
						Self::deposit_event(Event::<T>::GovKeyUpdatedHasFailed {
							chain,
							key: new_key.clone(),
						});
					}
					Self::deposit_event(Event::<T>::ProposalEnacted {
						proposal: Proposal::SetGovernanceKey(chain, new_key),
					});
					GovKeyUpdateAwaitingEnactment::<T>::kill();
					weight.saturating_accrue(T::WeightInfo::on_initialize_execute_proposal());
				}
			}
			if let Some((enactment_block, new_key)) = CommKeyUpdateAwaitingEnactment::<T>::get() {
				if enactment_block == current_block {
					T::CommKeyBroadcaster::broadcast(new_key);
					Self::deposit_event(Event::<T>::ProposalEnacted {
						proposal: Proposal::SetCommunityKey(new_key),
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
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::submit_proposal())]
		pub fn submit_proposal(
			origin: OriginFor<T>,
			proposal: Proposal,
		) -> DispatchResultWithPostInfo {
			let proposer = ensure_signed(origin)?;
			if let Proposal::SetGovernanceKey(chain, ref key) = proposal {
				ensure!(
					T::AnyChainGovKeyBroadcaster::is_govkey_compatible(chain, key),
					Error::<T>::IncompatibleGovkey
				);
			}
			T::FeePayment::try_burn_fee(&proposer, T::ProposalFee::get())?;
			Proposals::<T>::insert(
				<frame_system::Pallet<T>>::block_number() + T::VotingPeriod::get(),
				proposal.clone(),
			);
			Backers::<T>::insert(proposal.clone(), BTreeSet::from([proposer]));
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
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::back_proposal(Backers::<T>::decode_len(proposal).unwrap_or_default() as u32))]
		pub fn back_proposal(
			origin: OriginFor<T>,
			proposal: Proposal,
		) -> DispatchResultWithPostInfo {
			let backer = ensure_signed(origin)?;
			Backers::<T>::try_mutate_exists(proposal, |maybe_backers| match maybe_backers {
				Some(backers) => {
					if !backers.insert(backer) {
						return Err(Error::<T>::AlreadyBacked)
					}
					Ok(())
				},
				None => Err(Error::<T>::ProposalDoesntExist),
			})?;
			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn resolve_vote(proposal: Proposal) -> usize {
			let backers = Backers::<T>::take(&proposal);
			Self::deposit_event(
				if backers.iter().map(T::FundingInfo::total_balance_of).sum::<T::Amount>() >
					(T::FundingInfo::total_onchain_funds() / 3u32.into()) * 2u32.into()
				{
					let enactment_block =
						<frame_system::Pallet<T>>::block_number() + T::EnactmentDelay::get();
					match proposal.clone() {
						Proposal::SetGovernanceKey(chain, key) => {
							GovKeyUpdateAwaitingEnactment::<T>::put::<(
								BlockNumberFor<T>,
								(cf_chains::ForeignChain, Vec<u8>),
							)>((enactment_block, (chain, key)));
						},
						Proposal::SetCommunityKey(key) => {
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
