//! This file contains a Instruction builder for the Solana chain.
//!
//! The builder provides a interface for users to create Raw solana
//! Instructions and Instruction sets with some level of abstraction
//! so the user do not need to deal with low level code in `sol_tx_building_blocks.rs`.

use codec::{Decode, Encode};
use core::str::FromStr;
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;

use sp_std::marker::PhantomData;

use cf_primitives::chains::Solana;

use crate::{
	sol::{
		api::{SolanaEnvAccountLookupKey, SolanaEnvironment},
		compute_budget::ComputeBudgetInstruction,
		consts::SYSTEM_PROGRAM_ID,
		program_instructions::{ProgramInstruction, SystemProgramInstruction, VaultProgram},
		SolAccountMeta, SolInstruction, SolPubkey,
	},
	DepositChannel,
};

/// Errors that can arise when building Solana Instructions.
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum SolanaInstructionBuilderError {
	// The current Aggkey is not set
	CannotLookupAggKey,
	// Cannot obtain an available Nonce Account
	NoAvailableNonceAccount,
	// Failed to lookup Compute Limit
	CannotLookupComputeLimit,
	// Failed to lookup Compute Price
	CannotLookupComputePrice,
	// Failed to lookup Vault Program Data Account
	CannotLookupVaultProgramDataAccount,
}

pub struct SolanaInstructionBuilder<Environment: 'static> {
	instructions: Vec<SolInstruction>,
	_phantom: PhantomData<Environment>,
}

impl<Environment> Default for SolanaInstructionBuilder<Environment> {
	fn default() -> Self {
		Self { instructions: Default::default(), _phantom: Default::default() }
	}
}

impl<Environment: SolanaEnvironment> SolanaInstructionBuilder<Environment> {
	pub fn finalize(mut self) -> Result<Vec<SolInstruction>, SolanaInstructionBuilderError> {
		let mut final_instructions = vec![
			SystemProgramInstruction::advance_nonce_account(
				&Environment::lookup_account(SolanaEnvAccountLookupKey::NextNonceAccount)
					.ok_or(SolanaInstructionBuilderError::NoAvailableNonceAccount)?
					.into(),
				&Environment::lookup_account(SolanaEnvAccountLookupKey::AggKey)
					.ok_or(SolanaInstructionBuilderError::CannotLookupAggKey)?
					.into(),
			),
			ComputeBudgetInstruction::set_compute_unit_price(
				Environment::compute_price()
					.ok_or(SolanaInstructionBuilderError::CannotLookupComputePrice)?,
			),
			ComputeBudgetInstruction::set_compute_unit_limit(
				Environment::compute_limit()
					.ok_or(SolanaInstructionBuilderError::CannotLookupComputeLimit)?,
			),
		];

		final_instructions.append(&mut self.instructions);

		Ok(final_instructions)
	}

	pub fn fetch_from(
		mut self,
		deposit_channels: Vec<DepositChannel<Solana>>,
	) -> Result<Self, SolanaInstructionBuilderError> {
		// Lookup key accounts for building the Fetch instruction
		let vault_program_data_account =
			Environment::lookup_account(SolanaEnvAccountLookupKey::VaultProgramDataAccount)
				.ok_or(SolanaInstructionBuilderError::CannotLookupVaultProgramDataAccount)?
				.into();
		let agg_key = Environment::lookup_account(SolanaEnvAccountLookupKey::AggKey)
			.ok_or(SolanaInstructionBuilderError::CannotLookupAggKey)?
			.into();
		let system_program_id = SolPubkey::from_str(SYSTEM_PROGRAM_ID)
			.expect("Solana's System Program ID must be correct.");

		self.instructions
			.extend(&mut deposit_channels.into_iter().map(|deposit_channel| {
				ProgramInstruction::get_instruction(
					&VaultProgram::FetchNative {
						seed: deposit_channel.state.seed,
						bump: deposit_channel.state.bump,
					},
					vec![
						SolAccountMeta::new_readonly(vault_program_data_account, false),
						SolAccountMeta::new(agg_key, true),
						SolAccountMeta::new(deposit_channel.address.into(), false),
						SolAccountMeta::new_readonly(system_program_id, false),
					],
				)
			}));
		Ok(self)
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		sol::{
			extra_types_for_testing::{Keypair, Signer},
			sol_tx_building_blocks::VAULT_PROGRAM_DATA_ACCOUNT,
			SolAddress, SolAmount, SolComputeLimit, SolHash, SolMessage, SolTransaction,
			SolanaDepositChannelState,
		},
		ChainEnvironment,
	};

