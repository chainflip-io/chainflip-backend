//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_support::traits::{OnInitialize, OnRuntimeUpgrade};
use frame_system::RawOrigin;

const MINT_INTERVAL: u32 = 100;

benchmarks! {
	// Benchmark for the backup node emission inflation update extrinsic
	update_backup_node_emission_inflation {
	}: _(RawOrigin::Root, 100u32.into())
	verify {
		assert_eq!(CurrentAuthorityEmissionInflation::<T>::get(), 1000);
	}
	update_current_authority_emission_inflation {
	}: _(RawOrigin::Root, (100 as u32).into())
	verify {
		assert_eq!(BackupNodeEmissionInflation::<T>::get(), 100);
	}
	no_rewards_minted {
	} : {
		Pallet::<T>::on_initialize(5u32.into());
	}
	verify {
		assert_eq!(LastMintBlock::<T>::get(), 0u32.into());
	}
	// Benchmark for the rewards minted case in the on init hook
	rewards_minted {
	}: {
		Pallet::<T>::on_initialize((MINT_INTERVAL).into());
	}
	verify {
		assert_eq!(LastMintBlock::<T>::get(), MINT_INTERVAL.into());
	}
	update_mint_interval {
	}: _(RawOrigin::Root, (50 as u32).into())
	verify {
		 let mint_interval = Pallet::<T>::mint_interval();
		 assert_eq!(mint_interval, (50 as u32).into());
	}
	on_runtime_upgrade {
		StorageVersion::new(0).put::<Pallet<T>>();
	} : {
		Pallet::<T>::on_runtime_upgrade();
	} verify {
		assert_eq!(MintInterval::<T>::get(), 100u32.into());
	}
}

impl_benchmark_test_suite!(
	Pallet,
	crate::mock::new_test_ext(Default::default(), Default::default()),
	crate::mock::Test,
);
