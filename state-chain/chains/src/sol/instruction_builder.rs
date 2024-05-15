//! This file contains a Instruction builder for the Solana chain.
//!
//! The builder provides a interface for the API to create raw Solana
//! Instructions and Instruction sets with some level of abstraction.
//! This avoids the need to deal with low level Solana core types.

use sp_std::{vec, vec::Vec};

use cf_primitives::chains::Solana;

use crate::{
	sol::{
		sol_tx_core::{
			bpf_loader_instructions::set_upgrade_authority,
			compute_budget::ComputeBudgetInstruction,
			program_instructions::{ProgramInstruction, SystemProgramInstruction, VaultProgram},
		},
		SolAccountMeta, SolAddress, SolAmount, SolAsset, SolComputeLimit, SolInstruction,
		SolPubkey,
	},
	DepositChannel, TransferAssetParams,
};

pub struct SolanaInstructionBuilder;

/// TODO: Implement Compute Limit calculation. pro-1357
const COMPUTE_LIMIT: SolComputeLimit = 300_000u32;

impl SolanaInstructionBuilder {
	/// Finalize a Instruction Set. This should be internally called after a instruction set is
	/// complete. This will add some extra instruction required for the integrity of the Solana
	/// Transaction.
	///
	/// Returns the finished Instruction Set to construct the SolTransaction.
	fn finalize(
		mut instructions: Vec<SolInstruction>,
		nonce_account: SolPubkey,
		agg_key: SolPubkey,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		// TODO: implement compute limit calculation
		let compute_limit = COMPUTE_LIMIT;

		let mut final_instructions = vec![
			SystemProgramInstruction::advance_nonce_account(&nonce_account, &agg_key),
			ComputeBudgetInstruction::set_compute_unit_price(compute_price),
			ComputeBudgetInstruction::set_compute_unit_limit(compute_limit),
		];

		final_instructions.append(&mut instructions);

		final_instructions
	}

	/// Create an instruction set to fetch from each `deposit_channel` being passed in.
	/// Used to batch fetch from multiple deposit channels in a single transaction.
	pub fn fetch_from(
		deposit_channels: Vec<DepositChannel<Solana>>,
		vault_program_data_account: SolAddress,
		system_program_id: SolAddress,
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let instructions = deposit_channels
			.into_iter()
			.map(|deposit_channel| match deposit_channel.asset {
				SolAsset::Sol => ProgramInstruction::get_instruction(
					&VaultProgram::FetchNative {
						seed: deposit_channel.state.seed,
						bump: deposit_channel.state.bump,
					},
					vec![
						SolAccountMeta::new_readonly(vault_program_data_account.into(), false),
						SolAccountMeta::new(agg_key.into(), true),
						SolAccountMeta::new(deposit_channel.address.into(), false),
						SolAccountMeta::new_readonly(system_program_id.into(), false),
					],
				),
			})
			.collect::<Vec<_>>();

		Self::finalize(instructions, nonce_account.into(), agg_key.into(), compute_price)
	}

	/// Create an instruction set to `transfer` from our Vault account to a target account.
	pub fn transfer(
		to: TransferAssetParams<Solana>,
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let instructions = match to.asset {
			SolAsset::Sol =>
				vec![SystemProgramInstruction::transfer(&agg_key.into(), &to.to.into(), to.amount)],
		};

		Self::finalize(instructions, nonce_account.into(), agg_key.into(), compute_price)
	}

