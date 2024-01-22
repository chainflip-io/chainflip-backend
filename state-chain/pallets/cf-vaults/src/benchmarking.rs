//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::Pallet;
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_traits::EpochInfo;
use codec::Decode;
use frame_benchmarking::benchmarks_instance_pallet;
use frame_support::traits::UnfilteredDispatchable;

// Note: Currently we only have one chain (ETH) - as soon we've
// another chain we've to take this in account in our weight calculation benchmark.

const TX_HASH: [u8; 32] = [0xab; 32];

benchmarks_instance_pallet! {

	vault_key_rotated {
		let new_public_key = AggKeyFor::<T, I>::benchmark_value();
		PendingVaultActivation::<T, I>::put(
			VaultActivationStatus::<T, I>::AwaitingActivation { new_public_key },
		);
		let call = Call::<T, I>::vault_key_rotated {
			block_number: 5u32.into(),
			tx_id: Decode::decode(&mut &TX_HASH[..]).unwrap()
		};
		let origin = T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(VaultStartBlockNumbers::<T, I>::contains_key(T::EpochInfo::epoch_index().saturating_add(1)));
	}
	vault_key_rotated_externally {
		let origin = T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap();
		let call = Call::<T, I>::vault_key_rotated_externally {
			new_public_key: AggKeyFor::<T, I>::benchmark_value(),
			block_number: 5u32.into(),
			tx_id: Decode::decode(&mut &TX_HASH[..]).unwrap()
		};
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(VaultStartBlockNumbers::<T, I>::contains_key(T::EpochInfo::epoch_index().saturating_add(1)));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
