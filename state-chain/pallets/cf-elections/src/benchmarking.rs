#![cfg(feature = "runtime-benchmarks")]
use super::*;

use crate::{Config, Pallet};
use frame_benchmarking::v2::*;

use frame_system::RawOrigin;

// Keep this to avoid CI warnings about no benchmarks in the crate.
#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn ignore_my_votes() {
		let caller: T::AccountId = whitelisted_caller();

		// AuthorityVoteSynchronisationBarriers::<T>::insert(
		// 	caller.clone(),
		// 	VoteSynchronisationBarrier::from_u32(0u32),
		// );

		// ContributingAuthorities::<T>::insert(caller.clone(), ());

		// ElectionConsensusHistoryUpToDate::<T, I>::insert(1, 1);

		#[extrinsic_call]
		ignore_my_votes(RawOrigin::Signed(caller), VoteSynchronisationBarrier::from_u32(0u32));
	}

	#[benchmark]
	fn stop_ignoring_my_votes() {
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		stop_ignoring_my_votes(
			RawOrigin::Signed(caller),
			VoteSynchronisationBarrier::from_u32(0u32),
		);
	}
}
