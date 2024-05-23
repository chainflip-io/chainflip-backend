//! This file contains a Instruction builder for the Solana chain.
//!
//! The builder provides a interface for the API to create raw Solana
//! Instructions and Instruction sets with some level of abstraction.
//! This avoids the need to deal with low level Solana core types.

use cf_primitives::chains::Solana;
use sol_prim::AccountBump;
use sp_std::{vec, vec::Vec};

use crate::{
	sol::{
		api::SolanaTransactionBuildingError,
		consts::SOL_USDC_DECIMAL,
		sol_tx_core::{
			bpf_loader_instructions::set_upgrade_authority,
			compute_budget::ComputeBudgetInstruction,
			program_instructions::{SystemProgramInstruction, VaultProgram},
			token_instructions::AssociatedTokenAccountInstruction,
		},
		SolAccountMeta, SolAddress, SolAmount, SolAsset, SolCcmAccounts, SolComputeLimit,
		SolInstruction, SolPubkey, SolanaDepositFetchId,
	},
	FetchAssetParams, ForeignChainAddress,
};

/// Internal enum type that contains SolAsset with derived ATA
pub enum AssetWithDerivedAddress {
	Sol,
	Usdc((SolAddress, AccountBump)),
}

impl AssetWithDerivedAddress {
	pub fn decompose_fetch_params(
		fetch_params: FetchAssetParams<Solana>,
		token_mint_pubkey: SolAddress,
	) -> Result<(SolanaDepositFetchId, AssetWithDerivedAddress), SolanaTransactionBuildingError> {
		match fetch_params.asset {
			SolAsset::Sol => Ok((fetch_params.deposit_fetch_id, AssetWithDerivedAddress::Sol)),
			SolAsset::SolUsdc =>
				crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
					fetch_params.deposit_fetch_id.address,
					token_mint_pubkey,
				)
				.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)
				.map(|ata| (fetch_params.deposit_fetch_id, AssetWithDerivedAddress::Usdc(ata))),
		}
	}
}

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
		decomposed_fetch_params: Vec<(SolanaDepositFetchId, AssetWithDerivedAddress)>,
		token_mint_pubkey: SolAddress,
		token_vault_ata: SolAddress,
		token_program_id: SolAddress,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		system_program_id: SolAddress,
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let instructions = decomposed_fetch_params
			.into_iter()
			.map(|(fetch_id, asset)| match asset {
				AssetWithDerivedAddress::Sol => VaultProgram::with_id(vault_program).fetch_native(
					fetch_id.channel_id.to_le_bytes().to_vec(),
					fetch_id.bump,
					vault_program_data_account,
					agg_key,
					fetch_id.address.into(), false),
						SolAccountMeta::new_readonly(system_program_id.into(), false),
					],
				),
				AssetWithDerivedAddress::Usdc(ata) => ProgramInstruction::get_instruction(
					&VaultProgram::FetchTokens {
						seed: fetch_id.channel_id.to_le_bytes().to_vec(),
						bump: fetch_id.bump,
						decimals: SOL_USDC_DECIMAL,
					},
					vault_program.into(),
					vec![
						SolAccountMeta::new_readonly(vault_program_data_account.into(), false),
						SolAccountMeta::new_readonly(agg_key.into(), true),
						SolAccountMeta::new_readonly(fetch_id.address,
						SolAccountMeta::new(ata.0.into(), false),
						SolAccountMeta::new(token_vault_ata.into(), false),
						SolAccountMeta::new_readonly(token_mint_pubkey.into(), false),
						SolAccountMeta::new_readonly(token_program_id.into(), false),
					system_program_id,
				),
			})
			.collect::<Vec<_>>();

		Self::finalize(instructions, nonce_account.into(), agg_key.into(), compute_price)
	}

	/// Create an instruction set to `transfer` native Asset::Sol from our Vault account to a target
	/// account.
	pub fn transfer_native(
		amount: SolAmount,
		to: SolAddress,
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let instructions =
			vec![SystemProgramInstruction::transfer(&agg_key.into(), &to.into(), amount)];

		Self::finalize(instructions, nonce_account.into(), agg_key.into(), compute_price)
	}

	/// Create an instruction to `transfer` USDC token.
	pub fn transfer_usdc_token(
		ata: SolAddress,
		amount: SolAmount,
		address: SolAddress,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		token_vault_pda_account: SolAddress,
		token_vault_ata: SolAddress,
		token_mint_pubkey: SolAddress,
		token_program_id: SolAddress,
		system_program_id: SolAddress,
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let instructions = vec![
			AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
				&agg_key.into(),
				&address.into(),
				&token_mint_pubkey.into(),
				&ata.into(),
			),
			ProgramInstruction::get_instruction(
				&VaultProgram::TransferTokens { amount, decimals: SOL_USDC_DECIMAL },
				vault_program.into(),
				vec![
					SolAccountMeta::new_readonly(
						vault_program_data_account.into(),
						false,
					),
					SolAccountMeta::new_readonly(agg_key.into(), true),
					SolAccountMeta::new_readonly(token_vault_pda_account.into(), false),
					SolAccountMeta::new(token_vault_ata.into(), false),
					SolAccountMeta::new(ata.into(), false),
					SolAccountMeta::new_readonly(token_mint_pubkey.into(), false),
					SolAccountMeta::new_readonly(token_program_id.into(), false),
					SolAccountMeta::new_readonly(system_program_id.into(), false),
				],
			),
		];

		Self::finalize(instructions, nonce_account.into(), agg_key.into(), compute_price)
	}

	/// Create an instruction set to rotate the current Vault agg key to the next key.
	pub fn rotate_agg_key(
		new_agg_key: SolAddress,
		all_nonce_accounts: Vec<SolAddress>,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		system_program_id: SolAddress,
		upgrade_manager_program_data_account: SolAddress,
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let mut instructions = vec![
			VaultProgram::with_id(vault_program).rotate_agg_key(
				false,
				vault_program_data_account,
				agg_key,
				new_agg_key,
				system_program_id,
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

	/// Creates an instruction set for CCM messages that transfer native Sol token
	pub fn ccm_transfer_native(
		amount: SolAmount,
		to: SolAddress,
		source_chain: cf_primitives::ForeignChain,
		source_address: Option<ForeignChainAddress>,
		message: Vec<u8>,
		ccm_accounts: SolCcmAccounts,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		system_program_id: SolAddress,
		sys_var_instructions: SolAddress,
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let instructions = vec![
			SystemProgramInstruction::transfer(&agg_key.into(), &to.into(), amount),
			ProgramInstruction::get_instruction(
				&VaultProgram::ExecuteCcmNativeCall {
					source_chain: source_chain as u32,
					source_address: codec::Encode::encode(&source_address),
					message,
					amount,
				},
				vault_program.into(),
				vec![
					vec![
						SolAccountMeta::new_readonly(vault_program_data_account.into(), false),
						SolAccountMeta::new_readonly(agg_key.into(), true),
						SolAccountMeta::new(to.into(), false),
						SolAccountMeta::from(ccm_accounts.cf_receiver.clone()),
						SolAccountMeta::new_readonly(system_program_id.into(), false),
						SolAccountMeta::new_readonly(sys_var_instructions.into(), false),
					],
					ccm_accounts.remaining_account_metas(),
				]
				.into_iter()
				.flatten()
				.collect::<Vec<_>>(),
			),
		];

		Self::finalize(instructions, nonce_account.into(), agg_key.into(), compute_price)
	}

	pub fn ccm_transfer_usdc_token(
		ata: SolAddress,
		amount: SolAmount,
		to: SolAddress,
		source_chain: cf_primitives::ForeignChain,
		source_address: Option<ForeignChainAddress>,
		message: Vec<u8>,
		ccm_accounts: SolCcmAccounts,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		system_program_id: SolAddress,
		sys_var_instructions: SolAddress,
		token_vault_pda_account: SolAddress,
		token_vault_ata: SolAddress,
		token_mint_pubkey: SolAddress,
		token_program_id: SolAddress,
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let instructions = vec![
			AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
				&agg_key.into(),
				&to.into(),
				&token_mint_pubkey.into(),
				&ata.into(),
			),
			ProgramInstruction::get_instruction(
				&VaultProgram::TransferTokens { amount, decimals: SOL_USDC_DECIMAL },
				vault_program.into(),
				vec![
					SolAccountMeta::new_readonly(
						vault_program_data_account.into(),
						false,
					),
					SolAccountMeta::new_readonly(agg_key.into(), true),
					SolAccountMeta::new_readonly(token_vault_pda_account.into(), false),
					SolAccountMeta::new(token_vault_ata.into(), false),
					SolAccountMeta::new(ata.into(), false),
					SolAccountMeta::new_readonly(token_mint_pubkey.into(), false),
					SolAccountMeta::new_readonly(token_program_id.into(), false),
					SolAccountMeta::new_readonly(system_program_id.into(), false),
				],
			),
			ProgramInstruction::get_instruction(
				&VaultProgram::ExecuteCcmTokenCall {
					source_chain: source_chain as u32,
					source_address: codec::Encode::encode(&source_address),
					message,
					amount,
				},
				vault_program.into(),
				vec![
					vec![
						SolAccountMeta::new_readonly(vault_program_data_account.into(), false),
						SolAccountMeta::new_readonly(agg_key.into(), true),
						SolAccountMeta::new(ata.into(), false),
						ccm_accounts.cf_receiver.clone().into(),
						SolAccountMeta::new_readonly(token_program_id.into(), false),
						SolAccountMeta::new_readonly(token_mint_pubkey.into(), false),
						SolAccountMeta::new_readonly(sys_var_instructions.into(), false),
					],
					ccm_accounts.remaining_account_metas(),
				]
				.into_iter()
				.flatten()
				.collect::<Vec<_>>(),
			)];

		Self::finalize(instructions, nonce_account.into(), agg_key.into(), compute_price)
	}
}

#[cfg(test)]
mod test {
	use cf_primitives::ChannelId;

	use super::*;
	use crate::{
		sol::{
			consts::{MAX_TRANSACTION_LENGTH, TOKEN_PROGRAM_ID},
			sol_tx_core::{
				address_derivation::derive_deposit_address,
				extra_types_for_testing::{Keypair, Signer},
				sol_test_values::*,
			},
			SolHash, SolMessage, SolTransaction, SolanaDepositFetchId,
		},
		TransferAssetParams,
	};
	use core::str::FromStr;

	fn get_decomposed_fetch_params(
		channel_id: Option<ChannelId>,
		asset: SolAsset,
	) -> (SolanaDepositFetchId, AssetWithDerivedAddress) {
		let channel_id = channel_id.unwrap_or(923_601_931u64);
		let (address, bump) = derive_deposit_address(channel_id, vault_program()).unwrap();

		AssetWithDerivedAddress::decompose_fetch_params(
			crate::FetchAssetParams {
				deposit_fetch_id: SolanaDepositFetchId { channel_id, address, bump },
				asset,
			},
			token_mint_pubkey(),
		)
		.unwrap()
	}

	fn agg_key() -> SolAddress {
		Keypair::from_bytes(&RAW_KEYPAIR)
			.expect("Key pair generation must succeed")
			.pubkey()
			.into()
	}

	fn nonce_account() -> SolAddress {
		SolAddress::from_str(NONCE_ACCOUNTS[0]).unwrap()
	}

	fn durable_nonce() -> SolHash {
		SolHash::from_str(TEST_DURABLE_NONCE).unwrap()
	}

	fn vault_program() -> SolAddress {
		SolAddress::from_str(VAULT_PROGRAM).unwrap()
	}

	fn vault_program_data_account() -> SolAddress {
		SolAddress::from_str(VAULT_PROGRAM_DATA_ACCOUNT).unwrap()
	}

	fn token_vault_pda_account() -> SolAddress {
		SolAddress::from_str(TOKEN_VAULT_PDA_ACCOUNT).unwrap()
	}

	fn upgrade_manager_program_data_account() -> SolAddress {
		SolAddress::from_str(UPGRADE_MANAGER_PROGRAM_DATA_ACCOUNT).unwrap()
	}

	fn system_program_id() -> SolAddress {
		SolAddress::from_str(crate::sol::consts::SYSTEM_PROGRAM_ID).unwrap()
	}

	fn sys_var_instructions() -> SolAddress {
		SolAddress::from_str(crate::sol::consts::SYS_VAR_INSTRUCTIONS).unwrap()
	}

	fn compute_price() -> SolAmount {
		COMPUTE_UNIT_PRICE
	}

	fn token_vault_ata() -> SolAddress {
		SolAddress::from_str(TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT).unwrap()
	}

	fn token_program_id() -> SolAddress {
		SolAddress::from_str(TOKEN_PROGRAM_ID).unwrap()
	}

	fn token_mint_pubkey() -> SolAddress {
		SolAddress::from_str(MINT_PUB_KEY).unwrap()
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
		let durable_nonce = durable_nonce().into();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();

		// Construct the Transaction and sign it
		let message =
			SolMessage::new_with_blockhash(&instruction_set, Some(&agg_key_pubkey), &durable_nonce);
		let mut tx = SolTransaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		// println!("{:?}", tx);
		let serialized_tx = tx
			.clone()
			.finalize_and_serialize()
			.expect("Transaction serialization must succeed");

		println!("Serialized tx length: {:?}", serialized_tx.len());
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH);

		if serialized_tx != expected_serialized_tx {
			panic!(
				"Transaction mismatch. \nTx: {:?} \nSerialized: {:?}",
				tx,
				hex::encode(serialized_tx.clone())
			);
		}
	}

	#[test]
	fn can_create_fetch_native_instruction_set() {
		// Construct the batch fetch instruction set
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			vec![get_decomposed_fetch_params(None, SOL)],
			token_mint_pubkey(),
			token_vault_ata(),
			token_program_id(),
			vault_program(),
			vault_program_data_account(),
			system_program_id(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_fetch_native` test
		let expected_serialized_tx = hex_literal::hex!("0106c23d5531cfd1d8d543eb8f88dc346a540224a50930bb1c4509c0a5ad9da77a5fb097530c0d9fa9e35f65ce9445c02bdabef979967ee0d60ed0cc8cc0c7370001000508f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19233306d43f017cdb7b1a324afdc62c79317d5b93e2e63b870143344134db9c60000000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000004a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090340420f000000000004000502e0930400070406000203158e24658f6c59298c080000000b0c0d3700000000ff").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_batch_fetch_native_instruction_set() {
		let vault_program = vault_program();

		// Use valid Deposit channel derived from `channel_id`
		let fetch_param_0 = get_decomposed_fetch_params(Some(0), SOL);
		let fetch_param_1 = get_decomposed_fetch_params(Some(1), SOL);

		// Construct the batch fetch instruction set
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			vec![fetch_param_0, fetch_param_1],
			token_mint_pubkey(),
			token_vault_ata(),
			token_program_id(),
			vault_program,
			vault_program_data_account(),
			system_program_id(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_fetch_native_in_batch` test
		let expected_serialized_tx = hex_literal::hex!("010824d160477d5184765ad3ad95be7a17f20684fed88857acfde4c7f71e751177b741f6d25465e5530db686b2138e14fe9a6afca798c8349080f71f6621fb730701000509f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921e2fb5dc3bc76acc1a86ef6457885c32189c53b1db8a695267fed8f8d6921ec4ffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588be00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000004a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301060004040000000500090340420f000000000005000502e0930400080407000304158e24658f6c59298c080000000000000000000000fe080407000204158e24658f6c59298c080000000100000000000000ff").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_fetch_token_instruction_set() {
		// Construct the fetch instruction set
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			vec![get_decomposed_fetch_params(Some(0u64), USDC)],
			token_mint_pubkey(),
			token_vault_ata(),
			token_program_id(),
			vault_program(),
			vault_program_data_account(),
			system_program_id(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_fetch_tokens` test
		let expected_serialized_tx = hex_literal::hex!("01c2deaa4b670a3b7e1a661f106e3c63b0247aa3d30e44779c7024528636d643b2a2a167c2823643a38cf2bcb4ce77797cadb3bed6b1934d9380140555afa0520f0100080cf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925f2c4cda9625242d4cc2e114789f8a6b1fcc7b36decda03a639919cdce0be871b966a2b36557938f49cc5d00f8f12d86f16f48e03b63c8422967dba621ab60bf00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee874a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588bec27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004040301060004040000000500090340420f000000000005000502e09304000a0809000b020308070416494710642cb0c646080000000000000000000000fe06").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_fetch_mixed_asset_multiple_instruction_set() {
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			vec![
				get_decomposed_fetch_params(Some(0u64), USDC),
				get_decomposed_fetch_params(Some(1u64), USDC),
				get_decomposed_fetch_params(Some(2u64), SOL),
			],
			token_mint_pubkey(),
			token_vault_ata(),
			token_program_id(),
			vault_program(),
			vault_program_data_account(),
			system_program_id(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_batch_fetch` test
		let expected_serialized_tx = hex_literal::hex!("015980d922d0a6ed11c1d64c9a6ceba7a5d4e2eb1127bcdae1f4fb9343b3679b3ed09ba6cf10bb5c0cab6886afa7aee09f1b4ed3d1025ba60697428e81c246a40e0100090ff79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19255268e2506656a8aafc4689443bad81d0ca129f134075303ca77eefefc1b3b395f2c4cda9625242d4cc2e114789f8a6b1fcc7b36decda03a639919cdce0be871839f5b31e9ce2282c92310f62fa5e69302a0ae2e28ba1b99b0e7d57c10ab84c6b966a2b36557938f49cc5d00f8f12d86f16f48e03b63c8422967dba621ab60bf00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee871e2fb5dc3bc76acc1a86ef6457885c32189c53b1db8a695267fed8f8d6921ec44a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588bec27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890006060301080004040000000700090340420f000000000007000502e09304000d080c000e03050a090616494710642cb0c646080000000000000000000000fe060d080c000b04050a090616494710642cb0c646080000000100000000000000ff060d040c000206158e24658f6c59298c080000000200000000000000ff").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_transfer_native_instruction_set() {
		let instruction_set = SolanaInstructionBuilder::transfer_native(
			TRANSFER_AMOUNT,
			SolAddress::from_str(TRANSFER_TO_ACCOUNT).unwrap(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_transfer_native` test
		let expected_serialized_tx = hex_literal::hex!("01345c86d1be2bcdf2c93c75b6054b6232e5b1e7f2fe7b3ca241d48c8a5f993af3e474bf581b2e9a1543af13104b3f3a53530d849731cc403418da313743a57e0401000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090340420f000000000004000502e0930400030200020c0200000000ca9a3b00000000").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_transfer_usdc_token_instruction_set() {
		let to_pubkey = SolAddress::from_str(TRANSFER_TO_ACCOUNT).unwrap();
		let to_pubkey_ata =
			crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
				to_pubkey,
				token_mint_pubkey(),
			)
			.unwrap();

		let instruction_set = SolanaInstructionBuilder::transfer_usdc_token(
			to_pubkey_ata.0,
			TRANSFER_AMOUNT,
			to_pubkey,
			vault_program(),
			vault_program_data_account(),
			token_vault_pda_account(),
			token_vault_ata(),
			token_mint_pubkey(),
			token_program_id(),
			system_program_id(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_transfer_native` test
		let expected_serialized_tx = hex_literal::hex!("019df37a2382451b6663aebcba5cd4c8b220fa22fd10c1a32af8d26a4bca2403c06e5d449428e850aab2480a78c41393020761b558feded014ac0d158770a9c20c01000a0ef79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec4616b966a2b36557938f49cc5d00f8f12d86f16f48e03b63c8422967dba621ab60bf00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd44a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c81a0052237ad76cb6e88fe505dc3d96bba6d8889f098b1eaa342ec84458805218c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301060004040000000500090340420f000000000005000502e09304000d0600020908040701010b080a000c03020807041136b4eeaf4a557ebc00ca9a3b0000000006").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_rotate_agg_key() {
		let new_agg_key = SolAddress::from_str(NEW_AGG_KEY).unwrap();

		let instruction_set = SolanaInstructionBuilder::rotate_agg_key(
			new_agg_key,
			nonce_accounts(),
			vault_program(),
			vault_program_data_account(),
			system_program_id(),
			upgrade_manager_program_data_account(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_full_rotation` test
		let expected_serialized_tx = hex_literal::hex!("01bc10cb686da3b32ce8c910bfafeca7fccf81d729bcd5bcb06e01ea72ee9db7f16c1c0893f86bb04f931da2ac1f80cc9be4d5d6a64167126b676be1808de3cb0f01000513f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1924a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b6744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900448541f57201f277c5f3ffb631d0212e26e7f47749c26c4808718174a0ab2a09a18cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adba5cfec75730f8780ded36a7c8ae1dcc60d84e1a830765fc6108e7b40402e4951cd644e45426a41a7cb8369b8a0c1c89bb3f86cf278fdd9cc38b0f69784ad5667e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864e5e1869817a4fd88ddf7ab7a5f7252d7c345b39721769888608592912e8ca9acf0f13460b3fd04b7d53d7421fc874ec00eec769cf36480895e1a407bf1249475f2b2e24122be016983be9369965246cc45e1f621d40fba300c56c7ac50c3874df4f83bd213a59c9785110cf83c718f9486c3484f918593bce20c61dc6a96036afecc89e3b031824af6363174d19bbec12d3a13c4a173e5aeb349b63042bc138f000000000000000000000000000000000000000000000000000000000000000002a8f6914e88a1b0e210153ef763ae2b00c2b93d16c124d2c0537a10048000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000f0e0301110004040000001000090340420f000000000010000502e093040012040200030e094e518fabdda5d68b000f0306000304040000000e02010024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e020c0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e020a0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e020b0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e02080024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e02070024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e02040024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e020d0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e02090024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440e02050024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_ccm_native_instruction_set() {
		let ccm_param = ccm_parameter();
		let transfer_param = TransferAssetParams::<Solana> {
			asset: SOL,
			amount: TRANSFER_AMOUNT,
			to: SolPubkey::from_str(TRANSFER_TO_ACCOUNT).unwrap().into(),
		};

		let instruction_set = SolanaInstructionBuilder::ccm_transfer_native(
			transfer_param.amount,
			transfer_param.to,
			ccm_param.source_chain,
			ccm_param.source_address,
			ccm_param.channel_metadata.message.to_vec(),
			ccm_accounts(),
			vault_program(),
			vault_program_data_account(),
			system_program_id(),
			sys_var_instructions(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_ccm_native_transfer` test
		let expected_serialized_tx = hex_literal::hex!("019e8ac555f753d59579063aa9339e3c434b31aa4d26f4999e2bcad27812a70812a5c0aac063d036359f91c81d9fd67a0d309b471e9f1ff40de1fc9a7a39bbc2090100070bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb0575731869899efe0bd5d9161ad9f1db7c582c48c0b4ea7cff6a637c55c7310717eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000004a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040302070004040000000500090340420f000000000005000502e0930400040200030c0200000000ca9a3b0000000009070800030104060a367d050be38042e0b201000000160000000100ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_ccm_usdc_token_instruction_set() {
		let ccm_param = ccm_parameter();
		let to = SolAddress::from_str(TRANSFER_TO_ACCOUNT).unwrap();
		let to_ata = crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
			to,
			token_mint_pubkey(),
		)
		.unwrap();

		let instruction_set = SolanaInstructionBuilder::ccm_transfer_usdc_token(
			to_ata.0,
			TRANSFER_AMOUNT,
			to,
			ccm_param.source_chain,
			ccm_param.source_address,
			ccm_param.channel_metadata.message.to_vec(),
			ccm_accounts(),
			vault_program(),
			vault_program_data_account(),
			system_program_id(),
			sys_var_instructions(),
			token_vault_pda_account(),
			token_vault_ata(),
			token_mint_pubkey(),
			token_program_id(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_ccm_native_transfer` test
		let expected_serialized_tx = hex_literal::hex!("01105b6646cf4b5b42cd489b2123d18d253e8cb488f889078ada016a2daae5a7bcbef8f4cd5f603142f62fbb42965a49306535239617c13ba1fbca72cc571d7c0f01000c11f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb0575731869899efe0bd5d9161ad9f1db7c582c48c0b4ea7cff6a637c55c7310717eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec4616b966a2b36557938f49cc5d00f8f12d86f16f48e03b63c8422967dba621ab60bf00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd44a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c81a0052237ad76cb6e88fe505dc3d96bba6d8889f098b1eaa342ec84458805218c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890006050302080004040000000600090340420f000000000006000502e09304000f0600030b0a050901010d080c000e04030a09051136b4eeaf4a557ebc00ca9a3b00000000060d080c000301090a0710366cb8a27b9fdeaa2301000000160000000100ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}
}
