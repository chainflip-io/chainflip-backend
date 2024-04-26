use super::sol_tx_building_blocks::{
	AccountMeta, Instruction, Pubkey, SYSTEM_PROGRAM_ID, UPGRADE_MANAGER_PROGRAM, VAULT_PROGRAM,
};
use crate::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};
use core::str::FromStr;
use scale_info::prelude::string::String;
use serde::{Deserialize, Serialize};
use sp_io::hashing::sha2_256;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum SystemProgramInstruction {
	/// Create a new account
	///
	/// # Account references
	///   0. `[WRITE, SIGNER]` Funding account
	///   1. `[WRITE, SIGNER]` New account
	CreateAccount {
		/// Number of lamports to transfer to the new account
		lamports: u64,

		/// Number of bytes of memory to allocate
		space: u64,

		/// Address of program that will own the new account
		owner: Pubkey,
	},

	/// Assign account to a program
	///
	/// # Account references
	///   0. `[WRITE, SIGNER]` Assigned account public key
	Assign {
		/// Owner program account
		owner: Pubkey,
	},

	/// Transfer lamports
	///
	/// # Account references
	///   0. `[WRITE, SIGNER]` Funding account
	///   1. `[WRITE]` Recipient account
	Transfer { lamports: u64 },

	/// Create a new account at an address derived from a base pubkey and a seed
	///
	/// # Account references
	///   0. `[WRITE, SIGNER]` Funding account
	///   1. `[WRITE]` Created account
	///   2. `[SIGNER]` (optional) Base account; the account matching the base Pubkey below must be
	///      provided as a signer, but may be the same as the funding account and provided as
	///      account 0
	CreateAccountWithSeed {
		/// Base public key
		base: Pubkey,

		/// String of ASCII chars, no longer than `Pubkey::MAX_SEED_LEN`
		seed: String,

		/// Number of lamports to transfer to the new account
		lamports: u64,

		/// Number of bytes of memory to allocate
		space: u64,

		/// Owner program account address
		owner: Pubkey,
	},

	/// Consumes a stored nonce, replacing it with a successor
	///
	/// # Account references
	///   0. `[WRITE]` Nonce account
	///   1. `[]` RecentBlockhashes sysvar
	///   2. `[SIGNER]` Nonce authority
	AdvanceNonceAccount,

	/// Withdraw funds from a nonce account
	///
	/// # Account references
	///   0. `[WRITE]` Nonce account
	///   1. `[WRITE]` Recipient account
	///   2. `[]` RecentBlockhashes sysvar
	///   3. `[]` Rent sysvar
	///   4. `[SIGNER]` Nonce authority
	///
	/// The `u64` parameter is the lamports to withdraw, which must leave the
	/// account balance above the rent exempt reserve or at zero.
	WithdrawNonceAccount(u64),

	/// Drive state of Uninitialized nonce account to Initialized, setting the nonce value
	///
	/// # Account references
	///   0. `[WRITE]` Nonce account
	///   1. `[]` RecentBlockhashes sysvar
	///   2. `[]` Rent sysvar
	///
	/// The `Pubkey` parameter specifies the entity authorized to execute nonce
	/// instruction on the account
	///
	/// No signatures are required to execute this instruction, enabling derived
	/// nonce account addresses
	InitializeNonceAccount(Pubkey),

	/// Change the entity authorized to execute nonce instructions on the account
	///
	/// # Account references
	///   0. `[WRITE]` Nonce account
	///   1. `[SIGNER]` Nonce authority
	///
	/// The `Pubkey` parameter identifies the entity to authorize
	AuthorizeNonceAccount { new_authorized_pubkey: Pubkey },

	/// Allocate space in a (possibly new) account without funding
	///
	/// # Account references
	///   0. `[WRITE, SIGNER]` New account
	Allocate {
		/// Number of bytes of memory to allocate
		space: u64,
	},

	/// Allocate space for and assign an account at an address
	///    derived from a base public key and a seed
	///
	/// # Account references
	///   0. `[WRITE]` Allocated account
	///   1. `[SIGNER]` Base account
	AllocateWithSeed {
		/// Base public key
		base: Pubkey,

		/// String of ASCII chars, no longer than `pubkey::MAX_SEED_LEN`
		seed: String,

		/// Number of bytes of memory to allocate
		space: u64,

		/// Owner program account
		owner: Pubkey,
	},

	/// Assign account to a program based on a seed
	///
	/// # Account references
	///   0. `[WRITE]` Assigned account
	///   1. `[SIGNER]` Base account
	AssignWithSeed {
		/// Base public key
		base: Pubkey,

		/// String of ASCII chars, no longer than `pubkey::MAX_SEED_LEN`
		seed: String,

		/// Owner program account
		owner: Pubkey,
	},

	/// Transfer lamports from a derived address
	///
	/// # Account references
	///   0. `[WRITE]` Funding account
	///   1. `[SIGNER]` Base for funding account
	///   2. `[WRITE]` Recipient account
	TransferWithSeed {
		/// Amount to transfer
		lamports: u64,

		/// Seed to use to derive the funding account address
		from_seed: String,

		/// Owner to use to derive the funding account address
		from_owner: Pubkey,
	},

	/// One-time idempotent upgrade of legacy nonce versions in order to bump
	/// them out of chain blockhash domain.
	///
	/// # Account references
	///   0. `[WRITE]` Nonce account
	UpgradeNonceAccount,
}

