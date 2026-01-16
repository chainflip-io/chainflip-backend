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

//! This file contains a Transaction Builder for the Solana chain.
//!
//! The builder provides a interface for the API to create raw Solana
//! Transactions with some level of abstraction.
//! This avoids the need to deal with low level Solana core types.

use crate::{
	sol::{
		api::{DurableNonceAndAccount, SolanaTransactionBuildingError, VaultSwapAccountAndSender},
		compute_units_costs::{
			compute_limit_with_buffer, BASE_COMPUTE_UNITS_PER_TX,
			COMPUTE_UNITS_PER_BUMP_DERIVATION, COMPUTE_UNITS_PER_CLOSE_ACCOUNT,
			COMPUTE_UNITS_PER_ENABLE_TOKEN_SUPPORT,
			COMPUTE_UNITS_PER_FETCH_AND_CLOSE_VAULT_SWAP_ACCOUNTS, COMPUTE_UNITS_PER_FETCH_NATIVE,
			COMPUTE_UNITS_PER_FETCH_TOKEN, COMPUTE_UNITS_PER_NONCE_ROTATION,
			COMPUTE_UNITS_PER_ROTATION, COMPUTE_UNITS_PER_SET_GOV_KEY,
			COMPUTE_UNITS_PER_SET_PROGRAM_SWAPS_PARAMS, COMPUTE_UNITS_PER_TRANSFER_NATIVE,
			COMPUTE_UNITS_PER_TRANSFER_TOKEN, COMPUTE_UNITS_PER_UPGRADE_PROGRAM,
		},
		sol_tx_core::{
			address_derivation::{
				derive_associated_token_account, derive_fetch_account, derive_pda_signer,
				derive_program_data_address, derive_swap_endpoint_native_vault_account,
				derive_token_supported_account,
			},
			compute_budget::ComputeBudgetInstruction,
			consts::{
				BPF_LOADER_UPGRADEABLE_ID, MAX_TRANSACTION_LENGTH, SOL_USD_DECIMAL,
				SYSTEM_PROGRAM_ID, SYS_VAR_CLOCK, SYS_VAR_INSTRUCTIONS, SYS_VAR_RENT,
				TOKEN_PROGRAM_ID,
			},
			program_instructions::{
				alt_managers::AltManagerProgram, swap_endpoints::SwapEndpointProgram,
				InstructionExt, SystemProgramInstruction, VaultProgram,
			},
			token_instructions::AssociatedTokenAccountInstruction,
			AccountMeta,
		},
		AccountBump, SolAddress, SolAddressLookupTableAccount, SolAmount, SolApiEnvironment,
		SolAsset, SolCcmAccounts, SolComputeLimit, SolInstruction, SolPubkey, SolVersionedMessage,
		SolVersionedTransaction, Solana,
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

	/// Create an instruction for transferring native SOL.
	/// Returns the instruction and the compute units required for this instruction.
	fn create_transfer_native_instruction(
		amount: SolAmount,
		to: SolAddress,
		agg_key: SolAddress,
	) -> (SolInstruction, SolComputeLimit) {
		let instruction = SystemProgramInstruction::transfer(&agg_key.into(), &to.into(), amount);
		(instruction, COMPUTE_UNITS_PER_TRANSFER_NATIVE)
	}

	/// Create instructions for transferring tokens.
	/// Returns a vector of instructions needed for token transfer (ATA creation + transfer).
	fn create_transfer_token_instructions(
		ata: SolAddress,
		amount: SolAmount,
		address: SolAddress,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		token_vault_pda_account: SolAddress,
		token_vault_ata: SolAddress,
		token_mint_pubkey: SolAddress,
		agg_key: SolAddress,
		token_decimals: u8,
	) -> Vec<SolInstruction> {
		vec![
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
		]
	}

	/// Create an instruction for fetching assets from a single deposit channel.
	/// Returns the instruction and the compute units required for this instruction.
	fn create_fetch_instruction(
		param: FetchAssetParams<Solana>,
		sol_api_environment: &SolApiEnvironment,
		agg_key: SolAddress,
	) -> Result<(SolInstruction, SolComputeLimit), SolanaTransactionBuildingError> {
		match param.asset {
			SolAsset::Sol => {
				let fetch_pda_and_bump = derive_fetch_account(
					param.deposit_fetch_id.address,
					sol_api_environment.vault_program,
				)
				.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

				let compute_units = COMPUTE_UNITS_PER_FETCH_NATIVE +
					Self::derivation_compute_units(fetch_pda_and_bump.bump);

				let instruction = VaultProgram::with_id(sol_api_environment.vault_program)
					.fetch_native(
						param.deposit_fetch_id.channel_id.to_le_bytes().to_vec(),
						param.deposit_fetch_id.bump,
						sol_api_environment.vault_program_data_account,
						agg_key,
						param.deposit_fetch_id.address,
						fetch_pda_and_bump.address,
						system_program_id(),
					);

				Ok((instruction, compute_units))
			},
			SolAsset::SolUsdc => {
				let ata = derive_associated_token_account(
					param.deposit_fetch_id.address,
					sol_api_environment.usdc_token_mint_pubkey,
				)
				.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

				let fetch_pda_and_bump =
					derive_fetch_account(ata.address, sol_api_environment.vault_program)
						.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

				let compute_units = COMPUTE_UNITS_PER_FETCH_TOKEN +
					Self::derivation_compute_units(fetch_pda_and_bump.bump);

				let instruction = VaultProgram::with_id(sol_api_environment.vault_program)
					.fetch_tokens(
						param.deposit_fetch_id.channel_id.to_le_bytes().to_vec(),
						param.deposit_fetch_id.bump,
						SOL_USD_DECIMAL,
						sol_api_environment.vault_program_data_account,
						agg_key,
						param.deposit_fetch_id.address,
						ata.address,
						sol_api_environment.usdc_token_vault_ata,
						sol_api_environment.usdc_token_mint_pubkey,
						token_program_id(),
						fetch_pda_and_bump.address,
						system_program_id(),
					);

				Ok((instruction, compute_units))
			},
			SolAsset::SolUsdt => {
				let ata = derive_associated_token_account(
					param.deposit_fetch_id.address,
					sol_api_environment.usdt_token_mint_pubkey,
				)
				.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

				let fetch_pda_and_bump =
					derive_fetch_account(ata.address, sol_api_environment.vault_program)
						.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

				let compute_units = COMPUTE_UNITS_PER_FETCH_TOKEN +
					Self::derivation_compute_units(fetch_pda_and_bump.bump);

				let instruction = VaultProgram::with_id(sol_api_environment.vault_program)
					.fetch_tokens(
						param.deposit_fetch_id.channel_id.to_le_bytes().to_vec(),
						param.deposit_fetch_id.bump,
						SOL_USD_DECIMAL,
						sol_api_environment.vault_program_data_account,
						agg_key,
						param.deposit_fetch_id.address,
						ata.address,
						sol_api_environment.usdt_token_vault_ata,
						sol_api_environment.usdt_token_mint_pubkey,
						token_program_id(),
						fetch_pda_and_bump.address,
						system_program_id(),
					);

				Ok((instruction, compute_units))
			},
		}
	}

	/// Finalize a Instruction Set. This should be internally called after a instruction set is
	/// complete. This will add some extra instruction required for the integrity of the Solana
	/// Transaction.
	///
	/// Returns the finished Instruction Set to construct the SolVersionedTransaction.
	fn build(
		mut instructions: Vec<SolInstruction>,
		durable_nonce: DurableNonceAndAccount,
		agg_key: SolPubkey,
		compute_price: SolAmount,
		compute_limit: SolComputeLimit,
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
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
		let transaction = SolVersionedTransaction::new_unsigned(SolVersionedMessage::new(
			&final_instructions,
			Some(agg_key),
			Some(durable_nonce.1.into()),
			&address_lookup_tables[..],
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
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
		let mut compute_limit: SolComputeLimit = BASE_COMPUTE_UNITS_PER_TX;
		let instructions = fetch_params
			.into_iter()
			.map(|param| {
				let (instruction, instruction_compute_units) =
					Self::create_fetch_instruction(param, &sol_api_environment, agg_key)?;
				compute_limit += instruction_compute_units;
				Ok(instruction)
			})
			.collect::<Result<Vec<_>, SolanaTransactionBuildingError>>()?;

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(compute_limit),
			vec![sol_api_environment.address_lookup_table_account],
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
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
		let (instruction, compute_units) =
			Self::create_transfer_native_instruction(amount, to, agg_key);
		Self::build(
			vec![instruction],
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(BASE_COMPUTE_UNITS_PER_TX + compute_units),
			vec![],
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
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
		let instructions = Self::create_transfer_token_instructions(
			ata,
			amount,
			address,
			vault_program,
			vault_program_data_account,
			token_vault_pda_account,
			token_vault_ata,
			token_mint_pubkey,
			agg_key,
			token_decimals,
		);

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(BASE_COMPUTE_UNITS_PER_TX + COMPUTE_UNITS_PER_TRANSFER_TOKEN),
			address_lookup_tables,
		)
	}

	/// Create a refund transaction that fetches native SOL from a deposit channel and transfers it.
	/// This combines fetch and transfer operations in a single transaction for native SOL refunds.
	pub fn refund_native(
		fetch_param: FetchAssetParams<Solana>,
		transfer_amount: SolAmount,
		transfer_to: SolAddress,
		sol_api_environment: SolApiEnvironment,
		agg_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
		let (fetch_instruction, fetch_compute_units) =
			Self::create_fetch_instruction(fetch_param, &sol_api_environment, agg_key)?;

		let (transfer_instruction, transfer_compute_units) =
			Self::create_transfer_native_instruction(transfer_amount, transfer_to, agg_key);

		let instructions = vec![fetch_instruction, transfer_instruction];

		let total_compute_units =
			BASE_COMPUTE_UNITS_PER_TX + fetch_compute_units + transfer_compute_units;

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(total_compute_units),
			vec![sol_api_environment.address_lookup_table_account],
		)
	}

	/// Create a refund transaction that fetches tokens from a deposit channel and transfers them.
	/// This combines fetch and transfer operations in a single transaction for token refunds.
	pub fn refund_token(
		fetch_param: FetchAssetParams<Solana>,
		transfer_ata: SolAddress,
		transfer_amount: SolAmount,
		transfer_to: SolAddress,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		token_vault_pda_account: SolAddress,
		token_vault_ata: SolAddress,
		token_mint_pubkey: SolAddress,
		token_decimals: u8,
		sol_api_environment: SolApiEnvironment,
		agg_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
		let (fetch_instruction, fetch_compute_units) =
			Self::create_fetch_instruction(fetch_param, &sol_api_environment, agg_key)?;

		let transfer_instructions = Self::create_transfer_token_instructions(
			transfer_ata,
			transfer_amount,
			transfer_to,
			vault_program,
			vault_program_data_account,
			token_vault_pda_account,
			token_vault_ata,
			token_mint_pubkey,
			agg_key,
			token_decimals,
		);

		let mut instructions = vec![fetch_instruction];
		instructions.extend(transfer_instructions);

		let total_compute_units =
			BASE_COMPUTE_UNITS_PER_TX + fetch_compute_units + COMPUTE_UNITS_PER_TRANSFER_TOKEN;

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(total_compute_units),
			address_lookup_tables,
		)
	}

	/// Create an instruction set to rotate the current Vault agg key to the next key.
	pub fn rotate_agg_key(
		new_agg_key: SolAddress,
		all_nonce_accounts: Vec<SolAddress>,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		agg_key: SolAddress,
		alt_manager: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
		let number_of_nonces = all_nonce_accounts.len() as u32;
		let all_nonce_accounts_meta: Vec<AccountMeta> = all_nonce_accounts
			.into_iter()
			.map(|nonce_account| AccountMeta::new(nonce_account.into(), false))
			.collect();

		// Rotate nonces must come before the agg Key rotation, otherwise the aggKey
		// validation will fail on the rotate nonces instruction.
		let instructions = vec![
			AltManagerProgram::with_id(alt_manager)
				.rotate_nonces(
					vault_program_data_account,
					agg_key,
					new_agg_key,
					system_program_id(),
				)
				.with_additional_accounts(all_nonce_accounts_meta),
			VaultProgram::with_id(vault_program).rotate_agg_key(
				false,
				vault_program_data_account,
				agg_key,
				new_agg_key,
				system_program_id(),
			),
		];

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(
				COMPUTE_UNITS_PER_ROTATION + COMPUTE_UNITS_PER_NONCE_ROTATION * number_of_nonces,
			),
			address_lookup_tables,
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
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
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
				.with_additional_accounts(ccm_accounts.additional_account_metas()),
		];

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit,
			address_lookup_tables,
		)
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
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
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
		).with_additional_accounts(ccm_accounts.additional_account_metas())];

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit,
			address_lookup_tables,
		)
	}

	/// Create an instruction set to set the current GovKey with the agg key.
	pub fn set_gov_key_with_agg_key(
		new_gov_key: SolAddress,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		agg_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
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
			address_lookup_tables,
		)
	}

	/// Creates an instruction to close a number of open event swap accounts created via program
	/// swap.
	pub fn fetch_and_close_vault_swap_accounts(
		vault_swap_accounts: Vec<VaultSwapAccountAndSender>,
		vault_program_data_account: SolAddress,
		swap_endpoint_program: SolAddress,
		swap_endpoint_data_account: SolAddress,
		agg_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
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

		let swap_endpoint_native_vault_pda =
			derive_swap_endpoint_native_vault_account(swap_endpoint_program)
				.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

		let instructions = vec![
			SwapEndpointProgram::with_id(swap_endpoint_program).fetch_swap_endpoint_native_assets(
				swap_endpoint_native_vault_pda.bump,
				vault_program_data_account,
				swap_endpoint_native_vault_pda.address,
				agg_key,
				system_program_id(),
			),
			SwapEndpointProgram::with_id(swap_endpoint_program)
				.close_event_accounts(
					vault_program_data_account,
					agg_key,
					swap_endpoint_data_account,
				)
				.with_additional_accounts(swap_and_sender_vec),
		];

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(
				COMPUTE_UNITS_PER_FETCH_AND_CLOSE_VAULT_SWAP_ACCOUNTS +
					COMPUTE_UNITS_PER_CLOSE_ACCOUNT * number_of_accounts as u32,
			),
			address_lookup_tables,
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
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
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
			address_lookup_tables,
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
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
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
			address_lookup_tables,
		)
	}

	pub fn upgrade_program(
		program_address: SolAddress,
		buffer_address: SolAddress,
		vault_program: SolAddress,
		vault_program_data_account: SolAddress,
		gov_key: SolAddress,
		spill_address: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
		let program_data_address = derive_program_data_address(program_address)
			.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;
		let signer_pda = derive_pda_signer(vault_program)
			.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

		let instructions = vec![VaultProgram::with_id(vault_program).upgrade_program(
			vault_program_data_account,
			gov_key,
			program_data_address.address,
			program_address,
			buffer_address,
			spill_address,
			SYS_VAR_RENT,
			SYS_VAR_CLOCK,
			signer_pda.address,
			BPF_LOADER_UPGRADEABLE_ID,
		)];

		Self::build(
			instructions,
			durable_nonce,
			gov_key.into(),
			compute_price,
			compute_limit_with_buffer(COMPUTE_UNITS_PER_UPGRADE_PROGRAM),
			address_lookup_tables,
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
			sol_tx_core::{
				address_derivation::derive_deposit_address, consts::SOL_USD_DECIMAL,
				sol_test_values::*, PdaAndBump,
			},
			SolanaDepositFetchId, MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES,
			MAX_USER_CCM_BYTES_SOL, MAX_USER_CCM_BYTES_USDC,
		},
		TransferAssetParams,
	};

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
		let expected_serialized_tx = hex_literal::hex!("0183f284c4160d449a41f0a7b30c3710a7e1876d514ef6d87b89a35ae203d50c6928b1dcd2f821496ac4027bfd84e07f921a912537e3d3f3cd4530935b0cae36028001000509f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1923a4539fbb757256442c16343f639b15db95c39a6d35721439f7f94f5c8776b7bfd35d0bf8686de2e369c3d97a8033b31e6bc33518629f59314bc3d9050956c8d00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000404030106000404000000050009038096980000000000050005021f95000007050800030204158e24658f6c59298c080000000b0c0d3700000000fc00").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("01f6a40d02eb553db89f9be4a37bfccd9c9a18ea6687c6e092cec5935863f8b4416c2a290cc474be118a404dc3035866e714dd70d4c77a6549b59a21a6bdb6bf06800100050bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19238861d2f0bf5cd80031b701a6c25d13b4c812dd92f9d6301fafd9a58fb9e438646cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a708d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7dd00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900050603010800040400000007000903809698000000000007000502c34a010009050a00030206158e24658f6c59298c080000000000000000000000ff09050a00040506158e24658f6c59298c080000000100000000000000ff00").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("01457e11aafedb71af4453f26eccca8f58a7aacaf1970d89624ef0109c24bfd0c53065f0339584da31710b17dbc2cc5db2a7c63c40e8cafba6ce6a29e4694d8409800100080df79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19242ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d91079c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f3036ae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8746cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000405030107000404000000060009038096980000000000060005024f0a01000b090c000a02030908040516494710642cb0c646080000000000000000000000ff0600").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("017ce2f57249f5a6c59c301c42bdf7ec5511331df41c9c0e51358655a3041e4267e599aef4957ea2155834ddc60c5f62cf483cef06fd2e49e3569f8fd7e97cde048001000912f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921ad0968d57ee79348476716f9b2cd44ec4284b8f52c36648d560949e41589a5540de1c0451cccb6edd1fda9b4a48c282b279350b55a7a9716800cc0132b6f0b042ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d91079c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f3036a140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aaae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21fb4baefcd4965beb1c71311a2ffe76419d4b8f8d35fbc4cf514b1bd02da2df2e300000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8746cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900060903010b0004040000000a00090380969800000000000a000502e7bb02000f0911000e04050d0c070916494710642cb0c646080000000000000000000000ff060f0911001002050d0c030916494710642cb0c646080000000100000000000000ff060f051100060809158e24658f6c59298c080000000200000000000000ff00").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("01cf95647e8340f44ff54c971187192f31f12abe07d1a2c12bfa21c8a36b311efdcf4f80323c52025cea58480b3a62ad3b7e39b86fb1f6d2f793fe9d3bd1b3a2018001000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090380969800000000000400050284030000030200020c0200000000ca9a3b0000000000").to_vec();

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
			SOL_USD_DECIMAL,
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_transfer_token` test
		let expected_serialized_tx = hex_literal::hex!("01abc6484a0ab9ccaddff295edfae87effabae1313748b63c517fb9b5143ef88d703f6c02deecb0461eeabec0514bbca5061f950d50a2a564555e0dd3e31eb7b068001000508f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec461600000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000031e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000503030109000404000000040009038096980000000000040005029b27010007060002050b030a010106070c000d08020b0a1136b4eeaf4a557ebc00ca9a3b0000000006013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10105050d09030204").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_transfer_usdt_token_transaction() {
		let env = api_env();
		let to_pubkey = TRANSFER_TO_ACCOUNT;
		let to_pubkey_ata =
			crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
				to_pubkey,
				env.usdt_token_mint_pubkey,
			)
			.unwrap();

		let transaction = SolanaTransactionBuilder::transfer_token(
			to_pubkey_ata.address,
			TRANSFER_AMOUNT,
			to_pubkey,
			env.vault_program,
			env.vault_program_data_account,
			env.token_vault_pda_account,
			env.usdt_token_vault_ata,
			env.usdt_token_mint_pubkey,
			agg_key(),
			durable_nonce(),
			compute_price(),
			SOL_USD_DECIMAL,
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_transfer_token` test
		let expected_serialized_tx = hex_literal::hex!("01b1fa23f718f59873a4741d2492d7c600b427e6075a1c61cf717516471b93618424450be12f846c18cc34bb10dca683028e966e2b620e2bcc7c423ce50e244903800100060af79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192d773abc32ab6cfab6e0ee9c42f85eb56090ec1e10d8b0d9eb89a4ae6b3720694dad0fa06b8d244bffdd03c040a2c6cc35b4cb77eada8522113f82adcae8b29b600000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000031e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd46b198d3efdbc9673d29f3a2bcb3e70e699164df8cad366563693aa7ba0e7d28a72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900050403010a000404000000050009038096980000000000050005029b270100090600020607040b010108070c000d0302070b1136b4eeaf4a557ebc00ca9a3b0000000006013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a100040d090204").to_vec();

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
			env.alt_manager_program,
			durable_nonce(),
			compute_price(),
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `rotate_agg_key` test
		let expected_serialized_tx = hex_literal::hex!("018fa7c55c171bee421dca6dcc52da93857ddf8ef6cd793e6b00d905d453cea8777b90deb735d7ea9159de607c992bd64caff0a9e812b7cd01460a8a92f8e38d008001000407f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1926744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a400000002ec25ce83748eb28232064bd8f41d4f0e7e0cc1186b8704eabdb2461ef50e12c72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900050303011100040400000004000903809698000000000004000502ac070100050e09000203010f0d0e0b0a07100c080862f54977a55a39cb060409000203094e518fabdda5d68b00013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10a16190215141812131117010d").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_fetch_and_close_vault_swap_accounts() {
		let env = api_env();
		let vault_swap_accounts = vec![EVENT_AND_SENDER_ACCOUNTS[0]];
		let transaction = SolanaTransactionBuilder::fetch_and_close_vault_swap_accounts(
			vault_swap_accounts,
			env.vault_program_data_account,
			env.swap_endpoint_program,
			env.swap_endpoint_program_data_account,
			agg_key(),
			durable_nonce(),
			compute_price(),
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `fetch_and_close_vault_swap_accounts` test
		let expected_serialized_tx = hex_literal::hex!("01be52d00111e319a705246387c4a8fb47e4d245cc924efd2e7a3c6d292565439366882bac224e02a2c6613ddef87c28354945b8110f52ccafa72cb67c622dc2088001000308f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17e5cc1f4d51a40626e11c783b75a45a4922615ecd7f5320b9d4d46481a196a317eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192665730decf59d4cd6db8437dab77302287431eb7562b5997601851a0eab6946fc8bb64258728f7a98b57a72fade81639eb845674b3d259b51991a97a1821a31900000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a400000001ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc1622938c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000505030209000404000000060009038096980000000000060005025898000007040a040005098579b3e88abc5343fe07050a0008010308a5663d01b94dbd79013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10108020d02").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_close_max_vault_swap_accounts() {
		let env = api_env();

		// Take the max amount of event accounts we will use
		let event_accounts = EVENT_AND_SENDER_ACCOUNTS
			.iter()
			.take(MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES)
			.copied()
			.collect::<Vec<_>>();

		// We must be able to close MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES accounts without
		// reaching the transaction length limit.
		let transaction = SolanaTransactionBuilder::fetch_and_close_vault_swap_accounts(
			event_accounts,
			env.vault_program_data_account,
			env.swap_endpoint_program,
			env.swap_endpoint_program_data_account,
			agg_key(),
			durable_nonce(),
			compute_price(),
			vec![chainflip_alt()],
		)
		.unwrap();

		let expected_serialized_tx = hex_literal::hex!("01f9a9e327b9784d8ce3e1a5f4f9b06d5e5335962a17b16e0f9a4f1fca91bfdc025e741ed9acdbf1c49289686905b588719b318c71458193b3cf31f78dac283b068001000310f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb05a557a331e51b8bf444d3bacdf6f48d8fd583aa79f9b956dd68f13a67ad096417e5cc1f4d51a40626e11c783b75a45a4922615ecd7f5320b9d4d46481a196a317eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921c11d80c98e8c11e79fd97b6c10ad733782bdbe25a710b807bbf14dedaa3148657d7f5b3e6c340824caca3b6c34c03e2fe0e636430b2b729ddfe32146ba4b3795c4b1e73c84b8d3f9e006c22fe8b865b9900e296345d88cdaaa7077ef17d9a31665730decf59d4cd6db8437dab77302287431eb7562b5997601851a0eab6946fa7e867ab720f01897e5ede67fc232e41729d0be2a530391619743822ff6d95bea9dff663e1d13345d96daede8066cd30a1474635f2d64052d1a50ac04aed3f99bd9ce2f9674b65bfaefb62c9b8252fd0080357b1cbff44d0dad8568535dbc230c8bb64258728f7a98b57a72fade81639eb845674b3d259b51991a97a1821a319d33096c9d0fa193639345c07abfe81175fc4d153cf0ab7b5668006538f19538200000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a400000001ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc1622938c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900050d0303110004040000000e00090380969800000000000e000502f82401000f04120b000d098579b3e88abc5343fe0f0d120010020705010a090804060c08a5663d01b94dbd79013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10108020d02").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_ccm_native_transaction() {
		let ccm_param = ccm_parameter_v0();
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
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_ccm_native_transfer` test
		let expected_serialized_tx = hex_literal::hex!("015073f04feba1737ba0b50cb93817e479d376188176f7de2bcd6a6db3d8a401f933be16433f25729d6cff4ff2066a89a9ca577253c810f668c7f6126e61587d058001000408f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd47417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900050403010900040400000005000903809698000000000005000502e0930400040200020c0200000000ca9a3b0000000006070a000203040807347d050be38042e0b20100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a100030a0d02").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_ccm_usdc_token_transaction() {
		let env = api_env();
		let ccm_param = ccm_parameter_v0();
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
			SOL_USD_DECIMAL,
			TEST_COMPUTE_LIMIT,
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_ccm_token_transfer` test
		let expected_serialized_tx = hex_literal::hex!("01bcc5eb1da05587d0316622460d3076fa74a84f30bd1920ebcdb0105c1ebf6b27f22dcc82b0b5e702caf7be0d7c1c6150140b38573229d2f1d7ad16c11fef4c0d800100060af79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec46167417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000031e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900060403010c00040400000005000903809698000000000005000502e093040008060002060e040d010107070f00100a020e0d1136b4eeaf4a557ebc00ca9a3b000000000607080f0002030d0e0b09346cb8a27b9fdeaa230100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10105060a0d09030204").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_ccm_usdt_token_transaction() {
		let env = api_env();
		let ccm_param = ccm_parameter_v0();
		let to = TRANSFER_TO_ACCOUNT;
		let to_ata = crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
			to,
			env.usdt_token_mint_pubkey,
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
			env.usdt_token_vault_ata,
			env.usdt_token_mint_pubkey,
			agg_key(),
			durable_nonce(),
			compute_price(),
			SOL_USD_DECIMAL,
			TEST_COMPUTE_LIMIT,
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_ccm_token_transfer` test
		let expected_serialized_tx = hex_literal::hex!("01db23da1c3387bdbc1b3013590ab08e517be1db321bef35bf31e038a30dd04b5ccac177f83f603dc53aed2bbb8df579ce72a847c2106b8c73e3ed5fb3588fe70d800100070cf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1927417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed48d773abc32ab6cfab6e0ee9c42f85eb56090ec1e10d8b0d9eb89a4ae6b3720694dad0fa06b8d244bffdd03c040a2c6cc35b4cb77eada8522113f82adcae8b29b600000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000031e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd46b198d3efdbc9673d29f3a2bcb3e70e699164df8cad366563693aa7ba0e7d28a72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900060503010d00040400000006000903809698000000000006000502e09304000a0600030708050e010109070f00100403080e1136b4eeaf4a557ebc00ca9a3b000000000609080f0003020e080c0b346cb8a27b9fdeaa230100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a100050a0d090204").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn ccm_native_transfer_length_check() {
		let transfer_param = TransferAssetParams::<Solana> {
			asset: SOL,
			amount: TRANSFER_AMOUNT,
			to: TRANSFER_TO_ACCOUNT,
		};
		let env = api_env();
		let mut ccm_accounts = ccm_accounts();
		ccm_accounts.additional_accounts = vec![];

		let transaction = SolanaTransactionBuilder::ccm_transfer_native(
			transfer_param.amount,
			transfer_param.to,
			ccm_parameter_v0().source_chain,
			None,
			vec![],
			ccm_accounts,
			env.vault_program,
			env.vault_program_data_account,
			agg_key(),
			durable_nonce(),
			compute_price(),
			TEST_COMPUTE_LIMIT,
			vec![chainflip_alt()],
		)
		.unwrap();

		let serialized_tx = sign_and_serialize(transaction);

		// Check that a CCM native transfer with no additional accounts and an empty message
		// results in the expected number of expected bytes available to the user.
		assert_eq!(serialized_tx.len(), MAX_TRANSACTION_LENGTH - MAX_USER_CCM_BYTES_SOL);
	}

	#[test]
	fn ccm_token_transfer_length_check() {
		let env = api_env();
		let to = TRANSFER_TO_ACCOUNT;
		let to_ata = crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
			to,
			env.usdc_token_mint_pubkey,
		)
		.unwrap();
		let mut ccm_accounts = ccm_accounts();
		ccm_accounts.additional_accounts = vec![];

		let transaction = SolanaTransactionBuilder::ccm_transfer_token(
			to_ata.address,
			TRANSFER_AMOUNT,
			to,
			ccm_parameter_v0().source_chain,
			None,
			vec![],
			ccm_accounts,
			env.vault_program,
			env.vault_program_data_account,
			env.token_vault_pda_account,
			env.usdc_token_vault_ata,
			env.usdc_token_mint_pubkey,
			agg_key(),
			durable_nonce(),
			compute_price(),
			SOL_USD_DECIMAL,
			TEST_COMPUTE_LIMIT,
			vec![chainflip_alt()],
		)
		.unwrap();

		let serialized_tx = sign_and_serialize(transaction);

		// Check that a CCM token transfer with no additional accounts and an empty message
		// results in the expected number of expected bytes available to the user.
		assert_eq!(serialized_tx.len(), MAX_TRANSACTION_LENGTH - MAX_USER_CCM_BYTES_USDC);
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
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `set_gov_key_with_agg_key` test
		let expected_serialized_tx = hex_literal::hex!("01190278359dbb4fd23b7252e6da2d54e8e37ba0eeab66272e15d07de46d25945a55cedb7766d0ffb163218998ad037e6d388682f775c9b92dd829088cdde46f028001000305f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19200000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900040203010600040400000003000903809698000000000003000502e4570000040205002842403a280f4bd7a26744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10102010d").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn transactions_above_max_lengths_will_fail() {
		let limit = 9;

		assert_ok!(SolanaTransactionBuilder::fetch_from(
			(0..limit).map(|i| get_fetch_params(Some(i), SOL)).collect(),
			api_env(),
			agg_key(),
			durable_nonce(),
			compute_price(),
		));

		assert_err!(
			SolanaTransactionBuilder::fetch_from(
				(0..=limit).map(|i| get_fetch_params(Some(i), SOL)).collect(),
				api_env(),
				agg_key(),
				durable_nonce(),
				compute_price(),
			),
			SolanaTransactionBuildingError::FinalTransactionExceededMaxLength(1288)
		);
	}

	#[test]
	fn can_create_upgrade_program() {
		let env = api_env();

		let transaction = SolanaTransactionBuilder::upgrade_program(
			SWAP_ENDPOINT_PROGRAM,
			TRANSFER_TO_ACCOUNT, // using arbitrary account as buffer_address
			env.vault_program,
			env.vault_program_data_account,
			agg_key(),
			agg_key(),
			durable_nonce(),
			compute_price(),
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `set_gov_key_with_agg_key` test
		let expected_serialized_tx = hex_literal::hex!("01139cb059fc52af18be3ccec985f75f05ebc27f535cbab4880fe7bdb4c4286ebea74d2768c5caa93aed1ee386bc3c93549e1a77c24bff1317d0d17d9b9f793206800100070bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd4cde5ef84f05a81106a2008f93ecd3f1088dbacd4e5c592fbbfe28cc906702f4b000000000000000000000000000000000000000000000000000000000000000002a8f6914e88a1b0e210153ef763ae2b00c2b93d16c124d2c0537a10048000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d51718c774c928566398691d5eb68b5eb8a39b4b6d5c73555b210000000006a7d517192c5c51218cc94c3d4af17f58daee089ba1fd44e3dbd98a0000000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cef557795afc29ff257a1ea5fcd11ece260f1f96e50f3c1da477013d1d7f350fec27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900040403010c00040400000006000903809698000000000006000502f0490200090a0d00030b020008070a0508dfec27596fcc7225013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10107020d02").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}
}
