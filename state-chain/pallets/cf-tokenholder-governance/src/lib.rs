#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode};
use frame_support::{
	dispatch::{Weight},
};

use frame_support::{
	pallet_prelude::*,
};

use cf_chains::{ChainCrypto};

use codec::Encode;
use frame_support::RuntimeDebugNoBound;
use sp_std::cmp::PartialEq;

pub use pallet::*;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub type ProposalId = u32;

// type GovKey<T> = <<T as Config>::Keys as AuthorityKeys>::Gov;
// type CommKey<T> = <<T as Config>::Keys as AuthorityKeys>::Comm;

#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, TypeInfo, RuntimeDebugNoBound)]
#[scale_info(skip_type_params(T))]
pub enum Proposal<T: Config> {
	SetGovernanceKey(<<T as Config>::Chain as ChainCrypto>::GovKey),
	SetCommunityKey(<<T as Config>::Chain as ChainCrypto>::GovKey),
}
#[frame_support::pallet]
pub mod pallet {
	use super::*;
    use cf_chains::{ChainAbi};
	use cf_traits::ReplayProtectionProvider;
    use cf_traits::{Broadcaster, Chainflip, FeePayment, StakingInfo};

	use cf_chains::SetGovKey as SetGovKeyApiCall;
	use cf_chains::SetCommunityKey as SetCommunityKeyApiCall;

	use crate::pallet::Proposal::SetGovernanceKey;
	use crate::pallet::Proposal::SetCommunityKey;
	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	use frame_system::{pallet_prelude::*};
	use sp_runtime::traits::AtLeast32BitUnsigned;
	use sp_std::{vec::Vec};
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

		type SetGovKeyApiCall: SetGovKeyApiCall<Self::Chain>;

		type SetCommunityKeyApiCall: SetCommunityKeyApiCall<Self::Chain>;

		/// Something that can provide a nonce for the threshold signature.
		type ReplayProtectionProvider: ReplayProtectionProvider<Self::Chain>;

		/// The Flip token implementation.
		type Flip: StakingInfo<
			AccountId = <Self as frame_system::Config>::AccountId,
			Balance = Self::Balance,
		>;

		type GovKeyBroadcaster: Broadcaster<Self::Chain, ApiCall = Self::SetGovKeyApiCall>;

		type CommKeyBroadcaster: Broadcaster<Self::Chain, ApiCall = Self::SetCommunityKeyApiCall>;
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
	pub(super) type ProposalFee<T> = StorageValue<_, <T as Config>::Balance, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn proposals)]
	pub(super) type Proposals<T: Config> = StorageMap<_, Twox64Concat, BlockNumberFor<T>, Proposal<T>>;

	#[pallet::storage]
	#[pallet::getter(fn backers)]
	pub(super) type Backers<T: Config> =
		StorageMap<_, Twox64Concat, Proposal<T>, Vec<T::AccountId>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn gov_enactment)]
	pub type GovKeyUpdateAwaitingEnactment<T> = StorageValue<_, (BlockNumberFor<T>, <<T as Config>::Chain as ChainCrypto>::GovKey), OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn community_enactment)]
	pub type CommKeyUpdateAwaitingEnactment<T> = StorageValue<_, (BlockNumberFor<T>, <<T as Config>::Chain as ChainCrypto>::GovKey), OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		ProposalSubmitted(Proposal<T>),
		ProposalPassed(Proposal<T>),
		ProposalRejected(Proposal<T>),
		ProposalEnacted(Proposal<T>),
	}

	#[pallet::error]
	pub enum Error<T> {
		AlreadyBacked,
		ProposalDosentExists,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			if let Some(proposal) = Proposals::<T>::get(n) {
				Self::resolve_vote(proposal);
			}
			if let Some(gov_key) = GovKeyUpdateAwaitingEnactment::<T>::get() {
				if gov_key.0 == n {
					T::GovKeyBroadcaster::threshold_sign_and_broadcast(T::SetGovKeyApiCall::new_unsigned(T::ReplayProtectionProvider::replay_protection(), gov_key.1));
					GovKeyUpdateAwaitingEnactment::<T>::kill();
				}
			}
			if let Some(comm_key) = CommKeyUpdateAwaitingEnactment::<T>::get() {
				if comm_key.0 == n {
					T::CommKeyBroadcaster::threshold_sign_and_broadcast(T::SetCommunityKeyApiCall::new_unsigned(T::ReplayProtectionProvider::replay_protection(), comm_key.1));
					CommKeyUpdateAwaitingEnactment::<T>::kill();
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
			proposal: Proposal<T>,
		) -> DispatchResultWithPostInfo {
			let proposer = ensure_signed(origin)?;
			T::FeePayment::try_burn_fee(proposer, ProposalFee::<T>::get())?;
			Proposals::<T>::insert(
				<frame_system::Pallet<T>>::block_number() + VotingPeriod::<T>::get(),
				proposal.clone(),
			);
			Self::deposit_event(Event::<T>::ProposalSubmitted(proposal));
			Ok(().into())
		}

		#[pallet::weight(10_000)]
		pub fn back_proposal(
			origin: OriginFor<T>,
			proposal: Proposal<T>,
		) -> DispatchResultWithPostInfo {
			let baker = ensure_signed(origin)?;
			// ensure!(Backers::<T>::contains_key(proposal.clone()), Error::<T>::ProposalDosentExists);
			Backers::<T>::mutate(proposal, |bakers| {
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
		pub fn resolve_vote(proposal: Proposal<T>) {
			let total_baked: u128 = Backers::<T>::take(proposal.clone())
				.iter()
				.map(|baker| {
					T::Flip::total_balance_of(baker).into()
				})
				.sum::<u128>();
			let total_stake: u128 = T::Flip::onchain_funds().into();
			if total_baked > (total_stake / 3) * 2 {
				match proposal {
					SetGovernanceKey(key) => {
						GovKeyUpdateAwaitingEnactment::<T>::put((<frame_system::Pallet<T>>::block_number() + EnactmentDelay::<T>::get(), key));
					},
					SetCommunityKey(key) => {
						CommKeyUpdateAwaitingEnactment::<T>::put((<frame_system::Pallet<T>>::block_number() + EnactmentDelay::<T>::get(), key));
					}
				}
				Self::deposit_event(Event::<T>::ProposalPassed(proposal));
			} else {
				Self::deposit_event(Event::<T>::ProposalRejected(proposal));
			}
		}
	}
}
