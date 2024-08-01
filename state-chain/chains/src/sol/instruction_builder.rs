//! This file contains a Instruction builder for the Solana chain.
//!
//! The builder provides a interface for the API to create raw Solana
//! Instructions and Instruction sets with some level of abstraction.
//! This avoids the need to deal with low level Solana core types.

use sol_prim::consts::{
	LAMPORTS_PER_SIGNATURE, MAX_COMPUTE_UNITS_PER_TRANSACTION, MAX_TRANSACTION_LENGTH,
	MICROLAMPORTS_PER_LAMPORT, SOL_USDC_DECIMAL, SYSTEM_PROGRAM_ID, SYS_VAR_INSTRUCTIONS,
	TOKEN_PROGRAM_ID,
};

use crate::{
	sol::{
		api::{DurableNonceAndAccount, SolanaTransactionBuildingError},
		compute_units_costs::{
			compute_limit_with_buffer, BASE_COMPUTE_UNITS_PER_TX, COMPUTE_UNITS_PER_FETCH_NATIVE,
			COMPUTE_UNITS_PER_FETCH_TOKEN, COMPUTE_UNITS_PER_ROTATION,
			COMPUTE_UNITS_PER_TRANSFER_NATIVE, COMPUTE_UNITS_PER_TRANSFER_TOKEN,
		},
		sol_tx_core::{
			address_derivation::{derive_associated_token_account, derive_fetch_account},
			compute_budget::ComputeBudgetInstruction,
			program_instructions::{InstructionExt, SystemProgramInstruction, VaultProgram},
			token_instructions::AssociatedTokenAccountInstruction,
		},
		SolAddress, SolAmount, SolApiEnvironment, SolAsset, SolCcmAccounts, SolComputeLimit,
		SolInstruction, SolMessage, SolPubkey, SolTransaction, Solana,
	},
	FetchAssetParams, ForeignChainAddress,
};
use sp_std::{vec, vec::Vec};