	// Test value taken from tests in sol_tx_building_blocks.rs
	const NEXT_NONCE: &str = "2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw";
	const RAW_KEYPAIR: [u8; 64] = [
		6, 151, 150, 20, 145, 210, 176, 113, 98, 200, 192, 80, 73, 63, 133, 232, 208, 124, 81, 213,
		117, 199, 196, 243, 219, 33, 79, 217, 157, 69, 205, 140, 247, 157, 94, 2, 111, 18, 237,
		198, 68, 58, 83, 75, 44, 221, 80, 114, 35, 57, 137, 180, 21, 215, 89, 101, 115, 231, 67,
		243, 229, 179, 134, 251,
	];

	fn get_deposit_channel() -> DepositChannel<Solana> {
		DepositChannel::<Solana> {
			channel_id: 1u64,
			address: SolPubkey::from_str("DWHmaNGBzwMGjb6WP7G2Y6fbLunj6jjqHKjvxGSNo81G")
				.unwrap()
				.into(),
			asset: crate::assets::sol::Asset::Sol,
			state: SolanaDepositChannelState { seed: vec![11u8, 12u8, 13u8, 55u8], bump: 249u8 },
		}
	}

	pub struct MockSolanaEnvironment;
	impl ChainEnvironment<SolanaEnvAccountLookupKey, SolAddress> for MockSolanaEnvironment {
		fn lookup(s: SolanaEnvAccountLookupKey) -> Option<SolAddress> {
			Some(match s {
				SolanaEnvAccountLookupKey::AggKey => Keypair::from_bytes(&RAW_KEYPAIR)
					.expect("Key pair generation must succeed")
					.pubkey()
					.into(),
				SolanaEnvAccountLookupKey::NextNonceAccount =>
					SolAddress::from_str(NEXT_NONCE).unwrap(),
				SolanaEnvAccountLookupKey::VaultProgramDataAccount =>
					SolPubkey::from_str(VAULT_PROGRAM_DATA_ACCOUNT)
						.expect("Vault program data account must be correct")
						.into(),
			})
		}
	}

	impl ChainEnvironment<(), SolComputeLimit> for MockSolanaEnvironment {
		fn lookup(_s: ()) -> Option<u32> {
			Some(300_000u32)
		}
	}

	impl ChainEnvironment<(), SolAmount> for MockSolanaEnvironment {
		fn lookup(_s: ()) -> Option<u64> {
			Some(1_000_000u64)
		}
	}

	impl ChainEnvironment<(), SolHash> for MockSolanaEnvironment {
		fn lookup(_s: ()) -> Option<SolHash> {
			Some(
				SolHash::from_str("E6E2bNxGcgFyqeVRT3FSjw7YFbbMAZVQC21ZLVwrztRm")
					.expect("Blockhash must be valid"),
			)
		}
	}
	impl SolanaEnvironment for MockSolanaEnvironment {}

