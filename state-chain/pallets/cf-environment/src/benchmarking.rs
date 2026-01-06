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

use crate::submit_runtime_call::ChainflipExtrinsic;
use cf_primitives::TxId;
use cf_traits::VaultKeyWitnessedHandler;
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::UnfilteredDispatchable};

/// Representative benchmark types modeled after real pallet call parameters.
/// Based on `request_loan` from cf-lending-pools which has typical complexity.
#[allow(dead_code)]
pub mod benchmark_types {
	use cf_primitives::{Asset, AssetAmount};
	use codec::{Decode, DecodeWithMemTracking, Encode};
	use scale_info::TypeInfo;
	use sp_std::collections::btree_map::BTreeMap;

	/// Mimics a realistic pallet call similar to `request_loan`.
	/// Parameters: asset enum, amount (u128), optional asset, BTreeMap<Asset, Amount>
	#[derive(TypeInfo, Clone, Encode, Decode, DecodeWithMemTracking, Debug, PartialEq, Eq)]
	pub struct RealisticCallParams {
		pub loan_asset: Asset,
		pub loan_amount: AssetAmount,
		pub collateral_topup_asset: Option<Asset>,
		pub extra_collateral: BTreeMap<Asset, AssetAmount>,
	}

	impl Default for RealisticCallParams {
		fn default() -> Self {
			{
				let mut extra_collateral = BTreeMap::new();
				extra_collateral.insert(Asset::Eth, 1_000_000_000_000_000_000u128);
				extra_collateral.insert(Asset::Usdc, 50_000_000_000u128);

				RealisticCallParams {
					loan_asset: Asset::Usdc,
					loan_amount: 100_000_000_000u128,
					collateral_topup_asset: Some(Asset::Eth),
					extra_collateral,
				}
			}
		}
	}
}

#[benchmarks(
	where
	T: pallet_cf_flip::Config,
)]
mod benchmarks {
	use cf_primitives::FLIPPERINOS_PER_FLIP;
	use cf_traits::FeePayment;
	use scale_info::prelude::string::ToString;

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
			usdt_token_mint_pubkey: SolAddress(Default::default()),
			usdt_token_vault_ata: SolAddress(Default::default()),
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
	fn non_native_signed_call() {
		let system_call = frame_system::Call::<T>::remark { remark: vec![] };
		let runtime_call: <T as Config>::RuntimeCall = system_call.into();
		let call = scale_info::prelude::boxed::Box::new(runtime_call);

		let transaction_metadata = TransactionMetadata { nonce: 0, expiry_block: 10000u32 };
		let signature_data: SignatureData = SignatureData::Solana {
            signature: SolSignature(hex_literal::hex!(
                "1c3e51b4b12bcc95419a43dc4c1854663edda1df5dd788a059a66c6d237a32fafbeff6515d4b8af0267ce8365ba7a83cf483d7b66d3e3164db027302e308c60e"
            )),
            signer: SolAddress(cf_utilities::bs58_array("HfasueN6RNPjSM6rKGH5dga6kS2oUF8siGH3m4MXPURp")),
            sig_type: submit_runtime_call::SolEncodingType::Domain,
        };

		pallet_cf_flip::Pallet::<T>::mint_to_account(
			&signature_data.signer_account().unwrap(),
			(10 * FLIPPERINOS_PER_FLIP).into(),
		);

		#[extrinsic_call]
		non_native_signed_call(
			frame_system::RawOrigin::None,
			ChainflipExtrinsic { call, transaction_metadata },
			signature_data,
		);
	}

	#[benchmark]
	fn batch(c: Linear<0, 10>) {
		let caller: T::AccountId = whitelisted_caller();
		let calls = vec![frame_system::Call::remark { remark: vec![] }.into(); c as usize];

		#[extrinsic_call]
		batch(frame_system::RawOrigin::Signed(caller.clone()), calls.try_into().unwrap());
	}

	// Benchmarks for EIP-712 signature verification components.
	// These help identify which part of `is_valid_signature` is most expensive.

