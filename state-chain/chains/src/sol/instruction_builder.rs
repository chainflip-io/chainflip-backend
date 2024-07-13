//! This file contains a Instruction builder for the Solana chain.
//!
//! The builder provides a interface for the API to create raw Solana
//! Instructions and Instruction sets with some level of abstraction.
//! This avoids the need to deal with low level Solana core types.

use sol_prim::consts::{
	SOL_USDC_DECIMAL, SYSTEM_PROGRAM_ID, SYS_VAR_INSTRUCTIONS, TOKEN_PROGRAM_ID,
};

use crate::{
	sol::{
		api::SolanaTransactionBuildingError,
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
		SolInstruction, SolPubkey, Solana,
	},
	FetchAssetParams, ForeignChainAddress,
};
use sp_std::{vec, vec::Vec};

use super::{
	compute_units_costs::{
		DEFAULT_COMPUTE_UNITS_PER_CCM_TRANSFER, MIN_COMPUTE_LIMIT_PER_CCM_NATIVE_TRANSFER,
		MIN_COMPUTE_LIMIT_PER_CCM_TOKEN_TRANSFER,
	},
	LAMPORTS_PER_SIGNATURE, MAX_COMPUTE_UNITS_PER_TRANSACTION, MICROLAMPORTS_PER_LAMPORT,
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
	// TODO: Shares code with EVM's ExecutexSwapAndCall or make it a function of ForeignChainAddress
	fn destructure_address(source_address: Option<ForeignChainAddress>) -> Vec<u8> {
		match source_address {
			None => vec![],
			Some(ForeignChainAddress::Eth(source_address)) => source_address.0.to_vec(),
			Some(ForeignChainAddress::Dot(source_address)) => source_address.aliased_ref().to_vec(),
			Some(ForeignChainAddress::Btc(script)) => script.bytes(),
			Some(ForeignChainAddress::Arb(source_address)) => source_address.0.to_vec(),
			Some(ForeignChainAddress::Sol(source_address)) => source_address.0.to_vec(),
		}
	}
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
		compute_limit: SolComputeLimit,
	) -> Vec<SolInstruction> {
		let mut final_instructions =
			vec![SystemProgramInstruction::advance_nonce_account(&nonce_account, &agg_key)];

		if compute_price > 0 {
			final_instructions
				.push(ComputeBudgetInstruction::set_compute_unit_price(compute_price));
		}
		final_instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(compute_limit));

		final_instructions.append(&mut instructions);

		final_instructions
	}

	/// Create an instruction set to fetch from each `deposit_channel` being passed in.
	/// Used to batch fetch from multiple deposit channels in a single transaction.
	pub fn fetch_from(
		fetch_params: Vec<FetchAssetParams<Solana>>,
		sol_api_environment: SolApiEnvironment,
		agg_key: SolAddress,
		nonce_account: SolAddress,
		compute_price: SolAmount,
	) -> Result<Vec<SolInstruction>, SolanaTransactionBuildingError> {
		let mut compute_limit: SolComputeLimit = BASE_COMPUTE_UNITS_PER_TX;
		let maybe_instructions = fetch_params
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
			.collect::<Result<Vec<_>, SolanaTransactionBuildingError>>();

		maybe_instructions.map(|instructions| {
			Self::finalize(
				instructions,
				nonce_account.into(),
				agg_key.into(),
				compute_price,
				compute_limit_with_buffer(compute_limit),
			)
		})
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

		Self::finalize(
			instructions,
			nonce_account.into(),
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
		nonce_account: SolAddress,
		compute_price: SolAmount,
		token_decimals: u8,
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
			nonce_account.into(),
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

		Self::finalize(
			instructions,
			nonce_account.into(),
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
		nonce_account: SolAddress,
		compute_price: SolAmount,
		gas_budget: SolAmount,
	) -> Vec<SolInstruction> {
		let instructions = vec![
			SystemProgramInstruction::transfer(&agg_key.into(), &to.into(), amount),
			VaultProgram::with_id(vault_program)
				.execute_ccm_native_call(
					source_chain as u32,
					Self::destructure_address(source_address),
					message,
					amount,
					vault_program_data_account,
					agg_key,
					to,
					ccm_accounts.cf_receiver.clone(),
					system_program_id(),
					sys_var_instructions(),
				)
				.with_remaining_accounts(ccm_accounts.remaining_account_metas()),
		];

		Self::finalize(
			instructions,
			nonce_account.into(),
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
		nonce_account: SolAddress,
		compute_price: SolAmount,
		token_decimals: u8,
		gas_budget: SolAmount,
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
			Self::destructure_address(source_address),
			message,
			amount,
			vault_program_data_account,
			agg_key,
			ata,
			ccm_accounts.cf_receiver.clone(),
			token_program_id(),
			token_mint_pubkey,
			sys_var_instructions(),
		).with_remaining_accounts(ccm_accounts.remaining_account_metas())];

		Self::finalize(
			instructions,
			nonce_account.into(),
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
			// Budget is in lamports, compute price is in CU/microlamport
			std::cmp::min(
				MAX_COMPUTE_UNITS_PER_TRANSACTION as u128,
				(budget_after_signature as u128 * MICROLAMPORTS_PER_LAMPORT as u128)
					.div_ceil(compute_price as u128),
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

	use super::*;
	use crate::{
		sol::{
			signing_key::SolSigningKey,
			sol_tx_core::{
				address_derivation::derive_deposit_address, signer::Signer, sol_test_values::*,
			},
			SolHash, SolMessage, SolTransaction, SolanaDepositFetchId,
		},
		TransferAssetParams,
	};

	use sol_prim::{
		consts::{MAX_TRANSACTION_LENGTH, SOL_USDC_DECIMAL},
		DerivedAta,
	};

	// Arbitrary number used for testing
	const TEST_COMPUTE_LIMIT: SolComputeLimit = 300_000u32;

	fn get_fetch_params(
		channel_id: Option<ChannelId>,
		asset: SolAsset,
	) -> crate::FetchAssetParams<Solana> {
		let channel_id = channel_id.unwrap_or(923_601_931u64);
		let DerivedAta { address, bump } =
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

	fn nonce_account() -> SolAddress {
		NONCE_ACCOUNTS[0]
	}

	fn durable_nonce() -> SolHash {
		TEST_DURABLE_NONCE
	}

	fn api_env() -> SolApiEnvironment {
		SolApiEnvironment {
			vault_program: VAULT_PROGRAM,
			vault_program_data_account: VAULT_PROGRAM_DATA_ACCOUNT,
			token_vault_pda_account: TOKEN_VAULT_PDA_ACCOUNT,
			usdc_token_mint_pubkey: MINT_PUB_KEY,
			usdc_token_vault_ata: TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
		}
	}

	fn compute_price() -> SolAmount {
		COMPUTE_UNIT_PRICE
	}

	fn nonce_accounts() -> Vec<SolAddress> {
		NONCE_ACCOUNTS.to_vec()
	}

	#[track_caller]
	fn test_constructed_instruction_set(
		instruction_set: Vec<SolInstruction>,
		expected_serialized_tx: Vec<u8>,
	) {
		// Obtain required info from Chain Environment
		let durable_nonce = durable_nonce().into();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
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
			vec![get_fetch_params(None, SOL)],
			api_env(),
			agg_key(),
			nonce_account(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_fetch_native` test
		let expected_serialized_tx = hex_literal::hex!("0131fa5abf0d61f42dbe7ebdd7caa7ff0a7eb8045f50d3b2fc9f5c155f7eb3caa05cc4463ad31eff2c6b3b29d57dc21dfc116ac635e6e79be618462bdf8de2420601000509f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19233306d43f017cdb7b1a324afdc62c79317d5b93e2e63b870143344134db9c600606b9a783a1a2f182b11e9663561cde6ebc2a7d83e97922c214e25284519a68800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004040301060004040000000500090340420f000000000005000502a659000008050700020304158e24658f6c59298c080000000b0c0d3700000000ff").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_batch_fetch_native_instruction_set() {
		// Use valid Deposit channel derived from `channel_id`
		let fetch_param_0 = get_fetch_params(Some(0), SOL);
		let fetch_param_1 = get_fetch_params(Some(1), SOL);

		// Construct the batch fetch instruction set
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			vec![fetch_param_0, fetch_param_1],
			api_env(),
			agg_key(),
			nonce_account(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_fetch_native_in_batch` test
		let expected_serialized_tx = hex_literal::hex!("01333ef8b9f08da7362862a1975d0657975f3f68bb030bb3b04ff0e4903800587725e2c51ec756550097b7829bc23c505ad4d844ef736f81cb79dd082cdd8f7f040100050bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921e2fb5dc3bc76acc1a86ef6457885c32189c53b1db8a695267fed8f8d6921ec457965dbc726e7fe35896f2bf0b9c965ebeb488cb0534aed3a6bb35f6343f503c8c21729498a6919298e0c953bd5fc297329663d413cbaac7799a79bd75f7df47ffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588be00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005060301080004040000000700090340420f0000000000070005028ab100000a050900050406158e24658f6c59298c080000000000000000000000fe0a050900020306158e24658f6c59298c080000000100000000000000ff").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_fetch_token_instruction_set() {
		// Construct the fetch instruction set
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			vec![get_fetch_params(Some(0u64), USDC)],
			api_env(),
			agg_key(),
			nonce_account(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_fetch_tokens` test
		let expected_serialized_tx = hex_literal::hex!("01de0de0cd6d813901119d688dc8c48a375c783ebd58cae5d9ba1a8b47c856f809215b3480b65056d41daf5a16d1017557dfdfe24170333063f5d5077941072a0c0100080df79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925f2c4cda9625242d4cc2e114789f8a6b1fcc7b36decda03a639919cdce0be871dd6e0fc50e3b853cb77f36ec4fff9c847d1b12f83ae2535aa98f2bd1d627ad08e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8772b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588bec27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004050301070004040000000600090340420f0000000000060005026e0901000b0909000c02040a08030516494710642cb0c646080000000000000000000000fe06").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_fetch_mixed_asset_multiple_instruction_set() {
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			vec![
				get_fetch_params(Some(0u64), USDC),
				get_fetch_params(Some(1u64), USDC),
				get_fetch_params(Some(2u64), SOL),
			],
			api_env(),
			agg_key(),
			nonce_account(),
			compute_price(),
		)
		.unwrap();

		// Serialized tx built in `create_batch_fetch` test
		let expected_serialized_tx = hex_literal::hex!("01878ca4444163322f509da27602e25ff8ebdc4d9938b71c1f644f50024112f43924a8804b206435571f228729fe8979e95fbf305d349ce6126be3d3438457500901000912f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19234ba473530acb5fe214bcf1637a95dd9586131636adc3a27365264e64025a91c55268e2506656a8aafc4689443bad81d0ca129f134075303ca77eefefc1b3b395f2c4cda9625242d4cc2e114789f8a6b1fcc7b36decda03a639919cdce0be871839f5b31e9ce2282c92310f62fa5e69302a0ae2e28ba1b99b0e7d57c10ab84c6bd306154bf886039adbb6f2126a02d730889b6d320507c74f5c0240c8c406454dd6e0fc50e3b853cb77f36ec4fff9c847d1b12f83ae2535aa98f2bd1d627ad08e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee871e2fb5dc3bc76acc1a86ef6457885c32189c53b1db8a695267fed8f8d6921ec472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cffe38210450436716ebc835b8499c10c957d9fb8c4c8ef5a3c0473cf67b588bec27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900060903010b0004040000000a00090340420f00000000000a000502fe68020010090d001104080e0c070916494710642cb0c646080000000000000000000000fe0610090d000f05080e0c020916494710642cb0c646080000000100000000000000ff0610050d00030609158e24658f6c59298c080000000200000000000000ff").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_transfer_native_instruction_set() {
		let instruction_set = SolanaInstructionBuilder::transfer_native(
			TRANSFER_AMOUNT,
			TRANSFER_TO_ACCOUNT,
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_transfer_native` test
		let expected_serialized_tx = hex_literal::hex!("01303b85f1ede4c907ddf81d4881812c378dbb09c57ec4260e0871f4edaeaf563b402789edd97ef1e5d871d2ac4a160bd66f33ae62759b3bff9e5076ec4762110d01000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090340420f000000000004000502a3020000030200020c0200000000ca9a3b00000000").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_transfer_usdc_token_instruction_set() {
		let env = api_env();
		let to_pubkey = TRANSFER_TO_ACCOUNT;
		let to_pubkey_ata =
			crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
				to_pubkey,
				env.usdc_token_mint_pubkey,
			)
			.unwrap();

		let instruction_set = SolanaInstructionBuilder::transfer_token(
			to_pubkey_ata.address,
			TRANSFER_AMOUNT,
			to_pubkey,
			env.vault_program,
			env.vault_program_data_account,
			env.token_vault_pda_account,
			env.usdc_token_vault_ata,
			env.usdc_token_mint_pubkey,
			agg_key(),
			nonce_account(),
			compute_price(),
			SOL_USDC_DECIMAL,
		);

		// Serialized tx built in `create_transfer_token` test
		let expected_serialized_tx = hex_literal::hex!("0183480d727ce9fa62271a0d363526e8785609b47c9d73aa4afef4025e111d1864fe80f11f1c8deaec487bea2bcd1aa69a10e084af04be192b72f87f2387e1e30e01000a0ef79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec4616e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859ab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301060004040000000500090340420f000000000005000502ba2601000c0600020a09040701010b0708000d030209071136b4eeaf4a557ebc00ca9a3b0000000006").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_rotate_agg_key() {
		let new_agg_key = NEW_AGG_KEY;
		let env = api_env();
		let instruction_set = SolanaInstructionBuilder::rotate_agg_key(
			new_agg_key,
			nonce_accounts(),
			env.vault_program,
			env.vault_program_data_account,
			agg_key(),
			nonce_account(),
			compute_price(),
		);

		// Serialized tx built in `create_full_rotation` test
		let expected_serialized_tx = hex_literal::hex!("0180d9ae78d86dbf0895772b959d27110d09d8cb0f9bb388cbc84a53372b568ea56cb9f235f05bf59446a18b9e9babdf61e7194cd6f838d6fd6a741e6f60cc300d01000411f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb0e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0917eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1926744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900448541f57201f277c5f3ffb631d0212e26e7f47749c26c4808718174a0ab2a09a18cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adbcd644e45426a41a7cb8369b8a0c1c89bb3f86cf278fdd9cc38b0f69784ad5667e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864e5e1869817a4fd88ddf7ab7a5f7252d7c345b39721769888608592912e8ca9acf0f13460b3fd04b7d53d7421fc874ec00eec769cf36480895e1a407bf1249475f2b2e24122be016983be9369965246cc45e1f621d40fba300c56c7ac50c3874df4f83bd213a59c9785110cf83c718f9486c3484f918593bce20c61dc6a96036afecc89e3b031824af6363174d19bbec12d3a13c4a173e5aeb349b63042bc138f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000e0d03020f0004040000000e00090340420f00000000000e000502e02e000010040100030d094e518fabdda5d68b000d02020024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020b0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02090024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020a0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02070024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02060024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02040024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020c0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02080024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02050024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
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

			// Rounded up
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
				) + 1
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
	fn can_create_ccm_native_instruction_set() {
		let ccm_param = ccm_parameter();
		let transfer_param = TransferAssetParams::<Solana> {
			asset: SOL,
			amount: TRANSFER_AMOUNT,
			to: TRANSFER_TO_ACCOUNT,
		};
		let env = api_env();

		let instruction_set = SolanaInstructionBuilder::ccm_transfer_native(
			transfer_param.amount,
			transfer_param.to,
			ccm_param.source_chain,
			ccm_param.source_address,
			ccm_param.channel_metadata.message.to_vec(),
			ccm_accounts(),
			env.vault_program,
			env.vault_program_data_account,
			agg_key(),
			nonce_account(),
			compute_price(),
			(TEST_COMPUTE_LIMIT as u128 * compute_price() as u128)
				.div_ceil(MICROLAMPORTS_PER_LAMPORT.into()) as u64 +
				LAMPORTS_PER_SIGNATURE,
		);

		// Serialized tx built in `create_ccm_native_transfer` test
		let expected_serialized_tx = hex_literal::hex!("019934f0927bb3344080fc333956498280e7ff8959d7ad93e9f894cab5df74223752c3e6fc3607fec8b0a266d36a10b85bf3b9e4ab97f8204924130407c991690c0100070bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd47417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301070004040000000500090340420f000000000005000502e0930400040200020c0200000000ca9a3b0000000009070800020304060a347d050be38042e0b20100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}

	#[test]
	fn can_create_ccm_usdc_token_instruction_set() {
		let env = api_env();
		let ccm_param = ccm_parameter();
		let to = TRANSFER_TO_ACCOUNT;
		let to_ata = crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
			to,
			env.usdc_token_mint_pubkey,
		)
		.unwrap();

		let instruction_set = SolanaInstructionBuilder::ccm_transfer_token(
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
			nonce_account(),
			compute_price(),
			SOL_USDC_DECIMAL,
			(TEST_COMPUTE_LIMIT as u128 * compute_price() as u128)
				.div_ceil(MICROLAMPORTS_PER_LAMPORT.into()) as u64 +
				LAMPORTS_PER_SIGNATURE,
		);

		// Serialized tx built in `create_ccm_token_transfer` test
		let expected_serialized_tx = hex_literal::hex!("01b129476ffae4b80e116ceb457e9da19236c663373bc52d4e7cb5973429fb6157f74ac71e3168a286d7df90a1e259872cb64db6ee84fd6b44d504f529a5e8ea0c01000c11f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec46167417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed48e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5ab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890006050301080004040000000600090340420f000000000006000502e09304000e0600020c0b050901010d070a001004020b091136b4eeaf4a557ebc00ca9a3b00000000060d080a000203090b070f346cb8a27b9fdeaa230100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}
}
