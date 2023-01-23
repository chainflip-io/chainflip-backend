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
const INFLATION_RATE: u32 = 200;

fn on_initialize_setup<T: Config>(should_mint: bool) -> <T as frame_system::Config>::BlockNumber {
	use sp_runtime::{Digest, DigestItem};
	type System<T> = frame_system::Pallet<T>;
	let author_slot = 1u32;
	let pre_digest =
		Digest { logs: vec![DigestItem::PreRuntime(*b"aura", (author_slot as u64).encode())] };
	System::<T>::initialize(&author_slot.into(), &System::<T>::parent_hash(), &pre_digest);

	if should_mint {
		SupplyUpdateInterval::<T>::get() + 1u32.into()
	} else {
		1u32.into()
	}
}

benchmarks! {
	// Benchmark for the backup node emission inflation update extrinsic
	update_backup_node_emission_inflation {
		let call = Call::<T>::update_backup_node_emission_inflation{inflation: INFLATION_RATE};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	}
	verify {
		assert_eq!(BackupNodeEmissionInflation::<T>::get(), INFLATION_RATE);
	}
	update_current_authority_emission_inflation {
		let call = Call::<T>::update_current_authority_emission_inflation{inflation: INFLATION_RATE};
	}: {
		let _ = call.dispatch_bypass_filter(T::EnsureGovernance::successful_origin());
	}
	verify {
		assert_eq!(CurrentAuthorityEmissionInflation::<T>::get(), INFLATION_RATE);
	}
	rewards_minted {
		let block_number = on_initialize_setup::<T>(true);
	}: {
		Pallet::<T>::on_initialize(block_number);
	}
	rewards_not_minted {
		let block_number = on_initialize_setup::<T>(false);
	}: {
		Pallet::<T>::on_initialize(block_number);
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
