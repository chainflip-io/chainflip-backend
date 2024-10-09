//! This file contains a Instruction builder for the Solana chain.
//!
//! The builder provides a interface for the API to create raw Solana
//! Instructions and Instruction sets with some level of abstraction.
//! This avoids the need to deal with low level Solana core types.

use sol_prim::consts::{
	LAMPORTS_PER_SIGNATURE, MAX_TRANSACTION_LENGTH, MICROLAMPORTS_PER_LAMPORT, SOL_USDC_DECIMAL,
	SYSTEM_PROGRAM_ID, SYS_VAR_INSTRUCTIONS, TOKEN_PROGRAM_ID,
};

use crate::{
	sol::{
		api::{DurableNonceAndAccount, EventAccountAndSender, SolanaTransactionBuildingError},
		compute_units_costs::{
			compute_limit_with_buffer, BASE_COMPUTE_UNITS_PER_TX,
			COMPUTE_UNITS_PER_BUMP_DERIVATION, COMPUTE_UNITS_PER_CLOSE_ACCOUNT,
			COMPUTE_UNITS_PER_CLOSE_EVENT_ACCOUNTS, COMPUTE_UNITS_PER_FETCH_NATIVE,
			COMPUTE_UNITS_PER_FETCH_TOKEN, COMPUTE_UNITS_PER_ROTATION,
			COMPUTE_UNITS_PER_SET_GOV_KEY, COMPUTE_UNITS_PER_TRANSFER_NATIVE,
			COMPUTE_UNITS_PER_TRANSFER_TOKEN,
		},
		sol_tx_core::{
			address_derivation::{derive_associated_token_account, derive_fetch_account},
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

use super::compute_units_costs::{
	MAX_COMPUTE_UNITS_PER_CCM_TRANSFER, MIN_COMPUTE_LIMIT_PER_CCM_NATIVE_TRANSFER,
	MIN_COMPUTE_LIMIT_PER_CCM_TOKEN_TRANSFER, MIN_COMPUTE_PRICE,
};

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
			sp_std::cmp::max(compute_price, MIN_COMPUTE_PRICE),
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
		gas_budget: SolAmount,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let instructions = vec![
			SystemProgramInstruction::transfer(&agg_key.into(), &to.into(), amount),
			VaultProgram::with_id(vault_program)
				.execute_ccm_native_call(
					source_chain as u32,
					source_address.map_or_else(Vec::new, |address| address.to_source_address()),
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

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			Self::calculate_ccm_gas_limit(gas_budget, compute_price, SolAsset::Sol),
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
		gas_budget: SolAmount,
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
			source_address.map_or_else(Vec::new, |address| address.to_source_address()),
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

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			Self::calculate_ccm_gas_limit(gas_budget, compute_price, SolAsset::SolUsdc),
		)
	}

	fn calculate_ccm_gas_limit(
		gas_budget: SolAmount,
		compute_price: SolAmount,
		asset: SolAsset,
	) -> SolComputeLimit {
		let gas_budget_after_signature = gas_budget.saturating_sub(LAMPORTS_PER_SIGNATURE);

		let compute_limit_from_budget =
			// Budget is in lamports, compute price is in microlamports/CU.
			// A minimum compute price is set when building a transaction.
			sp_std::cmp::min(
				MAX_COMPUTE_UNITS_PER_CCM_TRANSFER as u128,
				(gas_budget_after_signature as u128 * MICROLAMPORTS_PER_LAMPORT as u128)
					/ sp_std::cmp::max(compute_price as u128, MIN_COMPUTE_PRICE as u128),
			) as SolComputeLimit;

		sp_std::cmp::max(
			compute_limit_from_budget,
			match asset {
				SolAsset::Sol => MIN_COMPUTE_LIMIT_PER_CCM_NATIVE_TRANSFER,
				SolAsset::SolUsdc => MIN_COMPUTE_LIMIT_PER_CCM_TOKEN_TRANSFER,
			},
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
	pub fn close_event_accounts(
		event_accounts: Vec<EventAccountAndSender>,
		vault_program_data_account: SolAddress,
		swap_endpoint_program: SolAddress,
		swap_endpoint_data_account: SolAddress,
		agg_key: SolAddress,
		durable_nonce: DurableNonceAndAccount,
		compute_price: SolAmount,
	) -> Result<SolTransaction, SolanaTransactionBuildingError> {
		let number_of_accounts = event_accounts.len();
		let event_and_sender_vec: Vec<AccountMeta> = event_accounts
			.into_iter()
			.flat_map(|(event_account, payee)| vec![event_account, payee])
			// Both event account and payee should be writable and non-signers
			.map(|address| AccountMeta::new(address.into(), false))
			.collect();

		let instructions = vec![SwapEndpointProgram::with_id(swap_endpoint_program)
			.close_event_accounts(vault_program_data_account, agg_key, swap_endpoint_data_account)
			.with_remaining_accounts(event_and_sender_vec)];

		Self::build(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			compute_limit_with_buffer(
				COMPUTE_UNITS_PER_CLOSE_EVENT_ACCOUNTS +
					COMPUTE_UNITS_PER_CLOSE_ACCOUNT * number_of_accounts as u32,
			),
		)
	}
}

#[cfg(test)]
mod test {
	use cf_primitives::ChannelId;
	use frame_support::{assert_err, assert_ok};

	use super::*;
	use crate::{
		sol::{
			signing_key::SolSigningKey,
			sol_tx_core::{address_derivation::derive_deposit_address, sol_test_values::*},
			SolanaDepositFetchId,
		},
		TransferAssetParams,
	};

	use sol_prim::{
		consts::{MAX_TRANSACTION_LENGTH, SOL_USDC_DECIMAL},
		PdaAndBump,
	};

	// Arbitrary number used for testing
	const TEST_COMPUTE_LIMIT: SolComputeLimit = 300_000u32;

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

	fn durable_nonce() -> DurableNonceAndAccount {
		(NONCE_ACCOUNTS[0], TEST_DURABLE_NONCE)
	}

	fn api_env() -> SolApiEnvironment {
		SolApiEnvironment {
			vault_program: VAULT_PROGRAM,
			vault_program_data_account: VAULT_PROGRAM_DATA_ACCOUNT,
			token_vault_pda_account: TOKEN_VAULT_PDA_ACCOUNT,
			usdc_token_mint_pubkey: USDC_TOKEN_MINT_PUB_KEY,
			usdc_token_vault_ata: USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
			swap_endpoint_program: SWAP_ENDPOINT_PROGRAM,
			swap_endpoint_program_data_account: SWAP_ENDPOINT_PROGRAM_DATA_ACCOUNT,
		}
	}

	fn compute_price() -> SolAmount {
		COMPUTE_UNIT_PRICE
	}

	fn nonce_accounts() -> Vec<SolAddress> {
		NONCE_ACCOUNTS.to_vec()
	}

	#[track_caller]
	fn test_constructed_transaction(
		mut transaction: SolTransaction,
		expected_serialized_tx: Vec<u8>,
	) {
		// Obtain required info from Chain Environment
		let durable_nonce = durable_nonce();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();

		// Construct the Transaction and sign it
		transaction.sign(&[&agg_key_keypair], durable_nonce.1.into());

		// println!("{:?}", tx);
		let serialized_tx = transaction
			.clone()
			.finalize_and_serialize()
			.expect("Transaction serialization must succeed");

		println!("Serialized tx length: {:?}", serialized_tx.len());
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH);

		if serialized_tx != expected_serialized_tx {
			panic!(
				"Transaction mismatch. \nTx: {:?} \nSerialized: {:?}",
				transaction,
				hex::encode(serialized_tx.clone())
			);
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
		let expected_serialized_tx = hex_literal::hex!("019c17eb101210e881f4ff847f558a9c5191b1d1921ffdb17b949e60bcb6b082785bc0b0cad309498363f88fb780f1b58a823b562924d766a13519a6fb4bae440501000509f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1923a4539fbb757256442c16343f639b15db95c39a6d35721439f7f94f5c8776b7bfd35d0bf8686de2e369c3d97a8033b31e6bc33518629f59314bc3d9050956c8d00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004040301060004040000000500090340420f0000000000050005021f95000008050700030204158e24658f6c59298c080000000b0c0d3700000000fc").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("0147f606ecc5a8ba6f9a5ef1b65fda44998ba6b2b6770f4dfec534b66ec6bf4a0a7768c8136d8a3df930d79bdfba71adf7b43fbcdda19a020afbc5ce5a1b6c10050100050bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19238861d2f0bf5cd80031b701a6c25d13b4c812dd92f9d6301fafd9a58fb9e438646cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a708d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7dd00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005060301080004040000000700090340420f000000000007000502c34a01000a050900030206158e24658f6c59298c080000000000000000000000ff0a050900040506158e24658f6c59298c080000000100000000000000ff").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("01fabad9f984bdb8cd256c36191cd9942a4de41daf32f057fff20a9e7ef0152246ca837ef6a3f720d450e4a03443bcbcffabb8c73e8151f950e733300584a57a040100080df79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19242ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910ae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21fe91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8746cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004050301070004040000000600090340420f0000000000060005024f0a01000c0909000b02040a08030516494710642cb0c646080000000000000000000000ff06").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("01d2bcb622d5297724275864176bad920a8d7b591a5d80746f0b2a63936bca1647225932b8b891fd547bb8652eab8bbfbc586a2fd34c892659325fbb6b3e42750f01000912f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921ad0968d57ee79348476716f9b2cd44ec4284b8f52c36648d560949e41589a5540de1c0451cccb6edd1fda9b4a48c282b279350b55a7a9716800cc0132b6f0b042ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910a140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aaae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21fb4baefcd4965beb1c71311a2ffe76419d4b8f8d35fbc4cf514b1bd02da2df2e3e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8746cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900060903010b0004040000000a00090340420f00000000000a000502e7bb020010090d000f04080e0c060916494710642cb0c646080000000000000000000000ff0610090d001102080e0c030916494710642cb0c646080000000100000000000000ff0610050d00050709158e24658f6c59298c080000000200000000000000ff").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("013474897b54f54c0cdb96ddd969eafd22d7960742882784621401dae7ad2baeede53bdbc2afc09dbcf11bc31c6c8c0af1a71c1d378ead8655c3718d7f33da3a0b01000a0ef79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec4616e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859ab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301060004040000000500090340420f0000000000050005029b2701000c0600020a09040701010b0708000d030209071136b4eeaf4a557ebc00ca9a3b0000000006").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("0180d9ae78d86dbf0895772b959d27110d09d8cb0f9bb388cbc84a53372b568ea56cb9f235f05bf59446a18b9e9babdf61e7194cd6f838d6fd6a741e6f60cc300d01000411f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb0e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0917eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1926744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900448541f57201f277c5f3ffb631d0212e26e7f47749c26c4808718174a0ab2a09a18cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adbcd644e45426a41a7cb8369b8a0c1c89bb3f86cf278fdd9cc38b0f69784ad5667e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864e5e1869817a4fd88ddf7ab7a5f7252d7c345b39721769888608592912e8ca9acf0f13460b3fd04b7d53d7421fc874ec00eec769cf36480895e1a407bf1249475f2b2e24122be016983be9369965246cc45e1f621d40fba300c56c7ac50c3874df4f83bd213a59c9785110cf83c718f9486c3484f918593bce20c61dc6a96036afecc89e3b031824af6363174d19bbec12d3a13c4a173e5aeb349b63042bc138f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000e0d03020f0004040000000e00090340420f00000000000e000502e02e000010040100030d094e518fabdda5d68b000d02020024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020b0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02090024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020a0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02070024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02060024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02040024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020c0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02080024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02050024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_close_event_accounts() {
		let env = api_env();
		let event_accounts = vec![EVENT_AND_SENDER_ACCOUNTS[0]];
		let transaction = SolanaTransactionBuilder::close_event_accounts(
			event_accounts,
			env.vault_program_data_account,
			env.swap_endpoint_program,
			env.swap_endpoint_program_data_account,
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `close_event_accounts` test
		let expected_serialized_tx = hex_literal::hex!("01026e2d4bdca9e638b59507a70ea62ad88f098ffb25df028a19288702698fdf6d1cf77618b2123c0205a8e8d272ba8ea645b7e75c606ca3aa4356b65fa52ca20b0100050af79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17e5cc1f4d51a40626e11c783b75a45a4922615ecd7f5320b9d4d46481a196a317eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921c1f0efc91eeb48bb80c90cf97775cd5d843a96f16500266cee2c20d053152d2665730decf59d4cd6db8437dab77302287431eb7562b5997601851a0eab6946f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e091ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc1622938c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004050302070004040000000600090340420f000000000006000502307500000905080003010408a5663d01b94dbd79").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_close_max_event_accounts() {
		let env = api_env();

		// We can close 11 accounts without reaching the transaction length limit.
		let transaction = SolanaTransactionBuilder::close_event_accounts(
			EVENT_AND_SENDER_ACCOUNTS.to_vec(),
			env.vault_program_data_account,
			env.swap_endpoint_program,
			env.swap_endpoint_program_data_account,
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		let expected_serialized_tx = hex_literal::hex!("01361c7baab92d2d4599136442ad7d4c1d51fedc6749d44f3a8e8405cc19983862c885526b293ed79d0253d44e174705db9050638d1ffc31b6f7b586b269fea4030100051ef79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb05a557a331e51b8bf444d3bacdf6f48d8fd583aa79f9b956dd68f13a67ad096412741ecfad8423dea0c173b354b32309c3e97bb1dc68e0d858c3caebc1a1701a178480c19a99c9f2b95d40ebcb55057a49f0df00e123da6ae5e85a77c282f7c117e5cc1f4d51a40626e11c783b75a45a4922615ecd7f5320b9d4d46481a196a317eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921c11d80c98e8c11e79fd97b6c10ad733782bdbe25a710b807bbf14dedaa314861c1f0efc91eeb48bb80c90cf97775cd5d843a96f16500266cee2c20d053152d2266d68abb283ba2f4cecb092e3cfed2cb1774468ebbc264426c268ff405aa5a837de225793278f0575804f7d969e1980caaa5c5ddb2aebfd8496b14e71c9fad657d7f5b3e6c340824caca3b6c34c03e2fe0e636430b2b729ddfe32146ba4b3795c4b1e73c84b8d3f9e006c22fe8b865b9900e296345d88cdaaa7077ef17d9a31665730decf59d4cd6db8437dab77302287431eb7562b5997601851a0eab6946f74dd7ddee33a59ae7431bb31fbeb738cbfd097a66fd6706cffe7fc7efb239ec67fab67806fbb92ffd9504f4411b7f4561a0efb16685e4a22c41373fedc50b4bf86554fe5208d48fc8198310804e59837443fdaab12ea97be0fa38049910da82987410536ffebba5f49e67bafd3aa4b6cc860a594641e801500e058f74bac504da054544b2425f722e18c810bbc6cb6b9045d0db0a62d529af30efde8c37255bda7e867ab720f01897e5ede67fc232e41729d0be2a530391619743822ff6d95bea9dff663e1d13345d96daede8066cd30a1474635f2d64052d1a50ac04aed3f99bd9ce2f9674b65bfaefb62c9b8252fd0080357b1cbff44d0dad8568535dbc230c78bf2e7aee8e16631746542ef634cee3ac9bdc044c491f06862590ff1029865ce904f76d0a0ffedad66f8e2c94bccc731cac372fef8bb12cd2c473d95acf366d33096c9d0fa193639345c07abfe81175fc4d153cf0ab7b5668006538f195382df0e412e53b45bf52f91fa8e70ea872687428e4cb372306b9e6073f8d3c270c400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e091ef91c791d2aa8492c90f12540abd10056ce5dd8d9ab08461476c1dcc1622938c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900041903051b0004040000001a00090340420f00000000001a00050220bf02001d191c0007040c0a01141312060b17100e02090d0f08151103161808a5663d01b94dbd79").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_calculate_gas_limit() {
		const TEST_EGRESS_BUDGET: SolAmount = 500_000;
		const TEST_COMPUTE_PRICE: SolAmount = 2_000_000;

		let compute_price_lamports = TEST_COMPUTE_PRICE.div_ceil(MICROLAMPORTS_PER_LAMPORT.into());
		for asset in &[SolAsset::Sol, SolAsset::SolUsdc] {
			let mut tx_compute_limit: u32 = SolanaTransactionBuilder::calculate_ccm_gas_limit(
				TEST_EGRESS_BUDGET * compute_price_lamports + LAMPORTS_PER_SIGNATURE,
				TEST_COMPUTE_PRICE,
				*asset,
			);
			assert_eq!(tx_compute_limit as u64, TEST_EGRESS_BUDGET);

			// Rounded down
			assert_eq!(
				SolanaTransactionBuilder::calculate_ccm_gas_limit(
					(TEST_EGRESS_BUDGET + 1) as SolAmount + LAMPORTS_PER_SIGNATURE,
					(MICROLAMPORTS_PER_LAMPORT * 10) as SolAmount,
					*asset,
				),
				SolanaTransactionBuilder::calculate_ccm_gas_limit(
					(TEST_EGRESS_BUDGET + 9) as SolAmount + LAMPORTS_PER_SIGNATURE,
					(MICROLAMPORTS_PER_LAMPORT * 10) as SolAmount,
					*asset,
				)
			);
			assert_eq!(
				SolanaTransactionBuilder::calculate_ccm_gas_limit(
					(TEST_EGRESS_BUDGET + 1) as SolAmount * compute_price_lamports +
						LAMPORTS_PER_SIGNATURE,
					(MICROLAMPORTS_PER_LAMPORT * 10) as SolAmount,
					*asset,
				),
				SolanaTransactionBuilder::calculate_ccm_gas_limit(
					TEST_EGRESS_BUDGET as SolAmount * compute_price_lamports +
						LAMPORTS_PER_SIGNATURE,
					(MICROLAMPORTS_PER_LAMPORT * 10) as SolAmount,
					*asset,
				)
			);

			// Test SolComputeLimit saturation
			assert_eq!(
				SolanaTransactionBuilder::calculate_ccm_gas_limit(
					(SolComputeLimit::MAX as SolAmount) * 2 * compute_price_lamports +
						LAMPORTS_PER_SIGNATURE,
					TEST_COMPUTE_PRICE,
					*asset,
				),
				MAX_COMPUTE_UNITS_PER_CCM_TRANSFER
			);

			// Test upper cap
			tx_compute_limit = SolanaTransactionBuilder::calculate_ccm_gas_limit(
				MAX_COMPUTE_UNITS_PER_CCM_TRANSFER as u64 * compute_price_lamports * 2,
				TEST_COMPUTE_PRICE,
				*asset,
			);
			assert_eq!(tx_compute_limit, MAX_COMPUTE_UNITS_PER_CCM_TRANSFER);

			tx_compute_limit =
				SolanaTransactionBuilder::calculate_ccm_gas_limit(TEST_EGRESS_BUDGET, 0, *asset);
			assert_eq!(tx_compute_limit, MAX_COMPUTE_UNITS_PER_CCM_TRANSFER);
		}

		// Test lower cap
		let mut tx_compute_limit =
			SolanaTransactionBuilder::calculate_ccm_gas_limit(10u64, 1, SolAsset::Sol);
		assert_eq!(tx_compute_limit, MIN_COMPUTE_LIMIT_PER_CCM_NATIVE_TRANSFER);

		tx_compute_limit =
			SolanaTransactionBuilder::calculate_ccm_gas_limit(10u64, 1, SolAsset::SolUsdc);
		assert_eq!(tx_compute_limit, MIN_COMPUTE_LIMIT_PER_CCM_TOKEN_TRANSFER);
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
			(TEST_COMPUTE_LIMIT as u128 * compute_price() as u128)
				.div_ceil(MICROLAMPORTS_PER_LAMPORT.into()) as u64 +
				LAMPORTS_PER_SIGNATURE,
		)
		.unwrap();

		// Serialized tx built in `create_ccm_native_transfer` test
		let expected_serialized_tx = hex_literal::hex!("019934f0927bb3344080fc333956498280e7ff8959d7ad93e9f894cab5df74223752c3e6fc3607fec8b0a266d36a10b85bf3b9e4ab97f8204924130407c991690c0100070bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd47417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301070004040000000500090340420f000000000005000502e0930400040200020c0200000000ca9a3b0000000009070800020304060a347d050be38042e0b20100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

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
			(TEST_COMPUTE_LIMIT as u128 * compute_price() as u128)
				.div_ceil(MICROLAMPORTS_PER_LAMPORT.into()) as u64 +
				LAMPORTS_PER_SIGNATURE,
		)
		.unwrap();

		// Serialized tx built in `create_ccm_token_transfer` test
		let expected_serialized_tx = hex_literal::hex!("01b129476ffae4b80e116ceb457e9da19236c663373bc52d4e7cb5973429fb6157f74ac71e3168a286d7df90a1e259872cb64db6ee84fd6b44d504f529a5e8ea0c01000c11f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec46167417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed48e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5ab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890006050301080004040000000600090340420f000000000006000502e09304000e0600020c0b050901010d070a001004020b091136b4eeaf4a557ebc00ca9a3b00000000060d080a000203090b070f346cb8a27b9fdeaa230100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

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
		let expected_serialized_tx = hex_literal::hex!("01e68b952350bb2bf6fbf87364ad259d8bd488c20b828c52491417e5df3db7178c6ae9f934dbf27f67c19b22b297416f486da0257efbfb829e0cdce0e4557ccf0401000407f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb0e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0917eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19200000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030302050004040000000400090340420f000000000004000502e4570000060201002842403a280f4bd7a26744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

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
