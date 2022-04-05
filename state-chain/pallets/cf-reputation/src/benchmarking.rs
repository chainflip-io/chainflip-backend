//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;

benchmarks! {
	update_accrual_ratio {
	} : _(RawOrigin::Root, 2, 151u32.into())
	set_penalty {
	} : _(RawOrigin::Root, PalletOffence::MissedHeartbeat.into(), Default::default())
	update_missed_heartbeat_penalty {
		 let reputation_points_penalty = ReputationPenaltyRate { points: 1, per_blocks: (10 as u32).into() };
	} : _(RawOrigin::Root, reputation_points_penalty)
	verify {
		assert_eq!(
			Pallet::<T>::resolve_penalty_for(PalletOffence::MissedHeartbeat),
			Penalty { reputation: 15, suspension: 0_u32.into() }
		);
	}
	on_runtime_upgrade {
	} : {
		<Pallet::<T> as Hooks<_>>::on_runtime_upgrade();
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
