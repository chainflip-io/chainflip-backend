//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;
use frame_support::traits::OnInitialize;
use frame_system::RawOrigin;

const SUPPLY_UPDATE_INTERVAL: u32 = 100;

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
		assert_eq!(LastSupplyUpdateBlock::<T>::get(), 0u32.into());
	}
	// Benchmark for the rewards minted case in the on init hook
	rewards_minted {
	}: {
		Pallet::<T>::on_initialize((SUPPLY_UPDATE_INTERVAL).into());
	}
	verify {
		assert_eq!(LastSupplyUpdateBlock::<T>::get(), SUPPLY_UPDATE_INTERVAL.into());
	}
	update_supply_update_interval {
	}: _(RawOrigin::Root, (50 as u32).into())
	verify {
		 let mint_interval = Pallet::<T>::supply_update_interval();
		 assert_eq!(mint_interval, (50 as u32).into());
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(Default::default(), Default::default()),
		crate::mock::Test,
	);
}
