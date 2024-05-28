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
		idl_path: $idl_path:expr,
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

			#[cfg(test)]
			mod $call_name {
				use super::*;
				use $crate::sol::sol_tx_core::program_instructions::idl::*;
				use heck::{ToSnakeCase, ToUpperCamelCase};
				use std::collections::BTreeSet;

				const IDL_RAW: &str = include_str!($idl_path);

				thread_local! {
					static IDL: Idl = serde_json::from_str(IDL_RAW).unwrap();
				}

				fn test(f: impl FnOnce(&Idl)) {
					IDL.with(|idl| {
						f(idl);
					});
				}

				#[test]
				fn program_name() {
					test(|idl| {
						assert_eq!(
							format!("{}_program", idl.metadata.name).to_upper_camel_case(),
							stringify!($program)
						);
					});
				}

				#[test]
				fn discriminator() {
					test(|idl| {
						assert_eq!(
							<$call as ProgramInstruction>::function_discriminator(),
							idl.instruction(stringify!($call_name)).discriminator
						);
					});
				}

				#[test]
				fn $call_name() {
					test(|idl| {
							let instruction = idl.instruction(stringify!($call_name));
							assert!(
								instruction
									.args
									.iter()
									.map(|arg| arg.name.as_str().to_snake_case())
									.collect::<BTreeSet<_>>().is_superset(&BTreeSet::from([
										$(
											stringify!($call_arg).to_owned(),
										)*
									])),
							);
							assert_eq!(
								instruction
									.accounts
									.iter()
									.map(|account| account.name.as_str().to_snake_case())
									.collect::<BTreeSet<_>>(),
								BTreeSet::from([
									$(
										stringify!($account).to_owned(),
									)*
								])
							);
					});
				}

				$(
					#[test]
					fn $call_arg() {
						test(|idl| {
							let idl_arg = idl.instruction(stringify!($call_name)).args
								.iter()
								.find(|arg| arg.name.to_snake_case() == stringify!($call_arg))
								.expect("arg not found in idl");

							assert_eq!(idl_arg.ty.to_string(), stringify!($arg_type));
						});
					}
				)*

				$(
					#[test]
					fn $account() {
						test(|idl| {
							let idl_account = idl.instruction(stringify!($call_name)).accounts
								.iter()
								.find(|account| account.name.to_snake_case() == stringify!($account))
								.expect("account not found in idl");

							assert_eq!(
								idl_account.signer,
								$is_signer,
								"is_signer doesn't match for {}", stringify!($account)
							);
							assert_eq!(
								idl_account.writable,
								$is_writable,
								"is_writable doesn't match for {}", stringify!($account)
							);
						});
					}
				)*
			}
		)+
	};
}

solana_program!(
	idl_path: concat!(
		env!("CF_SOL_PROGRAM_IDL_ROOT"), "/",
		env!("CF_SOL_PROGRAM_IDL_TAG"), "/" ,
		"upgrade_manager.json"
	),
	UpgradeManagerProgram {
		upgrade_vault_program => UpgradeVaultProgram {
			args: [
				seed: Vec<u8>,
				bump: u8
			],
			account_metas: [
				vault_data_account: { signer: false, writable: false },
				gov_key: { signer: true, writable: false },
				program_data_address: { signer: false, writable: true },
				program_address: { signer: false, writable: true },
				buffer_address: { signer: false, writable: true },
				spill_address: { signer: false, writable: true },
				sysvar_rent: { signer: false, writable: false },
				sysvar_clock: { signer: false, writable: false },
				signer_pda: { signer: false, writable: false },
				bpf_loader_upgradeable: { signer: false, writable: false },
			]
		},
		transfer_vault_upgrade_authority => TransferVaultUpgradeAuthority {
			args: [
				seed: Vec<u8>,
				bump: u8
			],
			account_metas: [
				vault_data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: false },
				program_data_address: { signer: false, writable: true },
				program_address: { signer: false, writable: false },
				new_authority: { signer: false, writable: false },
				signer_pda: { signer: false, writable: false },
				bpf_loader_upgradeable: { signer: false, writable: false },
			]
		},
	}
);

