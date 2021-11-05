//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

#[allow(unused)]
use crate::Pallet;

benchmarks! {
	keygen_success {
		let chain_id = ChainId::Ethereum;
		let caller: T::AccountId = whitelisted_caller();
		let new_public_key: [u8; 33] = [0x02; 33];
		// 1. Pass the active rotation check
		PendingVaultRotations::<T>::insert(
			chain_id,
			VaultRotationStatus::<T>::AwaitingRotation {  new_public_key: new_public_key.to_vec() },
		);
		let ceremony_id = Pallet::<T>::keygen_ceremony_id_counter();
		let call = Call::<T>::keygen_success(ceremony_id, chain_id, new_public_key.to_vec());
		let origin = T::EnsureWitnessed::successful_origin();
		// 2. Pass invalid rotations status check
	} : { call.dispatch_bypass_filter(origin)? }
	keygen_failure {
		let chain_id = ChainId::Ethereum;
		let caller: T::AccountId = whitelisted_caller();
		let new_public_key: [u8; 33] = [0x02; 33];
		// 1. Pass the active rotation check
		PendingVaultRotations::<T>::insert(
			chain_id,
			VaultRotationStatus::<T>::AwaitingRotation {  new_public_key: new_public_key.to_vec() },
		);
		let ceremony_id = Pallet::<T>::keygen_ceremony_id_counter();
		let call = Call::<T>::keygen_failure(ceremony_id, chain_id, vec![]);
		let origin = T::EnsureWitnessed::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	vault_key_rotated {
		let chain_id = ChainId::Ethereum;
		let caller: T::AccountId = whitelisted_caller();
		let new_public_key: [u8; 33] = [0x02; 33];
		let tx_hash: [u8; 32] = [0xab; 32];

		PendingVaultRotations::<T>::insert(
			chain_id,
			VaultRotationStatus::<T>::AwaitingRotation {  new_public_key: new_public_key.to_vec() },
		);
		let call = Call::<T>::vault_key_rotated(chain_id, new_public_key.to_vec(), 5 as u64, tx_hash.to_vec());
		let origin = T::EnsureWitnessed::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