impl SystemProgramInstruction {
	pub fn advance_nonce_account(nonce_pubkey: &Pubkey, authorized_pubkey: &Pubkey) -> Instruction {
		let account_metas = vec![
			AccountMeta::new(*nonce_pubkey, false),
			// the public key for RecentBlockhashes system variable.
			//
			// NOTE: According to the solana sdk, this system variable is deprecated and should not
			// be used. However, within the sdk itself they are still using this variable in the
			// advance_nonce_account function so we use it here aswell. This should be revisited to
			// make sure it is ok to use it, or if there is another way to advance the nonce
			// account.
			AccountMeta::new_readonly(
				Pubkey::from_str("SysvarRecentB1ockHashes11111111111111111111").unwrap(),
				false,
			),
			AccountMeta::new_readonly(*authorized_pubkey, true),
		];
		Instruction::new_with_bincode(
			// program id of the system program
			Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(),
			&Self::AdvanceNonceAccount,
			account_metas,
		)
	}

	pub fn nonce_authorize(
		nonce_pubkey: &Pubkey,
		authorized_pubkey: &Pubkey,
		new_authorized_pubkey: &Pubkey,
	) -> Instruction {
		let account_metas = vec![
			AccountMeta::new(*nonce_pubkey, false),
			AccountMeta::new_readonly(*authorized_pubkey, true),
		];
		Instruction::new_with_bincode(
			// program id of the system program
			Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(),
			&Self::AuthorizeNonceAccount { new_authorized_pubkey: *new_authorized_pubkey },
			account_metas,
		)
	}

	pub fn transfer(from_pubkey: &Pubkey, to_pubkey: &Pubkey, lamports: u64) -> Instruction {
		let account_metas =
			vec![AccountMeta::new(*from_pubkey, true), AccountMeta::new(*to_pubkey, false)];
		Instruction::new_with_bincode(
			Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(),
			&Self::Transfer { lamports },
			account_metas,
		)
	}
}

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Eq)]
pub enum VaultProgram {
	FetchNative {
		seed: Vec<u8>,
		bump: u8,
	},
	RotateAggKey {
		skip_transfer_funds: bool,
	},
	FetchTokens {
		seed: Vec<u8>,
		bump: u8,
		amount: u64,
		decimals: u8,
	},
	TransferTokens {
		amount: u64,
		decimals: u8,
	},
	ExecuteCcmNativeCall {
		source_chain: u32,
		source_address: Vec<u8>,
		message: Vec<u8>,
		amount: u64,
	},
	ExecuteCcmTokenCall {
		source_chain: u32,
		source_address: Vec<u8>,
		message: Vec<u8>,
		amount: u64,
	},
}

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Eq)]
pub enum UpgradeManagerProgram {
	UpgradeVaultProgram { seed: Vec<u8>, bump: u8 },
	TransferVaultUpgradeAuthority { seed: Vec<u8>, bump: u8 },
}

pub trait ProgramInstruction: BorshSerialize {
	fn get_program_id(&self) -> &str;

	fn get_instruction(&self, accounts: Vec<AccountMeta>) -> Instruction {
		let mut instruction = Instruction::new_with_borsh(
			Pubkey::from_str(self.get_program_id()).unwrap(),
			&self,
			accounts,
		);
		instruction.data.remove(0);
		let mut data = self.function_discriminator();
		data.append(&mut instruction.data);
		instruction.data = data;
		instruction
	}

	fn function_discriminator(&self) -> Vec<u8> {
		sha2_256((String::from_str("global:").unwrap() + self.call_name()).as_bytes())[..8].to_vec()
	}

	fn call_name(&self) -> &str;
}

impl ProgramInstruction for VaultProgram {
	fn get_program_id(&self) -> &str {
		VAULT_PROGRAM
	}

	fn call_name(&self) -> &str {
		match self {
			Self::FetchNative { seed: _, bump: _ } => "fetch_native",
			Self::RotateAggKey { skip_transfer_funds: _ } => "rotate_agg_key",
			Self::FetchTokens { seed: _, bump: _, amount: _, decimals: _ } => "fetch_tokens",
			Self::TransferTokens { amount: _, decimals: _ } => "transfer_tokens",
			Self::ExecuteCcmNativeCall {
				source_chain: _,
				source_address: _,
				message: _,
				amount: _,
			} => "execute_ccm_native_call",
			Self::ExecuteCcmTokenCall {
				source_chain: _,
				source_address: _,
				message: _,
				amount: _,
			} => "execute_ccm_token_call",
		}
	}
}

impl ProgramInstruction for UpgradeManagerProgram {
	fn get_program_id(&self) -> &str {
		UPGRADE_MANAGER_PROGRAM
	}

	fn call_name(&self) -> &str {
		match self {
			Self::UpgradeVaultProgram { seed: _, bump: _ } => "upgrade_vault_program",
			Self::TransferVaultUpgradeAuthority { seed: _, bump: _ } =>
				"transfer_vault_upgrade_authority",
		}
	}
}
