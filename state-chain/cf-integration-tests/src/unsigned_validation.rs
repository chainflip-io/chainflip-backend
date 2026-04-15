// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

//! End-to-end test for the `non_native_signed_call` block-import validation.
//!
//! Background: a previous version of `pallet_cf_environment::ValidateUnsigned::pre_dispatch`
//! only checked the nonce and skipped `is_valid_signature`. Because `pre_dispatch` is the
//! validation that Substrate's executive runs for unsigned extrinsics during *block
//! import*, a malicious block author could include a forged `non_native_signed_call` whose
//! `SignatureData::Solana::signer` aliases an arbitrary on-chain account (the 32 bytes are
//! copied straight into an `AccountId32`) without a valid signature, and the call would
//! dispatch as the impersonated account.
//!
//! This test exercises the real `Executive::apply_extrinsic` path — the exact block-import
//! code path that the original vulnerability lived on — and asserts that a forged
//! impersonation extrinsic is rejected with `BadProof`.

use super::*;
use cf_chains::sol::{SolAddress, SolSignature};
use cf_primitives::ChainflipNetwork;
use codec::Encode;
use frame_support::pallet_prelude::InvalidTransaction;
use pallet_cf_environment::{
	submit_runtime_call::{ChainflipExtrinsic, SignatureData},
	ChainflipNetworkName, SolEncodingType, TransactionMetadata,
};
use sp_consensus_aura::SlotDuration;
use sp_runtime::transaction_validity::TransactionValidityError;
use state_chain_runtime::{
	AllPalletsWithSystem, Executive, PalletExecutionOrder, UncheckedExtrinsic,
};

const SLOT_DURATION: u64 = 6000;

/// Minimal block-import harness: initialise a fresh block so the system is in
/// `ApplyExtrinsic` phase, dispatch the timestamp inherent, then yield to the caller.
/// Mirrors the production executive's `initialize_block` + inherents sequence without
/// requiring a full `testnet` network.
fn with_initialised_block<R>(f: impl FnOnce() -> R) -> R {
	use frame_support::{
		inherent::ProvideInherent, pallet_prelude::InherentData, traits::UnfilteredDispatchable,
	};

	let block_number = System::block_number() + 1;
	let timestamp = SLOT_DURATION * u64::from(block_number);
	let slot = sp_consensus_aura::Slot::from_timestamp(
		sp_timestamp::Timestamp::new(timestamp),
		SlotDuration::from_millis(SLOT_DURATION),
	);
	let mut inherent_data = InherentData::new();
	inherent_data.put_data(sp_timestamp::INHERENT_IDENTIFIER, &timestamp).unwrap();
	inherent_data
		.put_data(sp_consensus_aura::inherents::INHERENT_IDENTIFIER, &slot)
		.unwrap();

	let mut digest = sp_runtime::Digest::default();
	digest.push(sp_runtime::DigestItem::PreRuntime(
		sp_consensus_aura::AURA_ENGINE_ID,
		slot.encode(),
	));

	System::reset_events();
	System::initialize(&block_number, &System::block_hash(block_number), &digest);
	PalletExecutionOrder::on_initialize(block_number);
	assert_ok!(state_chain_runtime::Timestamp::create_inherent(&inherent_data)
		.unwrap()
		.dispatch_bypass_filter(RuntimeOrigin::none()));

	let result = f();

	AllPalletsWithSystem::on_idle(block_number, Weight::from_parts(2_000_000_000_000, u64::MAX));
	PalletExecutionOrder::on_finalize(block_number);
	result
}

/// The canonical CVE scenario: a malicious block author embeds a forged
/// `non_native_signed_call` whose Solana signer aliases an existing on-chain account
/// (here, the genesis validator ALICE) with a garbage signature. With the original bug,
/// `Executive::apply_extrinsic` would have accepted it and dispatched the inner call as
/// ALICE. With the fix, it must be rejected with `BadProof`, leaving ALICE's nonce
/// untouched.
#[test]
fn executive_rejects_forged_non_native_signed_call() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		// Sanity: ALICE is a funded genesis validator — a realistic impersonation target.
		let victim = AccountId::from(ALICE);
		assert!(frame_system::Account::<Runtime>::contains_key(&victim));
		assert_eq!(ChainflipNetworkName::<Runtime>::get(), ChainflipNetwork::Development);

		// A forged unsigned extrinsic: arbitrary inner call dispatched "as ALICE" with a
		// zero signature. The 32 bytes of ALICE's AccountId32 are reused as the Solana
		// signer so `signer_account()` decodes back to ALICE.
		let inner_call: RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();
		let forged_call: RuntimeCall =
			pallet_cf_environment::Call::<Runtime>::non_native_signed_call {
				chainflip_extrinsic: ChainflipExtrinsic {
					call: Box::new(inner_call),
					transaction_metadata: TransactionMetadata {
						nonce: 0,
						expiry_block: 10_000,
					},
				},
				signature_data: SignatureData::Solana {
					signature: SolSignature([0u8; 64]),
					signer: SolAddress(<[u8; 32]>::from(victim.clone())),
					sig_type: SolEncodingType::Domain,
				},
			}
			.into();
		let extrinsic = UncheckedExtrinsic::new_bare(forged_call);

		let result = with_initialised_block(|| Executive::apply_extrinsic(extrinsic));

		assert_eq!(
			result,
			Err(TransactionValidityError::Invalid(InvalidTransaction::BadProof)),
			"forged non_native_signed_call must be rejected at block import",
		);
		assert_eq!(
			frame_system::Pallet::<Runtime>::account_nonce(&victim),
			0,
			"victim's nonce must not be bumped when validation fails",
		);
	});
}
