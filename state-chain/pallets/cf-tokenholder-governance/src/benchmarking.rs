#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, whitelisted_caller, vec, Vec};
use frame_system::RawOrigin;
use cf_chains::benchmarking_value::BenchmarkValue;
use frame_benchmarking::account;
use cf_traits::StakingInfo;
use cf_traits::FeePayment;

fn generate_proposal<T: Config>() -> Proposal<T> {
    Proposal::SetGovernanceKey(<<T as pallet::Config>::Chain as cf_chains::ChainCrypto>::GovKey::benchmark_value())
}

benchmarks! {
    on_initialize_resolve_votes {
        // Number of bakers
        let a in 10..1000;
        let stake = <T as pallet::Config>::Balance::from(50_000_000_000_000_000_000_000u128);
        let total_onchain_funds = T::StakingInfo::onchain_funds();
        VotingPeriod::<T>::set(1u32.into());
        EnactmentDelay::<T>::set(10u32.into());
        let proposal = generate_proposal::<T>();
        Proposals::<T>::insert(
            VotingPeriod::<T>::get(),
            proposal.clone(),
        );
        let mut bakers: Vec<T::AccountId> = vec![];
        for i in 1..a {
            let account: T::AccountId = account("doogle", i, i);
            T::FeePayment::mint_to_account(account.clone(), stake);
            bakers.push(account);
        }
        Backers::<T>::insert(proposal, bakers);
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
        T::FeePayment::mint_to_account(caller.clone(), ProposalFee::<T>::get());
    }: _(RawOrigin::Signed(whitelisted_caller()), generate_proposal::<T>())
    verify {
        assert!(Proposals::<T>::contains_key(<frame_system::Pallet<T>>::block_number() + VotingPeriod::<T>::get()));
    }
    back_proposal {
        let caller: T::AccountId = whitelisted_caller();
        let proposal = generate_proposal::<T>();
        Proposals::<T>::insert(
            <frame_system::Pallet<T>>::block_number() + VotingPeriod::<T>::get(),
            proposal.clone(),
        );
    }: _(RawOrigin::Signed(caller.clone()), proposal.clone())
    verify {
        assert!(Backers::<T>::get(proposal).contains(&caller));
    }
}