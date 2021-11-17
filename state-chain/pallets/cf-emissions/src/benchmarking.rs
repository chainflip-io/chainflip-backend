//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::traits::OnInitialize;
use frame_system::RawOrigin;

const MINT_INTERVAL: u32 = 100;

#[allow(unused)]
use crate::Pallet as Emissions;

benchmarks! {
	// Benchmark for the backup validator extrinsic
	update_backup_validator_emission_inflation {
		let b in 1 .. 1_000;
	}: _(RawOrigin::Root, b.into())
	update_validator_emission_inflation {
		let b in 1 .. 1_000;
	}: _(RawOrigin::Root, b.into())
	no_rewards_minted {

	} : {
		Emissions::<T>::on_initialize((5 as u32).into());
	}
	// Benchmark for the rewards minted case in the on init hook
	rewards_minted {
	}: {
		Emissions::<T>::on_initialize((MINT_INTERVAL).into());
	}
}

impl_benchmark_test_suite!(
	Pallet,
	crate::mock::new_test_ext(Default::default(), Default::default()),
	crate::mock::Test,
);
