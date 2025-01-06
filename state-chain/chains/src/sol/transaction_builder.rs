//! This file contains a Transaction Builder for the Solana chain.
//!
//! The builder provides a interface for the API to create raw Solana
//! Transactions with some level of abstraction.
//! This avoids the need to deal with low level Solana core types.

use sol_prim::consts::{
	MAX_TRANSACTION_LENGTH, SOL_USDC_DECIMAL, SYSTEM_PROGRAM_ID, SYS_VAR_INSTRUCTIONS,
	TOKEN_PROGRAM_ID,
};

use crate::{
	sol::{
		api::{DurableNonceAndAccount, SolanaTransactionBuildingError, VaultSwapAccountAndSender},
		compute_units_costs::{
			compute_limit_with_buffer, BASE_COMPUTE_UNITS_PER_TX,
			COMPUTE_UNITS_PER_BUMP_DERIVATION, COMPUTE_UNITS_PER_CLOSE_ACCOUNT,
			COMPUTE_UNITS_PER_CLOSE_VAULT_SWAP_ACCOUNTS, COMPUTE_UNITS_PER_ENABLE_TOKEN_SUPPORT,
			COMPUTE_UNITS_PER_FETCH_NATIVE, COMPUTE_UNITS_PER_FETCH_TOKEN,
			COMPUTE_UNITS_PER_ROTATION, COMPUTE_UNITS_PER_SET_GOV_KEY,
			COMPUTE_UNITS_PER_SET_PROGRAM_SWAPS_PARAMS, COMPUTE_UNITS_PER_TRANSFER_NATIVE,
			COMPUTE_UNITS_PER_TRANSFER_TOKEN,
		},
		sol_tx_core::{
			address_derivation::{
				derive_associated_token_account, derive_fetch_account,
				derive_token_supported_account,
			},
			compute_budget::ComputeBudgetInstruction,
			program_instructions::{
				swap_endpoints::SwapEndpointProgram, InstructionExt, SystemProgramInstruction,
				VaultProgram,
			},
			token_instructions::AssociatedTokenAccountInstruction,
			AccountMeta,
		},
		AccountBump, SolAddress, SolAmount, SolApiEnvironment, SolAsset, SolCcmAccounts,
		SolComputeLimit, SolInstruction, SolMessage, SolPubkey, SolTransaction, Solana,
	},
	FetchAssetParams, ForeignChainAddress,
};
use sp_std::{vec, vec::Vec};

fn system_program_id() -> SolAddress {
	SYSTEM_PROGRAM_ID
}

fn sys_var_instructions() -> SolAddress {
	SYS_VAR_INSTRUCTIONS
}

fn token_program_id() -> SolAddress {
	TOKEN_PROGRAM_ID
}

pub struct SolanaTransactionBuilder;

impl SolanaTransactionBuilder {
	// Compute extra compute units for each bump derivation required on-chain. Bumps
	// start in reverse from `AccountBump::MAX` and decrease by 1 for each derivation.
	fn derivation_compute_units(bump: AccountBump) -> SolComputeLimit {
		(AccountBump::MAX - bump) as u32 * COMPUTE_UNITS_PER_BUMP_DERIVATION
	}

	/// Finalize a Instruction Set. This should be internally called after a instruction set is
	/// complete. This will add some extra instruction required for the integrity of the Solana
	/// Transaction.
	///
	/// Returns the finished Instruction Set to construct the SolTransaction.
	fn build(
		mut instructions: Vec<SolInstruction>,
		durable_nonce: DurableNonceAndAccount,
		agg_key: SolPubkey,
		compute_price: SolAmount,
		compute_limit: SolComputeLimit,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let mut final_instructions = vec![SystemProgramInstruction::advance_nonce_account(
			&durable_nonce.0.into(),
			&agg_key,
		)];

		// Set a minimum priority fee to maximize chances of inclusion
		final_instructions.push(ComputeBudgetInstruction::set_compute_unit_price(
			sp_std::cmp::max(compute_price, super::compute_units_costs::MIN_COMPUTE_PRICE),
		));

		final_instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(compute_limit));

