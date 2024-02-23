#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::{Chainflip, FeePayment};
use frame_benchmarking::v2::*;
use frame_support::sp_runtime::traits::UniqueSaturatedFrom;
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use sp_std::collections::btree_set::BTreeSet;

fn generate_proposal() -> Proposal {
	Proposal::SetGovernanceKey(ForeignChain::Ethereum, vec![1; 32])
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn on_initialize_resolve_votes(a: Linear<10, 1_000>) {
		// a: Number of backers

		let proposal = generate_proposal();
		Proposals::<T>::insert(BlockNumberFor::<T>::from(1u32), proposal.clone());
		let backers = (0..a).map(|i| account("doogle", i, 0)).collect::<BTreeSet<_>>();
		for account in &backers {
			T::FeePayment::mint_to_account(
				account,
				<T as Chainflip>::Amount::unique_saturated_from(50_000_000_000_000_000_000_000u128),
			);
		}
		Backers::<T>::insert(proposal, backers);

		#[block]
		{
			Pallet::<T>::on_initialize(1u32.into());
		}

		assert!(GovKeyUpdateAwaitingEnactment::<T>::get().is_some());
	}

	#[benchmark]
	fn on_initialize_execute_proposal() {
		GovKeyUpdateAwaitingEnactment::<T>::set(Some((
			1u32.into(),
			(ForeignChain::Ethereum, vec![1; 32]),
		)));

		#[block]
		{
			Pallet::<T>::on_initialize(1u32.into());
		}

		assert!(GovKeyUpdateAwaitingEnactment::<T>::get().is_none());
	}

	#[benchmark]
	fn submit_proposal() {
		let caller: T::AccountId = whitelisted_caller();
		T::FeePayment::mint_to_account(&caller, T::ProposalFee::get());

		#[extrinsic_call]
		submit_proposal(RawOrigin::Signed(whitelisted_caller()), generate_proposal());

		assert!(Proposals::<T>::contains_key(
			<frame_system::Pallet<T>>::block_number() + T::VotingPeriod::get()
		));
	}

	#[benchmark]
	fn back_proposal(a: Linear<1, 1_000>) {
		let caller: T::AccountId = whitelisted_caller();
		let proposal = generate_proposal();
		let backers = (0..a)
			.map(|i| account::<T::AccountId>("signers", i, 0))
			.collect::<BTreeSet<_>>();
		Proposals::<T>::insert(
			<frame_system::Pallet<T>>::block_number() + T::VotingPeriod::get(),
			proposal.clone(),
		);
		Backers::<T>::insert(proposal.clone(), backers);

		#[extrinsic_call]
		back_proposal(RawOrigin::Signed(caller.clone()), proposal.clone());

		assert!(Backers::<T>::get(proposal).contains(&caller));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
