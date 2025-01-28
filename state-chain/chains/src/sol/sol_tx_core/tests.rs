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
			program_instructions::{InstructionExt, SystemProgramInstruction, VaultProgram},
			signer::Signer,
			sol_test_values::*,
			token_instructions::AssociatedTokenAccountInstruction,
			AccountMeta, CompiledInstruction, Hash, Instruction, LegacyMessage, LegacyTransaction,
			MessageHeader, Pubkey,
		},
		SolAddress, SolHash, SolSignature,
	},
	ForeignChainAddress,
};

use core::str::FromStr;

use sol_prim::{
	consts::{
		MAX_TRANSACTION_LENGTH, SOL_USDC_DECIMAL, SYSTEM_PROGRAM_ID, SYS_VAR_INSTRUCTIONS,
		TOKEN_PROGRAM_ID,
	},
	PdaAndBump,
};

#[derive(BorshSerialize, BorshDeserialize)]
enum BankInstruction {
	Initialize,
	Deposit { lamports: u64 },
	Withdraw { lamports: u64 },
}

#[test]
fn create_simple_tx() {
	let program_id = Pubkey([0u8; 32]);
	let payer = SolSigningKey::new();
	let bank_instruction = BankInstruction::Initialize;

	let instruction = Instruction::new_with_borsh(program_id, &bank_instruction, vec![]);

	let mut tx = LegacyTransaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
	tx.sign(vec![payer].into(), Default::default());
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
	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01345c86d1be2bcdf2c93c75b6054b6232e5b1e7f2fe7b3ca241d48c8a5f993af3e474bf581b2e9a1543af13104b3f3a53530d849731cc403418da313743a57e0401000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090340420f000000000004000502e0930400030200020c0200000000ca9a3b00000000").to_vec();

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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
	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("017036ecc82313548a7f1ef280b9d7c53f9747e23abcb4e76d86c8df6aa87e82d460ad7cea2e8d972a833d3e1802341448a99be200ad4648c454b9d5a5e2d5020d01000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000012c57218f6315b83818802f3522fe7e04c596ae4fe08841e7940bc2f958aaaea04030301050004040000000400090340420f000000000004000502e0930400030200020c0200000040420f0000000000").to_vec();

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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
	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx =
		tx.finalize_and_serialize().expect("Transaction serialization should succeed");

	// With compute unit price and limit
	let expected_serialized_tx = hex_literal::hex!("01292f542c6677c72234d0783809765218bdae59e21008d91213520ab30603fb4af885f82cd76713deddec1d3843887f39c114016a48902d2eeb443877f1d01a0201000509f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921be0fac7f9583cfe14f5c09dd7653c597f93168e946760abaad3e3c2cc101f5233306d43f017cdb7b1a324afdc62c79317d5b93e2e63b870143344134db9c60000000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004040301060004040000000500090340420f000000000005000502e093040007050800030204158e24658f6c59298c080000000b0c0d3700000000ff").to_vec();

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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
	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx =
		tx.finalize_and_serialize().expect("Transaction serialization should succeed");

	// With compute unit price and limit
	let expected_serialized_tx = hex_literal::hex!("01eea631a27abfd2a361f68f2b4d6c25bc9fba2ad0b12dabaf12f1cf97cd47a453dbfb78dc6394c5844e4c198f9ffa508352b543b2c7414605a27e72a4dc4209000100050bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19238861d2f0bf5cd80031b701a6c25d13b4c812dd92f9d6301fafd9a58fb9e438646cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a708d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7dd00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005060301080004040000000700090340420f000000000007000502e093040009050a00030206158e24658f6c59298c080000000000000000000000ff09050a00040506158e24658f6c59298c080000000100000000000000ff").to_vec();

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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
	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01399c2b68c7fae634a32d15c3417c2a92e3632707fff366cf9f92a085642344915b7de51181defe33f87ac0a718cd6df5849e229d54d7a06b362b621855c367010100080df79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19242ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910ae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21fe91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8746cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004050301070004040000000600090340420f000000000006000502e09304000b090c000a02040908030516494710642cb0c646080000000000000000000000ff06").to_vec();

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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
	let message = LegacyMessage::new(&instructions, Some(&agg_key_pubkey));
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("018117c8fd4d21069f04599ab5c79c6d093991392ca54dacfcefac64585928ae13ae81a9aa51a003a10a47a0d9372301e36df2ca2a0e7797d179d030b30563b20d01000912f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921ad0968d57ee79348476716f9b2cd44ec4284b8f52c36648d560949e41589a5540de1c0451cccb6edd1fda9b4a48c282b279350b55a7a9716800cc0132b6f0b042ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910a140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aaae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21fb4baefcd4965beb1c71311a2ffe76419d4b8f8d35fbc4cf514b1bd02da2df2e3e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8746cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900060903010b0004040000000a00090340420f00000000000a000502e09304000f0911000e04080d0c060916494710642cb0c646080000000000000000000000ff060f0911001002080d0c030916494710642cb0c646080000000100000000000000ff060f051100050709158e24658f6c59298c080000000200000000000000ff").to_vec();

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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
	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("012c77634fbfac44e246e991202e24d1b6c2fc438482fa9dbea617b0387aa3d19e2561dd12929db8cc2ae43ccd0f19185882bfac2ef6f7baf1438929cd5b99dd0701000a0ef79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec4616e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301060004040000000500090340420f000000000005000502e09304000b0600020908040701010a070c000d030208071136b4eeaf4a557ebc00ca9a3b0000000006").to_vec();

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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

	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("0118e0ec3502ff3656ffeecb9c56cc0e6676bba9ac1026a535e24ab3e2f9ef78353e1c3698bd3582a1e711b68cc0e39802c04dd48a298a572e55fd2e2a6bc2770101000411f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1926744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900448541f57201f277c5f3ffb631d0212e26e7f47749c26c4808718174a0ab2a09a18cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adba1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bcd644e45426a41a7cb8369b8a0c1c89bb3f86cf278fdd9cc38b0f69784ad5667e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864e5e1869817a4fd88ddf7ab7a5f7252d7c345b39721769888608592912e8ca9acf0f13460b3fd04b7d53d7421fc874ec00eec769cf36480895e1a407bf1249475f2b2e24122be016983be9369965246cc45e1f621d40fba300c56c7ac50c3874df4f83bd213a59c9785110cf83c718f9486c3484f918593bce20c61dc6a96036afecc89e3b031824af6363174d19bbec12d3a13c4a173e5aeb349b63042bc138f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000e0d03010f0004040000000e00090340420f00000000000e000502e093040010040500020d094e518fabdda5d68b000d02010024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020b0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02090024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020a0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02070024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02060024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02030024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020c0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02080024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02040024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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
	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01be5b6acac88600095a934dc7ae8af889c78281664e6b561f3a18bc26887ae95f35fc76d892da32f4a7314a253c0abed2da1c89d5e6daede4d70cacd37942090a0100070bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd47417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00ba73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301070004040000000500090340420f000000000005000502e0930400040200020c0200000000ca9a3b0000000008070900020304060a347d050be38042e0b20100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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
	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01bd5f6ad13bbce1d97011814f8b7758b42a392ecc0b993c7b0be88499cbb089b3b364eaed5f3998f1dd97670f5a4b146c3be9681cb2d71fc81066657b7423d40501000c11f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec46167417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed48e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00ba73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5ab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890006050301080004040000000600090340420f000000000006000502e09304000d0600020b0a050901010c070e001004020a091136b4eeaf4a557ebc00ca9a3b00000000060c080e000203090a070f346cb8a27b9fdeaa230100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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
	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01eb287ff9329fbaf83592ec56709d52d3d7f7edcab7ab53fc8371acff871016c51dfadde692630545a91d6534095bb5697b5fb9ee17dc292552eabf9ab6e3390601000609f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192ca03f3e6d6fd79aaf8ebd4ce053492a34f22d0edafbfa88a380848d9a4735150000000000000000000000000000000000000000000000000000000000000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90c4a8e3702f6e26d9d0c900c1461da4e3debef5743ce253bb9f0308a68c944220f1b83220b1108ea0e171b5391e6c0157370c8353516b74e962f855be6d787038c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f85921b22d7dfc8cdeba6027384563948d038a11eba06289de51a15c3d649d1f7e2c020303010400040400000008060002060703050101").to_vec();

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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
	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = [
		1, 156, 69, 62, 23, 230, 241, 160, 66, 97, 25, 239, 135, 44, 207, 147, 43, 156, 74, 175,
		251, 75, 185, 120, 68, 77, 164, 78, 111, 76, 169, 205, 173, 15, 41, 205, 152, 228, 159,
		104, 73, 91, 32, 65, 149, 19, 118, 247, 242, 207, 13, 83, 20, 15, 183, 19, 46, 251, 113,
		166, 119, 114, 198, 182, 5, 1, 0, 3, 6, 247, 157, 94, 2, 111, 18, 237, 198, 68, 58, 83, 75,
		44, 221, 80, 114, 35, 57, 137, 180, 21, 215, 89, 101, 115, 231, 67, 243, 229, 179, 134,
		251, 23, 235, 43, 16, 211, 55, 123, 218, 43, 199, 190, 166, 91, 236, 107, 131, 114, 244,
		252, 52, 99, 236, 44, 214, 249, 253, 228, 178, 198, 51, 209, 146, 161, 224, 49, 200, 188,
		155, 236, 59, 97, 12, 247, 179, 110, 179, 191, 58, 164, 2, 55, 201, 229, 190, 44, 120, 147,
		135, 133, 120, 67, 158, 176, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 167, 213, 23, 25, 44, 86, 142, 224, 138, 132, 95,
		115, 210, 151, 136, 207, 3, 92, 49, 69, 178, 26, 179, 68, 216, 6, 46, 169, 64, 0, 0, 114,
		181, 210, 5, 29, 48, 11, 16, 183, 67, 20, 183, 226, 90, 206, 153, 152, 202, 102, 235, 44,
		127, 188, 16, 239, 19, 13, 214, 112, 40, 41, 60, 194, 126, 144, 116, 250, 197, 232, 211,
		108, 240, 79, 148, 160, 96, 111, 221, 141, 219, 180, 32, 233, 154, 72, 156, 121, 21, 206,
		86, 153, 228, 137, 0, 2, 3, 3, 1, 4, 0, 4, 4, 0, 0, 0, 5, 2, 2, 0, 30, 129, 254, 31, 151,
		111, 149, 135, 77, 0, 242, 5, 42, 1, 0, 0, 0, 128, 0, 16, 39, 0, 0, 208, 7, 0, 0, 244, 1,
		0, 0,
	];

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
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
	let message =
		LegacyMessage::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
	let mut tx = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![agg_key_keypair].into(), durable_nonce);

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = [
		1, 58, 110, 210, 42, 122, 243, 74, 142, 157, 80, 145, 131, 252, 211, 101, 40, 57, 232, 136,
		226, 205, 247, 146, 228, 92, 31, 106, 4, 105, 239, 129, 136, 136, 230, 249, 67, 29, 214,
		24, 179, 37, 62, 148, 135, 8, 72, 224, 203, 7, 166, 6, 80, 249, 224, 133, 102, 234, 148,
		160, 151, 33, 1, 69, 13, 1, 0, 4, 8, 247, 157, 94, 2, 111, 18, 237, 198, 68, 58, 83, 75,
		44, 221, 80, 114, 35, 57, 137, 180, 21, 215, 89, 101, 115, 231, 67, 243, 229, 179, 134,
		251, 23, 235, 43, 16, 211, 55, 123, 218, 43, 199, 190, 166, 91, 236, 107, 131, 114, 244,
		252, 52, 99, 236, 44, 214, 249, 253, 228, 178, 198, 51, 209, 146, 130, 120, 55, 161, 106,
		51, 56, 208, 20, 119, 164, 182, 206, 154, 185, 251, 31, 87, 31, 216, 245, 58, 8, 209, 87,
		23, 103, 27, 146, 29, 104, 253, 161, 224, 49, 200, 188, 155, 236, 59, 97, 12, 247, 179,
		110, 179, 191, 58, 164, 2, 55, 201, 229, 190, 44, 120, 147, 135, 133, 120, 67, 158, 176,
		11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 6, 167, 213, 23, 25, 44, 86, 142, 224, 138, 132, 95, 115, 210, 151, 136, 207, 3,
		92, 49, 69, 178, 26, 179, 68, 216, 6, 46, 169, 64, 0, 0, 15, 185, 186, 82, 177, 240, 148,
		69, 241, 227, 167, 80, 141, 89, 240, 121, 121, 35, 172, 247, 68, 251, 226, 218, 48, 63,
		176, 109, 168, 89, 238, 135, 114, 181, 210, 5, 29, 48, 11, 16, 183, 67, 20, 183, 226, 90,
		206, 153, 152, 202, 102, 235, 44, 127, 188, 16, 239, 19, 13, 214, 112, 40, 41, 60, 194,
		126, 144, 116, 250, 197, 232, 211, 108, 240, 79, 148, 160, 96, 111, 221, 141, 219, 180, 32,
		233, 154, 72, 156, 121, 21, 206, 86, 153, 228, 137, 0, 2, 4, 3, 1, 5, 0, 4, 4, 0, 0, 0, 7,
		5, 3, 0, 2, 6, 4, 16, 125, 160, 180, 50, 26, 27, 112, 153, 5, 0, 0, 0, 0, 0, 0, 0,
	];

	assert_eq!(serialized_tx, expected_serialized_tx);
	assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
}