		final_instructions.append(&mut instructions);

		// Test serialize the final transaction to obtain its length.
		let transaction = SolTransaction::new_unsigned(SolMessage::new_with_blockhash(
			&final_instructions,
			Some(&agg_key),
			&durable_nonce.1.into(),
		));

		let mock_serialized_tx = transaction
			.clone()
			.finalize_and_serialize()
			.map_err(|_| SolanaTransactionBuildingError::FailedToSerializeFinalTransaction)?;

		if mock_serialized_tx.len() > MAX_TRANSACTION_LENGTH {
			Err(SolanaTransactionBuildingError::FinalTransactionExceededMaxLength(
				mock_serialized_tx.len() as u32,
			))
		} else {
			Ok(transaction)
		}
	}

	/// Create an instruction set to fetch from each `deposit_channel` being passed in.
	/// Used to batch fetch from multiple deposit channels in a single transaction.
	pub fn fetch_from(
		fetch_params: Vec<FetchAssetParams<Solana>>,
		sol_api_environment: SolApiEnvironment,
		agg_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let mut compute_limit: SolComputeLimit = BASE_COMPUTE_UNITS_PER_TX;
		let instructions = fetch_params
			.into_iter()
			.map(|param| {
				match param.asset {
					SolAsset::Sol => {
						compute_limit += COMPUTE_UNITS_PER_FETCH_NATIVE;
						let fetch_pda_and_bump = derive_fetch_account(
							param.deposit_fetch_id.address,
							sol_api_environment.vault_program,
						)
						.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

						// Add extra compute units for on-chain derivation
						compute_limit += Self::derivation_compute_units(fetch_pda_and_bump.bump);

						Ok(VaultProgram::with_id(sol_api_environment.vault_program).fetch_native(
							param.deposit_fetch_id.channel_id.to_le_bytes().to_vec(),
							param.deposit_fetch_id.bump,
							sol_api_environment.vault_program_data_account,
							agg_key,
							param.deposit_fetch_id.address,
							fetch_pda_and_bump.address,
							system_program_id(),
						))
					},
					SolAsset::SolUsdc => {
						let ata = derive_associated_token_account(
							param.deposit_fetch_id.address,
							sol_api_environment.usdc_token_mint_pubkey,
						)
						.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

						compute_limit += COMPUTE_UNITS_PER_FETCH_TOKEN;

						let fetch_pda_and_bump =
							derive_fetch_account(ata.address, sol_api_environment.vault_program)
								.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

						// Add extra compute units for on-chain derivation
						compute_limit += Self::derivation_compute_units(fetch_pda_and_bump.bump);

						Ok(VaultProgram::with_id(sol_api_environment.vault_program).fetch_tokens(
							param.deposit_fetch_id.channel_id.to_le_bytes().to_vec(),
							param.deposit_fetch_id.bump,
							SOL_USDC_DECIMAL,
							sol_api_environment.vault_program_data_account,
							agg_key,
							param.deposit_fetch_id.address,
							// we can unwrap here since we are in token_asset match arm and every
							// token should have an ata
							ata.address,
							sol_api_environment.usdc_token_vault_ata,
							sol_api_environment.usdc_token_mint_pubkey,
							token_program_id(),
							fetch_pda_and_bump.address,
							system_program_id(),
						))
					},
				}
			})
			.collect::<Result<Vec<_>, SolanaTransactionBuildingError>>()?;

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(compute_limit),
		)
	}

	/// Create an instruction set to `transfer` native Asset::Sol from our Vault account to a target
	/// account.
	pub fn transfer_native(
		amount: SolAmount,
		to: SolAddress,
		agg_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let instructions =
			vec![SystemProgramInstruction::transfer(&agg_key.into(), &to.into(), amount)];

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(
				BASE_COMPUTE_UNITS_PER_TX + COMPUTE_UNITS_PER_TRANSFER_NATIVE,
			),
		)
	}

	/// Create an instruction to `transfer` token.
	pub fn transfer_token(
		ata: SolAddress,
		amount: SolAmount,
		address: SolAddress,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		token_vault_pda_account: SolAddress,
		token_vault_ata: SolAddress,
		token_mint_pubkey: SolAddress,
		agg_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
		token_decimals: u8,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let instructions = vec![
			AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
				&agg_key.into(),
				&address.into(),
				&token_mint_pubkey.into(),
				&ata.into(),
			),
			VaultProgram::with_id(vault_program).transfer_tokens(
				amount,
				token_decimals,
				vault_program_data_account,
				agg_key,
				token_vault_pda_account,
				token_vault_ata,
				ata,
				token_mint_pubkey,
				token_program_id(),
			),
		];

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(BASE_COMPUTE_UNITS_PER_TX + COMPUTE_UNITS_PER_TRANSFER_TOKEN),
		)
	}

	/// Create an instruction set to rotate the current Vault agg key to the next key.
	pub fn rotate_agg_key(
		new_agg_key: SolAddress,
		all_nonce_accounts: Vec<SolAddress>,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		agg_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
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

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(COMPUTE_UNITS_PER_ROTATION),
		)
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
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
		compute_limit: SolComputeLimit,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let instructions = vec![
			SystemProgramInstruction::transfer(&agg_key.into(), &to.into(), amount),
			VaultProgram::with_id(vault_program)
				.execute_ccm_native_call(
					source_chain as u32,
					source_address.map_or_else(Vec::new, |address| address.raw_bytes()),
					message,
					amount,
					vault_program_data_account,
					agg_key,
					to,
					ccm_accounts.cf_receiver,
					system_program_id(),
					sys_var_instructions(),
				)
				.with_remaining_accounts(ccm_accounts.remaining_account_metas()),
		];

		Self::build(instructions, durable_nonce, agg_key.into(), compute_price, compute_limit)
	}

	pub fn ccm_transfer_token(
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
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
		token_decimals: u8,
		compute_limit: SolComputeLimit,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let instructions = vec![
		AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
			&agg_key.into(),
			&to.into(),
			&token_mint_pubkey.into(),
			&ata.into(),
		),
		VaultProgram::with_id(vault_program).transfer_tokens(
			amount,
			token_decimals,
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
			source_address.map_or_else(Vec::new, |address| address.raw_bytes()),
			message,
			amount,
			vault_program_data_account,
			agg_key,
			ata,
			ccm_accounts.cf_receiver,
			token_program_id(),
			token_mint_pubkey,
			sys_var_instructions(),
		).with_remaining_accounts(ccm_accounts.remaining_account_metas())];

		Self::build(instructions, durable_nonce, agg_key.into(), compute_price, compute_limit)
	}

	/// Create an instruction set to set the current GovKey with the agg key.
	pub fn set_gov_key_with_agg_key(
		new_gov_key: SolAddress,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		agg_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let instructions = vec![VaultProgram::with_id(vault_program).set_gov_key_with_agg_key(
			new_gov_key.into(),
			vault_program_data_account,
			agg_key,
		)];

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(COMPUTE_UNITS_PER_SET_GOV_KEY),
		)
	}

	/// Creates an instruction to close a number of open event swap accounts created via program
	/// swap.
	pub fn close_vault_swap_accounts(
		vault_swap_accounts: Vec<VaultSwapAccountAndSender>,
		vault_program_data_account: SolAddress,
		swap_endpoint_program: SolAddress,
		swap_endpoint_data_account: SolAddress,
		agg_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let number_of_accounts = vault_swap_accounts.len();
		let swap_and_sender_vec: Vec<AccountMeta> = vault_swap_accounts
			.into_iter()
			// Both event account and payee should be writable and non-signers
			.flat_map(|VaultSwapAccountAndSender { vault_swap_account, swap_sender }| {
				vec![
					AccountMeta::new(vault_swap_account.into(), false),
					AccountMeta::new(swap_sender.into(), false),
				]
			})
			.collect();

		let instructions = vec![SwapEndpointProgram::with_id(swap_endpoint_program)
			.close_event_accounts(vault_program_data_account, agg_key, swap_endpoint_data_account)
			.with_remaining_accounts(swap_and_sender_vec)];

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(
				COMPUTE_UNITS_PER_CLOSE_VAULT_SWAP_ACCOUNTS +
					COMPUTE_UNITS_PER_CLOSE_ACCOUNT * number_of_accounts as u32,
			),
		)
	}

	/// Create an instruction to set on-chain vault swap governance values.
	pub fn set_program_swaps_parameters(
		min_native_swap_amount: u64,
		max_dst_address_len: u16,
		max_ccm_message_len: u32,
		max_cf_parameters_len: u32,
		max_event_accounts: u32,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		gov_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let instructions = vec![VaultProgram::with_id(vault_program).set_program_swaps_parameters(
			min_native_swap_amount,
			max_dst_address_len,
			max_ccm_message_len,
			max_cf_parameters_len,
			max_event_accounts,
			vault_program_data_account,
			gov_key,
		)];

		Self::build(
			instructions,
			durable_nonce,
			gov_key.into(),
			compute_price,
			compute_limit_with_buffer(COMPUTE_UNITS_PER_SET_PROGRAM_SWAPS_PARAMS),
		)
	}

	/// Enable support for a new token or update the min_swap_amount for an already supported token.
	pub fn enable_token_support(
		min_swap_amount: u64,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		token_mint_pubkey: SolAddress,
		gov_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let token_supported_account =
			derive_token_supported_account(vault_program, token_mint_pubkey)
				.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

		let instructions = vec![VaultProgram::with_id(vault_program).enable_token_support(
			min_swap_amount,
			vault_program_data_account,
			gov_key,
			token_supported_account.address,
			token_mint_pubkey,
			system_program_id(),
		)];

		Self::build(
			instructions,
			durable_nonce,
			gov_key.into(),
			compute_price,
			compute_limit_with_buffer(COMPUTE_UNITS_PER_ENABLE_TOKEN_SUPPORT),
		)
	}
}

