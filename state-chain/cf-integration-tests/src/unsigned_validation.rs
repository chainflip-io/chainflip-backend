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

//! Regression tests for block-import unsigned validation of
//! `non_native_signed_call`.
//!
//! These cover two related findings in
//! `pallet_cf_environment::ValidateUnsigned::pre_dispatch` — the validation
//! Substrate's executive runs for unsigned extrinsics during *block import*:
//!
//! 1. **Future-nonce replay.** `validate_metadata()` accepted any `tx_nonce >= current_nonce`,
//!    including for the block-import path, while `pre_dispatch()` only increments the account nonce
//!    by one. A future-nonce payload could therefore be included multiple times in a block,
//!    replaying the signed inner call. The fix makes nonce validation source-aware: the
//!    transaction-pool path (`Local`/`External`) still accepts future nonces (so the pool can order
//!    them via `requires`/`provides` tags), but the block-import path (`InBlock`, used by
//!    `pre_dispatch`) requires an exact nonce match against the signer's current account nonce.
//!
//! 2. **Forged-signature impersonation.** A previous version of `pre_dispatch` only checked the
//!    nonce and skipped `is_valid_signature`. A malicious block author could include a forged
//!    `non_native_signed_call` whose `SignatureData::Solana::signer` aliases an arbitrary on-chain
//!    account (the 32 bytes are copied straight into an `AccountId32`) without a valid signature,
//!    and the call would dispatch as the impersonated account. The fix rejects such extrinsics with
//!    `BadProof`.
//!
//! The tests route extrinsics through the real runtime `Executive`
//! (`Executive::apply_extrinsic` for block import, `Executive::validate_transaction`
//! for the pool). That exercises the runtime-level `ValidateUnsigned`
//! aggregation produced by `#[frame_support::runtime]`, so a misconfiguration
//! (for example, if `cf-environment`'s `ValidateUnsigned` impl were no
//! longer wired to the runtime) would fail these tests rather than slip
//! through. The inner call is a trivial `remark`; the security properties
//! live entirely in nonce and signature validation and are independent of the
//! inner call.

use super::*;
use cf_chains::sol::{
	signing_key::SolSigningKey, sol_tx_core::signer::Signer, SolAddress, SolSignature,
};
use cf_primitives::ChainflipNetwork;
use cf_traits::Funding as FundingTrait;
use cf_utilities::assert_ok;
use codec::Encode;
use frame_support::{
	pallet_prelude::{InvalidTransaction, TransactionSource, TransactionValidityError},
	sp_runtime::{transaction_validity::TransactionValidity, ApplyExtrinsicResult},
	traits::HandleLifetime,
};
use pallet_cf_environment::{
	submit_runtime_call::{ChainflipExtrinsic, SignatureData},
	ChainflipNetworkName, SolEncodingType, TransactionMetadata,
};
use sp_consensus_aura::SlotDuration;
use state_chain_runtime::{
	AccountId, AllPalletsWithSystem, Executive, Flip, PalletExecutionOrder, Runtime, RuntimeCall,
	UncheckedExtrinsic,
};

/// Builds a valid Solana-signed `non_native_signed_call` for `signing_key`
/// over `inner_call` at the given `nonce`, mirroring how an off-chain wallet
/// constructs the payload.
fn signed_sol_non_native_call(
	signing_key: &SolSigningKey,
	nonce: u32,
	inner_call: RuntimeCall,
) -> (pallet_cf_environment::Call<Runtime>, AccountId) {
	let signer = SolAddress(signing_key.pubkey().0);
	let transaction_metadata = TransactionMetadata { nonce, expiry_block: 10_000 };
	let domain = pallet_cf_environment::submit_runtime_call::build_domain_data(
		&inner_call,
		&ChainflipNetworkName::<Runtime>::get(),
		&transaction_metadata,
		state_chain_runtime::VERSION.spec_version,
	);
	let message = format!("{}{}", pallet_cf_environment::DOMAIN_OFFCHAIN_PREFIX, domain);
	let signature = SolSignature::from(signing_key.sign_message(message.as_bytes()).0);
	let signature_data =
		SignatureData::Solana { signature, signer, sig_type: SolEncodingType::Domain };
	let account = signature_data.signer_account().unwrap();

	(
		pallet_cf_environment::Call::<Runtime>::non_native_signed_call {
			chainflip_extrinsic: ChainflipExtrinsic {
				call: Box::new(inner_call),
				transaction_metadata,
			},
			signature_data,
		},
		account,
	)
}

fn remark_call() -> RuntimeCall {
	frame_system::Call::<Runtime>::remark { remark: vec![] }.into()
}

/// Creates and funds `signer_account` so unsigned validation (account
/// existence + fee validation) can succeed.
fn provision_signer(signer_account: &AccountId) {
	frame_system::Provider::<Runtime>::created(signer_account).unwrap();
	<Flip as FundingTrait>::credit_funds(signer_account, super::genesis::GENESIS_BALANCE);
}

