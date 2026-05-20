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

//! Regression tests for unsigned validation of `non_native_signed_call`.
//!
//! These cover the "Future-nonce non-native signed calls can be replayed at
//! block import" finding. The vulnerable behaviour was that
//! `validate_metadata()` accepted any `tx_nonce >= current_nonce`, including
//! for the block-import path (`ValidateUnsigned::pre_dispatch`), while
//! `pre_dispatch()` only increments the account nonce by one. A future-nonce
//! payload could therefore be included multiple times in a block, replaying
//! the signed inner call.
//!
//! The fix makes nonce validation source-aware: the transaction-pool path
//! (`Local`/`External`) still accepts future nonces (so the pool can order
//! them via `requires`/`provides` tags), but the block-import path
//! (`InBlock`, used by `pre_dispatch`) requires an exact nonce match against
//! the signer's current account nonce.
//!
//! The tests route extrinsics through the real runtime `Executive`
//! (`Executive::apply_extrinsic` for block import, `Executive::validate_transaction`
//! for the pool). That exercises the runtime-level `ValidateUnsigned`
//! aggregation produced by `#[frame_support::runtime]`, so a misconfiguration
//! (for example, if `cf-environment`'s `ValidateUnsigned` impl were no
//! longer wired to the runtime) would fail these tests rather than slip
//! through. The inner call is a trivial `remark`; the security property
//! lives entirely in nonce validation and is independent of the inner call.

use cf_chains::sol::{
	signing_key::SolSigningKey, sol_tx_core::signer::Signer, SolAddress, SolSignature,
};
use cf_traits::Funding as FundingTrait;
use cf_utilities::assert_ok;
use frame_support::{
	pallet_prelude::{InvalidTransaction, TransactionSource, TransactionValidityError},
	sp_runtime::{transaction_validity::TransactionValidity, ApplyExtrinsicResult},
	traits::HandleLifetime,
};
use pallet_cf_environment::{
	submit_runtime_call::{ChainflipExtrinsic, SignatureData},
	ChainflipNetworkName, SolEncodingType, TransactionMetadata,
};
use state_chain_runtime::{AccountId, Executive, Flip, Runtime, RuntimeCall, UncheckedExtrinsic};

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