#[cfg(test)]
pub mod test {
	use cf_primitives::ChannelId;
	use frame_support::{assert_err, assert_ok};

	use super::*;
	use crate::{
		sol::{
			sol_tx_core::{address_derivation::derive_deposit_address, sol_test_values::*},
			SolanaDepositFetchId,
		},
		TransferAssetParams,
	};

	use sol_prim::{consts::SOL_USDC_DECIMAL, PdaAndBump};

	fn get_fetch_params(
		channel_id: Option<ChannelId>,
		asset: SolAsset,
	) -> crate::FetchAssetParams<Solana> {
		let channel_id = channel_id.unwrap_or(923_601_931u64);
		let PdaAndBump { address, bump } =
			derive_deposit_address(channel_id, api_env().vault_program).unwrap();

		crate::FetchAssetParams {
			deposit_fetch_id: SolanaDepositFetchId { channel_id, address, bump },
			asset,
		}
	}

	#[test]
	fn can_create_fetch_native_transaction() {
		// Construct the batch fetch instruction set
		let transaction = SolanaTransactionBuilder::fetch_from(
			vec![get_fetch_params(None, SOL)],
			api_env(),
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_fetch_native` test
		let expected_serialized_tx = hex_literal::hex!("013002620f83b5f90be2c825c184294e43237a0eb29d86f0f342007cc65b2c9613cbe668afec893c1290c50369e8e362e7524feb41b7898089029fa75baff6930b01000509f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1923a4539fbb757256442c16343f639b15db95c39a6d35721439f7f94f5c8776b7bfd35d0bf8686de2e369c3d97a8033b31e6bc33518629f59314bc3d9050956c8d00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004040301060004040000000500090340420f0000000000050005021f95000007050800030204158e24658f6c59298c080000000b0c0d3700000000fc").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_batch_fetch_native_transaction() {
		// Use valid Deposit channel derived from `channel_id`
		let fetch_param_0 = get_fetch_params(Some(0), SOL);
		let fetch_param_1 = get_fetch_params(Some(1), SOL);

		// Construct the batch fetch instruction set
		let transaction = SolanaTransactionBuilder::fetch_from(
			vec![fetch_param_0, fetch_param_1],
			api_env(),
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_fetch_native_in_batch` test
		let expected_serialized_tx = hex_literal::hex!("01ee8b317fe54aeed44d1910614da6b826d28ba872fb3d6848aef198c7695cedb1fe0c45332021efaaa84d6e8810a6d17cc7bbb3ea0b395a8c636bdde25a4f40000100050bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19238861d2f0bf5cd80031b701a6c25d13b4c812dd92f9d6301fafd9a58fb9e438646cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a708d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7dd00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005060301080004040000000700090340420f000000000007000502c34a010009050a00030206158e24658f6c59298c080000000000000000000000ff09050a00040506158e24658f6c59298c080000000100000000000000ff").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_fetch_token_transaction() {
		// Construct the fetch instruction set
		let transaction = SolanaTransactionBuilder::fetch_from(
			vec![get_fetch_params(Some(0u64), USDC)],
			api_env(),
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_fetch_tokens` test
		let expected_serialized_tx = hex_literal::hex!("01227728cb25afb56a17ed28784b78529adadf64e30717bbd5f2c5eb5442dd1f8a0f45b6c48eac4dcbd50aba21c111812c3b9cb2234c3e7c245f832f7fede1a2040100080df79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19242ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910ae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21fe91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8746cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004050301070004040000000600090340420f0000000000060005024f0a01000b090c000a02040908030516494710642cb0c646080000000000000000000000ff06").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_fetch_mixed_asset_multiple_transaction() {
		let transaction = SolanaTransactionBuilder::fetch_from(
			vec![
				get_fetch_params(Some(0u64), USDC),
				get_fetch_params(Some(1u64), USDC),
				get_fetch_params(Some(2u64), SOL),
			],
			api_env(),
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_batch_fetch` test
		let expected_serialized_tx = hex_literal::hex!("01aca7c388689758e539a841810e4f1c5e1d4fcbf0ed35f16b1a5e4798a5512be788cf7963ef48682fc8c68080d57109c80c2e1a6c61519edad1f043d106cdc70601000912f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921ad0968d57ee79348476716f9b2cd44ec4284b8f52c36648d560949e41589a5540de1c0451cccb6edd1fda9b4a48c282b279350b55a7a9716800cc0132b6f0b042ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910a140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aaae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21fb4baefcd4965beb1c71311a2ffe76419d4b8f8d35fbc4cf514b1bd02da2df2e3e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8746cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900060903010b0004040000000a00090340420f00000000000a000502e7bb02000f0911000e04080d0c060916494710642cb0c646080000000000000000000000ff060f0911001002080d0c030916494710642cb0c646080000000100000000000000ff060f051100050709158e24658f6c59298c080000000200000000000000ff").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_transfer_native_transaction() {
		let transaction = SolanaTransactionBuilder::transfer_native(
			TRANSFER_AMOUNT,
			TRANSFER_TO_ACCOUNT,
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_transfer_native` test
		let expected_serialized_tx = hex_literal::hex!("01cd34e19b7d94e6a4c475f6d3b15a568461ddcec9144b00de60defb84bd3b8145fac2ef29643fd015219ae4caeec1664ef8877810e8d2f0cd0da81115915d190301000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090340420f00000000000400050284030000030200020c0200000000ca9a3b00000000").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_transfer_usdc_token_transaction() {
		let env = api_env();
		let to_pubkey = TRANSFER_TO_ACCOUNT;
		let to_pubkey_ata =
			crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
				to_pubkey,
				env.usdc_token_mint_pubkey,
			)
			.unwrap();

		let transaction = SolanaTransactionBuilder::transfer_token(
			to_pubkey_ata.address,
			TRANSFER_AMOUNT,
			to_pubkey,
			env.vault_program,
			env.vault_program_data_account,
			env.token_vault_pda_account,
			env.usdc_token_vault_ata,
			env.usdc_token_mint_pubkey,
			agg_key(),
			durable_nonce(),
			compute_price(),
			SOL_USDC_DECIMAL,
		)
		.unwrap();

		// Serialized tx built in `create_transfer_token` test
		let expected_serialized_tx = hex_literal::hex!("01b66c78120e767c24c587448275e38bfd5083c0e3af5b6bc068ac4551b2e90505c0e9bc90d8cd3705a5e2fdd2475f5f416c6fe80cc1640b9453534368ae5ede0d01000a0ef79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec4616e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301060004040000000500090340420f0000000000050005029b2701000b0600020908040701010a070c000d030208071136b4eeaf4a557ebc00ca9a3b0000000006").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_rotate_agg_key() {
		let new_agg_key = NEW_AGG_KEY;
		let env = api_env();
		let transaction = SolanaTransactionBuilder::rotate_agg_key(
			new_agg_key,
			nonce_accounts(),
			env.vault_program,
			env.vault_program_data_account,
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `rotate_agg_key` test
		let expected_serialized_tx = hex_literal::hex!("013d9d730f9e08f6d234a7c841cce326b8618eec786f660be417f560382546960686e7a43b47b513f6cfcbb0366d817be201af34eb99eb5eaf1ba11298d06b150401000411f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1926744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900448541f57201f277c5f3ffb631d0212e26e7f47749c26c4808718174a0ab2a09a18cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adba1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bcd644e45426a41a7cb8369b8a0c1c89bb3f86cf278fdd9cc38b0f69784ad5667e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864e5e1869817a4fd88ddf7ab7a5f7252d7c345b39721769888608592912e8ca9acf0f13460b3fd04b7d53d7421fc874ec00eec769cf36480895e1a407bf1249475f2b2e24122be016983be9369965246cc45e1f621d40fba300c56c7ac50c3874df4f83bd213a59c9785110cf83c718f9486c3484f918593bce20c61dc6a96036afecc89e3b031824af6363174d19bbec12d3a13c4a173e5aeb349b63042bc138f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000e0d03010f0004040000000e00090340420f00000000000e000502e02e000010040500020d094e518fabdda5d68b000d02010024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020b0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02090024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020a0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02070024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02060024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02030024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020c0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02080024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02040024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_close_vault_swap_accounts() {
		let env = api_env();
		let vault_swap_accounts = vec![EVENT_AND_SENDER_ACCOUNTS[0]];
		let transaction = SolanaTransactionBuilder::close_vault_swap_accounts(
			vault_swap_accounts,
			env.vault_program_data_account,
			env.swap_endpoint_program,
			env.swap_endpoint_program_data_account,
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `close_vault_swap_accounts` test
		let expected_serialized_tx = hex_literal::hex!("01ef4d3377e34a95eb29dda4ff8eba6a03a08e5283321ab0629d0fba948eb641d78956ff656a8e36628c040ce38707bb8382418dca4b8ebd43816d05c1b6c6ba0b0100050af79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17e5cc1f4d51a40626e11c783b75a45a4922615ecd7f5320b9d4d46481a196a317eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921c1f0efc91eeb48bb80c90cf97775cd5d843a96f16500266cee2c20d053152d2665730decf59d4cd6db8437dab77302287431eb7562b5997601851a0eab6946f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000001ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc1622938a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004050302070004040000000600090340420f000000000006000502307500000805090003010408a5663d01b94dbd79").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_close_max_vault_swap_accounts() {
		let env = api_env();

		// We can close 11 accounts without reaching the transaction length limit.
		let transaction = SolanaTransactionBuilder::close_vault_swap_accounts(
			EVENT_AND_SENDER_ACCOUNTS.to_vec(),
			env.vault_program_data_account,
			env.swap_endpoint_program,
			env.swap_endpoint_program_data_account,
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		let expected_serialized_tx = hex_literal::hex!("0180568a598edecde53b1de53cb14f73e6e046a2db48a5c5a687ec135b2d9aac8d58a9472f4fb6ae6fb180f2b912a5e6272e1c8a1ba2e346dda2581a8a0040f80a0100051ef79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb05a557a331e51b8bf444d3bacdf6f48d8fd583aa79f9b956dd68f13a67ad096412741ecfad8423dea0c173b354b32309c3e97bb1dc68e0d858c3caebc1a1701a178480c19a99c9f2b95d40ebcb55057a49f0df00e123da6ae5e85a77c282f7c117e5cc1f4d51a40626e11c783b75a45a4922615ecd7f5320b9d4d46481a196a317eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921c11d80c98e8c11e79fd97b6c10ad733782bdbe25a710b807bbf14dedaa314861c1f0efc91eeb48bb80c90cf97775cd5d843a96f16500266cee2c20d053152d2266d68abb283ba2f4cecb092e3cfed2cb1774468ebbc264426c268ff405aa5a837de225793278f0575804f7d969e1980caaa5c5ddb2aebfd8496b14e71c9fad657d7f5b3e6c340824caca3b6c34c03e2fe0e636430b2b729ddfe32146ba4b3795c4b1e73c84b8d3f9e006c22fe8b865b9900e296345d88cdaaa7077ef17d9a31665730decf59d4cd6db8437dab77302287431eb7562b5997601851a0eab6946f74dd7ddee33a59ae7431bb31fbeb738cbfd097a66fd6706cffe7fc7efb239ec67fab67806fbb92ffd9504f4411b7f4561a0efb16685e4a22c41373fedc50b4bf86554fe5208d48fc8198310804e59837443fdaab12ea97be0fa38049910da82987410536ffebba5f49e67bafd3aa4b6cc860a594641e801500e058f74bac504da054544b2425f722e18c810bbc6cb6b9045d0db0a62d529af30efde8c37255bda7e867ab720f01897e5ede67fc232e41729d0be2a530391619743822ff6d95bea9dff663e1d13345d96daede8066cd30a1474635f2d64052d1a50ac04aed3f99bd9ce2f9674b65bfaefb62c9b8252fd0080357b1cbff44d0dad8568535dbc230c78bf2e7aee8e16631746542ef634cee3ac9bdc044c491f06862590ff1029865ce904f76d0a0ffedad66f8e2c94bccc731cac372fef8bb12cd2c473d95acf366d33096c9d0fa193639345c07abfe81175fc4d153cf0ab7b5668006538f195382df0e412e53b45bf52f91fa8e70ea872687428e4cb372306b9e6073f8d3c270c400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000001ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc1622938a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900041903051b0004040000001a00090340420f00000000001a00050220bf02001c191d0007040c0a01141312060b17100e02090d0f08151103161808a5663d01b94dbd79").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_ccm_native_transaction() {
		let ccm_param = ccm_parameter();
		let transfer_param = TransferAssetParams::<Solana> {
			asset: SOL,
			amount: TRANSFER_AMOUNT,
			to: TRANSFER_TO_ACCOUNT,
		};
		let env = api_env();

		let transaction = SolanaTransactionBuilder::ccm_transfer_native(
			transfer_param.amount,
			transfer_param.to,
			ccm_param.source_chain,
			ccm_param.source_address,
			ccm_param.channel_metadata.message.to_vec(),
			ccm_accounts(),
			env.vault_program,
			env.vault_program_data_account,
			agg_key(),
			durable_nonce(),
			compute_price(),
			TEST_COMPUTE_LIMIT,
		)
		.unwrap();

		// Serialized tx built in `create_ccm_native_transfer` test
		let expected_serialized_tx = hex_literal::hex!("01be5b6acac88600095a934dc7ae8af889c78281664e6b561f3a18bc26887ae95f35fc76d892da32f4a7314a253c0abed2da1c89d5e6daede4d70cacd37942090a0100070bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd47417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00ba73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301070004040000000500090340420f000000000005000502e0930400040200020c0200000000ca9a3b0000000008070900020304060a347d050be38042e0b20100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_ccm_usdc_token_transaction() {
		let env = api_env();
		let ccm_param = ccm_parameter();
		let to = TRANSFER_TO_ACCOUNT;
		let to_ata = crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
			to,
			env.usdc_token_mint_pubkey,
		)
		.unwrap();

		let transaction = SolanaTransactionBuilder::ccm_transfer_token(
			to_ata.address,
			TRANSFER_AMOUNT,
			to,
			ccm_param.source_chain,
			ccm_param.source_address,
			ccm_param.channel_metadata.message.to_vec(),
			ccm_accounts(),
			env.vault_program,
			env.vault_program_data_account,
			env.token_vault_pda_account,
			env.usdc_token_vault_ata,
			env.usdc_token_mint_pubkey,
			agg_key(),
			durable_nonce(),
			compute_price(),
			SOL_USDC_DECIMAL,
			TEST_COMPUTE_LIMIT,
		)
		.unwrap();

		// Serialized tx built in `create_ccm_token_transfer` test
		let expected_serialized_tx = hex_literal::hex!("01bd5f6ad13bbce1d97011814f8b7758b42a392ecc0b993c7b0be88499cbb089b3b364eaed5f3998f1dd97670f5a4b146c3be9681cb2d71fc81066657b7423d40501000c11f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec46167417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed48e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00ba73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5ab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890006050301080004040000000600090340420f000000000006000502e09304000d0600020b0a050901010c070e001004020a091136b4eeaf4a557ebc00ca9a3b00000000060c080e000203090a070f346cb8a27b9fdeaa230100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_set_gov_key_with_agg_key() {
		let new_gov_key = NEW_AGG_KEY;
		let env = api_env();
		let transaction = SolanaTransactionBuilder::set_gov_key_with_agg_key(
			new_gov_key,
			env.vault_program,
			env.vault_program_data_account,
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `set_gov_key_with_agg_key` test
		let expected_serialized_tx = hex_literal::hex!("01bacea44c25fa4431d42e34f3f13c6758790133e850b8fbd42b2e432c205f53cf065b37978b6cb4395c56c80caf7e4daaa53fc36461af75fa7066f344d71e690701000407f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00b00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090340420f000000000004000502e4570000060202002842403a280f4bd7a26744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn transactions_above_max_lengths_will_fail() {
		// with 28 Fetches, the length is 1232 <= 1232
		assert_ok!(SolanaTransactionBuilder::fetch_from(
			[get_fetch_params(None, SOL); 28].to_vec(),
			api_env(),
			agg_key(),
			durable_nonce(),
			compute_price(),
		));

		assert_err!(
			SolanaTransactionBuilder::fetch_from(
				[get_fetch_params(None, SOL); 29].to_vec(),
				api_env(),
				agg_key(),
				durable_nonce(),
				compute_price(),
			),
			SolanaTransactionBuildingError::FinalTransactionExceededMaxLength(1261)
		);
	}
}