// Test taken from https://docs.rs/solana-sdk/latest/src/solana_sdk/transaction/mod.rs.html#1354
// using current serialization (bincode::serde::encode_to_vec) and ensure that it's correct
fn create_sample_transaction() -> LegacyTransaction {
	let keypair = SolSigningKey::from_bytes(&[
		255, 101, 36, 24, 124, 23, 167, 21, 132, 204, 155, 5, 185, 58, 121, 75, 156, 227, 116, 193,
		215, 38, 142, 22, 8, 14, 229, 239, 119, 93, 5, 218,
	])
	.unwrap();
	let to = Pubkey::from([
		1, 1, 1, 4, 5, 6, 7, 8, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 8, 7, 6, 5, 4, 1,
		1, 1,
	]);

	let program_id = Pubkey::from([
		2, 2, 2, 4, 5, 6, 7, 8, 9, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 9, 8, 7, 6, 5, 4, 2,
		2, 2,
	]);
	let account_metas = vec![AccountMeta::new(keypair.pubkey(), true), AccountMeta::new(to, false)];
	let instruction = Instruction::new_with_bincode(program_id, &(1u8, 2u8, 3u8), account_metas);
	let message = LegacyMessage::new(&[instruction], Some(&keypair.pubkey()));
	let mut tx: LegacyTransaction = LegacyTransaction::new_unsigned(message);
	tx.sign(vec![keypair].into(), Hash::default());
	tx
}

