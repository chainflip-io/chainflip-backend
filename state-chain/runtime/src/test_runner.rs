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

use crate::{AllPalletsWithSystem, Runtime, RuntimeGenesisConfig};
cf_test_utilities::impl_test_helpers!(Runtime);

/// Regression tests for the unsigned-call validation path used by `non_native_signed_call`.
///
/// Background: a previous version of `pallet_cf_environment::ValidateUnsigned::pre_dispatch`
/// validated only the nonce and skipped signature verification. Because `pre_dispatch` is the
/// validation that Substrate's executive runs for unsigned extrinsics during *block import*,
/// a malicious block author could include a forged `non_native_signed_call` whose
/// `SignatureData::Solana { signer: <any 32 bytes> }` impersonates an arbitrary on-chain
/// account (the 32 bytes are copied straight into an `AccountId32`). The fix routes both
/// `validate_unsigned` and `pre_dispatch` through `validate_unsigned_call`, which performs
/// the full signature check.
///
/// These tests exercise the *real* `Runtime` rather than a per-pallet mock, so they catch
/// regressions in how `Runtime` wires the pallet's `Config` (e.g. `ChainflipNetworkName`,
/// `GetTransactionPayments`, etc.).
#[cfg(test)]
mod non_native_signed_call_validation {
	use super::*;
	use crate::{AccountId, RuntimeCall};
	use cf_chains::sol::{
		signing_key::SolSigningKey, sol_tx_core::signer::Signer, SolAddress, SolSignature,
	};
	use cf_primitives::ChainflipNetwork;
	use frame_support::{
		pallet_prelude::{InvalidTransaction, TransactionSource, ValidateUnsigned},
		traits::HandleLifetime,
	};
	use pallet_cf_environment::{
		submit_runtime_call::{ChainflipExtrinsic, SignatureData},
		ChainflipNetworkName, SolEncodingType, TransactionMetadata,
	};

	const EXPIRY_BLOCK: u32 = 10_000;

	/// Create the account and fund it with enough FLIP to satisfy the runtime's payment
	/// validation, so the validation pipeline reaches the signature check rather than
	/// short-circuiting on `InvalidTransaction::Payment`.
	fn ensure_funded_account(account: &AccountId) {
		if !frame_system::Account::<Runtime>::contains_key(account) {
			frame_system::Provider::<Runtime>::created(account).unwrap();
		}
		// Seed the offchain pool so `bridge_in` (called transitively from `credit_funds`)
		// has funds to draw from. Default genesis leaves `OffchainFunds` at zero.
		const FUNDING: u128 = 1_000 * cf_primitives::FLIPPERINOS_PER_FLIP;
		pallet_cf_flip::OffchainFunds::<Runtime>::mutate(|f| *f = f.saturating_add(FUNDING));
		let new_balance =
			<pallet_cf_flip::Pallet<Runtime> as cf_traits::Funding>::credit_funds(
				account, FUNDING,
			);
		assert!(
			new_balance > 0,
			"funding helper failed to credit the account (got balance {})",
			new_balance,
		);
	}

	fn build_signed_call(
		signing_key: &SolSigningKey,
		nonce: u32,
	) -> (pallet_cf_environment::Call<Runtime>, AccountId) {
		let signer = SolAddress(signing_key.pubkey().0);
		let transaction_metadata = TransactionMetadata { nonce, expiry_block: EXPIRY_BLOCK };
		let inner_call: RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();

		let domain = pallet_cf_environment::build_domain_data(
			&inner_call,
			&ChainflipNetworkName::<Runtime>::get(),
			&transaction_metadata,
			crate::VERSION.spec_version,
		);
		let message =
			format!("{}{}", pallet_cf_environment::DOMAIN_OFFCHAIN_PREFIX, domain);
		let signature = SolSignature::from(signing_key.sign_message(message.as_bytes()).0);

		let signature_data = SignatureData::Solana {
			signature,
			signer,
			sig_type: SolEncodingType::Domain,
		};
		let account: AccountId = signature_data.signer_account().unwrap();

		let call = pallet_cf_environment::Call::non_native_signed_call {
			chainflip_extrinsic: ChainflipExtrinsic {
				call: Box::new(inner_call),
				transaction_metadata,
			},
			signature_data,
		};
		(call, account)
	}

