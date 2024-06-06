//! This file contains a Instruction builder for the Solana chain.
//!
//! The builder provides a interface for the API to create raw Solana
//! Instructions and Instruction sets with some level of abstraction.
//! This avoids the need to deal with low level Solana core types.

use crate::sol::{
	sol_tx_core::address_derivation::{derive_associated_token_account, derive_fetch_account},
	Solana,
};
use codec::Encode;
use core::str::FromStr;
use sol_prim::AccountBump;
use sp_std::{vec, vec::Vec};

use crate::{
	sol::{
		api::SolanaTransactionBuildingError,
		consts::{
			SOL_USDC_DECIMAL, SYSTEM_PROGRAM_ID,
			SYS_VAR_INSTRUCTIONS, TOKEN_PROGRAM_ID,
		},
		sol_tx_core::{
			compute_budget::ComputeBudgetInstruction,
			program_instructions::{SystemProgramInstruction, VaultProgram},
			token_instructions::AssociatedTokenAccountInstruction,
		},
		SolAddress, SolAmount, SolAsset, SolCcmAccounts, SolComputeLimit, SolInstruction,
		SolPubkey, SolanaDepositFetchId,
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
		vault_program: SolAddress,
	) -> Result<
		(SolanaDepositFetchId, AssetWithDerivedAddress, SolAddress),
		SolanaTransactionBuildingError,
	> {
		match fetch_params.asset {
			SolAsset::Sol => {
				let historical_fetch_account =
					derive_fetch_account(fetch_params.deposit_fetch_id.address, vault_program)
						.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;
				Ok((
					fetch_params.deposit_fetch_id,
					AssetWithDerivedAddress::Sol,
					historical_fetch_account.0,
				))
			},
			SolAsset::SolUsdc => {
				let ata = derive_associated_token_account(
					fetch_params.deposit_fetch_id.address,
					token_mint_pubkey,
				)
				.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;
				let historical_fetch_account = derive_fetch_account(ata.0, vault_program)
					.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;
				Ok((
					fetch_params.deposit_fetch_id,
					AssetWithDerivedAddress::Usdc(ata),
					historical_fetch_account.0,
				))
			},
		}
	}
}

fn system_program_id() -> SolAddress {
	SolAddress::from_str(SYSTEM_PROGRAM_ID).unwrap()
}

fn sys_var_instructions() -> SolAddress {
	SolAddress::from_str(SYS_VAR_INSTRUCTIONS).unwrap()
}

