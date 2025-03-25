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

#![cfg(test)]

use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
	sol::{
		signing_key::SolSigningKey,
		sol_tx_core::{
			address_derivation::{
				derive_associated_token_account, derive_deposit_address, derive_fetch_account,
				derive_token_supported_account,
			},
			compute_budget::ComputeBudgetInstruction,
			consts::{
				MAX_TRANSACTION_LENGTH, SOL_USDC_DECIMAL, SYSTEM_PROGRAM_ID, SYS_VAR_INSTRUCTIONS,
				TOKEN_PROGRAM_ID,
			},
			program_instructions::{InstructionExt, SystemProgramInstruction, VaultProgram},
			signer::Signer,
			sol_test_values::*,
			token_instructions::AssociatedTokenAccountInstruction,
			transaction::{v0::VersionedMessageV0, VersionedMessage, VersionedTransaction},
			CompiledInstruction, Hash, MessageHeader, PdaAndBump, Pubkey,
		},
		SolAddress, SolHash, SolSignature,
	},
	ForeignChainAddress,
};

use core::str::FromStr;

#[derive(BorshSerialize, BorshDeserialize)]
enum BankInstruction {
	Initialize,
	Deposit { lamports: u64 },
	Withdraw { lamports: u64 },
}

fn check_tx_encoding(serialized: Vec<u8>, expected: Vec<u8>) {
	assert!(serialized.len() <= MAX_TRANSACTION_LENGTH);
	if serialized != expected {
		println!("Actual: {:?}", hex::encode(serialized.clone()));
		println!("Expected: {:?}", hex::encode(expected.clone()));
		panic!("Serialized encoding does not match expected value.")
	}
}

#[cfg(test)]
mod versioned_transaction {
	use crate::sol::{
		sol_tx_core::consts::{const_address, const_hash},
		SolAddressLookupTableAccount, SolVersionedMessage, SolVersionedMessageV0,
		SolVersionedTransaction,
	};

	use super::*;

	#[test]
	fn create_transfer_native_no_address_lookup_table() {
		let durable_nonce = TEST_DURABLE_NONCE.into();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();

		let to_pubkey = TRANSFER_TO_ACCOUNT.into();
		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			SystemProgramInstruction::transfer(&agg_key_pubkey, &to_pubkey, TRANSFER_AMOUNT),
		];

		let mut tx = SolVersionedTransaction::new_unsigned(SolVersionedMessage::new(
			&instructions,
			Some(agg_key_pubkey),
			Some(durable_nonce),
			&[],
		));
		tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("012e1beb02a24f6e59148fc4eb64aeaeaad291e5f241b8b2d01775a6d3956392ac7186fbee0963d6ca0720bddb5d8b555ada6beb2cd3e9bd0415c343a5ca0cde0b8001000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090340420f000000000004000502e0930400030200020c0200000000ca9a3b0000000000");

		check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
	}

	#[test]
	fn create_transfer_native_with_address_lookup_table() {
		let durable_nonce = (
			const_address("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw").into(),
			const_hash("2qVz58R5aPmF5Q61VaKXnpWQtngdh4Jgbeko32fEcECu").into(),
		);
		let alt = SolAddressLookupTableAccount {
			key: const_address("4EQ4ZTskvNwkBaQjBJW5grcmV5Js82sUooNLHNTpdHdi").into(),
			addresses: vec![const_address("CFnQk1nVmkPThKvLU8EUPFtTuJro45JLSoqux4v23ZGy").into()],
		};

		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();

		let to_pubkey = const_address("CFnQk1nVmkPThKvLU8EUPFtTuJro45JLSoqux4v23ZGy").into();
		let instructions = [
			SystemProgramInstruction::advance_nonce_account(&durable_nonce.0, &agg_key_pubkey),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			SystemProgramInstruction::transfer(&agg_key_pubkey, &to_pubkey, TRANSFER_AMOUNT),
		];

		let mut tx = SolVersionedTransaction::new_unsigned(SolVersionedMessage::V0(
			SolVersionedMessageV0::new_with_blockhash(
				&instructions,
				Some(agg_key_pubkey),
				durable_nonce.1,
				&[alt],
			),
		));
		tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce.1);

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("01ed1357672e0e660e9afd6dd948bee446639a232171900b89a1d403e78e58ad30d8da3986888c9e07ec066b19198b59f99428c00fcf858040e669185473ded5008001000305f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19200000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000001b48568b09b08111ebdcf9a5073d86a4506a3c3fe2a6d47a8a5ce0c459a65bce04020301040004040000000300090340420f000000000003000502e0930400020200050c0200000000ca9a3b00000000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1010000");

		check_tx_encoding(serialized_tx.clone(), expected_serialized_tx.to_vec());
	}
}