	#[test]
	fn pre_dispatch_rejects_bad_signature_against_real_runtime() {
		new_test_ext().execute_with(|| {
			ChainflipNetworkName::<Runtime>::set(ChainflipNetwork::Development);

			let key = SolSigningKey::new();
			let (mut call, account) = build_signed_call(&key, 0);
			ensure_funded_account(&account);

			// Tamper the signature.
			if let pallet_cf_environment::Call::non_native_signed_call { signature_data, .. } =
				&mut call
			{
				if let SignatureData::Solana { signature, .. } = signature_data {
					signature.0[0] ^= 0x01;
				}
			}

			assert_eq!(
				<pallet_cf_environment::Pallet<Runtime> as ValidateUnsigned>::pre_dispatch(&call),
				Err(InvalidTransaction::BadProof.into()),
			);
			assert_eq!(
				<pallet_cf_environment::Pallet<Runtime> as ValidateUnsigned>::validate_unsigned(
					TransactionSource::InBlock,
					&call,
				),
				Err(InvalidTransaction::BadProof.into()),
			);
		});
	}

	/// Regression for the impersonation vector: a forged `SignatureData::Solana` whose
	/// 32-byte signer is chosen to alias any `AccountId32` in the system. With the original
	/// bug, this would have passed `pre_dispatch` and dispatched the inner call as the
	/// victim. The fix must reject it with `BadProof`.
	#[test]
	fn solana_impersonation_attempt_is_rejected_against_real_runtime() {
		new_test_ext().execute_with(|| {
			ChainflipNetworkName::<Runtime>::set(ChainflipNetwork::Development);

			// Pick an arbitrary victim AccountId32 and create the account so the validation
			// can't short-circuit on `BadSigner`.
			let victim = AccountId::new([0xAA; 32]);
			ensure_funded_account(&victim);

			let signer = SolAddress(victim.clone().into());
			let inner_call: RuntimeCall = frame_system::Call::remark { remark: vec![] }.into();
			let call = pallet_cf_environment::Call::<Runtime>::non_native_signed_call {
				chainflip_extrinsic: ChainflipExtrinsic {
					call: Box::new(inner_call),
					transaction_metadata: TransactionMetadata {
						nonce: 0,
						expiry_block: EXPIRY_BLOCK,
					},
				},
				signature_data: SignatureData::Solana {
					signature: SolSignature([0u8; 64]),
					signer,
					sig_type: SolEncodingType::Domain,
				},
			};

			// Sanity: the forged payload decodes to the victim account.
			if let pallet_cf_environment::Call::non_native_signed_call { signature_data, .. } =
				&call
			{
				let decoded: AccountId = signature_data.signer_account().unwrap();
				assert_eq!(decoded, victim);
			}

			assert_eq!(
				<pallet_cf_environment::Pallet<Runtime> as ValidateUnsigned>::pre_dispatch(&call),
				Err(InvalidTransaction::BadProof.into()),
			);

			// Confirm the victim's nonce is untouched (validation failed before any state
			// change).
			assert_eq!(frame_system::Pallet::<Runtime>::account_nonce(&victim), 0);
		});
	}

	// NOTE: An end-to-end test that drives the forged extrinsic through
	// `Executive::apply_extrinsic` would exercise the exact block-import path that the
	// original vulnerability lived on. It is intentionally not included here because the
	// `Executive::initialize_block` hooks (e.g. `cf_emissions::on_initialize` →
	// `cf_broadcast`) require a fully configured genesis (validators, vault keys, etc.)
	// that the bare `test_runner::new_test_ext()` does not provide. The natural home for
	// that test is `cf-integration-tests`, which sets up a complete chainflip network.
}
