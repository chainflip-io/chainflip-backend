//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;
use frame_support::{
	dispatch::UnfilteredDispatchable,
	traits::{EnsureOrigin, OnInitialize},
};

use codec::Encode;
use sp_std::vec;

const SUPPLY_UPDATE_INTERVAL: u32 = 100;

benchmarks! {
	// Benchmark for the backup node emission inflation update extrinsic
	update_backup_node_emission_inflation {
		let call = Call::<T>::update_backup_node_emission_inflation{inflation: 100u32};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	}
	verify {
		assert_eq!(CurrentAuthorityEmissionInflation::<T>::get(), 2720);
	}
	update_current_authority_emission_inflation {
		let call = Call::<T>::update_current_authority_emission_inflation{inflation: 100u32};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	}
	verify {
		assert_eq!(BackupNodeEmissionInflation::<T>::get(), 284);
	}
	//TODO: Benchmarks for the case where the supply update is broadcasted, the future case where rewards for backup nodes are minted.
	// Benchmark for the rewards minted case in the on init hook
	rewards_minted {
		use sp_runtime::{Digest, DigestItem};
		type System<T> = frame_system::Pallet<T>;
		let author_slot = 1u32;
		let pre_digest =
			Digest { logs: vec![DigestItem::PreRuntime(*b"aura", (author_slot as u64).encode())] };
		System::<T>::reset_events();
		System::<T>::initialize(&author_slot.into(), &System::<T>::parent_hash(), &pre_digest);
	}: {
		Pallet::<T>::on_initialize(author_slot.into());
	}
	verify {}
	update_supply_update_interval {
		let call = Call::<T>::update_supply_update_interval { value: SUPPLY_UPDATE_INTERVAL.into() };
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	}
	verify {
		 let supply_update_interval = Pallet::<T>::supply_update_interval();
		 assert_eq!(supply_update_interval, (100_u32).into());
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(Default::default(), Default::default()),
		crate::mock::Test,
	);
}
