use super::{AccountMeta, Instruction, Pubkey};

use crate::sol::consts::SYSTEM_PROGRAM_ID;
use borsh::BorshSerialize;
use cf_utilities::SliceToArray;
use core::str::FromStr;
use scale_info::prelude::string::String;
use serde::{Deserialize, Serialize};
use sp_std::{vec, vec::Vec};

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
			// advance_nonce_account function so we use it here as well. This should be revisited
			// to make sure it is ok to use it, or if there is another way to advance the nonce
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

pub trait ProgramInstruction: BorshSerialize {
	const CALL_NAME: &'static str;
	const FN_DISCRIMINATOR_HASH: [u8; 32] = sha2_const::Sha256::new()
		.update(b"global:")
		.update(Self::CALL_NAME.as_bytes())
		.finalize();

	fn get_instruction(&self, program_id: Pubkey, accounts: Vec<AccountMeta>) -> Instruction {
		Instruction::new_with_borsh(program_id, &(Self::function_discriminator(), self), accounts)
	}

	fn function_discriminator() -> [u8; 8] {
		Self::FN_DISCRIMINATOR_HASH[..8].as_array::<8>()
	}
}

// TODO: Derive this from ABI JSON instead. (or at least generate tests to ensure it matches)
macro_rules! solana_program {
	(
		$program:ident {
			$(
				$call_name:ident => $call:ident {
					args: [
						$(
							$call_arg:ident: $arg_type:ty
						),*
						$(,)?
					],
					account_metas: [
						$(
							$account:ident: { signer: $is_signer:expr, writable: $is_writable:expr }
						),*
						$(,)?
					]
					$(,)?
				}
			),+ $(,)?
		}
	) => {
		pub struct $program {
			program_id: Pubkey,
		}

		impl $program {
			pub fn with_id(program_id: impl Into<Pubkey>) -> Self {
				Self { program_id: program_id.into() }
			}

			$(
				pub fn $call_name(
					&self,
					$( $call_arg: $arg_type ),+,
					// AccountMetas
					$( $account: impl Into<Pubkey> ),*
				) -> Instruction {
					$call {
						$(
							$call_arg,
						)+
					}.get_instruction(
						self.program_id,
						vec![
							$(
								AccountMeta {
									pubkey: $account.into(),
									is_signer: $is_signer,
									is_writable: $is_writable,
								},
							)*
						]
					)
				}
			)+
		}

		$(
			#[derive(BorshSerialize, Debug, Clone, PartialEq, Eq)]
			pub struct $call {
				$(
					$call_arg: $arg_type,
				)+
			}

			impl ProgramInstruction for $call {
				const CALL_NAME: &'static str = stringify!($call_name);
			}
		)+
	};
}

solana_program!(
	UpgradeManagerProgram {
		upgrade_vault_program => UpgradeVaultProgram {
			args: [
				seed: Vec<u8>,
				bump: u8
			],
			account_metas: [
				vault_program_data_account: { signer: false, writable: false },
				govkey: { signer: true, writable: false },
				vault_program_data_address: { signer: false, writable: true },
				vault_program_address: { signer: false, writable: true },
				buffer_address: { signer: false, writable: true },
				spill_address: { signer: false, writable: true },
				sys_var_rent: { signer: false, writable: false },
				sys_var_clock: { signer: false, writable: false },
				upgrade_manager_pda_signer: { signer: false, writable: false },
				bpf_loader_upgradeable: { signer: false, writable: false },
			]
		},
		transfer_vault_upgrade_authority => TransferVaultUpgradeAuthority {
			args: [
				seed: Vec<u8>,
				bump: u8
			],
			account_metas: [
				vault_program_data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: false },
				vault_program_data_address: { signer: false, writable: true },
				vault_program_address: { signer: false, writable: false },
				new_authority: { signer: false, writable: false },
				signer_pda: { signer: false, writable: false },
				bpf_loader_upgradeable: { signer: false, writable: false },
			]
		},
	}
);

