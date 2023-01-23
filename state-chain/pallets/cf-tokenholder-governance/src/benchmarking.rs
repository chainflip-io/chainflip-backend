#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::{Chainflip, FeePayment};
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::sp_runtime::traits::UniqueSaturatedFrom;
use frame_system::RawOrigin;
use sp_std::collections::btree_set::BTreeSet;

fn generate_proposal<T: Config>() -> Proposal {
	Proposal::SetGovernanceKey((ForeignChain::Ethereum, vec![1; 32]))
}

benchmarks! {
	on_initialize_resolve_votes {
		// Number of backers
		let a in 10..1000;
		let stake = <T as Chainflip>::Amount::unique_saturated_from(50_000_000_000_000_000_000_000u128);
		let proposal = generate_proposal::<T>();
		Proposals::<T>::insert(
			T::BlockNumber::from(1u32),
			proposal.clone(),
		);
		let backers = (0..a).map(|i| account("doogle", i, 0)).collect::<BTreeSet<_>>();
		for account in &backers {
			T::FeePayment::mint_to_account(account, stake);
		}
		Backers::<T>::insert(proposal, backers);
	} : {
		Pallet::<T>::on_initialize(1u32.into());
	} verify {
		assert!(GovKeyUpdateAwaitingEnactment::<T>::get().is_some());
	}
	on_initialize_execute_proposal {
		GovKeyUpdateAwaitingEnactment::<T>::set(Some((1u32.into(), (ForeignChain::Ethereum, vec![1; 32]))));
	}: {
		Pallet::<T>::on_initialize(1u32.into());
	} verify {
		assert!(GovKeyUpdateAwaitingEnactment::<T>::get().is_none());
	}
	submit_proposal {
		let caller: T::AccountId = whitelisted_caller();
		T::FeePayment::mint_to_account(&caller, T::ProposalFee::get());
	}: _(RawOrigin::Signed(whitelisted_caller()), generate_proposal::<T>())
	verify {
		assert!(Proposals::<T>::contains_key(<frame_system::Pallet<T>>::block_number() + T::VotingPeriod::get()));
	}
	back_proposal {
		let a in 1..1000;
		let caller: T::AccountId = whitelisted_caller();
		let proposal = generate_proposal::<T>();
		let backers = (0..a).map(|i| account::<T::AccountId>("signers", i, 0)).collect::<BTreeSet<_>>();
		Proposals::<T>::insert(
			<frame_system::Pallet<T>>::block_number() + T::VotingPeriod::get(),
			proposal.clone(),
		);
		Backers::<T>::insert(proposal.clone(), backers);
	}: _(RawOrigin::Signed(caller.clone()), proposal.clone())
	verify {
		assert!(Backers::<T>::get(proposal).contains(&caller));
	}
}
