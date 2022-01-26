//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;

benchmarks! {
	update_accrual_ratio {
	} : _(RawOrigin::Root, 2, (151 as u32).into())
	update_reputation_point_penalty {
		 let reputation_points_penalty = ReputationPenalty { points: 1, blocks: (10 as u32).into() };
	} : _(RawOrigin::Root, reputation_points_penalty)
	verify {
		assert_eq!(ReputationPointPenalty::<T>::get(), ReputationPenalty { points: 1, blocks: (10 as u32).into() });
	}
	on_runtime_upgrade {
		releases::V1.put::<Pallet<T>>();
	} : {
		Pallet::<T>::on_runtime_upgrade();
	} verify {
		assert_eq!(ReputationPointPenalty::<T>::get(), ReputationPenalty { points: 1, blocks: (10 as u32).into() });
	}
	on_runtime_upgrade_v1 {
		releases::V0.put::<Pallet<T>>();
	} : {
		Pallet::<T>::on_runtime_upgrade();
	}

}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
