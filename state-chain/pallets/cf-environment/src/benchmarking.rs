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

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::TxId;
use cf_traits::VaultKeyWitnessedHandler;
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::UnfilteredDispatchable};

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn update_safe_mode() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let call = Call::<T>::update_safe_mode { update: SafeModeUpdate::CodeRed };

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(RuntimeSafeMode::<T>::get(), SafeMode::code_red());
	}

	#[benchmark]
	fn update_consolidation_parameters() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let call =
			Call::<T>::update_consolidation_parameters { params: INITIAL_CONSOLIDATION_PARAMETERS };

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(ConsolidationParameters::<T>::get(), INITIAL_CONSOLIDATION_PARAMETERS);
	}

	#[benchmark]
	fn force_recover_sol_nonce() {
		let nonce_account = SolAddress([0x01; 32]);
		let old_hash = SolHash([0x02; 32]);
		let new_hash = SolHash([0x10; 32]);

		// Setup unavailable Nonce
		SolanaUnavailableNonceAccounts::<T>::insert(nonce_account, old_hash);

		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let call =
			Call::<T>::force_recover_sol_nonce { nonce_account, durable_nonce: Some(new_hash) };

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert!(SolanaAvailableNonceAccounts::<T>::get().contains(&(nonce_account, new_hash)));
		assert!(SolanaUnavailableNonceAccounts::<T>::get(nonce_account).is_none());
	}

	#[benchmark]
	fn witness_polkadot_vault_creation() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		let dot_pure_proxy_vault_key = PolkadotAccountId(Default::default());
		let call = Call::<T>::witness_polkadot_vault_creation {
			dot_pure_proxy_vault_key,
			tx_id: TxId { block_number: 1_000u32, extrinsic_index: 1_000u32 },
		};

		T::PolkadotVaultKeyWitnessedHandler::setup_key_activation();

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(PolkadotVaultAccountId::<T>::get(), Some(dot_pure_proxy_vault_key));
	}

	#[benchmark]
	fn witness_current_bitcoin_block_number_for_key() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		let call = Call::<T>::witness_current_bitcoin_block_number_for_key {
			block_number: 10u64,
			new_public_key: cf_chains::btc::AggKey {
				previous: Some([2u8; 32]),
				current: [1u8; 32],
			},
		};
		T::BitcoinVaultKeyWitnessedHandler::setup_key_activation();

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}
	}

	#[benchmark]
	fn witness_initialize_arbitrum_vault() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let call = Call::<T>::witness_initialize_arbitrum_vault { block_number: 10u64 };

		T::ArbitrumVaultKeyWitnessedHandler::setup_key_activation();
		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}
	}

	#[benchmark]
	fn witness_initialize_solana_vault() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();
		let call = Call::<T>::witness_initialize_solana_vault { block_number: 10u64 };

		T::SolanaVaultKeyWitnessedHandler::setup_key_activation();
		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}
	}

	#[benchmark]
	fn witness_assethub_vault_creation() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		let hub_pure_proxy_vault_key = PolkadotAccountId(Default::default());
		let call = Call::<T>::witness_assethub_vault_creation {
			hub_pure_proxy_vault_key,
			tx_id: TxId { block_number: 1_000u32, extrinsic_index: 1_000u32 },
		};

		T::AssethubVaultKeyWitnessedHandler::setup_key_activation();
		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(AssethubVaultAccountId::<T>::get(), Some(hub_pure_proxy_vault_key));
	}

	#[benchmark]
	fn dispatch_solana_gov_call() {
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		let call = Call::<T>::dispatch_solana_gov_call {
			gov_call: SolanaGovCall::SetProgramSwapsParameters {
				min_native_swap_amount: 1u64,
				max_dst_address_len: 2u16,
				max_ccm_message_len: 3u32,
				max_cf_parameters_len: 4u32,
				max_event_accounts: 5u32,
			},
		};

		SolanaApiEnvironment::<T>::put(SolApiEnvironment {
			vault_program: SolAddress(Default::default()),
			vault_program_data_account: SolAddress(Default::default()),
			token_vault_pda_account: SolAddress(Default::default()),
			usdc_token_mint_pubkey: SolAddress(Default::default()),
			usdc_token_vault_ata: SolAddress(Default::default()),
			swap_endpoint_program: SolAddress(Default::default()),
			swap_endpoint_program_data_account: SolAddress(Default::default()),
			alt_manager_program: SolAddress(Default::default()),
			address_lookup_table_account: cf_chains::sol::AddressLookupTableAccount {
				key: cf_chains::sol::SolPubkey(Default::default()),
				addresses: vec![
					cf_chains::sol::SolPubkey([0x01; 32]),
					cf_chains::sol::SolPubkey([0x02; 32]),
				],
			},
		});
		SolanaAvailableNonceAccounts::<T>::put(vec![
			(SolAddress([0x10; 32]), SolHash([0x20; 32])),
			(SolAddress([0x11; 32]), SolHash([0x21; 32])),
			(SolAddress([0x12; 32]), SolHash([0x22; 32])),
			(SolAddress([0x13; 32]), SolHash([0x23; 32])),
			(SolAddress([0x14; 32]), SolHash([0x24; 32])),
		]);

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}
	}

	#[benchmark]
	fn submit_signed_runtime_call(c: Linear<0, 1000>) {
		let calls = vec![frame_system::Call::remark { remark: vec![] }.into(); c as usize];
		let transaction_metadata =
			TransactionMetadata { nonce: 0, expiry_block: 10000u32, atomic: true };
		let user_signature_data: UserSignatureData = UserSignatureData::Solana {
            signature: SolSignature(hex_literal::hex!(
                "1c3e51b4b12bcc95419a43dc4c1854663edda1df5dd788a059a66c6d237a32fafbeff6515d4b8af0267ce8365ba7a83cf483d7b66d3e3164db027302e308c60e"
            )),
            signer: SolAddress(cf_utilities::bs58_array("HfasueN6RNPjSM6rKGH5dga6kS2oUF8siGH3m4MXPURp")),
            sig_type: SolSigType::Domain,
        };

		#[extrinsic_call]
		submit_signed_runtime_call(
			frame_system::RawOrigin::None,
			calls,
			transaction_metadata,
			user_signature_data,
		);

		assert!(frame_system::Pallet::<T>::events().len() > 0);
	}

	#[benchmark]
	fn submit_batch_runtime_call(c: Linear<0, 1000>) {
		let caller: T::AccountId = whitelisted_caller();
		let calls = vec![frame_system::Call::remark { remark: vec![] }.into(); c as usize];

		#[extrinsic_call]
		submit_batch_runtime_call(frame_system::RawOrigin::Signed(caller.clone()), calls, true);
		assert!(frame_system::Pallet::<T>::events().len() > 0);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
