#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::benchmarking_value::BenchmarkValue;
use cf_traits::{FeePayment, StakingInfo};
use frame_benchmarking::{account, benchmarks, vec, whitelisted_caller, Vec};
use frame_system::RawOrigin;

fn generate_proposal<T: Config>() -> Proposal<T> {
	Proposal::SetGovernanceKey(
		<<T as pallet::Config>::Chain as cf_chains::ChainCrypto>::GovKey::benchmark_value(),
	)
}

benchmarks! {
	on_initialize_resolve_votes {
		// Number of backers
		let a in 10..1000;
		let stake = <T as pallet::Config>::Balance::from(50_000_000_000_000_000_000_000u128);
		let total_onchain_funds = T::StakingInfo::total_onchain_stake();
		let proposal = generate_proposal::<T>();
		Proposals::<T>::insert(
			T::VotingPeriod::get(),
			proposal.clone(),
		);
		let mut backers: Vec<T::AccountId> = vec![];
		for i in 1..a {
			let account: T::AccountId = account("doogle", i, i);
			T::FeePayment::mint_to_account(account.clone(), stake);
			backers.push(account);
		}
		Backers::<T>::insert(proposal, backers);
	} : {
		Pallet::<T>::on_initialize(1u32.into());
	} verify {
		assert!(GovKeyUpdateAwaitingEnactment::<T>::get().is_some());
	}
	on_initialize_execute_proposal {
		GovKeyUpdateAwaitingEnactment::<T>::set(Some((1u32.into(), <<T as pallet::Config>::Chain as cf_chains::ChainCrypto>::GovKey::benchmark_value())));
	}: {
		Pallet::<T>::on_initialize(1u32.into());
	} verify {
		assert!(GovKeyUpdateAwaitingEnactment::<T>::get().is_none());
	}
	submit_proposal {
		let caller: T::AccountId = whitelisted_caller();
		T::FeePayment::mint_to_account(caller.clone(), T::ProposalFee::get());
	}: _(RawOrigin::Signed(whitelisted_caller()), generate_proposal::<T>())
	verify {
		assert!(Proposals::<T>::contains_key(<frame_system::Pallet<T>>::block_number() + T::VotingPeriod::get()));
	}
	back_proposal {
		let caller: T::AccountId = whitelisted_caller();
		let a in 1..1000;
		let proposal = generate_proposal::<T>();
		let backers = (0..a).map(|i| account::<T::AccountId>("signers", i, 0)).collect::<Vec<T::AccountId>>();
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