solana_program!(
	idl_path: concat!(
		env!("CF_SOL_PROGRAM_IDL_ROOT"), "/",
		env!("CF_SOL_PROGRAM_IDL_TAG"), "/" ,
		"vault.json"
	),
	VaultProgram {
		fetch_native => FetchNative {
			args: [
				seed: Vec<u8>,
				bump: u8,
			],
			account_metas: [
				data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: true },
				deposit_channel_pda: { signer: false, writable: true },
				system_program: { signer: false, writable: false },
			]
		},
		rotate_agg_key => RotateAggKey {
			args: [
				skip_transfer_funds: bool,
			],
			account_metas: [
				data_account: { signer: false, writable: true },
				agg_key: { signer: true, writable: true },
				new_agg_key: { signer: false, writable: true },
				system_program: { signer: false, writable: false },
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
				data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: false },
				deposit_channel_pda: { signer: false, writable: false },
				deposit_channel_associated_token_account: { signer: false, writable: true },
				token_vault_associated_token_account: { signer: false, writable: true },
				mint: { signer: false, writable: false },
				token_program: { signer: false, writable: false },
			]
		},
		transfer_tokens => TransferTokens {
			args: [
				amount: u64,
				decimals: u8,
			],
			account_metas: [
				data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: false },
				token_vault_pda: { signer: false, writable: false },
				token_vault_associated_token_account: { signer: false, writable: true },
				to_token_account: { signer: false, writable: true },
				mint: { signer: false, writable: false },
				token_program: { signer: false, writable: false },
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
				data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: false },
				receiver_token_account: { signer: false, writable: true },
				cf_receiver: { signer: false, writable: false },
				token_program: { signer: false, writable: false },
				mint: { signer: false, writable: false },
				instruction_sysvar: { signer: false, writable: false },
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
				data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: false },
				receiver_native: { signer: false, writable: true },
				cf_receiver: { signer: false, writable: false },
				system_program: { signer: false, writable: false },
				instruction_sysvar: { signer: false, writable: false },
			]
		},
	}
);

#[cfg(test)]
mod idl {
	use serde::{Deserialize, Serialize};

	#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
	#[serde(rename_all = "camelCase")]
	pub struct IdlInstruction {
		pub name: String,
		pub args: Vec<IdlArg>,
		pub accounts: Vec<IdlAccountMeta>,
		pub discriminator: [u8; 8],
	}

	#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
	pub struct IdlArg {
		pub name: String,
		#[serde(rename = "type")]
		pub ty: IdlType,
	}

	#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
	#[serde(rename_all = "camelCase")]
	pub enum IdlType {
		Bytes,
		U8,
		U64,
		U32,
		Bool,
		Pubkey,
		Defined { name: String },
		Option(Box<IdlType>),
	}

	impl std::fmt::Display for IdlType {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			match self {
				IdlType::Bytes => write!(f, "Vec<u8>"),
				IdlType::U8 => write!(f, "u8"),
				IdlType::U64 => write!(f, "u64"),
				IdlType::U32 => write!(f, "u32"),
				IdlType::Bool => write!(f, "bool"),
				IdlType::Pubkey => write!(f, "Pubkey"),
				IdlType::Defined { name } => write!(f, "{}", name),
				IdlType::Option(ty) => write!(f, "Option<{}>", ty),
			}
		}
	}

	#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
	pub struct IdlError {
		pub code: u32,
		pub name: String,
		pub msg: String,
	}

	#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
	#[serde(rename_all = "camelCase")]
	pub struct IdlAccountMeta {
		pub name: String,
		#[serde(default)]
		pub signer: bool,
		#[serde(default)]
		pub writable: bool,
	}

	#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
	#[serde(rename_all = "camelCase")]
	pub struct IdlMetadata {
		pub name: String,
		pub version: String,
		pub spec: String,
		pub description: String,
	}

	#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
	pub struct Idl {
		pub address: String,
		pub metadata: IdlMetadata,
		pub instructions: Vec<IdlInstruction>,
		pub errors: Vec<IdlError>,
	}

	impl Idl {
		pub fn instruction(&self, name: &str) -> &IdlInstruction {
			self.instructions
				.iter()
				.find(|instr| instr.name == name)
				.expect("instruction not found")
		}
	}
}
