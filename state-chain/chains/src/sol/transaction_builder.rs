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
			COMPUTE_UNITS_PER_FETCH_TOKEN, COMPUTE_UNITS_PER_ROTATION,
			COMPUTE_UNITS_PER_SET_GOV_KEY, COMPUTE_UNITS_PER_SET_PROGRAM_SWAPS_PARAMS,
			COMPUTE_UNITS_PER_TRANSFER_NATIVE, COMPUTE_UNITS_PER_TRANSFER_TOKEN,
		},
		sol_tx_core::{
			address_derivation::{
				derive_associated_token_account, derive_fetch_account,
				derive_swap_endpoint_native_vault_account, derive_token_supported_account,
			},
			compute_budget::ComputeBudgetInstruction,
			consts::{
				MAX_TRANSACTION_LENGTH, SOL_USDC_DECIMAL, SYSTEM_PROGRAM_ID, SYS_VAR_INSTRUCTIONS,
				TOKEN_PROGRAM_ID,
			},
			program_instructions::{
				swap_endpoints::SwapEndpointProgram, InstructionExt, SystemProgramInstruction,
				VaultProgram,
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

use super::sol_tx_core::program_instructions::alt_managers::AltManagerProgram;

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
		address_lookup_tables: Vec<SolAddressLookupTableAccount>,
	) -> Result<SolVersionedTransaction, SolanaTransactionBuildingError> {
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
			address_lookup_tables,
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

		// TODO: This will fail for now due to transaction being too long. It will work only when we
		// implement versioned transactions.
		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(COMPUTE_UNITS_PER_ROTATION),
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
}

#[cfg(test)]
pub mod test {
	use cf_primitives::ChannelId;
	use frame_support::{assert_err, assert_ok};

	use super::*;
	use crate::{
		sol::{
			sol_tx_core::{
				address_derivation::derive_deposit_address, consts::SOL_USDC_DECIMAL,
				sol_test_values::*, PdaAndBump,
			},
			SolanaDepositFetchId, MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES,
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
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_fetch_native` test
		let expected_serialized_tx = hex_literal::hex!("014c9e86bf4a01223aaad71560605c3bf8925aebaec28dad02ae7008290cf3682e649ab82d087c387d5ae72d362852737ce5802d48b17e054500bf8ad40787890e8001000407f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb3a4539fbb757256442c16343f639b15db95c39a6d35721439f7f94f5c8776b7bfd35d0bf8686de2e369c3d97a8033b31e6bc33518629f59314bc3d9050956c8d00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000403030705000404000000040009038096980000000000040005021f95000006050800020103158e24658f6c59298c080000000b0c0d3700000000fc013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a101090102").to_vec();

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
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_fetch_native_in_batch` test
		let expected_serialized_tx = hex_literal::hex!("012a95015c2c1a08af3013fd2575a7f61bf3a4faca1a010f7ec29f017ff10b52c49b9b8113e1adc89a9248c9537bff2eb3b2ca5bfafe9abbe4fbea753fccc139008001000409f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb38861d2f0bf5cd80031b701a6c25d13b4c812dd92f9d6301fafd9a58fb9e438646cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a708d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7dd00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900050503090700040400000006000903809698000000000006000502c34a010008050a00020105158e24658f6c59298c080000000000000000000000ff08050a00030405158e24658f6c59298c080000000100000000000000ff013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a101090102").to_vec();

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
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_fetch_tokens` test
		let expected_serialized_tx = hex_literal::hex!("0171e6c4881666cb11c8ab4402a1b0b5728c5a18acd668b0ffb524d913ff5bb717597564d05880a8e693ea3c6fd329750e88a0b6de6f97fa62a0c428b020cbea0e8001000609f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb42ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910ae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a946cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000403030905000404000000040009038096980000000000040005024f0a010008090c0007010a0b06020316494710642cb0c646080000000000000000000000ff06013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1020905020302").to_vec();

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
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_batch_fetch` test
		let expected_serialized_tx = hex_literal::hex!("015eab79254444f6f235494a180b8c0c11fe4ee73c85656eda36b4dccf717ace5d0613540e9d3febb4b439f86b45bfdb26ba4810e17d224efd8ad13c2f34f09f0a800100070ef79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb1ad0968d57ee79348476716f9b2cd44ec4284b8f52c36648d560949e41589a5540de1c0451cccb6edd1fda9b4a48c282b279350b55a7a9716800cc0132b6f0b042ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910a140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aaae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21fb4baefcd4965beb1c71311a2ffe76419d4b8f8d35fbc4cf514b1bd02da2df2e300000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a946cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000607030e0900040400000008000903809698000000000008000502e7bb02000c0911000b030f100a050716494710642cb0c646080000000000000000000000ff060c0911000d010f100a020716494710642cb0c646080000000100000000000000ff060c051100040607158e24658f6c59298c080000000200000000000000ff013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a1020905020302").to_vec();

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
			SOL_USDC_DECIMAL,
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_transfer_token` test
		let expected_serialized_tx = hex_literal::hex!("0128fe27ffdfed999b2b9e00e1327bcc1e22370dd2475633023172607ee98df3e14e3f6f45c23ee0988c1b605be91f416ab6d5f0d477a6f30615429d20e443a7098001000709f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb5ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec461600000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a931e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000502030904000404000000030009038096980000000000030005029b27010008060001060b0205010107070c000d0a010b051136b4eeaf4a557ebc00ca9a3b0000000006013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a102090503030204").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("01ab37f60681ba0afe0aeda3c39f5021f49332695e63243af28769b59de314f4b62445271f2155e4d49c83153b63a9c64ddeb2feda66acf2ad6058f411c95cf7038001000406f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb6744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000e0203060400040400000003000903809698000000000003000502e02e0000050409000102094e518fabdda5d68b000202060024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020f0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020d0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020e0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020b0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020a0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440202070024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440202100024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be54399004402020c0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440202080024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10b090f12020e0d110b0c0a1000").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("01d61138dfa0a3d4d9b19a9f10db9d7ad45fcac1a27766ce497bc7490ddceebc91046ae06adfcda2d9533c5670f1bf868e2adc607a9c16c2efe8e969531ec009048001000408f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17e5cc1f4d51a40626e11c783b75a45a4922615ecd7f5320b9d4d46481a196a3665730decf59d4cd6db8437dab77302287431eb7562b5997601851a0eab6946fc8bb64258728f7a98b57a72fade81639eb845674b3d259b51991a97a1821a31900000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000001ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc1622938c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000504030806000404000000050009038096980000000000050005025898000007040a030004098579b3e88abc5343fe07050a0009010208a5663d01b94dbd79013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10209080102").to_vec();

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

		let expected_serialized_tx = hex_literal::hex!("013ce4a7169ecb011374e4cea953491bf340bd8144f7c97b7ceb930310cda703b4e7f8296a987623c00879ddffc8dff71e3b9db2da8ef6895d50a58e92a6bf020f8001000410f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb05a557a331e51b8bf444d3bacdf6f48d8fd583aa79f9b956dd68f13a67ad096417e5cc1f4d51a40626e11c783b75a45a4922615ecd7f5320b9d4d46481a196a31c11d80c98e8c11e79fd97b6c10ad733782bdbe25a710b807bbf14dedaa3148657d7f5b3e6c340824caca3b6c34c03e2fe0e636430b2b729ddfe32146ba4b3795c4b1e73c84b8d3f9e006c22fe8b865b9900e296345d88cdaaa7077ef17d9a31665730decf59d4cd6db8437dab77302287431eb7562b5997601851a0eab6946fa7e867ab720f01897e5ede67fc232e41729d0be2a530391619743822ff6d95bea9dff663e1d13345d96daede8066cd30a1474635f2d64052d1a50ac04aed3f99bd9ce2f9674b65bfaefb62c9b8252fd0080357b1cbff44d0dad8568535dbc230c8bb64258728f7a98b57a72fade81639eb845674b3d259b51991a97a1821a319d33096c9d0fa193639345c07abfe81175fc4d153cf0ab7b5668006538f19538200000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000001ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc1622938c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900050c03100e0004040000000d00090380969800000000000d000502f82401000f04120a000c098579b3e88abc5343fe0f0d1200110206040109080703050b08a5663d01b94dbd79013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a10209080102").to_vec();

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
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_ccm_native_transfer` test
		let expected_serialized_tx = hex_literal::hex!("0102281baf609788e68ce97ba072a021a6c9788bb08b6f69546b713ffa6faabf5e925c5396ea66e8dd7f2cffe0c66fa7268c63019720b752ee073e3f4bd8fac0058001000609f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb31e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd47417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900050303090600040400000004000903809698000000000004000502e0930400030200010c0200000000ca9a3b0000000007070a000102030508347d050be38042e0b20100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a101090102").to_vec();

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
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `create_ccm_token_transfer` test
		let expected_serialized_tx = hex_literal::hex!("016f23b283e6b171c042d714ad6d24c6ab09fcbb22fcd977a2fb9b2ab53a3bf36418a2b7f8814672c10c7be41272e64d585e5fc7e913ad455d4e8d9d797ecdbb0e800100090cf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb5ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec46167417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a931e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000603030c0600040400000004000903809698000000000004000502e09304000a060001080e0307010109070f00100d010e071136b4eeaf4a557ebc00ca9a3b000000000609080f000102070e050b346cb8a27b9fdeaa230100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a102090503030204").to_vec();

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
			vec![chainflip_alt()],
		)
		.unwrap();

		// Serialized tx built in `set_gov_key_with_agg_key` test
		let expected_serialized_tx = hex_literal::hex!("01b08fe749db9efd9ce87fa0c45c816cc395b344bf5e0acdb172c7f6549c50651a1965b52a7a2150cdd579d40b9a6b07dbd734e59d59130b4f62f990a2fd28d8048001000405f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900040103050300040400000002000903809698000000000002000502e4570000040206002842403a280f4bd7a26744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044013001afd71da9456a977233960b08eba77d2e3690b8c7259637c8fb8f82cf58a102090200").to_vec();

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
			vec![chainflip_alt()],
		));

		assert_err!(
			SolanaTransactionBuilder::fetch_from(
				(0..=limit).map(|i| get_fetch_params(Some(i), SOL)).collect(),
				api_env(),
				agg_key(),
				durable_nonce(),
				compute_price(),
				vec![chainflip_alt()],
			),
			SolanaTransactionBuildingError::FinalTransactionExceededMaxLength(1260)
		);
	}
}