	/// Create an instruction set to rotate the current Vault agg key to the next key.
	pub fn rotate_agg_key(
		new_agg_key: SolAddress,
		all_nonce_accounts: Vec<SolAddress>,
		vault_program_data_account: SolAddress,
		system_program_id: SolAddress,
		upgrade_manager_program_data_account: SolAddress,
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let mut instructions = vec![
			ProgramInstruction::get_instruction(
				&VaultProgram::RotateAggKey { skip_transfer_funds: false },
				vec![
					SolAccountMeta::new(vault_program_data_account.into(), false),
					SolAccountMeta::new(agg_key.into(), true),
					SolAccountMeta::new(new_agg_key.into(), false),
					SolAccountMeta::new_readonly(system_program_id.into(), false),
				],
			),
			set_upgrade_authority(
				upgrade_manager_program_data_account.into(),
				&agg_key.into(),
				Some(&new_agg_key.into()),
			),
		];
		instructions.extend(all_nonce_accounts.into_iter().map(|nonce_account| {
			SystemProgramInstruction::nonce_authorize(
				&nonce_account.into(),
				&agg_key.into(),
				&new_agg_key.into(),
			)
		}));

		Self::finalize(instructions, nonce_account.into(), agg_key.into(), compute_price)
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::sol::{
		consts::{MAX_TRANSACTION_LENGTH, SYSTEM_PROGRAM_ID},
		sol_tx_core::{
			extra_types_for_testing::{Keypair, Signer},
			generate_deposit_channel,
			sol_test_values::*,
		},
		SolAmount, SolHash, SolMessage, SolTransaction, SolanaDepositChannelState,
	};
	use core::str::FromStr;

	const NEXT_NONCE: &str = NONCE_ACCOUNTS[0];
	const SOL: SolAsset = SolAsset::Sol;

	/// Test deposit channel derived from `sol_tx_core::can_generate_address()`
	/// This is used to check consistency in Fetch logic. This is not a "Valid" deposit channel
	/// since the `seed` is NOT derived from the `channel_id`.
	fn get_deposit_channel() -> DepositChannel<Solana> {
		DepositChannel::<Solana> {
			channel_id: 1u64,
			address: SolPubkey::from_str(FETCH_FROM_ACCOUNT).unwrap().into(),
			asset: SOL,
			state: SolanaDepositChannelState { seed: vec![11u8, 12u8, 13u8, 55u8], bump: 255u8 },
		}
	}

	fn agg_key() -> SolAddress {
		Keypair::from_bytes(&RAW_KEYPAIR)
			.expect("Key pair generation must succeed")
			.pubkey()
			.into()
	}

	fn next_nonce() -> SolAddress {
		SolAddress::from_str(NEXT_NONCE).unwrap()
	}

	fn vault_program_data_account() -> SolAddress {
		SolAddress::from_str(VAULT_PROGRAM_DATA_ACCOUNT).unwrap()
	}

	fn upgrade_manager_program_data_account() -> SolAddress {
		SolAddress::from_str(UPGRADE_MANAGER_PROGRAM_DATA_ACCOUNT).unwrap()
	}

	fn system_program_id() -> SolAddress {
		SolAddress::from_str(SYSTEM_PROGRAM_ID).unwrap()
	}

	fn compute_price() -> SolAmount {
		COMPUTE_UNIT_PRICE
	}

	fn durable_nonce() -> SolHash {
		SolHash::from_str(TEST_DURABLE_NONCE).unwrap()
	}

	fn nonce_accounts() -> Vec<SolAddress> {
		NONCE_ACCOUNTS
			.into_iter()
			.map(|key| SolAddress::from_str(key).unwrap())
			.collect::<Vec<_>>()
	}

	#[track_caller]
	fn test_constructed_instruction_set(
		instruction_set: Vec<SolInstruction>,
		expected_serialized_tx: Vec<u8>,
	) {
		// Obtain required info from Chain Environment
		let durable_nonce = durable_nonce();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();

		// Construct the Transaction and sign it
		let message = SolMessage::new(&instruction_set, Some(&agg_key_pubkey));
		let mut tx = SolTransaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce.into());

		// println!("{:?}", tx);
		let serialized_tx =
			tx.finalize_and_serialize().expect("Transaction serialization must succeed");

		//println!("tx:{:?}", hex::encode(serialized_tx.clone()));
		assert_eq!(serialized_tx, expected_serialized_tx);
		println!("Serialized tx length: {:?}", serialized_tx.len());
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn can_create_fetch_instruction_set() {
		// Construct the batch fetch instruction set
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			vec![get_deposit_channel()],
			vault_program_data_account(),
			system_program_id(),
			agg_key(),
			next_nonce(),
			compute_price(),
		);

		// Serialized tx built in `create_fetch_native` test
		let expected_serialized_tx = hex_literal::hex!("011691ba07e3fc47bd0d4172288ed4ff8df2a7b6b66ce4237ff8330bab7692ded233fbe3efbe9c17e8a7592968c02136bc45cfc93015003d06fbe3fbd69d7cad0501000508f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb07c0202da00e4ac49553356529d5d45fc631c1d5eaee3d483667cad61d63692a17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19200000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000004a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030302050004040000000400090340420f000000000004000502e0930400070406000103118e24658f6c59298c040000000b0c0d37ff").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_batch_fetch_instruction_set() {
		// Use valid Deposit channel derived from `channel_id`
		let deposit_channel_0 = generate_deposit_channel(0u64);
		let deposit_channel_1 = generate_deposit_channel(1u64);

		// Construct the batch fetch instruction set
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			vec![deposit_channel_0, deposit_channel_1],
			vault_program_data_account(),
			system_program_id(),
			agg_key(),
			next_nonce(),
			compute_price(),
		);

		// Serialized tx built in `create_fetch_native_in_batch` test
		let expected_serialized_tx = hex_literal::hex!("010824d160477d5184765ad3ad95be7a17f20684fed88857acfde4c7f71e751177b741f6d25465e5530db686b2138e14fe9a6afca798c8349080f71f6621fb730701000509f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921e2fb5dc3bc76acc1a86ef6457885c32189c53b1db8a695267fed8f8d6921ec4ffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588be00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000004a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301060004040000000500090340420f000000000005000502e0930400080407000304158e24658f6c59298c080000000000000000000000fe080407000204158e24658f6c59298c080000000100000000000000ff").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_transfer_instruction_set() {
		let transfer_param = TransferAssetParams::<Solana> {
			asset: SOL,
			amount: 1_000_000_000u64,
			to: SolPubkey::from_str(TRANSFER_TO_ACCOUNT).unwrap().into(),
		};

		let instruction_set = SolanaInstructionBuilder::transfer(
			transfer_param,
			agg_key(),
			next_nonce(),
			compute_price(),
		);

		// Serialized tx built in `create_transfer_native` test
		let expected_serialized_tx = hex_literal::hex!("01345c86d1be2bcdf2c93c75b6054b6232e5b1e7f2fe7b3ca241d48c8a5f993af3e474bf581b2e9a1543af13104b3f3a53530d849731cc403418da313743a57e0401000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090340420f000000000004000502e0930400030200020c0200000000ca9a3b00000000").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_rotate_agg_key() {
		let new_agg_key = SolAddress::from_str(NEW_AGG_KEY).unwrap();

		let instruction_set = SolanaInstructionBuilder::rotate_agg_key(
			new_agg_key,
			nonce_accounts(),
			vault_program_data_account(),
			system_program_id(),
			upgrade_manager_program_data_account(),
			agg_key(),
			next_nonce(),
			compute_price(),
		);

		// Serialized tx built in `create_full_rotation` test
		let expected_serialized_tx = hex_literal::hex!("01bc10cb686da3b32ce8c910bfafeca7fccf81d729bcd5bcb06e01ea72ee9db7f16c1c0893f86bb04f931da2ac1f80cc9be4d5d6a64167126b676be1808de3cb0f01000513f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1924a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b6744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900448541f57201f277c5f3ffb631d0212e26e7f47749c26c4808718174a0ab2a09a18cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adba5cfec75730f8780ded36a7c8ae1dcc60d84e1a830765fc6108e7b40402e4951cd644e45426a41a7cb8369b8a0c1c89bb3f86cf278fdd9cc38b0f69784ad5667e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864e5e1869817a4fd88ddf7ab7a5f7252d7c345b39721769888608592912e8ca9acf0f13460b3fd04b7d53d7421fc874ec00eec769cf36480895e1a407bf1249475f2b2e24122be016983be9369965246cc45e1f621d40fba300c56c7ac50c3874df4f83bd213a59c9785110cf83c718f9486c3484f918593bce20c61dc6a96036afecc89e3b031824af6363174d19bbec12d3a13c4a173e5aeb349b63042bc138f000000000000000000000000000000000000000000000000000000000000000002a8f6914e88a1b0e210153ef763ae2b00c2b93d16c124d2c0537a10048000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000f0e0301110004040000001000090340420f000000000010000502e093040012040200030e094e518fabdda5d68b000f0306000304040000000e02010024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e020c0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e020a0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e020b0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e02080024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e02070024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e02040024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e020d0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e02090024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e02050024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}
}
