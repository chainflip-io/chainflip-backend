#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::Pallet;
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_traits::EpochInfo;
use codec::Decode;
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::UnfilteredDispatchable};

// Note: Currently we only have one chain (ETH) - as soon we've
// another chain we've to take this in account in our weight calculation benchmark.

const TX_HASH: [u8; 32] = [0xab; 32];

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn vault_key_rotated_externally() {
		let origin = T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap();
		let call = Call::<T, I>::vault_key_rotated_externally {
			new_public_key: AggKeyFor::<T, I>::benchmark_value(),
			block_number: 5u32.into(),
			tx_id: Decode::decode(&mut &TX_HASH[..]).unwrap(),
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert!(VaultStartBlockNumbers::<T, I>::contains_key(
			T::EpochInfo::epoch_index().saturating_add(1)
		));
	}

	#[cfg(test)]
	use crate::mock::*;

	#[test]
	fn benchmark_works() {
		new_test_ext().execute_with(|| {
			_vault_key_rotated_externally::<Test, ()>(true);
		});
	}
}