	#[benchmark]
	fn eip712_build_domain_data() {
		use crate::submit_runtime_call::build_domain_data;

		let system_call = frame_system::Call::<T>::remark { remark: vec![] };
		let runtime_call: <T as Config>::RuntimeCall = system_call.into();
		let transaction_metadata = TransactionMetadata { nonce: 0, expiry_block: 10000u32 };
		let chainflip_network = ChainflipNetwork::Testnet;
		let spec_version = 1u32;

		#[block]
		{
			let _ = build_domain_data(
				&runtime_call,
				&chainflip_network,
				&transaction_metadata,
				spec_version,
			);
		}
	}

	#[benchmark]
	fn eip712_build_typed_data() {
		use ethereum_eip712::build_eip712_data::build_eip712_typed_data;

		let system_call = frame_system::Call::<T>::remark { remark: vec![] };
		let runtime_call: <T as Config>::RuntimeCall = system_call.into();
		let transaction_metadata = TransactionMetadata { nonce: 0, expiry_block: 10000u32 };
		let chainflip_extrinsic = ChainflipExtrinsic { call: runtime_call, transaction_metadata };
		let chainflip_network_name = "Perseverance".to_string();
		let spec_version = 1u32;

		#[block]
		{
			let _ =
				build_eip712_typed_data(chainflip_extrinsic, chainflip_network_name, spec_version);
		}
	}

	#[benchmark]
	fn eip712_build_typed_data_simple() {
		use ethereum_eip712::build_eip712_data::build_eip712_typed_data;

		let encodable = b"chainflip/create-account/0xdeadbeef0000000000000000000000000000000000000000000000000000";
		let transaction_metadata = TransactionMetadata { nonce: 0, expiry_block: 10000u32 };
		let chainflip_extrinsic = ChainflipExtrinsic { call: encodable, transaction_metadata };
		let chainflip_network_name = "Testnet".to_string();
		let spec_version = 1u32;

		#[block]
		{
			let _ =
				build_eip712_typed_data(chainflip_extrinsic, chainflip_network_name, spec_version);
		}
	}

	#[benchmark]
	fn eip712_encode_using_type_info() {
		use ethereum_eip712::{eip712::EIP712Domain, encode_eip712_using_type_info};

		let system_call = frame_system::Call::<T>::remark { remark: vec![] };
		let runtime_call: <T as Config>::RuntimeCall = system_call.into();
		let transaction_metadata = TransactionMetadata { nonce: 0, expiry_block: 10000u32 };
		let chainflip_extrinsic = ChainflipExtrinsic { call: runtime_call, transaction_metadata };

		let domain = EIP712Domain {
			name: Some("Testnet".to_string()),
			version: Some("1".to_string()),
			chain_id: None,
			verifying_contract: None,
			salt: None,
		};

		#[block]
		{
			// Measure encode_eip712_using_type_info which includes:
			// 1. Registry creation & type registration
			// 2. SCALE encode + decode via type info
			// 3. Recursive type construction
			// 4. MinimizedScaleValue conversion
			let _ = encode_eip712_using_type_info(chainflip_extrinsic, domain);
		}
	}

	#[benchmark]
	fn eip712_encode_using_type_info_fast() {
		use ethereum_eip712::{eip712::EIP712Domain, encode_eip712_using_type_info_fast};

		let system_call = frame_system::Call::<T>::remark { remark: vec![] };
		let runtime_call: <T as Config>::RuntimeCall = system_call.into();
		let transaction_metadata = TransactionMetadata { nonce: 0, expiry_block: 10000u32 };
		let chainflip_extrinsic = ChainflipExtrinsic { call: runtime_call, transaction_metadata };

		let domain = EIP712Domain {
			name: Some("Testnet".to_string()),
			version: Some("1".to_string()),
			chain_id: None,
			verifying_contract: None,
			salt: None,
		};

		#[block]
		{
			// Measure optimized version that bypasses registry construction
			let _ = encode_eip712_using_type_info_fast(chainflip_extrinsic, domain);
		}
	}

