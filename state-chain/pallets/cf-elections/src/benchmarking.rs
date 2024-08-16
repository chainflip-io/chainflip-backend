#![cfg(feature = "runtime-benchmarks")]

use crate::{electoral_system::ElectoralSystem, Config, Pallet};
use cf_primitives::AccountRole;
use cf_traits::{AccountRoleRegistry, EpochInfo};
use frame_benchmarking::v2::*;
use frame_support::storage::bounded_btree_map::BoundedBTreeMap;
use frame_system::RawOrigin;
use sp_std::collections::btree_map::BTreeMap;

use crate::Call;

// Keep this to avoid CI warnings about no benchmarks in the crate.
#[instance_benchmarks]
mod benchmarks {
	use __private::traits::OnFinalize;

	use super::*;

	#[benchmark]
	fn vote() {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();

		T::EpochInfo::add_authority_info_for_epoch(1, vec![validator_id.clone()]);

		// kick off an election
		Pallet::<T, I>::on_finalize(frame_system::Pallet::<T>::block_number());

		// Set the sync barrier to 0
		let zero_sync_barrier = VoteSynchronisationBarrier::from_u32(0);
		Pallet::<T, I>::ignore_my_votes(
			RawOrigin::Signed(caller.clone()).into(),
			zero_sync_barrier.clone(),
		)
		.unwrap();

		Pallet::<T, I>::stop_ignoring_my_votes(
			RawOrigin::Signed(caller.clone()).into(),
			zero_sync_barrier.clone(),
		)
		.unwrap();

		let (elections, sync_barrier) =
			Pallet::<T, I>::validator_election_data(&validator_id).unwrap();

		let next_election = elections.into_iter().next().unwrap();

		#[extrinsic_call]
		vote(
			RawOrigin::Signed(caller),
			BoundedBTreeMap::try_from(
				[(next_election.0, T::ElectoralSystem::benchmark_authority_vote())]
					.into_iter()
					.collect::<BTreeMap<_, _>>(),
			)
			.unwrap(),
			sync_barrier.unwrap(),
		);
	}

	#[cfg(test)]
	use crate::mock::*;

	#[cfg(test)]
	use crate::Instance1;
	use crate::VoteSynchronisationBarrier;

	#[test]
	fn benchmark_works() {
		new_test_ext().execute_with(|| {
			_vote::<Test, Instance1>(true);
		});
	}
}