#[test]
fn test_sdk_serialize() {
	let tx = create_sample_transaction();
	let serialized_tx = tx.finalize_and_serialize().unwrap();
	// SDK uses serde::serialize instead, but looks like this works.

	assert_eq!(
		serialized_tx,
		vec![
			1, 120, 138, 162, 185, 59, 209, 241, 157, 71, 157, 74, 131, 4, 87, 54, 28, 38, 180,
			222, 82, 64, 62, 61, 62, 22, 46, 17, 203, 187, 136, 62, 43, 11, 38, 235, 17, 239, 82,
			240, 139, 130, 217, 227, 214, 9, 242, 141, 223, 94, 29, 184, 110, 62, 32, 87, 137, 63,
			139, 100, 221, 20, 137, 4, 5, 1, 0, 1, 3, 36, 100, 158, 252, 33, 161, 97, 185, 62, 89,
			99, 195, 250, 249, 187, 189, 171, 118, 241, 90, 248, 14, 68, 219, 231, 62, 157, 5, 142,
			27, 210, 117, 1, 1, 1, 4, 5, 6, 7, 8, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
			8, 7, 6, 5, 4, 1, 1, 1, 2, 2, 2, 4, 5, 6, 7, 8, 9, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
			1, 1, 9, 8, 7, 6, 5, 4, 2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 2, 0, 1, 3, 1, 2, 3
		]
	);
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
	let tx: LegacyTransaction = LegacyTransaction {
        signatures: vec![
            SolSignature(hex_literal::hex!(
                "d1144b223b6b600de4b2d96bdceb03573a3e9781953e4c668c57e505f017859d96543243b4d904dc2f02f2f5ab5db7ba4551c7e015e64078add4674ac2e7460c"
            )),
        ],
        message: LegacyMessage {
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
        },
    };

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("01d1144b223b6b600de4b2d96bdceb03573a3e9781953e4c668c57e505f017859d96543243b4d904dc2f02f2f5ab5db7ba4551c7e015e64078add4674ac2e7460c0100080f2e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef222020a74fd97df45db96d2bbf4e485ccbec56945155ff8f668856be26c9de4a979c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f30368cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adb8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7ddf53a2f4350451db5595a75e231519bc2758798f72550e57487722e7cbe954dbc00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8772b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aaa1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bf7f02ac4729abaa97c01aa6526ba909c3bcb16c7f47c7e13dfdc5a1b15f647b40507030309000404000000080009030a0000000000000008000502336201000c050e00040507158e24658f6c59298c080000000100000000000000ff0c090e000d01020b0a060716494710642cb0c646080000000200000000000000ff06").to_vec();
	assert_eq!(serialized_tx, expected_serialized_tx);
}