	#[track_caller]
	fn test_constructed_instruction_set(
		instruction_set: Vec<SolInstruction>,
		expected_serialized_tx: Vec<u8>,
	) {
		// Obtain required info from Chain Environment
		let recent_block = MockSolanaEnvironment::recent_block_hash().unwrap();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();

		// Construct the Transaction and sign it
		let message = SolMessage::new(&instruction_set, Some(&agg_key_pubkey));
		let mut tx = SolTransaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], recent_block.into());

		// println!("{:?}", tx);
		let serialized_tx =
			tx.finalize_and_serialize().expect("Transaction serialization must succeed");

		//println!("tx:{:?}", hex::encode(serialized_tx.clone()));
		assert_eq!(serialized_tx, expected_serialized_tx);
	}

	#[test]
	fn can_create_fetch_instruction_set() {
		// Construct the batch fetch instruction set
		let instruction_set = SolanaInstructionBuilder::<MockSolanaEnvironment>::default()
			.fetch_from(vec![get_deposit_channel()])
			.expect("fetch_from instruction can be built")
			.finalize()
			.expect("Instruction builder's finalize() must succeed");

		// Serialized tx built in can_fetch_native test
		let expected_serialized_tx = hex_literal::hex!("01badc36c70df4ea08ee3e390d55ce1df255fba407b75dd7c0a6c6b82d9ed4d0b77ca8d760820e45fb6f837edd6ed392c340ceaf8757be9246565e0e5f6a163f0201000508f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192b9cd0bfce0d0c993da26980648022f34b2e9a33794312b94eb3f8cad440e3e6b00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000004a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090340420f000000000004000502e0930400070406000203118e24658f6c59298c040000000b0c0d37f9").to_vec();

		test_constructed_instruction_set(instruction_set, expected_serialized_tx);
	}
}
/*
#[test]
fn create_fetch_native() {
	let durable_nonce = Hash::from_str("E6E2bNxGcgFyqeVRT3FSjw7YFbbMAZVQC21ZLVwrztRm").unwrap();
	let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
	let agg_key_pubkey = agg_key_keypair.pubkey();
	let deposit_channel =
		Pubkey::from_str("DWHmaNGBzwMGjb6WP7G2Y6fbLunj6jjqHKjvxGSNo81G").unwrap();
	let compute_unit_price = 100_0000;
	let compute_unit_limit = 300_000;

	let instructions = [
		SystemProgramInstruction::advance_nonce_account(
			&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
			&agg_key_pubkey,
		),
		ComputeBudgetInstruction::set_compute_unit_price(compute_unit_price),
		ComputeBudgetInstruction::set_compute_unit_limit(compute_unit_limit),
		ProgramInstruction::get_instruction(
			&VaultProgram::FetchNative { seed: vec![11u8, 12u8, 13u8, 55u8], bump: 249 },
			vec![
				AccountMeta::new_readonly(
					Pubkey::from_str(VAULT_PROGRAM_DATA_ACCOUNT).unwrap(),
					false,
				),
				AccountMeta::new(agg_key_pubkey, true),
				AccountMeta::new(deposit_channel, false),
				AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(), false),
			],
		),
	];
	let message = Message::new(&instructions, Some(&agg_key_pubkey));
	let mut tx = Transaction::new_unsigned(message);
	tx.sign(&[&agg_key_keypair], durable_nonce);
	println!("{:?}", tx);

	let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();

	// With compute unit price and limit
	let expected_serialized_tx = hex_literal::hex!("01badc36c70df4ea08ee3e390d55ce1df255fba407b75dd7c0a6c6b82d9ed4d0b77ca8d760820e45fb6f837edd6ed392c340ceaf8757be9246565e0e5f6a163f0201000508f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192b9cd0bfce0d0c993da26980648022f34b2e9a33794312b94eb3f8cad440e3e6b00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000004a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090340420f000000000004000502e0930400070406000203118e24658f6c59298c040000000b0c0d37f9").to_vec();

	// Without compute unit price and limit
	// let expected_serialized_tx = hex_literal::hex!("018876d689319695f4e695d1bc6dc71b36cb7e81093d7122aa6153de24aeafe251d589f63321272cb2f195f81acf5617b16994bcdbdc070cfd459a2f76d0c4650701000407f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192b9cd0bfce0d0c993da26980648022f34b2e9a33794312b94eb3f8cad440e3e6b000000000000000000000000000000000000000000000000000000000000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000004a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000203030104000404000000060405000203118e24658f6c59298c040000000b0c0d37f9").to_vec();
	println!("tx:{:?}", hex::encode(serialized_tx.clone()));

	assert_eq!(serialized_tx, expected_serialized_tx);

	let transaction_length = serialized_tx.len();
	println!("Fetch native length:  {} bytes", transaction_length);
	assert!(transaction_length <= MAX_TRANSACTION_LENGTH)
}
 */
