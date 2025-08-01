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

//! This file contains a Instruction builder for the Solana chain.
//!
//! The builder is used to build single Solana Instruction used for Vault Swap.
//!
//! Such Instruction can be signed and sent to the Program on Solana directly to invoke
//! certain functions.

use crate::{
	address::EncodedAddress,
	sol::{
		sol_tx_core::{
			consts::{SOL_USDC_DECIMAL, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
			program_instructions::swap_endpoints::{
				SwapEndpointProgram, SwapNativeParams, SwapTokenParams,
			},
		},
		SolAddress, SolAmount, SolApiEnvironment, SolInstruction, SolPubkey, SolSeed,
	},
	CcmChannelMetadataChecked,
};
use cf_primitives::chains::assets::any::Asset;
use sp_std::vec::Vec;

fn system_program_id() -> SolAddress {
	SYSTEM_PROGRAM_ID
}

fn token_program_id() -> SolAddress {
	TOKEN_PROGRAM_ID
}

pub struct SolanaInstructionBuilder;

impl SolanaInstructionBuilder {
	pub fn x_swap_native(
		api_environment: SolApiEnvironment,
		swap_endpoint_native_vault: SolPubkey,
		destination_asset: Asset,
		destination_address: EncodedAddress,
		from: SolPubkey,
		seed: SolSeed,
		event_data_account: SolPubkey,
		input_amount: SolAmount,
		cf_parameters: Vec<u8>,
		ccm: Option<CcmChannelMetadataChecked>,
	) -> SolInstruction {
		SwapEndpointProgram::with_id(api_environment.swap_endpoint_program).x_swap_native(
			SwapNativeParams {
				amount: input_amount,
				dst_chain: destination_address.chain() as u32,
				dst_address: destination_address.into_vec(),
				dst_token: destination_asset as u32,
				ccm_parameters: ccm.map(|metadata| metadata.into()),
				cf_parameters,
			},
			seed.into(),
			api_environment.vault_program_data_account,
			swap_endpoint_native_vault,
			from,
			event_data_account,
			api_environment.swap_endpoint_program_data_account,
			system_program_id(),
		)
	}

	pub fn x_swap_usdc(
		api_environment: SolApiEnvironment,
		destination_asset: Asset,
		destination_address: EncodedAddress,
		from: SolPubkey,
		from_token_account: SolPubkey,
		seed: SolSeed,
		event_data_account: SolPubkey,
		token_supported_account: SolPubkey,
		input_amount: SolAmount,
		cf_parameters: Vec<u8>,
		ccm: Option<CcmChannelMetadataChecked>,
	) -> SolInstruction {
		SwapEndpointProgram::with_id(api_environment.swap_endpoint_program).x_swap_token(
			SwapTokenParams {
				amount: input_amount,
				dst_chain: destination_address.chain() as u32,
				dst_address: destination_address.into_vec(),
				dst_token: destination_asset as u32,
				ccm_parameters: ccm.map(|metadata| metadata.into()),
				cf_parameters,
				decimals: SOL_USDC_DECIMAL,
			},
			seed.into(),
			api_environment.vault_program_data_account,
			api_environment.usdc_token_vault_ata,
			from,
			from_token_account,
			event_data_account,
			api_environment.swap_endpoint_program_data_account,
			token_supported_account,
			token_program_id(),
			api_environment.usdc_token_mint_pubkey,
			system_program_id(),
		)
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		cf_parameters::build_and_encode_cf_parameters,
		sol::{
			signing_key::SolSigningKey,
			sol_tx_core::{
				self,
				consts::{const_address, MAX_TRANSACTION_LENGTH},
				sol_test_values::*,
			},
			SolAddress, SolAddressLookupTableAccount, SolHash, SolVersionedMessage,
			SolVersionedTransaction,
		},
		ChannelRefundParametersForChain,
	};
	use cf_primitives::{
		chains::Solana, AccountId, AffiliateAndFee, AffiliateShortId, BasisPoints, DcaParameters,
		MAX_AFFILIATES,
	};
	use sp_core::ConstU32;
	use sp_runtime::BoundedVec;

	// private key: ead22312d80f573924a27595271bd2ec0aa20a270587c8a399136166561ea58c
	const DESTINATION_ADDRESS_ETH: EncodedAddress =
		EncodedAddress::Eth(hex_literal::hex!("756FBdE9c71EaE05C2f7169f816b0Bd11D978020"));

	// Test Solana accounts. Generated by
	// ```rust
	// let key = SolSigningKey.new();
	// key.print_pub_and_private_keys();
	// ```

	//const DESTINATION_ADDRESS_KEY_BYTES: [u8; 32] = [242, 33, 23, 21, 58, 254, 23, 134, 199, 91,
	// 117, 2, 20, 116, 174, 15, 191, 69, 254, 42, 135, 88, 210, 88, 225, 158, 31, 184, 181, 50, 16,
	// 195];
	const DESTINATION_ADDRESS_SOL: SolAddress =
		const_address("BdyHK5DckpAFGcbZveGLPjjMEaADGfNeqcRXKoyd33kA");

	const FROM_KEY_BYTES: [u8; 32] = [
		130, 14, 62, 77, 129, 146, 185, 187, 159, 15, 165, 161, 93, 111, 249, 198, 145, 149, 193,
		229, 147, 69, 73, 190, 10, 208, 151, 131, 194, 205, 116, 232,
	];
	const FROM: SolAddress = const_address("EwgZksaPybTUyhcEMn3aR46HZokR4NH6d1Wy8d51qZ6G");
	const FROM_TOKEN: SolAddress = const_address("4dTsLjw5c75UXF59KAKHqyeBJVKZibA9a4LsDLc1CPbB");

	const VAULT_SWAP_SEED: &[u8] = &[1; 32];

	//const TOKEN_SUPPORTED_ACCOUNT_KEY_BYTES: [u8; 32] = [0, 81, 206, 126, 204, 53, 163, 79, 5,
	// 119, 184, 1, 97, 237, 114, 120, 23, 6, 227, 206, 239, 132, 130, 212, 241, 12, 21, 185, 66,
	// 252, 127, 8];
	const TOKEN_SUPPORTED_ACCOUNT: SolAddress =
		const_address("48vfkGMyVDUNq689aocfYQsoqubZjjme7cja21cbnMnK");

	const BROKER_COMMISSION: BasisPoints = 1u16;
	const BOOST_FEE: u8 = 2u8;
	const INPUT_AMOUNT: SolAmount = 1_234_567_890u64;

	const BLOCKHASH: SolHash = SolHash([0x00; 32]);

	// Sisyphos broker account: 0xa622ebf634ff6cdafe1b7912d8699b34a4d9a08598af0b0c90eaf1e912de1f19
	fn broker_id() -> AccountId {
		AccountId::from(hex_literal::hex!(
			"a0edda1a4beee4fe2df32c0802aa6759da49ae6165fcdb5c40d7f4cd5a30db0e"
		))
	}

	fn channel_refund_parameters() -> ChannelRefundParametersForChain<Solana> {
		ChannelRefundParametersForChain::<Solana> {
			min_price: sp_core::U256::default(),
			refund_address: DESTINATION_ADDRESS_SOL,
			retry_duration: 10u32,
			refund_ccm_metadata: None,
		}
	}

	fn affiliate_and_fees() -> BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>> {
		vec![
			AffiliateAndFee { affiliate: AffiliateShortId(1u8), fee: 10u8 },
			AffiliateAndFee { affiliate: AffiliateShortId(2u8), fee: 20u8 },
		]
		.try_into()
		.unwrap()
	}

	fn dca_parameters() -> DcaParameters {
		DcaParameters { number_of_chunks: 10u32, chunk_interval: 20u32 }
	}

	fn cf_parameter(with_ccm: bool) -> Vec<u8> {
		build_and_encode_cf_parameters(
			channel_refund_parameters(),
			Some(dca_parameters()),
			BOOST_FEE,
			broker_id(),
			BROKER_COMMISSION,
			affiliate_and_fees(),
			with_ccm.then_some(&ccm_parameter_v0().channel_metadata),
		)
	}

	fn vault_swap_account(seed: &[u8]) -> SolPubkey {
		crate::sol::sol_tx_core::address_derivation::derive_vault_swap_account(
			SWAP_ENDPOINT_PROGRAM,
			FROM,
			seed,
		)
		.unwrap()
		.address
		.into()
	}

	fn into_transaction(
		instructions: SolInstruction,
		payer: SolPubkey,
		alt: &[SolAddressLookupTableAccount],
	) -> SolVersionedTransaction {
		// Build mock Transaction for testing.
		let transaction = SolVersionedTransaction::new_unsigned(SolVersionedMessage::new(
			&[instructions],
			Some(payer),
			Default::default(),
			alt,
		));

		let mock_serialized_tx = transaction
			.clone()
			.finalize_and_serialize()
			.expect("Transaction building must succeed.");

		assert!(
			mock_serialized_tx.len() < MAX_TRANSACTION_LENGTH,
			"Transaction exceeded max length"
		);

		transaction
	}

	#[test]
	fn can_build_x_swap_native_instruction_no_ccm() {
		let transaction = into_transaction(
			SolanaInstructionBuilder::x_swap_native(
				api_env(),
				agg_key().into(),
				Asset::Eth,
				DESTINATION_ADDRESS_ETH,
				FROM.into(),
				VAULT_SWAP_SEED.to_vec().try_into().unwrap(),
				vault_swap_account(VAULT_SWAP_SEED),
				INPUT_AMOUNT,
				cf_parameter(false),
				None,
			),
			FROM.into(),
			&[chainflip_alt()],
		);

		let expected_serialized_tx = hex_literal::hex!("018537b72c5bbc431911900e7d77c424a6910f01ab915caf0ed25232d6f5e9997d3dc3319d5b8c878f35296feaad4f2a03f27dd1d08b552e3c62b52a536a4410058001000104cf2a079e1506b29d02c8feac98d589a9059a740891dcd3dab6c64b3160bc2831e563af24863f1e2042e809741323618edbd325a60421e41dd9985a6af1188193f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb1ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc16229380000000000000000000000000000000000000000000000000000000000000000010306060200010405d001a3265ce2f3698dc4d2029649000000000100000014000000756fbde9c71eae05c2f7169f816b0bd11d978020010000000077000000010a0000009e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f000000000000000000000000000000000000000000000000000000000000000000010a0000001400000002a0edda1a4beee4fe2df32c0802aa6759da49ae6165fcdb5c40d7f4cd5a30db0e010008010a0214200000000101010101010101010101010101010101010101010101010101010101010101013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10108020c02").to_vec();

		let from_signing_key = SolSigningKey::from_bytes(&FROM_KEY_BYTES).unwrap();

		test_constructed_transaction_with_signer(
			transaction,
			expected_serialized_tx,
			vec![from_signing_key].into(),
			BLOCKHASH.into(),
		);
	}

	#[test]
	fn can_build_x_swap_native_instruction_with_ccm() {
		let transaction = into_transaction(
			SolanaInstructionBuilder::x_swap_native(
				api_env(),
				agg_key().into(),
				Asset::SolUsdc,
				EncodedAddress::Sol(DESTINATION_ADDRESS_SOL.0),
				FROM.into(),
				VAULT_SWAP_SEED.to_vec().try_into().unwrap(),
				vault_swap_account(VAULT_SWAP_SEED),
				INPUT_AMOUNT,
				cf_parameter(true),
				Some(ccm_parameter_v0().channel_metadata),
			),
			FROM.into(),
			&[chainflip_alt()],
		);

		let expected_serialized_tx = hex_literal::hex!("01b3e831ed85b01852f3580d1baae31ac9f5a9f5e21a6c64298819670f5c2fe70929fadcb08681bb208e9cbef63beb2de7bbbab535971bd3b95393c2bdf60dfa018001000104cf2a079e1506b29d02c8feac98d589a9059a740891dcd3dab6c64b3160bc2831e563af24863f1e2042e809741323618edbd325a60421e41dd9985a6af1188193f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb1ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc16229380000000000000000000000000000000000000000000000000000000000000000010306060200010405d202a3265ce2f3698dc4d20296490000000005000000200000009e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f0a00000001040000007c1d0f070000000000000000dd000000019101007417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed480104a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e50090e0b0f5b60147b325842c1fc68f6c90fe26419ea7c4afeb982f71f1f54b5b440a0000009e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f000000000000000000000000000000000000000000000000000000000000000000010a0000001400000002a0edda1a4beee4fe2df32c0802aa6759da49ae6165fcdb5c40d7f4cd5a30db0e010008010a0214200000000101010101010101010101010101010101010101010101010101010101010101013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10108020c02").to_vec();

		let from_signing_key = SolSigningKey::from_bytes(&FROM_KEY_BYTES).unwrap();

		test_constructed_transaction_with_signer(
			transaction,
			expected_serialized_tx,
			vec![from_signing_key].into(),
			BLOCKHASH.into(),
		);
	}

	#[test]
	fn can_build_x_swap_token_instruction_no_ccm() {
		let from_usdc_account =
			crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
				FROM,
				api_env().usdc_token_mint_pubkey,
			)
			.unwrap()
			.address
			.into();

		let transaction = into_transaction(
			SolanaInstructionBuilder::x_swap_usdc(
				api_env(),
				Asset::Eth,
				DESTINATION_ADDRESS_ETH,
				FROM.into(),
				from_usdc_account,
				VAULT_SWAP_SEED.to_vec().try_into().unwrap(),
				vault_swap_account(VAULT_SWAP_SEED),
				TOKEN_SUPPORTED_ACCOUNT.into(),
				INPUT_AMOUNT,
				cf_parameter(false),
				None,
			),
			FROM.into(),
			&[chainflip_alt()],
		);

		let expected_serialized_tx = hex_literal::hex!("01ca269319ce27986e9512f2a8404c1ec6ea1ed171305a965854b5910c5ff6529343c6da33c2326ac42dc1b8af25c3dc9401479c27f338729432e8bab01fdfd1088001000205cf2a079e1506b29d02c8feac98d589a9059a740891dcd3dab6c64b3160bc283127fefc7ec198ef3eacb0f17871e1fad81f07a40cd55f4f364c3915877d89bd8ae563af24863f1e2042e809741323618edbd325a60421e41dd9985a6af11881931ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc16229382e9acf1ff8568fbe655e616a167591aeedc250afbc88d759ec959b1982e8769c000000000000000000000000000000000000000000000000000000000000000001030a0a060001020504080907d1014532fc63e55377ebd2029649000000000100000014000000756fbde9c71eae05c2f7169f816b0bd11d978020010000000077000000010a0000009e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f000000000000000000000000000000000000000000000000000000000000000000010a0000001400000002a0edda1a4beee4fe2df32c0802aa6759da49ae6165fcdb5c40d7f4cd5a30db0e010008010a021406200000000101010101010101010101010101010101010101010101010101010101010101013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1020805040c090302").to_vec();

		let from_signing_key = SolSigningKey::from_bytes(&FROM_KEY_BYTES).unwrap();

		test_constructed_transaction_with_signer(
			transaction,
			expected_serialized_tx,
			vec![from_signing_key].into(),
			BLOCKHASH.into(),
		);
	}

	#[test]
	fn can_build_x_swap_token_instruction_with_ccm() {
		let from_usdc_account =
			crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
				FROM,
				api_env().usdc_token_mint_pubkey,
			)
			.unwrap()
			.address
			.into();

		let transaction = into_transaction(
			SolanaInstructionBuilder::x_swap_usdc(
				api_env(),
				Asset::Sol,
				EncodedAddress::Sol(DESTINATION_ADDRESS_SOL.0),
				FROM.into(),
				from_usdc_account,
				VAULT_SWAP_SEED.to_vec().try_into().unwrap(),
				vault_swap_account(VAULT_SWAP_SEED),
				TOKEN_SUPPORTED_ACCOUNT.into(),
				INPUT_AMOUNT,
				cf_parameter(true),
				Some(ccm_parameter_v0().channel_metadata),
			),
			FROM.into(),
			&[chainflip_alt()],
		);

		let expected_serialized_tx = hex_literal::hex!("015fd086b6abef6890d1654d0024017f84492aa2832faf4e3b675a71e5e4883faaef53f36a02ba6c3a0f95ce6bd2e7cae9a936d2c696072eecd895a3fb53ef2e038001000205cf2a079e1506b29d02c8feac98d589a9059a740891dcd3dab6c64b3160bc283127fefc7ec198ef3eacb0f17871e1fad81f07a40cd55f4f364c3915877d89bd8ae563af24863f1e2042e809741323618edbd325a60421e41dd9985a6af11881931ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc16229382e9acf1ff8568fbe655e616a167591aeedc250afbc88d759ec959b1982e8769c000000000000000000000000000000000000000000000000000000000000000001030a0a060001020504080907d3024532fc63e55377ebd20296490000000005000000200000009e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f0900000001040000007c1d0f070000000000000000dd000000019101007417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed480104a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e50090e0b0f5b60147b325842c1fc68f6c90fe26419ea7c4afeb982f71f1f54b5b440a0000009e0d6a70e12d54edf90971cc977fa26a1d3bb4b0b26e72470171c36b0006b01f000000000000000000000000000000000000000000000000000000000000000000010a0000001400000002a0edda1a4beee4fe2df32c0802aa6759da49ae6165fcdb5c40d7f4cd5a30db0e010008010a021406200000000101010101010101010101010101010101010101010101010101010101010101013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1020805040c090302").to_vec();

		let from_signing_key = SolSigningKey::from_bytes(&FROM_KEY_BYTES).unwrap();

		test_constructed_transaction_with_signer(
			transaction,
			expected_serialized_tx,
			vec![from_signing_key].into(),
			BLOCKHASH.into(),
		);
	}

	#[test]
	fn instruction_accounts_len_matches_consts() {
		assert_eq!(
			SolanaInstructionBuilder::x_swap_native(
				api_env(),
				agg_key().into(),
				Asset::Eth,
				DESTINATION_ADDRESS_ETH,
				FROM.into(),
				VAULT_SWAP_SEED.to_vec().try_into().unwrap(),
				vault_swap_account(VAULT_SWAP_SEED),
				INPUT_AMOUNT,
				cf_parameter(false),
				None,
			)
			.accounts
			.len(),
			sol_tx_core::consts::X_SWAP_NATIVE_ACC_LEN as usize
		);

		assert_eq!(
			SolanaInstructionBuilder::x_swap_usdc(
				api_env(),
				Asset::Sol,
				EncodedAddress::Sol(DESTINATION_ADDRESS_SOL.0),
				Default::default(),
				Default::default(),
				VAULT_SWAP_SEED.to_vec().try_into().unwrap(),
				vault_swap_account(VAULT_SWAP_SEED),
				TOKEN_SUPPORTED_ACCOUNT.into(),
				INPUT_AMOUNT,
				cf_parameter(true),
				Some(ccm_parameter_v0().channel_metadata)
			)
			.accounts
			.len(),
			sol_tx_core::consts::X_SWAP_TOKEN_ACC_LEN as usize
		);
	}

	#[test]
	fn instruction_accounts_location_matches_consts() {
		let native_instruction_acc = SolanaInstructionBuilder::x_swap_native(
			api_env(),
			agg_key().into(),
			Asset::Eth,
			DESTINATION_ADDRESS_ETH,
			FROM.into(),
			VAULT_SWAP_SEED.to_vec().try_into().unwrap(),
			vault_swap_account(VAULT_SWAP_SEED),
			INPUT_AMOUNT,
			cf_parameter(false),
			None,
		)
		.accounts;

		assert_eq!(
			native_instruction_acc[sol_tx_core::consts::X_SWAP_FROM_ACC_IDX as usize].pubkey,
			FROM.into()
		);
		assert_eq!(
			native_instruction_acc[sol_tx_core::consts::X_SWAP_NATIVE_EVENT_DATA_ACC_IDX as usize]
				.pubkey,
			vault_swap_account(VAULT_SWAP_SEED)
		);

		let token_instruction_acc = SolanaInstructionBuilder::x_swap_usdc(
			api_env(),
			Asset::Sol,
			EncodedAddress::Sol(DESTINATION_ADDRESS_SOL.0),
			FROM.into(),
			FROM_TOKEN.into(),
			VAULT_SWAP_SEED.to_vec().try_into().unwrap(),
			vault_swap_account(VAULT_SWAP_SEED),
			TOKEN_SUPPORTED_ACCOUNT.into(),
			INPUT_AMOUNT,
			cf_parameter(true),
			Some(ccm_parameter_v0().channel_metadata),
		)
		.accounts;
		assert_eq!(
			token_instruction_acc[sol_tx_core::consts::X_SWAP_FROM_ACC_IDX as usize].pubkey,
			FROM.into()
		);
		assert_eq!(
			token_instruction_acc[sol_tx_core::consts::X_SWAP_TOKEN_FROM_TOKEN_ACC_IDX as usize]
				.pubkey,
			FROM_TOKEN.into()
		);
		assert_eq!(
			token_instruction_acc[sol_tx_core::consts::X_SWAP_TOKEN_EVENT_DATA_ACC_IDX as usize]
				.pubkey,
			vault_swap_account(VAULT_SWAP_SEED)
		);
	}
}