#[ignore]
#[test]
fn test_encode_tx_fetch() {
	let tx: LegacyTransaction = LegacyTransaction {
        signatures: vec![
            SolSignature(hex_literal::hex!(
                "94b38e57e31dc130acdec802f60b2095b72916a44834f8b0a40b7e4949661c9e4e05aa3fa5a3dc3e285c8d16c8eaab079d4477daa76e9e4a1915603eda58bc0c"
            )),
        ],
        message: LegacyMessage {
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
        },
    };

	let serialized_tx = tx.finalize_and_serialize().unwrap();
	let expected_serialized_tx = hex_literal::hex!("0194b38e57e31dc130acdec802f60b2095b72916a44834f8b0a40b7e4949661c9e4e05aa3fa5a3dc3e285c8d16c8eaab079d4477daa76e9e4a1915603eda58bc0c010009162e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef2114f68f4ee9add615457c9a7791269b4d4ab3168d43d5da0e018e2d547d8be92287f3b39b93c6699d704cb3d3edcf633cb8068010c5e5f6e64583078f5cd370e3e1cb8c1bfc20346cebcaa28a53b234acf92771f72151b2d6aaa1d765be4b93c45f3121cddc0bab152917a22710c9fab5be66d121bf2474d4d484f0f2eed97804813c8373d2bfc1592855e2d93b70ecd407fe9338b11ff0bb10650716709f6a7491102d3be1d348108b41a801904392e50cd5b443a0991f3c1db0427634627da5d89a80ca1700def3a784b845b59f9c2a61bb07941ddcb4fd2d709c3243c135079c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f3036c9b5b17535d2dcb7a1a505fbadc9ea27cddada4b7c144e549cf880e8db046d77ca586493b85289057a8661f9f2a81e546fcf8cc6f5c9df1f5441c822f6fabfc9e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864efe57cc00ff8edda422ba876d38f5905694bfbef1c35deaea90295968dc1333900000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee872b635a1da73cd5bf15a26f1170f49366f0f48d28b0a8b1cebc5f98c75e475e6842be1bb8dfd763b0e83541c9767712ad0d89cecea13b46504370096a20c762fb72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00b9a5e41fc2cbe01a629ce980d5c6aa9c0a8b7be9d83ac835586feba35181d4246080d030b0f0004040000000e0009030a000000000000000e000502a71903001409150012090811100a0d16494710642cb0c646080000001e00000000000000fd061405150003010d158e24658f6c59298c080000001400000000000000fd14091500130c081110020d16494710642cb0c646080000000e00000000000000ff061405150004060d158e24658f6c59298c080000000f00000000000000fb1405150007050d158e24658f6c59298c080000000500000000000000fe").to_vec();
	assert_eq!(serialized_tx, expected_serialized_tx);
}
