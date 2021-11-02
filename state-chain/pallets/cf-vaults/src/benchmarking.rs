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
	// TODO: implement benchmark
	keygen_success {
		let chain_id = ChainId::Ethereum;
		let caller: T::AccountId = whitelisted_caller();
		let new_public_key: [u8; 33] = [0x02; 33];
		// 1. Pass the active rotation check
		PendingVaultRotations::<T>::insert(
			chain_id,
			VaultRotationStatus::<T>::AwaitingRotation {  new_public_key: new_public_key.to_vec() },
		);
		let call = Call::<T>::keygen_success(1, chain_id, new_public_key.to_vec());
		let origin = T::EnsureWitnessed::successful_origin();
		// 2. Pass invalid rotations status check
	} : { call.dispatch_bypass_filter(origin)? }
	// TODO: implement benchmark
	keygen_failure {} : {}
	// TODO: implement benchmark
	vault_key_rotated {} : {}
	// TODO: implement benchmark
	// threshold_signature_response {
	// 	// let caller: T::AccountId = whitelisted_caller();
	// 	// let ceremony_id = Pallet::<T>::next_ceremony_id();
	// 	// // let keygen_request = KeygenRequest {
	// 	// // 	chain: Chain::Ethereum,
	// 	// // 	validator_candidates: vec![caller.clone().into()],
	// 	// // };
	// 	// // KeygenRequestResponse::<T>::make_request(ceremony_id, keygen_request);
	// 	// let origin = T::EnsureWitnessed::successful_origin();
	// 	// // TODO: fails on InvalidCeremonyId - no idea were this comes from
	// 	// let call = Call::<T>::threshold_signature_response(ceremony_id, ThresholdSignatureResponse::Success {
	// 	// 	message_hash: [0; 32],
	// 	// 	signature: SchnorrSigTruncPubkey::default(),
	// 	// });
	// } : { }
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