solana_program!(
	VaultProgram {
		fetch_native => FetchNative {
			args: [
				seed: Vec<u8>,
				bump: u8,
			],
			account_metas: [
				vault_program_data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: true },
				deposit_address: { signer: false, writable: true },
				system_program_id: { signer: false, writable: false },
			]
		},
		rotate_agg_key => RotateAggKey {
			args: [
				skip_transfer_funds: bool,
			],
			account_metas: [
				vault_program_data_account: { signer: false, writable: true },
				agg_key: { signer: true, writable: true },
				new_agg_key: { signer: false, writable: true },
				system_program_id: { signer: false, writable: false },
			]
		},
		fetch_tokens => FetchTokens {
			args: [
				seed: Vec<u8>,
				bump: u8,
				amount: u64,
				decimals: u8,
			],
			account_metas: [
				vault_program_data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: false },
				deposit_address: { signer: false, writable: false },
				deposit_address_ata: { signer: false, writable: true },
				token_vault_ata: { signer: false, writable: true },
				mint_pubkey: { signer: false, writable: false },
				token_program_id: { signer: false, writable: false },
				system_program_id: { signer: false, writable: false },
			]
		},
		transfer_tokens => TransferTokens {
			args: [
				amount: u64,
				decimals: u8,
			],
			account_metas: [
				vault_program_data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: false },
				token_vault_pda: { signer: false, writable: false },
				token_vault_ata: { signer: false, writable: true },
				token_destination: { signer: false, writable: true },
				mint_pubkey: { signer: false, writable: false },
				token_program_id: { signer: false, writable: false },
				system_program_id: { signer: false, writable: false },
			]
		},
		execute_ccm_native_call => ExecuteCcmNativeCall {
			args: [
				source_chain: u32,
				source_address: Vec<u8>,
				message: Vec<u8>,
				amount: u64,
			],
			account_metas: [
				vault_program_data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: false },
				destination: { signer: false, writable: true },
				cf_receiver: { signer: false, writable: false },
				system_program_id: { signer: false, writable: false },
				sys_var_instructions: { signer: false, writable: false },
			]
		},
		execute_ccm_token_call => ExecuteCcmTokenCall {
			args: [
				source_chain: u32,
				source_address: Vec<u8>,
				message: Vec<u8>,
				amount: u64,
			],
			account_metas: [
				vault_program_data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: false },
				destination: { signer: false, writable: true },
				cf_receiver: { signer: false, writable: false },
				token_program_id: { signer: false, writable: false },
				mint_pubkey: { signer: false, writable: false },
				sys_var_instructions: { signer: false, writable: false },
				remaining_account: { signer: false, writable: true },
			]
		},
	}
);

// TODO: Pull and compare discriminators and function from the contracts-interfaces
#[test]
fn test_function_discriminators() {
	assert_eq!(
		<RotateAggKey as ProgramInstruction>::function_discriminator(),
		[78u8, 81u8, 143u8, 171u8, 221u8, 165u8, 214u8, 139u8]
	);
	assert_eq!(
		<FetchTokens as ProgramInstruction>::function_discriminator(),
		[73u8, 71u8, 16u8, 100u8, 44u8, 176u8, 198u8, 70u8]
	);
	assert_eq!(
		<TransferTokens as ProgramInstruction>::function_discriminator(),
		[54u8, 180u8, 238u8, 175u8, 74u8, 85u8, 126u8, 188u8]
	);
	assert_eq!(
		<FetchNative as ProgramInstruction>::function_discriminator(),
		[142u8, 36u8, 101u8, 143u8, 108u8, 89u8, 41u8, 140u8]
	);
	assert_eq!(
		<ExecuteCcmNativeCall as ProgramInstruction>::function_discriminator(),
		[125u8, 5u8, 11u8, 227u8, 128u8, 66u8, 224u8, 178u8]
	);
	assert_eq!(
		<ExecuteCcmTokenCall as ProgramInstruction>::function_discriminator(),
		[108u8, 184u8, 162u8, 123u8, 159u8, 222u8, 170u8, 35u8]
	);
	assert_eq!(
		<UpgradeVaultProgram as ProgramInstruction>::function_discriminator(),
		[72u8, 211u8, 76u8, 189u8, 84u8, 176u8, 62u8, 101u8]
	);
	assert_eq!(
		<TransferVaultUpgradeAuthority as ProgramInstruction>::function_discriminator(),
		[114u8, 247u8, 72u8, 110u8, 145u8, 65u8, 236u8, 153u8]
	);
}