#[test]
fn create_transfer_native() {
	let durable_nonce = TEST_DURABLE_NONCE.into();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();
	let to_pubkey = TRANSFER_TO_ACCOUNT.into();
	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
		ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
		SystemProgramInstruction::transfer(&agg_key_pubkey, &to_pubkey, TRANSFER_AMOUNT),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01dae8967ce403afab6fd43f373ac954fb165b3b1c4cd22bdca4c2812f900d307378c75dbb573553610c1fbde0696b206272d199463fad04e3f043fde8d966550e8001000204f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb31e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a40000000c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004020304050004040000000300090340420f000000000003000502e0930400020200010c0200000000ca9a3b00000000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1010f010d").to_vec();

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[test]
fn create_transfer_cu_priority_fees() {
	let durable_nonce = Hash::from_str("2GGxiEHwtWPGNKH5czvxRGvQTayRvCT1PFsA9yK2iMnq").unwrap();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();
	let to_pubkey = TRANSFER_TO_ACCOUNT.into();

	let lamports = 1_000_000;
	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
		ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
		SystemProgramInstruction::transfer(&agg_key_pubkey, &to_pubkey, lamports),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("019769274c91a9d2e01f3b914586a00d4ff5353bdec53f9794a46a351b327d0ff4259fcf552320d3cbc780a36d27998f05a6376a253472072d34cedb7b5ea2950e8001000204f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb31e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000012c57218f6315b83818802f3522fe7e04c596ae4fe08841e7940bc2f958aaaea04020304050004040000000300090340420f000000000003000502e0930400020200010c0200000040420f0000000000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1010f010d").to_vec();

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[test]
fn create_fetch_native() {
	let durable_nonce = TEST_DURABLE_NONCE.into();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();
	let vault_program_id = VAULT_PROGRAM;
	let deposit_channel: Pubkey = FETCH_FROM_ACCOUNT.into();
	let deposit_channel_historical_fetch =
		derive_fetch_account(SolAddress::from(deposit_channel), vault_program_id)
			.unwrap()
			.address;

	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
		ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
		VaultProgram::with_id(VAULT_PROGRAM).fetch_native(
			vec![11u8, 12u8, 13u8, 55u8, 0u8, 0u8, 0u8, 0u8],
			255,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
			deposit_channel,
			deposit_channel_historical_fetch,
			SYSTEM_PROGRAM_ID,
		),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx =
		tx.finalize_and_serialize().expect("Transaction serialization should succeed");

	// With compute unit price and limit
	let expected_serialized_tx = hex_literal::hex!("013ee259150af559c72cea571486b4ed3dc82b9802f2e7d2e038890cb10ce895a8f177988836623ecb7f026eee7da0d09c69a2c8d606c3dcf4f4197d323142250b8001000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb1be0fac7f9583cfe14f5c09dd7653c597f93168e946760abaad3e3c2cc101f5233306d43f017cdb7b1a324afdc62c79317d5b93e2e63b870143344134db9c60000000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030306070004040000000400090340420f000000000004000502e093040005050800020103158e24658f6c59298c080000000b0c0d3700000000ff013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1010f020d02").to_vec();

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[test]
fn create_fetch_native_in_batch() {
	let durable_nonce = TEST_DURABLE_NONCE.into();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();
	let vault_program_id = VAULT_PROGRAM;

	let deposit_channel_0 = derive_deposit_address(0u64, vault_program_id).unwrap();
	let deposit_channel_1 = derive_deposit_address(1u64, vault_program_id).unwrap();

	let deposit_channel_historical_fetch_0 =
		derive_fetch_account(deposit_channel_0.address, vault_program_id).unwrap();
	let deposit_channel_historical_fetch_1 =
		derive_fetch_account(deposit_channel_1.address, vault_program_id).unwrap();

	let vault_program = VaultProgram::with_id(VAULT_PROGRAM);

	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
		ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
		vault_program.fetch_native(
			0u64.to_le_bytes().to_vec(),
			deposit_channel_0.bump,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
			deposit_channel_0.address,
			deposit_channel_historical_fetch_0.address,
			SYSTEM_PROGRAM_ID,
		),
		vault_program.fetch_native(
			1u64.to_le_bytes().to_vec(),
			deposit_channel_1.bump,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
			deposit_channel_1.address,
			deposit_channel_historical_fetch_1.address,
			SYSTEM_PROGRAM_ID,
		),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx =
		tx.finalize_and_serialize().expect("Transaction serialization should succeed");

	// With compute unit price and limit
	let expected_serialized_tx = hex_literal::hex!("014c5652540033d7e050c36a4e77098fcb21c9be0fc16b3612f2f800280471333d3e57eb63f854ece184a632f6ed9f88a5c349f7584d6240eca86f3df2cf2d50018001000308f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb38861d2f0bf5cd80031b701a6c25d13b4c812dd92f9d6301fafd9a58fb9e438646cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a708d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7dd00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005050308090004040000000600090340420f000000000006000502e093040007050a00020105158e24658f6c59298c080000000000000000000000ff07050a00030405158e24658f6c59298c080000000100000000000000ff013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1010f020d02").to_vec();

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[test]
fn create_fetch_tokens() {
	let durable_nonce = TEST_DURABLE_NONCE.into();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();
	let vault_program_id = VAULT_PROGRAM;
	let token_mint_pubkey = USDC_TOKEN_MINT_PUB_KEY;

	let seed = 0u64;
	let deposit_channel = derive_deposit_address(seed, vault_program_id).unwrap();
	let deposit_channel_ata =
		derive_associated_token_account(deposit_channel.address, token_mint_pubkey).unwrap();
	let deposit_channel_historical_fetch =
		derive_fetch_account(deposit_channel_ata.address, vault_program_id).unwrap();

	// Deposit channel derived from the Vault address from the seed and the bump
	assert_eq!(
		deposit_channel,
		PdaAndBump {
			address: SolAddress::from_str("5mP7x1r66PC62PFxXTiEEJVd2Guddc3vWEAkhgWxXehm").unwrap(),
			bump: 255u8
		},
	);
	assert_eq!(
		deposit_channel_ata,
		PdaAndBump {
			address: SolAddress::from_str("5WXnwDp1AA4QZqi3CJEx7HGjTPBj9h42pLwCRuV7AmGs").unwrap(),
			bump: 255u8
		},
	);
	// Historical fetch account derived from the Vault address using the ATA as the seed
	assert_eq!(
		deposit_channel_historical_fetch,
		PdaAndBump {
			address: SolAddress::from_str("CkGQUU19izDobt5NLGmj2h6DBMFRkmj6WN6onNtQVwzn").unwrap(),
			bump: 255u8
		},
	);
	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
		ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
		VaultProgram::with_id(VAULT_PROGRAM).fetch_tokens(
			seed.to_le_bytes().to_vec(),
			deposit_channel.bump,
			6,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
			deposit_channel.address,
			deposit_channel_ata.address,
			USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
			USDC_TOKEN_MINT_PUB_KEY,
			TOKEN_PROGRAM_ID,
			deposit_channel_historical_fetch.address,
			SYSTEM_PROGRAM_ID,
		),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("0181f90395d5dc341ad7ff266b0d3cc42942977bd4257d5df86260ce99a338250c2d76f6d4cf4c7cc51e6c65eb697c82564d2bdb302520671b6680f9da6417c1078001000407f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb42ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910ae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000046cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030307090004040000000400090340420f000000000004000502e093040006090c000501080b0a020316494710642cb0c646080000000000000000000000ff06013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1020f05040d090302").to_vec();

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[test]
fn create_batch_fetch() {
	let durable_nonce = TEST_DURABLE_NONCE.into();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();
	let vault_program_id = VAULT_PROGRAM;
	let token_mint_pubkey = USDC_TOKEN_MINT_PUB_KEY;

	let deposit_channel_0 = derive_deposit_address(0u64, vault_program_id).unwrap();
	let deposit_channel_ata_0 =
		derive_associated_token_account(deposit_channel_0.address, token_mint_pubkey).unwrap();
	let deposit_channel_historical_fetch_0 =
		derive_fetch_account(deposit_channel_ata_0.address, vault_program_id).unwrap();

	let deposit_channel_1 = derive_deposit_address(1u64, vault_program_id).unwrap();
	let deposit_channel_ata_1 =
		derive_associated_token_account(deposit_channel_1.address, token_mint_pubkey).unwrap();
	let deposit_channel_historical_fetch_1 =
		derive_fetch_account(deposit_channel_ata_1.address, vault_program_id).unwrap();

	let deposit_channel_2 = derive_deposit_address(2u64, vault_program_id).unwrap();
	let deposit_channel_historical_fetch_2 =
		derive_fetch_account(deposit_channel_2.address, vault_program_id).unwrap();

	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
		ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
		VaultProgram::with_id(VAULT_PROGRAM).fetch_tokens(
			0u64.to_le_bytes().to_vec(),
			deposit_channel_0.bump,
			6,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
			deposit_channel_0.address,
			deposit_channel_ata_0.address,
			USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
			USDC_TOKEN_MINT_PUB_KEY,
			TOKEN_PROGRAM_ID,
			deposit_channel_historical_fetch_0.address,
			SYSTEM_PROGRAM_ID,
		),
		VaultProgram::with_id(VAULT_PROGRAM).fetch_tokens(
			1u64.to_le_bytes().to_vec(),
			deposit_channel_1.bump,
			6,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
			deposit_channel_1.address,
			deposit_channel_ata_1.address,
			USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
			USDC_TOKEN_MINT_PUB_KEY,
			TOKEN_PROGRAM_ID,
			deposit_channel_historical_fetch_1.address,
			SYSTEM_PROGRAM_ID,
		),
		VaultProgram::with_id(VAULT_PROGRAM).fetch_native(
			2u64.to_le_bytes().to_vec(),
			deposit_channel_2.bump,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
			deposit_channel_2.address,
			deposit_channel_historical_fetch_2.address,
			SYSTEM_PROGRAM_ID,
		),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01109e7735c3b1929c43f910f62843e8702fe4c625015b5f4753f94816fbefff3919c65c69117fd51b32cb8e35b4f974e9d9176cb46d1860d87a4bf13d78991c07800100050cf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb1ad0968d57ee79348476716f9b2cd44ec4284b8f52c36648d560949e41589a5540de1c0451cccb6edd1fda9b4a48c282b279350b55a7a9716800cc0132b6f0b042ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910a140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aaae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21fb4baefcd4965beb1c71311a2ffe76419d4b8f8d35fbc4cf514b1bd02da2df2e300000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000046cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000607030c0e0004040000000800090340420f000000000008000502e09304000a09110009030d100f050716494710642cb0c646080000000000000000000000ff060a0911000b010d100f020716494710642cb0c646080000000100000000000000ff060a051100040607158e24658f6c59298c080000000200000000000000ff013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1020f05040d090302").to_vec();

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[test]
fn create_transfer_tokens() {
	let durable_nonce = TEST_DURABLE_NONCE.into();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();
	let token_mint_pubkey = USDC_TOKEN_MINT_PUB_KEY;

	let to_pubkey = TRANSFER_TO_ACCOUNT;
	let to_pubkey_ata = derive_associated_token_account(to_pubkey, token_mint_pubkey).unwrap();

	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
		ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
		AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
			&agg_key_pubkey,
			&to_pubkey.into(),
			&USDC_TOKEN_MINT_PUB_KEY.into(),
			&to_pubkey_ata.address.into(),
		),
		VaultProgram::with_id(VAULT_PROGRAM).transfer_tokens(
			TRANSFER_AMOUNT,
			SOL_USDC_DECIMAL,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
			TOKEN_VAULT_PDA_ACCOUNT,
			USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
			to_pubkey_ata.address,
			USDC_TOKEN_MINT_PUB_KEY,
			TOKEN_PROGRAM_ID,
		),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("012c98746f87d936c0fb88b47ed6552936ae0ca2f2c80e9da10fe185d3400a4fc49310af53a6707f08f095a6097e96b61034288eb456624fe18b5f3c3aa9169c028001000507f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb5ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec461600000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000031e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005020307090004040000000300090340420f000000000003000502e093040006060001040b020a010105070c000d08010b0a1136b4eeaf4a557ebc00ca9a3b0000000006013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1020f05050d09030204").to_vec();

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

// Full rotation: Use nonce, rotate agg key, transfer nonce authority and transfer upgrade
// manager's upgrade authority
#[test]
fn create_full_rotation() {
	let durable_nonce = TEST_DURABLE_NONCE.into();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();
	let new_agg_key_pubkey = NEW_AGG_KEY.into();

	let mut instructions = vec![
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
		ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
		VaultProgram::with_id(VAULT_PROGRAM).rotate_agg_key(
			false,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
			new_agg_key_pubkey,
			SYSTEM_PROGRAM_ID,
		),
	];
	instructions.extend(NONCE_ACCOUNTS.into_iter().map(|nonce_account| {
		SystemProgramInstruction::nonce_authorize(
			&nonce_account.into(),
			&agg_key_pubkey,
			&new_agg_key_pubkey,
		)
	}));

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("015130c13d6ad86f0a6896c0903a53556ff1a66c933f43452f0b4e46362504f6a5dcfefe025fa4e164a65d40f79fbcc3bc10181feb55a8cafec4eec143f59f57098001000305f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb6744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000e020305100004040000000300090340420f000000000003000502e0930400040408000102094e518fabdda5d68b000202050024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020e0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020c0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020d0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020a0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440202090024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440202060024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020f0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020b0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440202070024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10b0f15180214131711121016010d").to_vec();

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[test]
fn create_ccm_native_transfer() {
	let durable_nonce = TEST_DURABLE_NONCE.into();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();
	let to_pubkey = TRANSFER_TO_ACCOUNT.into();
	let extra_accounts = ccm_accounts();

	let ccm_parameter = ccm_parameter();

	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
		ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
		SystemProgramInstruction::transfer(&agg_key_pubkey, &to_pubkey, TRANSFER_AMOUNT),
		VaultProgram::with_id(VAULT_PROGRAM)
			.execute_ccm_native_call(
				ccm_parameter.source_chain as u32,
				ForeignChainAddress::raw_bytes(ccm_parameter.source_address.unwrap()),
				ccm_parameter.channel_metadata.message.to_vec(),
				TRANSFER_AMOUNT,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				to_pubkey,
				extra_accounts.clone().cf_receiver,
				SYSTEM_PROGRAM_ID,
				SYS_VAR_INSTRUCTIONS,
			)
			.with_additional_accounts(extra_accounts.additional_account_metas()),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01f9a72d745012db811148e23848ad4efdfe281a27e1913b97eabe6de985281cd801d1881ba769beb7b59ac78643ee9aff77964a245f4fcd9a545f2858395963038001000407f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb31e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd47417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005030307090004040000000400090340420f000000000004000502e0930400030200010c0200000000ca9a3b0000000005070a000102030806347d050be38042e0b20100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1010f030a0d02").to_vec();

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[test]
fn create_ccm_token_transfer() {
	let durable_nonce = TEST_DURABLE_NONCE.into();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();
	let amount = TRANSFER_AMOUNT;
	let token_mint_pubkey = USDC_TOKEN_MINT_PUB_KEY;
	let extra_accounts = ccm_accounts();
	let ccm_parameter = ccm_parameter();

	let to_pubkey = TRANSFER_TO_ACCOUNT;
	let to_pubkey_ata = derive_associated_token_account(to_pubkey, token_mint_pubkey).unwrap();

	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
		ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
		AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
			&agg_key_pubkey,
			&to_pubkey.into(),
			&token_mint_pubkey.into(),
			&to_pubkey_ata.address.into(),
		),
		VaultProgram::with_id(VAULT_PROGRAM).transfer_tokens(
			amount,
			SOL_USDC_DECIMAL,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
			TOKEN_VAULT_PDA_ACCOUNT,
			USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
			to_pubkey_ata.address,
			USDC_TOKEN_MINT_PUB_KEY,
			TOKEN_PROGRAM_ID,
		),
		VaultProgram::with_id(VAULT_PROGRAM)
			.execute_ccm_token_call(
				ccm_parameter.source_chain as u32,
				ForeignChainAddress::raw_bytes(ccm_parameter.source_address.unwrap()),
				ccm_parameter.channel_metadata.message.to_vec(),
				amount,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				to_pubkey_ata.address,
				extra_accounts.clone().cf_receiver,
				TOKEN_PROGRAM_ID,
				USDC_TOKEN_MINT_PUB_KEY,
				SYS_VAR_INSTRUCTIONS,
			)
			.with_additional_accounts(extra_accounts.additional_account_metas()),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("018b98e8b7845bdc24a31f886d4c9c389cd04ca5b8343d2b978fd688123b445865c89be8c1994bc52aefb1337694a54037db2204930c4631ef8b086eab4c55ff0d8001000609f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb5ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec46167417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000031e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900060303090c0004040000000400090340420f000000000004000502e093040007060001050e030d010106070f00100a010e0d1136b4eeaf4a557ebc00ca9a3b000000000606080f0001020d0e0b08346cb8a27b9fdeaa230100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1020f05060a0d09030204").to_vec();

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[test]
fn create_idempotent_associated_token_account() {
	let durable_nonce = Hash::from_str("3GY33ibbFkTSdXeXuPAh2NxGTwm1TfEFNKKG9XjxFa67").unwrap();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();

	// This is needed to derive the pda_ata to create the
	// createAssociatedTokenAccountIdempotentInstruction but for now we just derive it manually
	let to = Pubkey::from_str("pyq7ySiH5RvKteu2vdXKC7SNyNDp9vNDkGXdHxSpPtu").unwrap();
	let to_ata = Pubkey::from_str("EbarLzqEb9jf2ZHUdDf5nuBP52Ut3ddLZtYrGwKh3Bbd").unwrap();
	let mint_pubkey = Pubkey::from_str("21ySx9qZoscVT8ViTZjcudCCJeThnXfLPe1sLvezqRCv").unwrap();

	// This would lack the idempotent account creating but that's fine for the test
	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
			&agg_key_pubkey,
			&to,
			&mint_pubkey,
			&to_ata,
		),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01b04eca57a861bafa1bfba213483a31e569f77a1b4031e46f89bc64524b76c7bb75b03e2695b2b3f5c4b5d3ec49935975b6e04efc2498bed00db0005e038a5d098001000406f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fbca03f3e6d6fd79aaf8ebd4ce053492a34f22d0edafbfa88a380848d9a473515000000000000000000000000000000000000000000000000000000000000000000c4a8e3702f6e26d9d0c900c1461da4e3debef5743ce253bb9f0308a68c944220f1b83220b1108ea0e171b5391e6c0157370c8353516b74e962f855be6d787038c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f85921b22d7dfc8cdeba6027384563948d038a11eba06289de51a15c3d649d1f7e2c020203060700040400000005060001030402080101013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1010f020d09").to_vec();

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[test]
fn create_set_program_swaps_parameters() {
	let durable_nonce = TEST_DURABLE_NONCE.into();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();

	let min_native_swap_amount = 5000000000;
	let max_dst_address_len = 128;
	let max_ccm_message_len = 10000;
	let max_cf_parameters_len = 2000;
	let max_event_accounts = 500;

	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		VaultProgram::with_id(VAULT_PROGRAM).set_program_swaps_parameters(
			min_native_swap_amount,
			max_dst_address_len,
			max_ccm_message_len,
			max_cf_parameters_len,
			max_event_accounts,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
		),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01c4ae4790cb7839ef2788e714b98d912c708bfc2302d669dcc46173e503574f5c9cdf11e74412c99647ded332b29bd50986f814c053d6d5a8d11be7dc90194c048001000203f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb000000000000000000000000000000000000000000000000000000000000000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000201030305000404000000020204001e81fe1f976f95874d00f2052a01000000800010270000d0070000f4010000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1020f02010d");

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[test]
fn create_enable_token_support() {
	let durable_nonce = TEST_DURABLE_NONCE.into();
	let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();

	let min_swap_amount = 5;
	let token_mint_pubkey = USDC_TOKEN_MINT_PUB_KEY;

	let token_supported_account =
		derive_token_supported_account(VAULT_PROGRAM, token_mint_pubkey).unwrap();

	let instructions = [
		SystemProgramInstruction::advance_nonce_account(&NONCE_ACCOUNTS[0].into(), &agg_key_pubkey),
		VaultProgram::with_id(VAULT_PROGRAM).enable_token_support(
			min_swap_amount,
			VAULT_PROGRAM_DATA_ACCOUNT,
			agg_key_pubkey,
			token_supported_account.address,
			token_mint_pubkey,
			SYSTEM_PROGRAM_ID,
		),
	];

	let mut tx = VersionedTransaction::new_unsigned(VersionedMessage::new(
		&instructions,
		Some(agg_key_pubkey),
		Some(durable_nonce),
		&[chainflip_alt()],
	));
	tx.test_only_sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("0113a564a5d82ec331ca625650bff5e4397f07ab1b1ad82934122fda8f8d19e5c2a11983dd56a223dec7a683e5d276bd206c8fef0f4bc9727e0b976f21c16fef0a8001000204f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb827837a16a3338d01477a4b6ce9ab9fb1f571fd8f53a08d15717671b921d68fd000000000000000000000000000000000000000000000000000000000000000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900020203040600040400000003050500010702107da0b4321a1b70990500000000000000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1020f02020d03");

	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

// These tests can be used to manually serialize a transaction from a Solana Transaction in
// storage, for instance if a transaction has failed to be broadcasted. While this won't be
// necessary after PR#5229 it might be needed before that if we need to debug and/or manually
// broadcast a transaction. The serialized output (hex string) can be broadcasted via the Solana
// RPC call `sendTransaction` or the web3js `sendRawTransaction.
// The Transaction values to serialize are obtained from querying storage element
// solanaBroadcaster < pendingApiCalls. The signature of the transaction is what in storage is
// named `transactionOutId`, since in Solana the signature is the transaction identifier.
// The test parameters are from a localnet run where we get both the Transaction and the
// resulting serialized transaction so we can compare that this serialization matches the
// successful one.
#[ignore]
#[test]
fn test_encode_tx() {
	let tx: VersionedTransaction = VersionedTransaction {
        signatures: vec![
            SolSignature(hex_literal::hex!(
                "d1144b223b6b600de4b2d96bdceb03573a3e9781953e4c668c57e505f017859d96543243b4d904dc2f02f2f5ab5db7ba4551c7e015e64078add4674ac2e7460c"
            )),
        ],
        message: VersionedMessage::V0( VersionedMessageV0 {
            header: MessageHeader {
                num_required_signatures: 1,
                num_readonly_signed_accounts: 0,
                num_readonly_unsigned_accounts: 8,
            },
            account_keys: vec![
                Pubkey(hex_literal::hex!(
                    "2e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef2"
                )),
                Pubkey(hex_literal::hex!(
                    "22020a74fd97df45db96d2bbf4e485ccbec56945155ff8f668856be26c9de4a9"
                )),
                Pubkey(hex_literal::hex!(
                    "79c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f3036"
                )),
                Pubkey(hex_literal::hex!(
                    "8cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adb"
                )),
                Pubkey(hex_literal::hex!(
                    "8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870"
                )),
                Pubkey(hex_literal::hex!(
                    "b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7dd"
                )),
                Pubkey(hex_literal::hex!(
                    "f53a2f4350451db5595a75e231519bc2758798f72550e57487722e7cbe954dbc"
                )),
                Pubkey(hex_literal::hex!(
                    "0000000000000000000000000000000000000000000000000000000000000000"
                )),
                Pubkey(hex_literal::hex!(
                    "0306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a40000000"
                )),
                Pubkey(hex_literal::hex!(
                    "06a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000"
                )),
                Pubkey(hex_literal::hex!(
                    "06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9"
                )),
                Pubkey(hex_literal::hex!(
                    "0fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee87"
                )),
                Pubkey(hex_literal::hex!(
                    "72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c"
                )),
                Pubkey(hex_literal::hex!(
                    "a140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aa"
                )),
                Pubkey(hex_literal::hex!(
                    "a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00b"
                )),
            ],
            recent_blockhash: SolHash(hex_literal::hex!(
                "f7f02ac4729abaa97c01aa6526ba909c3bcb16c7f47c7e13dfdc5a1b15f647b4"
            ))
            .into(),
            instructions: vec![
                CompiledInstruction {
                    program_id_index: 7,
                    accounts: hex_literal::hex!("030900").to_vec(),
                    data: hex_literal::hex!("04000000").to_vec(),
                },
                CompiledInstruction {
                    program_id_index: 8,
                    accounts: vec![],
                    data: hex_literal::hex!("030a00000000000000").to_vec(),
                },
                CompiledInstruction {
                    program_id_index: 8,
                    accounts: vec![],
                    data: hex_literal::hex!("0233620100").to_vec(),
                },
                CompiledInstruction {
                    program_id_index: 12,
                    accounts: hex_literal::hex!("0e00040507").to_vec(),
                    data: hex_literal::hex!("8e24658f6c59298c080000000100000000000000ff").to_vec(),
                },
                CompiledInstruction {
                    program_id_index: 12,
                    accounts: hex_literal::hex!("0e000d01020b0a0607").to_vec(),
                    data: hex_literal::hex!("494710642cb0c646080000000200000000000000ff06").to_vec(),
                },
            ],
			address_table_lookups: vec![],
        }),
    };

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01d1144b223b6b600de4b2d96bdceb03573a3e9781953e4c668c57e505f017859d96543243b4d904dc2f02f2f5ab5db7ba4551c7e015e64078add4674ac2e7460c800100080f2e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef222020a74fd97df45db96d2bbf4e485ccbec56945155ff8f668856be26c9de4a979c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f30368cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adb8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7ddf53a2f4350451db5595a75e231519bc2758798f72550e57487722e7cbe954dbc00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8772b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aaa1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bf7f02ac4729abaa97c01aa6526ba909c3bcb16c7f47c7e13dfdc5a1b15f647b40507030309000404000000080009030a0000000000000008000502336201000c050e00040507158e24658f6c59298c080000000100000000000000ff0c090e000d01020b0a060716494710642cb0c646080000000200000000000000ff0600").to_vec();
	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}

#[ignore]
#[test]
fn test_encode_tx_fetch() {
	let tx: VersionedTransaction = VersionedTransaction {
        signatures: vec![
            SolSignature(hex_literal::hex!(
                "94b38e57e31dc130acdec802f60b2095b72916a44834f8b0a40b7e4949661c9e4e05aa3fa5a3dc3e285c8d16c8eaab079d4477daa76e9e4a1915603eda58bc0c"
            )),
        ],
        message: VersionedMessage::V0( VersionedMessageV0 {
            header: MessageHeader {
                num_required_signatures: 1,
                num_readonly_signed_accounts: 0,
                num_readonly_unsigned_accounts: 9,
            },
            account_keys: vec![
                Pubkey(hex_literal::hex!(
                    "2e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef2"
                )),
                Pubkey(hex_literal::hex!(
                    "114f68f4ee9add615457c9a7791269b4d4ab3168d43d5da0e018e2d547d8be92"
                )),
                Pubkey(hex_literal::hex!(
                    "287f3b39b93c6699d704cb3d3edcf633cb8068010c5e5f6e64583078f5cd370e"
                )),
                Pubkey(hex_literal::hex!(
                    "3e1cb8c1bfc20346cebcaa28a53b234acf92771f72151b2d6aaa1d765be4b93c"
                )),
                Pubkey(hex_literal::hex!(
                    "45f3121cddc0bab152917a22710c9fab5be66d121bf2474d4d484f0f2eed9780"
                )),
                Pubkey(hex_literal::hex!(
                    "4813c8373d2bfc1592855e2d93b70ecd407fe9338b11ff0bb10650716709f6a7"
                )),
                Pubkey(hex_literal::hex!(
                    "491102d3be1d348108b41a801904392e50cd5b443a0991f3c1db0427634627da"
                )),
                Pubkey(hex_literal::hex!(
                    "5d89a80ca1700def3a784b845b59f9c2a61bb07941ddcb4fd2d709c3243c1350"
                )),
                Pubkey(hex_literal::hex!(
                    "79c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f3036"
                )),
                Pubkey(hex_literal::hex!(
                    "c9b5b17535d2dcb7a1a505fbadc9ea27cddada4b7c144e549cf880e8db046d77"
                )),
                Pubkey(hex_literal::hex!(
                    "ca586493b85289057a8661f9f2a81e546fcf8cc6f5c9df1f5441c822f6fabfc9"
                )),
                Pubkey(hex_literal::hex!(
                    "e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864"
                )),
                Pubkey(hex_literal::hex!(
                    "efe57cc00ff8edda422ba876d38f5905694bfbef1c35deaea90295968dc13339"
                )),
                Pubkey(hex_literal::hex!(
                    "0000000000000000000000000000000000000000000000000000000000000000"
                )),
                Pubkey(hex_literal::hex!(
                    "0306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a40000000"
                )),
                Pubkey(hex_literal::hex!(
                    "06a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000"
                )),
                Pubkey(hex_literal::hex!(
                    "06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9"
                )),
                Pubkey(hex_literal::hex!(
                    "0fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee87"
                )),
                Pubkey(hex_literal::hex!(
                    "2b635a1da73cd5bf15a26f1170f49366f0f48d28b0a8b1cebc5f98c75e475e68"
                )),
                Pubkey(hex_literal::hex!(
                    "42be1bb8dfd763b0e83541c9767712ad0d89cecea13b46504370096a20c762fb"
                )),
                Pubkey(hex_literal::hex!(
                    "72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c"
                )),
                Pubkey(hex_literal::hex!(
                    "a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00b"
                )),
            ],
            recent_blockhash: SolHash(hex_literal::hex!(
                "9a5e41fc2cbe01a629ce980d5c6aa9c0a8b7be9d83ac835586feba35181d4246"
            ))
            .into(),
            instructions: vec![
                CompiledInstruction {
                    program_id_index: 13,
                    accounts: hex_literal::hex!("0b0f00").to_vec(),
                    data: hex_literal::hex!("04000000").to_vec(),
                },
                CompiledInstruction {
                    program_id_index: 14,
                    accounts: vec![],
                    data: hex_literal::hex!("030a00000000000000").to_vec(),
                },
                CompiledInstruction {
                    program_id_index: 14,
                    accounts: vec![],
                    data: hex_literal::hex!("02a7190300").to_vec(),
                },
                CompiledInstruction {
                    program_id_index: 20,
                    accounts: hex_literal::hex!("150012090811100a0d").to_vec(),
                    data: hex_literal::hex!("494710642cb0c646080000001e00000000000000fd06").to_vec(),
                },
                CompiledInstruction {
                    program_id_index: 20,
                    accounts: hex_literal::hex!("150003010d").to_vec(),
                    data: hex_literal::hex!("8e24658f6c59298c080000001400000000000000fd").to_vec(),
                },
                CompiledInstruction {
                    program_id_index: 20,
                    accounts: hex_literal::hex!("1500130c081110020d").to_vec(),
                    data: hex_literal::hex!("494710642cb0c646080000000e00000000000000ff06").to_vec(),
                },
                CompiledInstruction {
                    program_id_index: 20,
                    accounts: hex_literal::hex!("150004060d").to_vec(),
                    data: hex_literal::hex!("8e24658f6c59298c080000000f00000000000000fb").to_vec(),
                },
                CompiledInstruction {
                    program_id_index: 20,
                    accounts: hex_literal::hex!("150007050d").to_vec(),
                    data: hex_literal::hex!("8e24658f6c59298c080000000500000000000000fe").to_vec(),
                },
            ],
			address_table_lookups: vec![],
        }),
    };

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("0194b38e57e31dc130acdec802f60b2095b72916a44834f8b0a40b7e4949661c9e4e05aa3fa5a3dc3e285c8d16c8eaab079d4477daa76e9e4a1915603eda58bc0c80010009162e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef2114f68f4ee9add615457c9a7791269b4d4ab3168d43d5da0e018e2d547d8be92287f3b39b93c6699d704cb3d3edcf633cb8068010c5e5f6e64583078f5cd370e3e1cb8c1bfc20346cebcaa28a53b234acf92771f72151b2d6aaa1d765be4b93c45f3121cddc0bab152917a22710c9fab5be66d121bf2474d4d484f0f2eed97804813c8373d2bfc1592855e2d93b70ecd407fe9338b11ff0bb10650716709f6a7491102d3be1d348108b41a801904392e50cd5b443a0991f3c1db0427634627da5d89a80ca1700def3a784b845b59f9c2a61bb07941ddcb4fd2d709c3243c135079c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f3036c9b5b17535d2dcb7a1a505fbadc9ea27cddada4b7c144e549cf880e8db046d77ca586493b85289057a8661f9f2a81e546fcf8cc6f5c9df1f5441c822f6fabfc9e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864efe57cc00ff8edda422ba876d38f5905694bfbef1c35deaea90295968dc1333900000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee872b635a1da73cd5bf15a26f1170f49366f0f48d28b0a8b1cebc5f98c75e475e6842be1bb8dfd763b0e83541c9767712ad0d89cecea13b46504370096a20c762fb72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00b9a5e41fc2cbe01a629ce980d5c6aa9c0a8b7be9d83ac835586feba35181d4246080d030b0f0004040000000e0009030a000000000000000e000502a71903001409150012090811100a0d16494710642cb0c646080000001e00000000000000fd061405150003010d158e24658f6c59298c080000001400000000000000fd14091500130c081110020d16494710642cb0c646080000000e00000000000000ff061405150004060d158e24658f6c59298c080000000f00000000000000fb1405150007050d158e24658f6c59298c080000000500000000000000fe00").to_vec();
	check_tx_encoding(serialized_tx, expected_serialized_tx.to_vec());
}
