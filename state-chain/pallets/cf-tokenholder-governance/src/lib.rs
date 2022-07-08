#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode};
use frame_support::{
	dispatch::{Weight},
};
pub use frame_system::pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub use pallet::*;
#[frame_support::pallet]
pub mod pallet {
	use super::*;
    use cf_chains::eth::api::EthereumReplayProtection;
    use cf_chains::{ChainAbi};
	use cf_traits::ReplayProtectionProvider;
	use cf_chains::eth::api::set_gov_key::SetGovKey;
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

	#[derive(Encode, Decode, TypeInfo, Clone, Debug, PartialEq, Eq)]
	pub enum Proposal {
		SetGovernanceKey(cf_chains::eth::Address),
		SetCommunityKey(cf_chains::eth::Address),
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

		/// Something that can provide a nonce for the threshold signature.
		type ReplayProtectionProvider: ReplayProtectionProvider<Self::Chain>;

		type Chain: ChainAbi;

		type Broadcaster: Broadcaster<
			Self::Chain,
			ApiCall = cf_chains::eth::api::EthereumApi,
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
	pub(super) type Proposals<T> = StorageMap<_, Twox64Concat, BlockNumberFor<T>, Proposal>;

	#[pallet::storage]
	#[pallet::getter(fn backers)]
	pub(super) type Backers<T: Config> =
		StorageMap<_, Twox64Concat, Proposal, Vec<T::AccountId>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn gov_enactment)]
	pub type GovKeyUpdateAwaitingEnactment<T> = StorageValue<_, (BlockNumberFor<T>, cf_chains::eth::Address), OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn community_enactment)]
	pub type CommKeyUpdateAwaitingEnactment<T> = StorageValue<_, (BlockNumberFor<T>, cf_chains::eth::Address), OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		ProposalSubmitted(Proposal),
		ProposalPassed(Proposal),
		ProposalRejected(Proposal),
		ProposalEnacted(Proposal),
	}

	#[pallet::error]
	pub enum Error<T> {
		AlreadyBacked,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			if let Some(gov_key) = GovKeyUpdateAwaitingEnactment::<T>::get() {
				if gov_key.0 == n {
					// TODO: Start the broadcast
					// let replay_protection = T::ReplayProtectionProvider::replay_protection();
					// let api_call = cf_chains::SetGovKey::new_unsigned(replay_protection as EthereumReplayProtection, gov_key.1);
					// T::Broadcaster::threshold_sign_and_broadcast(api_call);
					GovKeyUpdateAwaitingEnactment::<T>::kill();
				}
			}
			if let Some(comm_key) = CommKeyUpdateAwaitingEnactment::<T>::get() {
				if comm_key.0 == n {
					// TODO: Start the broadcast
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
			proposal: Proposal,
		) -> DispatchResultWithPostInfo {
			let _ = ensure_signed(origin)?;
			// TOOD: Burn FLIP
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
			proposal: Proposal,
		) -> DispatchResultWithPostInfo {
			let baker = ensure_signed(origin)?;
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
		pub fn resolve_vote(proposal: Proposal) {
			let total_baked: u128 = Backers::<T>::take(proposal.clone())
				.iter()
				.map(|baker| {
					T::Flip::total_balance_of(baker).into()
				})
				.sum::<u128>();
			let total_stake: u128 = T::Flip::onchain_funds().into();
			if total_baked > total_stake / 2 {
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
