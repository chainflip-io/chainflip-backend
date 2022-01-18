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
	}: _(RawOrigin::Root, 100u32.into())
	verify {
		assert_eq!(ValidatorEmissionInflation::<T>::get(), 1000);
	}
	update_validator_emission_inflation {
	}: _(RawOrigin::Root, (100 as u32).into())
	verify {
		assert_eq!(BackupValidatorEmissionInflation::<T>::get(), 100);
	}
	no_rewards_minted {
	} : {
		Emissions::<T>::on_initialize(5u32.into());
	}
	verify {
		assert_eq!(LastMintBlock::<T>::get(), 5u32.into());
	}
	// Benchmark for the rewards minted case in the on init hook
	rewards_minted {
	}: {
		Emissions::<T>::on_initialize((MINT_INTERVAL).into());
	}
	verify {
		assert_eq!(LastMintBlock::<T>::get(), MINT_INTERVAL.into());
	}
}

impl_benchmark_test_suite!(
	Pallet,
	crate::mock::new_test_ext(Default::default(), Default::default()),
	crate::mock::Test,
);
