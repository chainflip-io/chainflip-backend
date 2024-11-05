use super::{AccountMeta, Instruction, Pubkey};

use borsh::{BorshDeserialize, BorshSerialize};
use cf_utilities::SliceToArray;
use core::str::FromStr;
use scale_info::prelude::string::String;
use serde::{Deserialize, Serialize};
use sol_prim::consts::SYSTEM_PROGRAM_ID;
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
			SYSTEM_PROGRAM_ID.into(),
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
			SYSTEM_PROGRAM_ID.into(),
			&Self::AuthorizeNonceAccount { new_authorized_pubkey: *new_authorized_pubkey },
			account_metas,
		)
	}

	pub fn transfer(from_pubkey: &Pubkey, to_pubkey: &Pubkey, lamports: u64) -> Instruction {
		let account_metas =
			vec![AccountMeta::new(*from_pubkey, true), AccountMeta::new(*to_pubkey, false)];
		Instruction::new_with_bincode(
			SYSTEM_PROGRAM_ID.into(),
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

pub trait InstructionExt {
	fn with_remaining_accounts(self, accounts: Vec<AccountMeta>) -> Self;
}

impl InstructionExt for Instruction {
	fn with_remaining_accounts(mut self, accounts: Vec<AccountMeta>) -> Self {
		self.accounts.extend(accounts);
		self
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
		$(,
			types: [
				$(
					$type_name:ident {
						$(
							$type_arg:ident: $type_arg_type:ty
						),+
						$(,)?
					}
				),+
				$(,)?
			]
		)?
		$(,
			accounts: [
				$(
					{
						$account_type:ident,
						discriminator: $discriminator:expr $(,)?
					}
				),+
				$(,)?
			]
		)?
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
					$( $call_arg: $arg_type, )*
					$( $account: impl Into<AccountMeta>, )*
				) -> Instruction {
					$call {
						$(
							$call_arg,
						)*
					}.get_instruction(
						self.program_id,
						vec![
							$(
								{
									let mut account = $account.into();
									account.is_signer |= $is_signer;
									account.is_writable |= $is_writable;
									account
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
				)*
			}

			impl ProgramInstruction for $call {
				const CALL_NAME: &'static str = stringify!($call_name);
			}
		)+

		$(
			pub mod types {
				use super::*;

				$(
					#[derive(BorshDeserialize, BorshSerialize, Debug, Default, Clone, PartialEq, Eq)]
					pub struct $type_name {
						$(
							pub $type_arg: $type_arg_type,
						)+
					}
				)+
			}
		)?

		$(
			pub mod accounts {
				use super::*;
				$(
					impl super::types::$account_type {
						pub const fn discriminator() -> [u8; 8] {
							$discriminator
						}

						pub fn check_and_deserialize(bytes: &[u8]) -> borsh::io::Result<Self> {
							use borsh::io::{ErrorKind, Error};
							if bytes.len() < ANCHOR_PROGRAM_DISCRIMINATOR_LENGTH {
								return Err(Error::new(ErrorKind::Other, "No account discriminator"));
							}
							let (discriminator, rest) = bytes.split_at(ANCHOR_PROGRAM_DISCRIMINATOR_LENGTH);
							if discriminator != Self::discriminator() {
								return Err(Error::new(ErrorKind::Other, "Unexpected account discriminator"));
							}
							Self::try_from_slice(rest)
						}
					}
				)+
			}
		)?

		#[cfg(test)]
		mod test {
			use super::*;
			use std::collections::BTreeSet;
			use $crate::sol::sol_tx_core::program_instructions::idl::Idl;

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
			fn instructions_exist_in_idl() {
				test(|idl| {
					let defined_in_idl = idl.instructions.iter().map(|instr| instr.name.clone()).collect::<BTreeSet<_>>();
					let defined_in_code = [
						$(
							stringify!($call_name).to_owned(),
						)*
					].into_iter().collect::<BTreeSet<_>>();
					assert!(defined_in_code.is_subset(&defined_in_idl), "Some instructions are not defined in the IDL");
				});
			}


			$(
				#[test]
				fn types_exist_in_idl() {
					use std::collections::BTreeMap;
					test(|idl| {
						$(
							let ty = idl.types.iter().find(|ty| ty.name == stringify!($type_name)).expect("Type not found in IDL").ty.clone();
							assert!(ty.kind == "struct", "Non-struct IDL types not supported.");
							let fields = ty.fields.into_iter().map(|field| (field.name, field.ty)).collect::<BTreeMap<_,_>>();
							$(
								assert_eq!(
									fields.get(stringify!($type_arg)).map(|f| f.to_string()),
									Some(stringify!($type_arg_type).to_owned()),
									"Field {} of type {} not found in IDL",
									stringify!($type_arg),
									stringify!($type_arg_type),
								);
							)+
						)+
					});
				}
			)?
			$(
				#[test]
				fn accounts_exist_in_idl() {
					test(|idl| {
						let defined_in_idl = idl.accounts.iter().map(|acc| acc.name.clone()).collect::<BTreeSet<_>>();
						let defined_in_code = [
							$(
								stringify!($account_type).to_owned(),
							)+
						].into_iter().collect::<BTreeSet<_>>();
						assert!(
							defined_in_code.is_subset(&defined_in_idl),
							"Some accounts are not defined in the IDL: {:?}",
							defined_in_code.difference(&defined_in_idl).cloned().collect::<Vec<_>>()
						);
						$(
							assert_eq!(
								types::$account_type::discriminator(),
								idl.accounts.iter().find(|acc| acc.name == stringify!($account_type)).unwrap().discriminator
							);
						)+
					});
				}
			)?

			$(
				#[cfg(test)]
				mod $call_name {
					use super::*;
					use heck::{ToSnakeCase, ToUpperCamelCase};

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
								assert_eq!(
									instruction
										.args
										.iter()
										.map(|arg| arg.name.as_str().to_snake_case())
										.collect::<Vec<String>>(),
										[
											$(
												stringify!($call_arg).to_owned(),
											)*
										].into_iter().collect::<Vec<String>>(),
									"Arguments don't match for instruction {}",
									stringify!($call_name),
								);
								assert_eq!(
									instruction
										.accounts
										.iter()
										.map(|account| account.name.as_str().to_snake_case())
										.collect::<Vec<String>>(),
									[
										$(
											stringify!($account).to_owned(),
										)*
									].into_iter().collect::<Vec<String>>(),
									"Accounts don't match for instruction {}",
									stringify!($call_name),
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
		}
	};
}

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
				deposit_channel_historical_fetch: { signer: false, writable: true },
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
				decimals: u8,
			],
			account_metas: [
				data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: true },
				deposit_channel_pda: { signer: false, writable: false },
				deposit_channel_associated_token_account: { signer: false, writable: true },
				token_vault_associated_token_account: { signer: false, writable: true },
				mint: { signer: false, writable: false },
				token_program: { signer: false, writable: false },
				deposit_channel_historical_fetch: { signer: false, writable: true },
				system_program: { signer: false, writable: false },
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

		set_gov_key_with_agg_key => SetGovKeyWithAggKey {
			args: [
				new_gov_key: Pubkey,
			],
			account_metas: [
				data_account: { signer: false, writable: true },
				agg_key: { signer: true, writable: false },
			]
		},

		set_gov_key_with_gov_key => SetGovKeyWithGovKey {
			args: [
				new_gov_key: Pubkey,
			],
			account_metas: [
				data_account: { signer: false, writable: true },
				gov_key: { signer: true, writable: false },
			]
		},

		set_suspended_state => SetSuspendedState {
			args: [
				suspend: bool,
				suspend_legacy_swaps: bool,
				suspend_event_swaps: bool,
			],
			account_metas: [
				data_account: { signer: false, writable: true },
				gov_key: { signer: true, writable: false },
			]
		},

		transfer_vault_upgrade_authority => TransferVaultUpgradeAuthority {
			args: [],
			account_metas: [
				data_account: { signer: false, writable: false },
				agg_key: { signer: true, writable: false },
				program_data_address: { signer: false, writable: true },
				program_address: { signer: false, writable: false },
				new_authority: { signer: false, writable: false },
				signer_pda: { signer: false, writable: false },
				bpf_loader_upgradeable: { signer: false, writable: false },
			]
		},

		upgrade_program => UpgradeVaultProgram {
			args: [],
			account_metas: [
				data_account: { signer: false, writable: false },
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
	},
	types: [
		DepositChannelHistoricalFetch {
			amount: u128,
		}
	],
	accounts: [
		{
			DepositChannelHistoricalFetch,
			discriminator: [188, 68, 197, 38, 48, 192, 81, 100],
		},
	]
);

pub const FETCH_ACCOUNT_DISCRIMINATOR: [u8; ANCHOR_PROGRAM_DISCRIMINATOR_LENGTH] =
	types::DepositChannelHistoricalFetch::discriminator();

pub const ANCHOR_PROGRAM_DISCRIMINATOR_LENGTH: usize = 8;

pub mod swap_endpoints {
	use super::*;

	solana_program!(
		idl_path: concat!(
			env!("CF_SOL_PROGRAM_IDL_ROOT"), "/",
			env!("CF_SOL_PROGRAM_IDL_TAG"), "/" ,
			"swap_endpoint.json"
		),
		SwapEndpointProgram {
			close_event_accounts => CloseEventAccounts {
				args: [],
				account_metas: [
					data_account: { signer: false, writable: false },
					agg_key: { signer: true, writable: true },
					swap_endpoint_data_account: { signer: false, writable: true },
				]
			}
		},
		types: [
			CcmParams {
				message: Vec<u8>,
				gas_amount: u64,
			},
			SwapEvent {
				creation_slot: u64,
				sender: Pubkey,
				dst_chain: u32,
				dst_address: Vec<u8>,
				dst_token: u32,
				amount: u64,
				src_token: Option<Pubkey>,
				ccm_parameters: Option<CcmParams>,
				cf_parameters: Vec<u8>,
			},
			SwapEndpointDataAccount {
				historical_number_event_accounts: u128,
				open_event_accounts: Vec<Pubkey>,
			},
		],
		accounts: [
			{
				SwapEvent,
				discriminator: [150, 251, 114, 94, 200, 113, 248, 70],
			},
			{
				SwapEndpointDataAccount,
				discriminator: [79, 152, 191, 225, 128, 108, 11, 139],
			},
		]
	);

	pub const SWAP_EVENT_ACCOUNT_DISCRIMINATOR: [u8; ANCHOR_PROGRAM_DISCRIMINATOR_LENGTH] =
		types::SwapEvent::discriminator();
	pub const SWAP_ENDPOINT_DATA_ACCOUNT_DISCRIMINATOR: [u8; ANCHOR_PROGRAM_DISCRIMINATOR_LENGTH] =
		types::SwapEndpointDataAccount::discriminator();
}

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
		pub ty: IdlFieldType,
	}

	#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
	#[serde(rename_all = "camelCase")]
	pub enum IdlFieldType {
		Bytes,
		U8,
		U16,
		U64,
		U32,
		U128,
		Bool,
		Pubkey,
		Defined { name: String },
		Option(Box<IdlFieldType>),
		Vec(Box<IdlFieldType>),
	}

	impl std::fmt::Display for IdlFieldType {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			match self {
				IdlFieldType::Bytes => write!(f, "Vec<u8>"),
				IdlFieldType::U8 => write!(f, "u8"),
				IdlFieldType::U16 => write!(f, "u16"),
				IdlFieldType::U64 => write!(f, "u64"),
				IdlFieldType::U32 => write!(f, "u32"),
				IdlFieldType::U128 => write!(f, "u128"),
				IdlFieldType::Bool => write!(f, "bool"),
				IdlFieldType::Pubkey => write!(f, "Pubkey"),
				IdlFieldType::Defined { name } => write!(f, "{}", name),
				IdlFieldType::Option(ty) => write!(f, "Option<{}>", ty),
				IdlFieldType::Vec(ty) => write!(f, "Vec<{}>", ty),
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
	#[serde(rename_all = "camelCase")]
	pub struct IdlAccount {
		pub name: String,
		pub discriminator: [u8; 8],
	}

	#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
	#[serde(rename_all = "camelCase")]
	pub struct IdlType {
		pub kind: String,
		pub fields: Vec<IdlArg>,
	}

	#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
	#[serde(rename_all = "camelCase")]
	pub struct IdlTypes {
		pub name: String,
		#[serde(rename = "type")]
		pub ty: IdlType,
	}

	#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
	pub struct Idl {
		pub address: String,
		pub metadata: IdlMetadata,
		pub instructions: Vec<IdlInstruction>,
		pub errors: Vec<IdlError>,
		pub accounts: Vec<IdlAccount>,
		pub types: Vec<IdlTypes>,
	}

	impl Idl {
		pub fn instruction(&self, name: &str) -> &IdlInstruction {
			self.instructions
				.iter()
				.find(|instr| instr.name == name)
				.expect("instruction not found")
		}
		pub fn account(&self, name: &str) -> &IdlAccount {
			self.accounts
				.iter()
				.find(|account| account.name == name)
				.expect("account not found")
		}
	}
}