use super::compute_units_costs::{
	DEFAULT_COMPUTE_UNITS_PER_CCM_TRANSFER, MIN_COMPUTE_LIMIT_PER_CCM_NATIVE_TRANSFER,
	MIN_COMPUTE_LIMIT_PER_CCM_TOKEN_TRANSFER,
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
pub struct SolanaInstructionBuilder;

impl SolanaInstructionBuilder {
	/// Finalize a Instruction Set. This should be internally called after a instruction set is
	/// complete. This will add some extra instruction required for the integrity of the Solana
	/// Transaction.
	///
	/// Returns the finished Instruction Set to construct the SolTransaction.
	fn finalize(
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

		if compute_price > 0 {
			final_instructions
				.push(ComputeBudgetInstruction::set_compute_unit_price(compute_price));
		}
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
						Ok(VaultProgram::with_id(sol_api_environment.vault_program).fetch_native(
							param.deposit_fetch_id.channel_id.to_le_bytes().to_vec(),
							param.deposit_fetch_id.bump,
							sol_api_environment.vault_program_data_account,
							agg_key,
							param.deposit_fetch_id.address,
							derive_fetch_account(
								param.deposit_fetch_id.address,
								sol_api_environment.vault_program,
							)
							.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?
							.address,
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
							derive_fetch_account(ata.address, sol_api_environment.vault_program)
								.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?
								.address,
							system_program_id(),
						))
					},
				}
			})
			.collect::<Result<Vec<_>, SolanaTransactionBuildingError>>()?;

		Self::finalize(
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

		Self::finalize(
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

		Self::finalize(
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

		Self::finalize(
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

		Self::finalize(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			Self::calculate_gas_limit(gas_budget, compute_price, SolAsset::Sol),
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

		Self::finalize(
			instructions,
			durable_nonce,
			agg_key.into(),
			compute_price,
			Self::calculate_gas_limit(gas_budget, compute_price, SolAsset::SolUsdc),
		)
	}

	fn calculate_gas_limit(
		gas_budget: SolAmount,
		compute_price: SolAmount,
		asset: SolAsset,
	) -> SolComputeLimit {
		let budget_after_signature = gas_budget.saturating_sub(LAMPORTS_PER_SIGNATURE);
		if compute_price == 0 {
			return DEFAULT_COMPUTE_UNITS_PER_CCM_TRANSFER;
		}
		let compute_budget =
			// Budget is in lamports, compute price is in microlamports/CU
			sp_std::cmp::min(
				MAX_COMPUTE_UNITS_PER_TRANSACTION as u128,
				(budget_after_signature as u128 * MICROLAMPORTS_PER_LAMPORT as u128)
					/ (compute_price as u128),
			) as SolComputeLimit;

		sp_std::cmp::max(
			compute_budget,
			match asset {
				SolAsset::Sol => MIN_COMPUTE_LIMIT_PER_CCM_NATIVE_TRANSFER,
				SolAsset::SolUsdc => MIN_COMPUTE_LIMIT_PER_CCM_TOKEN_TRANSFER,
			},
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
			sol_tx_core::{
				address_derivation::derive_deposit_address, signer::Signer, sol_test_values::*,
			},
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

	fn agg_key() -> SolAddress {
		SolSigningKey::from_bytes(&RAW_KEYPAIR)
			.expect("Key pair generation must succeed")
			.pubkey()
			.into()
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
		let transaction = SolanaInstructionBuilder::fetch_from(
			vec![get_fetch_params(None, SOL)],
			api_env(),
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_fetch_native` test
		let expected_serialized_tx = hex_literal::hex!("0148d83989bd4eb7bfe89c29af09ffb7e88901cf1065914ec7623382b2257cabc21acdd8d8bc095ca733267f22bbc75ecaed11d3e7774c6b6f87d5146ca1b37c0301000509f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19233306d43f017cdb7b1a324afdc62c79317d5b93e2e63b870143344134db9c600606b9a783a1a2f182b11e9663561cde6ebc2a7d83e97922c214e25284519a68800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004040301060004040000000500090340420f000000000005000502875a000008050700020304158e24658f6c59298c080000000b0c0d3700000000ff").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_batch_fetch_native_transaction() {
		// Use valid Deposit channel derived from `channel_id`
		let fetch_param_0 = get_fetch_params(Some(0), SOL);
		let fetch_param_1 = get_fetch_params(Some(1), SOL);

		// Construct the batch fetch instruction set
		let transaction = SolanaInstructionBuilder::fetch_from(
			vec![fetch_param_0, fetch_param_1],
			api_env(),
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_fetch_native_in_batch` test
		let expected_serialized_tx = hex_literal::hex!("019e3bef60c12bbb5ba315e3768a735415d6aea628502c685911e0caf72c1e620e309fa75c7069792bda27eba0ddca79beeb588077c6e34ea51e93fa01a93be3010100050bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921e2fb5dc3bc76acc1a86ef6457885c32189c53b1db8a695267fed8f8d6921ec457965dbc726e7fe35896f2bf0b9c965ebeb488cb0534aed3a6bb35f6343f503c8c21729498a6919298e0c953bd5fc297329663d413cbaac7799a79bd75f7df47ffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588be00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005060301080004040000000700090340420f0000000000070005026bb200000a050900050406158e24658f6c59298c080000000000000000000000fe0a050900020306158e24658f6c59298c080000000100000000000000ff").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_fetch_token_transaction() {
		// Construct the fetch instruction set
		let transaction = SolanaInstructionBuilder::fetch_from(
			vec![get_fetch_params(Some(0u64), USDC)],
			api_env(),
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_fetch_tokens` test
		let expected_serialized_tx = hex_literal::hex!("01f6faa1394ebce55db0f4b4887818c48d54d0be2dc4ece0eb6d4e411f1204d629a7115f43aaa9c0e7fb77244f0bc30db744f63ed5e0391730aab43ca8a8ca8d020100080df79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925f2c4cda9625242d4cc2e114789f8a6b1fcc7b36decda03a639919cdce0be871dd6e0fc50e3b853cb77f36ec4fff9c847d1b12f83ae2535aa98f2bd1d627ad08e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8772b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588bec27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004050301070004040000000600090340420f0000000000060005024f0a01000b0909000c02040a08030516494710642cb0c646080000000000000000000000fe06").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_fetch_mixed_asset_multiple_transaction() {
		let transaction = SolanaInstructionBuilder::fetch_from(
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
		let expected_serialized_tx = hex_literal::hex!("0150c470c3e09a4a75b745c238eeb19bac147b466e83e340dcbdd9eec04cbfad0f6452c138d5bbc9871bae658c145892e77ee330af7c710b465713fc2201dd180e01000912f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19234ba473530acb5fe214bcf1637a95dd9586131636adc3a27365264e64025a91c55268e2506656a8aafc4689443bad81d0ca129f134075303ca77eefefc1b3b395f2c4cda9625242d4cc2e114789f8a6b1fcc7b36decda03a639919cdce0be871839f5b31e9ce2282c92310f62fa5e69302a0ae2e28ba1b99b0e7d57c10ab84c6bd306154bf886039adbb6f2126a02d730889b6d320507c74f5c0240c8c406454dd6e0fc50e3b853cb77f36ec4fff9c847d1b12f83ae2535aa98f2bd1d627ad08e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee871e2fb5dc3bc76acc1a86ef6457885c32189c53b1db8a695267fed8f8d6921ec472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588bec27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900060903010b0004040000000a00090340420f00000000000a000502df69020010090d001104080e0c070916494710642cb0c646080000000000000000000000fe0610090d000f05080e0c020916494710642cb0c646080000000100000000000000ff0610050d00030609158e24658f6c59298c080000000200000000000000ff").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_create_transfer_native_transaction() {
		let transaction = SolanaInstructionBuilder::transfer_native(
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

		let transaction = SolanaInstructionBuilder::transfer_token(
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
		let transaction = SolanaInstructionBuilder::rotate_agg_key(
			new_agg_key,
			nonce_accounts(),
			env.vault_program,
			env.vault_program_data_account,
			agg_key(),
			durable_nonce(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_full_rotation` test
		let expected_serialized_tx = hex_literal::hex!("0180d9ae78d86dbf0895772b959d27110d09d8cb0f9bb388cbc84a53372b568ea56cb9f235f05bf59446a18b9e9babdf61e7194cd6f838d6fd6a741e6f60cc300d01000411f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb0e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0917eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1926744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900448541f57201f277c5f3ffb631d0212e26e7f47749c26c4808718174a0ab2a09a18cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adbcd644e45426a41a7cb8369b8a0c1c89bb3f86cf278fdd9cc38b0f69784ad5667e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864e5e1869817a4fd88ddf7ab7a5f7252d7c345b39721769888608592912e8ca9acf0f13460b3fd04b7d53d7421fc874ec00eec769cf36480895e1a407bf1249475f2b2e24122be016983be9369965246cc45e1f621d40fba300c56c7ac50c3874df4f83bd213a59c9785110cf83c718f9486c3484f918593bce20c61dc6a96036afecc89e3b031824af6363174d19bbec12d3a13c4a173e5aeb349b63042bc138f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000e0d03020f0004040000000e00090340420f00000000000e000502e02e000010040100030d094e518fabdda5d68b000d02020024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020b0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02090024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020a0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02070024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02060024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02040024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020c0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02080024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02050024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

		test_constructed_transaction(transaction, expected_serialized_tx);
	}

	#[test]
	fn can_calculate_gas_limit() {
		const TEST_EGRESS_BUDGET: SolAmount = 500_000;
		const TEST_COMPUTE_PRICE: SolAmount = 2_000_000;

		let compute_price_lamports = TEST_COMPUTE_PRICE.div_ceil(MICROLAMPORTS_PER_LAMPORT.into());
		for asset in &[SolAsset::Sol, SolAsset::SolUsdc] {
			let mut tx_compute_limit: u32 = SolanaInstructionBuilder::calculate_gas_limit(
				TEST_EGRESS_BUDGET * compute_price_lamports + LAMPORTS_PER_SIGNATURE,
				TEST_COMPUTE_PRICE,
				*asset,
			);
			assert_eq!(tx_compute_limit as u64, TEST_EGRESS_BUDGET);

			// Rounded down
			assert_eq!(
				SolanaInstructionBuilder::calculate_gas_limit(
					(TEST_EGRESS_BUDGET + 1) as SolAmount + LAMPORTS_PER_SIGNATURE,
					(MICROLAMPORTS_PER_LAMPORT * 10) as SolAmount,
					*asset,
				),
				SolanaInstructionBuilder::calculate_gas_limit(
					(TEST_EGRESS_BUDGET + 9) as SolAmount + LAMPORTS_PER_SIGNATURE,
					(MICROLAMPORTS_PER_LAMPORT * 10) as SolAmount,
					*asset,
				)
			);
			assert_eq!(
				SolanaInstructionBuilder::calculate_gas_limit(
					(TEST_EGRESS_BUDGET + 1) as SolAmount * compute_price_lamports +
						LAMPORTS_PER_SIGNATURE,
					(MICROLAMPORTS_PER_LAMPORT * 10) as SolAmount,
					*asset,
				),
				SolanaInstructionBuilder::calculate_gas_limit(
					TEST_EGRESS_BUDGET as SolAmount * compute_price_lamports +
						LAMPORTS_PER_SIGNATURE,
					(MICROLAMPORTS_PER_LAMPORT * 10) as SolAmount,
					*asset,
				)
			);

			// Test SolComputeLimit saturation
			assert_eq!(
				SolanaInstructionBuilder::calculate_gas_limit(
					(SolComputeLimit::MAX as SolAmount) * 2 * compute_price_lamports +
						LAMPORTS_PER_SIGNATURE,
					TEST_COMPUTE_PRICE,
					*asset,
				),
				MAX_COMPUTE_UNITS_PER_TRANSACTION
			);

			// Test upper cap
			tx_compute_limit = SolanaInstructionBuilder::calculate_gas_limit(
				MAX_COMPUTE_UNITS_PER_TRANSACTION as u64 * compute_price_lamports * 2,
				TEST_COMPUTE_PRICE,
				*asset,
			);
			assert_eq!(tx_compute_limit, MAX_COMPUTE_UNITS_PER_TRANSACTION);

			tx_compute_limit =
				SolanaInstructionBuilder::calculate_gas_limit(TEST_EGRESS_BUDGET, 0, *asset);
			assert_eq!(tx_compute_limit, DEFAULT_COMPUTE_UNITS_PER_CCM_TRANSFER);
		}

		// Test lower cap
		let mut tx_compute_limit =
			SolanaInstructionBuilder::calculate_gas_limit(10u64, 1, SolAsset::Sol);
		assert_eq!(tx_compute_limit, MIN_COMPUTE_LIMIT_PER_CCM_NATIVE_TRANSFER);

		tx_compute_limit =
			SolanaInstructionBuilder::calculate_gas_limit(10u64, 1, SolAsset::SolUsdc);
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

		let transaction = SolanaInstructionBuilder::ccm_transfer_native(
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

		let transaction = SolanaInstructionBuilder::ccm_transfer_token(
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
	fn transactions_above_max_lengths_will_fail() {
		// with 28 Fetches, the length is 1232 <= 1232
		assert_ok!(SolanaInstructionBuilder::fetch_from(
			[get_fetch_params(None, SOL); 28].to_vec(),
			api_env(),
			agg_key(),
			durable_nonce(),
			compute_price(),
		));

		assert_err!(
			SolanaInstructionBuilder::fetch_from(
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