	// Benchmarks with a deliberately complex call to stress-test registry construction.
	// Uses benchmark_realistic_call call which embeds RealisticCallParams in the RuntimeCall type
	// tree.

	#[benchmark]
	fn eip712_encode_realistic_call() {
		use crate::benchmarking::benchmark_types::RealisticCallParams;
		use ethereum_eip712::{eip712::EIP712Domain, encode_eip712_using_type_info};

		// Use the benchmark_realistic_call call which embeds RealisticCallParams in RuntimeCall
		let realistic_call =
			crate::Call::<T>::benchmark_realistic_call { params: RealisticCallParams::default() };
		let runtime_call: <T as Config>::RuntimeCall = realistic_call.into();
		let transaction_metadata = TransactionMetadata { nonce: 0, expiry_block: 10000u32 };
		let chainflip_extrinsic = ChainflipExtrinsic { call: runtime_call, transaction_metadata };

		let domain = EIP712Domain {
			name: Some("Testnet".to_string()),
			version: Some("1".to_string()),
			chain_id: None,
			verifying_contract: None,
			salt: None,
		};

		#[block]
		{
			let _ = encode_eip712_using_type_info(chainflip_extrinsic, domain);
		}
	}

	#[benchmark]
	fn eip712_encode_realistic_call_fast() {
		use crate::benchmarking::benchmark_types::RealisticCallParams;
		use ethereum_eip712::{eip712::EIP712Domain, encode_eip712_using_type_info_fast};

		// Use the benchmark_realistic_call call which embeds RealisticCallParams in RuntimeCall
		let realistic_call =
			crate::Call::<T>::benchmark_realistic_call { params: RealisticCallParams::default() };
		let runtime_call: <T as Config>::RuntimeCall = realistic_call.into();
		let transaction_metadata = TransactionMetadata { nonce: 0, expiry_block: 10000u32 };
		let chainflip_extrinsic = ChainflipExtrinsic { call: runtime_call, transaction_metadata };

		let domain = EIP712Domain {
			name: Some("Testnet".to_string()),
			version: Some("1".to_string()),
			chain_id: None,
			verifying_contract: None,
			salt: None,
		};

		#[block]
		{
			let _ = encode_eip712_using_type_info_fast(chainflip_extrinsic, domain);
		}
	}

	// Individual step benchmarks for encode_eip712_using_type_info breakdown

	#[benchmark]
	fn eip712_step1_registry_creation() {
		use ethereum_eip712::benchmark_helpers::step1_registry_and_type_registration;

		type ExtrinsicType<T> = ChainflipExtrinsic<<T as Config>::RuntimeCall>;

		#[block]
		{
			let _ = step1_registry_and_type_registration::<ExtrinsicType<T>>();
		}
	}

	#[benchmark]
	fn eip712_step2_encode_decode() {
		use crate::benchmarking::benchmark_types::RealisticCallParams;
		use ethereum_eip712::benchmark_helpers::{
			step1_registry_and_type_registration, step2_encode_decode,
		};

		type ExtrinsicType<T> = ChainflipExtrinsic<<T as Config>::RuntimeCall>;

		let realistic_call =
			crate::Call::<T>::benchmark_realistic_call { params: RealisticCallParams::default() };
		let runtime_call: <T as Config>::RuntimeCall = realistic_call.into();
		let transaction_metadata = TransactionMetadata { nonce: 0, expiry_block: 10000u32 };
		let chainflip_extrinsic: ExtrinsicType<T> =
			ChainflipExtrinsic { call: runtime_call, transaction_metadata };

		// Pre-compute registry outside the benchmark block
		let (portable_registry, type_id) =
			step1_registry_and_type_registration::<ExtrinsicType<T>>();

		#[block]
		{
			let _ = step2_encode_decode::<ExtrinsicType<T>>(
				&chainflip_extrinsic,
				&portable_registry,
				type_id,
			);
		}
	}