fn token_program_id() -> SolAddress {
	SolAddress::from_str(TOKEN_PROGRAM_ID).unwrap()
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
		decomposed_fetch_params: Vec<(SolanaDepositFetchId, AssetWithDerivedAddress, SolAddress)>,
		token_mint_pubkey: SolAddress,
		token_vault_ata: SolAddress,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let instructions = decomposed_fetch_params
			.into_iter()
			.map(|(fetch_id, asset, deposit_historical_fetch_account)| match asset {
				AssetWithDerivedAddress::Sol => VaultProgram::with_id(vault_program).fetch_native(
					fetch_id.channel_id.to_le_bytes().to_vec(),
					fetch_id.bump,
					vault_program_data_account,
					agg_key,
					fetch_id.address,
					deposit_historical_fetch_account,
					system_program_id(),
				),
				AssetWithDerivedAddress::Usdc((ata, _bump)) => VaultProgram::with_id(vault_program)
					.fetch_tokens(
						fetch_id.channel_id.to_le_bytes().to_vec(),
						fetch_id.bump,
						SOL_USDC_DECIMAL,
						vault_program_data_account,
						agg_key,
						fetch_id.address,
						ata,
						token_vault_ata,
						token_mint_pubkey,
						token_program_id(),
						deposit_historical_fetch_account,
						system_program_id(),
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
			VaultProgram::with_id(vault_program).transfer_tokens(
				amount,
				SOL_USDC_DECIMAL,
				vault_program_data_account,
				agg_key,
				token_vault_pda_account,
				token_vault_ata,
				ata,
				token_mint_pubkey,
				token_program_id(),
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
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let mut instructions = vec![VaultProgram::with_id(vault_program).rotate_agg_key(
			false,
			vault_program_data_account,
			agg_key,
			new_agg_key,
			system_program_id(),
		)];
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
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Vec<SolInstruction> {
		let instructions = vec![
			SystemProgramInstruction::transfer(&agg_key.into(), &to.into(), amount),
			VaultProgram::with_id(vault_program).execute_ccm_native_call(
				source_chain as u32,
				source_address.encode(), // TODO: check if this is correct (scale encoding?)
				message,
				amount,
				vault_program_data_account,
				agg_key,
				to,
				ccm_accounts.cf_receiver,
				system_program_id(),
				sys_var_instructions(),
				// TODO: We should be passing this!
				// ccm_accounts.remaining_account_metas(),
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
		token_vault_pda_account: SolAddress,
		token_vault_ata: SolAddress,
		token_mint_pubkey: SolAddress,
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
		VaultProgram::with_id(vault_program).transfer_tokens(
			amount,
			SOL_USDC_DECIMAL,
			vault_program_data_account,
			agg_key,
			token_vault_pda_account,
			token_vault_ata,
			ata,
			token_mint_pubkey,
			token_program_id(),
		),
		VaultProgram::with_id(vault_program).execute_ccm_token_call(
			source_chain as u32,
			source_address.encode(), // TODO: check if this is correct (scale encoding?)
			message,
			amount,
			vault_program_data_account,
			agg_key,
			ata,
			ccm_accounts.cf_receiver,
			token_program_id(),
			token_mint_pubkey,
			sys_var_instructions(),
			// TODO: We should be passing this!
			// ccm_accounts.remaining_account_metas(),
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
			consts::MAX_TRANSACTION_LENGTH,
			sol_tx_core::{
				address_derivation::derive_deposit_address,
				extra_types_for_testing::{Keypair, Signer},
				sol_test_values::*,
			},
			SolHash, SolMessage, SolTransaction, SolanaDepositFetchId,
		},
		TransferAssetParams,
	};

	fn get_decomposed_fetch_params(
		channel_id: Option<ChannelId>,
		asset: SolAsset,
	) -> (SolanaDepositFetchId, AssetWithDerivedAddress, SolAddress) {
		let channel_id = channel_id.unwrap_or(923_601_931u64);
		let (address, bump) = derive_deposit_address(channel_id, vault_program()).unwrap();

		AssetWithDerivedAddress::decompose_fetch_params(
			crate::FetchAssetParams {
				deposit_fetch_id: SolanaDepositFetchId { channel_id, address, bump },
				asset,
			},
			token_mint_pubkey(),
			SolAddress::from_str(VAULT_PROGRAM).unwrap(),
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



	fn compute_price() -> SolAmount {
		COMPUTE_UNIT_PRICE
	}

	fn token_vault_ata() -> SolAddress {
		SolAddress::from_str(TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT).unwrap()
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
			vault_program(),
			vault_program_data_account(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_fetch_native` test
		let expected_serialized_tx = hex_literal::hex!("01bc4310ab1e81ef7f80ee1df5d2dedb76e59d0d34a356e4682e6fa86019619cbc25a752fa9260e743b7fb382fc1790e91c651b6fe0fe7bdb3f8e37477788f2c0001000509f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19233306d43f017cdb7b1a324afdc62c79317d5b93e2e63b870143344134db9c600606b9a783a1a2f182b11e9663561cde6ebc2a7d83e97922c214e25284519a68800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004040301060004040000000500090340420f000000000005000502e093040008050700020304158e24658f6c59298c080000000b0c0d3700000000ff").to_vec();

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
			vault_program,
			vault_program_data_account(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_fetch_native_in_batch` test
		let expected_serialized_tx = hex_literal::hex!("01ccc4ac6b89b9f73dc3842397bd950c9ad3236cbb053a67d88682a8477388fb1b957236441bc313b51f3470935110a47b916acf23b7018e65aabccd48b1b9640f0100050bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921e2fb5dc3bc76acc1a86ef6457885c32189c53b1db8a695267fed8f8d6921ec457965dbc726e7fe35896f2bf0b9c965ebeb488cb0534aed3a6bb35f6343f503c8c21729498a6919298e0c953bd5fc297329663d413cbaac7799a79bd75f7df47ffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588be00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005060301080004040000000700090340420f000000000007000502e09304000a050900050406158e24658f6c59298c080000000000000000000000fe0a050900020306158e24658f6c59298c080000000100000000000000ff").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_fetch_token_instruction_set() {
		// Construct the fetch instruction set
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			vec![get_decomposed_fetch_params(Some(0u64), USDC)],
			token_mint_pubkey(),
			token_vault_ata(),
			vault_program(),
			vault_program_data_account(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_fetch_tokens` test
		let expected_serialized_tx = hex_literal::hex!("01907513e65d06e24f79271d06e201ff07785c517b24ca2f90ec9405716411bbd6fa53db355d3d233b8efd438aad241380e2c27bae161b81230061486fe99abd080100080df79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925f2c4cda9625242d4cc2e114789f8a6b1fcc7b36decda03a639919cdce0be871dd6e0fc50e3b853cb77f36ec4fff9c847d1b12f83ae2535aa98f2bd1d627ad08e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8772b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588bec27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004050301070004040000000600090340420f000000000006000502e09304000b0909000c02040a08030516494710642cb0c646080000000000000000000000fe06").to_vec();

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
			vault_program(),
			vault_program_data_account(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_batch_fetch` test
		let expected_serialized_tx = hex_literal::hex!("0119dcae48dbdc663efcc8be9fe79d4207d606afd050f8fb62a82775764257124f24fc08a56351a5ae1259029a1525e0e14b6c20abf187187aadf0157af34a200401000912f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19234ba473530acb5fe214bcf1637a95dd9586131636adc3a27365264e64025a91c55268e2506656a8aafc4689443bad81d0ca129f134075303ca77eefefc1b3b395f2c4cda9625242d4cc2e114789f8a6b1fcc7b36decda03a639919cdce0be871839f5b31e9ce2282c92310f62fa5e69302a0ae2e28ba1b99b0e7d57c10ab84c6bd306154bf886039adbb6f2126a02d730889b6d320507c74f5c0240c8c406454dd6e0fc50e3b853cb77f36ec4fff9c847d1b12f83ae2535aa98f2bd1d627ad08e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee871e2fb5dc3bc76acc1a86ef6457885c32189c53b1db8a695267fed8f8d6921ec472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588bec27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900060903010b0004040000000a00090340420f00000000000a000502e093040010090d001104080e0c070916494710642cb0c646080000000000000000000000fe0610090d000f05080e0c020916494710642cb0c646080000000100000000000000ff0610050d00030609158e24658f6c59298c080000000200000000000000ff").to_vec();

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
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_transfer_native` test
		let expected_serialized_tx = hex_literal::hex!("014b3dcc9d694f8f0175546e0c8b0cedbe4c1a371cac7108d5029b625ced6dee9d38a97458a3dfa3efbc0d26545fec4f7fa199b41317b219b6ff6c93070d8dd10501000a0ef79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec4616e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859ab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301060004040000000500090340420f000000000005000502e09304000c0600020a09040701010b0708000d030209071136b4eeaf4a557ebc00ca9a3b0000000006").to_vec();

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
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_full_rotation` test
		let expected_serialized_tx = hex_literal::hex!("017663fd8be6c54a3ce492a4aac1f50ed8a1589f8aa091d04b52e6fa8a43f22d359906e21630ca3dd93179e989bc1fdccbae8f9a30f6470ef9d5c17a7625f0050a01000411f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb0e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0917eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1926744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900448541f57201f277c5f3ffb631d0212e26e7f47749c26c4808718174a0ab2a09a18cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adbcd644e45426a41a7cb8369b8a0c1c89bb3f86cf278fdd9cc38b0f69784ad5667e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864e5e1869817a4fd88ddf7ab7a5f7252d7c345b39721769888608592912e8ca9acf0f13460b3fd04b7d53d7421fc874ec00eec769cf36480895e1a407bf1249475f2b2e24122be016983be9369965246cc45e1f621d40fba300c56c7ac50c3874df4f83bd213a59c9785110cf83c718f9486c3484f918593bce20c61dc6a96036afecc89e3b031824af6363174d19bbec12d3a13c4a173e5aeb349b63042bc138f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000e0d03020f0004040000000e00090340420f00000000000e000502e093040010040100030d094e518fabdda5d68b000d02020024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020b0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02090024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020a0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02070024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02060024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02040024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020c0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02080024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02050024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

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
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_ccm_native_transfer` test
		let expected_serialized_tx = hex_literal::hex!("01ad6676e85ac9bbd102f00368c9f8c09bf343fb82b7954167fd11e979e997aac7fef42f22a763dafce1ae1d6900817d5b1e5c913b2edcd9387d9133c02af00d0a0100060af79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd47417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301070004040000000500090340420f000000000005000502e0930400040200020c0200000000ca9a3b000000000906080002030406367d050be38042e0b201000000160000000100ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

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
			token_vault_pda_account(),
			token_vault_ata(),
			token_mint_pubkey(),
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_ccm_token_transfer` test
		let expected_serialized_tx = hex_literal::hex!("01769e0efc2bf125c0db3fd8b5e5b24f144d917153b447793483f86387615edadb27b38fc7d46f705ae98128f2f030bb210644d4d47eed43436fe51909ce49e10d01000b10f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec46167417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed48e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859ab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890006050301080004040000000600090340420f000000000006000502e09304000e0600020c0b050901010d070a000f04020b091136b4eeaf4a557ebc00ca9a3b00000000060d070a000203090b07366cb8a27b9fdeaa2301000000160000000100ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}
}