fn into_bare_uxt(call: pallet_cf_environment::Call<Runtime>) -> UncheckedExtrinsic {
	UncheckedExtrinsic::new_bare(RuntimeCall::from(call))
}

/// Apply an extrinsic the way block import does: the runtime's `Executive`
/// runs the aggregated `ValidateUnsigned::pre_dispatch` and then dispatches.
fn apply(call: pallet_cf_environment::Call<Runtime>) -> ApplyExtrinsicResult {
	Executive::apply_extrinsic(into_bare_uxt(call))
}

/// Validate an extrinsic the way the transaction pool does (uses `Executive::validate_transaction`,
/// which is what the `TaggedTransactionQueue` runtime API calls).
fn validate_in_pool(
	source: TransactionSource,
	call: pallet_cf_environment::Call<Runtime>,
) -> TransactionValidity {
	let parent_hash = frame_system::Pallet::<Runtime>::block_hash(
		frame_system::Pallet::<Runtime>::block_number(),
	);
	Executive::validate_transaction(source, into_bare_uxt(call), parent_hash)
}

const STALE_OUTCOME: ApplyExtrinsicResult =
	Err(TransactionValidityError::Invalid(InvalidTransaction::Stale));

#[test]
fn future_nonce_non_native_call_rejected_at_block_import() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let signing_key = SolSigningKey::new();

		// The on-chain account nonce is 0, but the payload authorises nonce 2.
		let (call, signer_account) = signed_sol_non_native_call(&signing_key, 2, remark_call());
		provision_signer(&signer_account);

		assert_eq!(frame_system::Pallet::<Runtime>::account_nonce(&signer_account), 0);

		// Transaction-pool validation still accepts future nonces: the pool
		// orders them with requires/provides tags. This is the supported
		// behaviour the fix deliberately preserves.
		assert_ok!(validate_in_pool(TransactionSource::External, call.clone()));

		// Block-import (Executive::apply_extrinsic) must reject the future nonce.
		// Before the fix this returned Ok and incremented the nonce, allowing
		// the same signed payload to be replayed within a block.
		assert_eq!(apply(call), STALE_OUTCOME);

		// The rejection happens before the nonce is advanced or the inner
		// call is dispatched.
		assert_eq!(frame_system::Pallet::<Runtime>::account_nonce(&signer_account), 0);
	});
}

#[test]
fn future_nonce_non_native_call_replay_blocked_at_block_import() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let signing_key = SolSigningKey::new();
		let (call, signer_account) = signed_sol_non_native_call(&signing_key, 2, remark_call());
		provision_signer(&signer_account);

		assert_eq!(frame_system::Pallet::<Runtime>::account_nonce(&signer_account), 0);

		for _ in 0..3 {
			assert_eq!(apply(call.clone()), STALE_OUTCOME);
			assert_eq!(
				frame_system::Pallet::<Runtime>::account_nonce(&signer_account),
				0,
				"rejected future-nonce replays must not advance the nonce or dispatch the inner call",
			);
		}
	});
}

#[test]
fn non_native_exact_nonce_sequence_accepted_at_block_import() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let signing_key = SolSigningKey::new();

		let (call_nonce_0, signer_account) =
			signed_sol_non_native_call(&signing_key, 0, remark_call());
		provision_signer(&signer_account);

		// nonce 0 against account nonce 0 is accepted by Executive::apply_extrinsic.
		assert_eq!(apply(call_nonce_0), Ok(Ok(())));
		assert_eq!(frame_system::Pallet::<Runtime>::account_nonce(&signer_account), 1);

		// The next sequential nonce (1) against account nonce 1 is accepted.
		// `remark { remark: vec![1] }` keeps the signed payload distinct from
		// the first call so this is a genuinely different authorisation.
		let (call_nonce_1, signer_account_1) = signed_sol_non_native_call(
			&signing_key,
			1,
			frame_system::Call::<Runtime>::remark { remark: vec![1] }.into(),
		);
		assert_eq!(signer_account_1, signer_account);

		assert_eq!(apply(call_nonce_1), Ok(Ok(())));
		assert_eq!(frame_system::Pallet::<Runtime>::account_nonce(&signer_account), 2);
	});
}

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
	digest
		.push(sp_runtime::DigestItem::PreRuntime(sp_consensus_aura::AURA_ENGINE_ID, slot.encode()));

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
		let forged_call: RuntimeCall =
			pallet_cf_environment::Call::<Runtime>::non_native_signed_call {
				chainflip_extrinsic: ChainflipExtrinsic {
					call: Box::new(remark_call()),
					transaction_metadata: TransactionMetadata { nonce: 0, expiry_block: 10_000 },
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