	#[benchmark]
	fn eip712_step3_recursive_type_construction() {
		use crate::benchmarking::benchmark_types::RealisticCallParams;
		use ethereum_eip712::benchmark_helpers::{
			step1_registry_and_type_registration, step2_encode_decode,
			step3_recursive_type_construction,
		};

		type ExtrinsicType<T> = ChainflipExtrinsic<<T as Config>::RuntimeCall>;

		let realistic_call =
			crate::Call::<T>::benchmark_realistic_call { params: RealisticCallParams::default() };
		let runtime_call: <T as Config>::RuntimeCall = realistic_call.into();
		let transaction_metadata = TransactionMetadata { nonce: 0, expiry_block: 10000u32 };
		let chainflip_extrinsic: ExtrinsicType<T> =
			ChainflipExtrinsic { call: runtime_call, transaction_metadata };

		// Pre-compute steps 1 and 2 outside the benchmark block
		let (portable_registry, type_id) =
			step1_registry_and_type_registration::<ExtrinsicType<T>>();
		let value = step2_encode_decode::<ExtrinsicType<T>>(
			&chainflip_extrinsic,
			&portable_registry,
			type_id,
		)
		.expect("decode should succeed");

		#[block]
		{
			let _ = step3_recursive_type_construction::<ExtrinsicType<T>>(value);
		}
	}

	#[benchmark]
	fn eip712_step4_minimized_conversion() {
		use crate::benchmarking::benchmark_types::RealisticCallParams;
		use ethereum_eip712::benchmark_helpers::{
			step1_registry_and_type_registration, step2_encode_decode,
			step3_recursive_type_construction, step4_minimized_scale_value_conversion,
		};

		type ExtrinsicType<T> = ChainflipExtrinsic<<T as Config>::RuntimeCall>;

		let realistic_call =
			crate::Call::<T>::benchmark_realistic_call { params: RealisticCallParams::default() };
		let runtime_call: <T as Config>::RuntimeCall = realistic_call.into();
		let transaction_metadata = TransactionMetadata { nonce: 0, expiry_block: 10000u32 };
		let chainflip_extrinsic: ExtrinsicType<T> =
			ChainflipExtrinsic { call: runtime_call, transaction_metadata };

		// Pre-compute steps 1, 2, and 3 outside the benchmark block
		let (portable_registry, type_id) =
			step1_registry_and_type_registration::<ExtrinsicType<T>>();
		let value = step2_encode_decode::<ExtrinsicType<T>>(
			&chainflip_extrinsic,
			&portable_registry,
			type_id,
		)
		.expect("decode should succeed");
		let (_primary_type, minimized_value, _types) =
			step3_recursive_type_construction::<ExtrinsicType<T>>(value)
				.expect("type construction should succeed");

		#[block]
		{
			let _ = step4_minimized_scale_value_conversion(minimized_value);
		}
	}

	#[benchmark]
	fn eip712_verify_signature() {
		use cf_chains::evm::{
			verify_evm_signature, Address as EvmAddress, Signature as EthereumSignature,
		};

		// Pre-computed test data: a valid EIP-712 payload and matching signature.
		// This is a keccak256 hash of a sample EIP-712 encoded message.
		let payload: [u8; 66] = hex_literal::hex!(
			"1901"
			"8d4a3f4082945b7879e2b55f181c31a77c8c0a464b70669458abbaaf99de4c38"
			"8d4a3f4082945b7879e2b55f181c31a77c8c0a464b70669458abbaaf99de4c38"
		);

		// A valid ECDSA signature (r, s, v) for the payload above.
		// Note: In real benchmarks, we just need a structurally valid signature.
		// The signature verification will run the full crypto regardless of validity.
		let signature = EthereumSignature::from(hex_literal::hex!(
			"0000000000000000000000000000000000000000000000000000000000000001"
			"0000000000000000000000000000000000000000000000000000000000000002"
			"1b"
		));
		let signer =
			EvmAddress::from(hex_literal::hex!("0000000000000000000000000000000000000001"));

		#[block]
		{
			// The result doesn't matter - we're measuring the crypto work
			let _ = verify_evm_signature(&signer, &payload, &signature);
		}
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::benchmarks_mock::new_test_ext(),
		crate::mock::benchmarks_mock::BenchmarksTest
	);
}
