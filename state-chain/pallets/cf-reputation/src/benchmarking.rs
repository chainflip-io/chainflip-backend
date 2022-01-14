//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;

#[allow(unused)]
use crate::Pallet as Reputation;

benchmarks! {
	update_accrual_ratio {
	} : _(RawOrigin::Root, 2, (151 as u32).into())
	// verify {
	// 	assert_eq!(Pallet::<T>::accrual_ratio(), (2, 150).into())
	// }
	update_reputation_point_penalty {
		 let reputation_points_penalty = ReputationPenalty { points: 1, blocks: (10 as u32).into() };
	} : _(RawOrigin::Root, reputation_points_penalty)
	verify {
		assert!(ReputationPointPenalty::<T>::get().is_some());
	}
	on_runtime_upgrade {
	} : {
		Reputation::<T>::on_runtime_upgrade();
	} verify {
		assert!(ReputationPointPenalty::<T>::get().is_some());
	}

}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
