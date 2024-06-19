use core::{marker::PhantomData, str::FromStr};
use std::collections::HashMap;

use codec::{Decode, Encode};
use frame_support::{
	sp_runtime::DispatchError, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::{prelude::format, TypeInfo};
use sp_core::RuntimeDebug;
use sp_std::{boxed::Box, vec, vec::Vec};

use crate::{
	sol::{
		consts::{SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
		instruction_builder::{AssetWithDerivedAddress, SolanaInstructionBuilder},
		SolAddress, SolAmount, SolAsset, SolCcmAccounts, SolHash, SolMessage, SolTransaction,
		SolanaCrypto,
	},
	AllBatch, AllBatchError, ApiCall, Chain, ChainCrypto, ChainEnvironment, ConsolidateCall,
	ConsolidationError, ExecutexSwapAndCall, FetchAssetParams, ForeignChainAddress,
	SetAggKeyWithAggKey, Solana, TransferAssetParams, TransferFallback,
};

use cf_primitives::ForeignChain;

use super::consts::SOL_USDC_DECIMAL;

#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct TokenEnvironment {
	pub token_mint_pubkey: SolAddress,
	pub token_vault_ata: SolAddress,
	pub token_vault_pda_account: SolAddress,
}

#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct ComputePrice;
#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct NonceAccount;
#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct AllNonceAccounts;

/// Super trait combining all Environment lookups required for the Solana chain.
/// Also contains some calls for easy data retrieval.
pub trait SolanaEnvironment:
	ChainEnvironment<SolanaEnvAccountLookupKey, SolAddress>
	+ ChainEnvironment<ComputePrice, SolAmount>
	+ ChainEnvironment<NonceAccount, (SolAddress, SolHash)>
	+ ChainEnvironment<AllNonceAccounts, Vec<(SolAddress, SolHash)>>
	+ ChainEnvironment<SolAsset, TokenEnvironment>
{
	fn compute_price() -> Result<SolAmount, SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<ComputePrice, SolAmount>>::lookup(ComputePrice)
			.ok_or(SolanaTransactionBuildingError::CannotLookupComputePrice)
	}

	fn nonce_account() -> Result<(SolAddress, SolHash), SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<NonceAccount, (SolAddress, SolHash)>>::lookup(NonceAccount)
			.ok_or(SolanaTransactionBuildingError::NoAvailableNonceAccount)
	}

	fn lookup_account(
		key: SolanaEnvAccountLookupKey,
	) -> Result<SolAddress, SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<SolanaEnvAccountLookupKey, SolAddress>>::lookup(key).ok_or(
			match key {
				SolanaEnvAccountLookupKey::AggKey =>
					SolanaTransactionBuildingError::CannotLookupAggKey,
				SolanaEnvAccountLookupKey::VaultProgram =>
					SolanaTransactionBuildingError::CannotLookupVaultProgram,
				SolanaEnvAccountLookupKey::VaultProgramDataAccount =>
					SolanaTransactionBuildingError::CannotLookupVaultProgramDataAccount,
				SolanaEnvAccountLookupKey::UpgradeManagerProgramDataAccount =>
					SolanaTransactionBuildingError::CannotLookupUpgradeManagerProgramDataAccount,
			},
		)
	}

	fn lookup_account_retain(
		key: SolanaEnvAccountLookupKey,
		sol_environment: &mut HashMap<SolanaEnvAccountLookupKey, SolAddress>,
	) -> Result<SolAddress, SolanaTransactionBuildingError> {
		match sol_environment.get(&key) {
			Some(address) => Ok(*address),
			None => Self::lookup_account(key).map(|addr| {
				sol_environment.insert(key, addr);
				addr
			}),
		}
	}

	fn all_nonce_accounts() -> Result<Vec<SolAddress>, SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<AllNonceAccounts, Vec<(SolAddress, SolHash)>>>::lookup(
			AllNonceAccounts,
		)
		.map(|nonces| nonces.into_iter().map(|(addr, _hash)| addr).collect::<Vec<_>>())
		.ok_or(SolanaTransactionBuildingError::NoNonceAccountsSet)
	}

	fn get_token_environment(
		token_environments: &mut HashMap<SolAsset, TokenEnvironment>,
		asset: SolAsset,
	) -> Result<TokenEnvironment, SolanaTransactionBuildingError> {
		match token_environments.get(&asset) {
			Some(token_environment) => Ok((*token_environment).clone()),
			None => {
				let token_environment =
					<Self as ChainEnvironment<SolAsset, TokenEnvironment>>::lookup(asset).ok_or(
						SolanaTransactionBuildingError::CannotLookupTokenEnvironment(asset),
					)?;
				token_environments.insert(asset, token_environment.clone());
				Ok(token_environment)
			},
		}
	}
}

/// For looking up different accounts from the Solana Environment.
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, Hash)]
pub enum SolanaEnvAccountLookupKey {
	AggKey,
	VaultProgram,
	VaultProgramDataAccount,
	UpgradeManagerProgramDataAccount,
}

/// Errors that can arise when building Solana Transactions.
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum SolanaTransactionBuildingError {
	CannotLookupAggKey,
	CannotLookupVaultProgram,
	CannotLookupVaultProgramDataAccount,
	CannotLookupComputePrice,
	CannotLookupUpgradeManagerProgramDataAccount,
	CannotLookupTokenEnvironment(SolAsset),
	NoNonceAccountsSet,
	NoAvailableNonceAccount,
	FailedToDeriveAddress(crate::sol::AddressDerivationError),
	CannotDecodeCcmCfParam,
}

impl sp_std::fmt::Display for SolanaTransactionBuildingError {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		write!(f, "{:?}", self)
	}
}

impl From<SolanaTransactionBuildingError> for DispatchError {
	fn from(value: SolanaTransactionBuildingError) -> DispatchError {
		DispatchError::Other(Box::leak(
			format!("Failed to build Solana Transaction. {:?}", value).into_boxed_str(),
		))
	}
}

impl From<SolanaTransactionBuildingError> for AllBatchError {
	fn from(value: SolanaTransactionBuildingError) -> AllBatchError {
		AllBatchError::DispatchError(value.into())
	}
}

/// Indicates the purpose of the Solana Api call.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
pub enum SolanaTransactionType {
	BatchFetch,
	Transfer,
	RotateAggKey,
	CcmTransfer,
}

/// The Solana Api call. Contains a call_type and the actual Transaction itself.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub struct SolanaApi<Environment: 'static> {
	call_type: SolanaTransactionType,
	transaction: SolTransaction,
	#[doc(hidden)]
	#[codec(skip)]
	_phantom: PhantomData<Environment>,
}

impl<Environment: SolanaEnvironment> SolanaApi<Environment> {
	pub fn batch_fetch(
		fetch_params: Vec<FetchAssetParams<Solana>>,
	) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup the current Aggkey
		let agg_key = Environment::lookup_account(SolanaEnvAccountLookupKey::AggKey)?;
		let vault_program = Environment::lookup_account(SolanaEnvAccountLookupKey::VaultProgram)?;
		let vault_program_data_account =
			Environment::lookup_account(SolanaEnvAccountLookupKey::VaultProgramDataAccount)?;
		let system_program_id = SolAddress::from_str(SYSTEM_PROGRAM_ID)
			.expect("Preset Solana System Program ID account must be valid.");
		let (nonce_account, durable_nonce) = Environment::nonce_account()?;
		let compute_price = Environment::compute_price()?;

		let mut token_environments = HashMap::new();
		let decomposed_fetch_params = fetch_params
			.into_iter()
			.map(|param| {
				AssetWithDerivedAddress::decompose_fetch_params::<Environment>(
					param,
					&mut token_environments,
				)
			})
			.collect::<Result<Vec<_>, _>>()?;

		// Build the instruction_set
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			decomposed_fetch_params,
			token_environments,
			vault_program,
			vault_program_data_account,
			system_program_id,
			agg_key,
			nonce_account,
			compute_price,
		);
		let transaction = SolTransaction::new_unsigned(SolMessage::new_with_blockhash(
			&instruction_set,
			Some(&agg_key.into()),
			&durable_nonce.into(),
		));

		Ok(Self {
			call_type: SolanaTransactionType::BatchFetch,
			transaction,
			_phantom: Default::default(),
		})
	}

	pub fn transfer(
		transfer_params: Vec<TransferAssetParams<Solana>>,
	) -> Result<Vec<Self>, SolanaTransactionBuildingError> {
		// Lookup the current Aggkey
		let agg_key = Environment::lookup_account(SolanaEnvAccountLookupKey::AggKey)?;
		let (nonce_account, durable_nonce) = Environment::nonce_account()?;
		let compute_price = Environment::compute_price()?;

		let mut sol_environment = HashMap::new();
		let mut token_environments = HashMap::new();
		transfer_params
			.into_iter()
			.map(|transfer_param| {
				let transfer_instruction_set = match transfer_param.asset {
					SolAsset::Sol => SolanaInstructionBuilder::transfer_native(
						transfer_param.amount,
						transfer_param.to,
						agg_key,
						nonce_account,
						compute_price,
					),
					SolAsset::SolUsdc => {
						let vault_program = Environment::lookup_account_retain(
							SolanaEnvAccountLookupKey::VaultProgram,
							&mut sol_environment,
						)?;
						let vault_program_data_account = Environment::lookup_account_retain(
							SolanaEnvAccountLookupKey::VaultProgramDataAccount,
							&mut sol_environment,
						)?;
						let sol_usdc_environment = Environment::get_token_environment(
							&mut token_environments,
							SolAsset::SolUsdc,
						)?;
						let token_program_id = SolAddress::from_str(TOKEN_PROGRAM_ID)
							.expect("Preset Solana Token program ID must be valid.");
						let system_program_id = SolAddress::from_str(SYSTEM_PROGRAM_ID)
							.expect("Preset Solana System Program ID account must be valid.");

						let ata =
						crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
							transfer_param.to,
							sol_usdc_environment.token_mint_pubkey,
						)
						.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;
						SolanaInstructionBuilder::transfer_token(
							ata.address,
							transfer_param.amount,
							transfer_param.to,
							vault_program,
							vault_program_data_account,
							sol_usdc_environment.token_vault_pda_account,
							sol_usdc_environment.token_vault_ata,
							sol_usdc_environment.token_mint_pubkey,
							token_program_id,
							system_program_id,
							agg_key,
							nonce_account,
							compute_price,
							SOL_USDC_DECIMAL,
						)
					},
				};

				Ok(Self {
					call_type: SolanaTransactionType::Transfer,
					transaction: SolTransaction::new_unsigned(SolMessage::new_with_blockhash(
						&transfer_instruction_set,
						Some(&agg_key.into()),
						&durable_nonce.into(),
					)),
					_phantom: Default::default(),
				})
			})
			.collect::<Result<Vec<_>, SolanaTransactionBuildingError>>()
	}

	pub fn rotate_agg_key(new_agg_key: SolAddress) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup the current Aggkey
		let agg_key = Environment::lookup_account(SolanaEnvAccountLookupKey::AggKey)?;
		let nonce_accounts = Environment::all_nonce_accounts()?;
		let vault_program = Environment::lookup_account(SolanaEnvAccountLookupKey::VaultProgram)?;
		let vault_program_data_account =
			Environment::lookup_account(SolanaEnvAccountLookupKey::VaultProgramDataAccount)?;
		let system_program_id = SolAddress::from_str(SYSTEM_PROGRAM_ID)
			.expect("Preset System Program ID account must be valid.");
		let upgrade_manager_program_data_account = Environment::lookup_account(
			SolanaEnvAccountLookupKey::UpgradeManagerProgramDataAccount,
		)?;
		let (nonce_account, durable_nonce) = Environment::nonce_account()?;
		let compute_price = Environment::compute_price()?;

		// Build the instruction_set
		let instruction_set = SolanaInstructionBuilder::rotate_agg_key(
			new_agg_key,
			nonce_accounts,
			vault_program,
			vault_program_data_account,
			system_program_id,
			upgrade_manager_program_data_account,
			agg_key,
			nonce_account,
			compute_price,
		);

		let transaction = SolTransaction::new_unsigned(SolMessage::new_with_blockhash(
			&instruction_set,
			Some(&agg_key.into()),
			&durable_nonce.into(),
		));

		Ok(Self {
			call_type: SolanaTransactionType::RotateAggKey,
			transaction,
			_phantom: Default::default(),
		})
	}

	pub fn ccm_transfer(
		transfer_param: TransferAssetParams<Solana>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		message: Vec<u8>,
		cf_parameters: Vec<u8>,
	) -> Result<Self, SolanaTransactionBuildingError> {
		let ccm_accounts = SolCcmAccounts::decode(&mut &cf_parameters[..])
			.map_err(|_| SolanaTransactionBuildingError::CannotDecodeCcmCfParam)?;

		let vault_program = Environment::lookup_account(SolanaEnvAccountLookupKey::VaultProgram)?;
		let vault_program_data_account =
			Environment::lookup_account(SolanaEnvAccountLookupKey::VaultProgramDataAccount)?;
		let system_program_id = SolAddress::from_str(crate::sol::consts::SYSTEM_PROGRAM_ID)
			.expect("Solana System program ID must be valid.");
		let sys_var_instructions = SolAddress::from_str(crate::sol::consts::SYS_VAR_INSTRUCTIONS)
			.expect("Solana System Var Instruction must be valid.");
		let agg_key = Environment::lookup_account(SolanaEnvAccountLookupKey::AggKey)?;
		let (nonce_account, durable_nonce) = Environment::nonce_account()?;
		let compute_price = Environment::compute_price()?;

		// Build the instruction_set
		let instruction_set = match transfer_param.asset {
			SolAsset::Sol => Ok(SolanaInstructionBuilder::ccm_transfer_native(
				transfer_param.amount,
				transfer_param.to,
				source_chain,
				source_address,
				message,
				ccm_accounts,
				vault_program,
				vault_program_data_account,
				system_program_id,
				sys_var_instructions,
				agg_key,
				nonce_account,
				compute_price,
			)),
			SolAsset::SolUsdc => {
				let token_environment = <Environment as ChainEnvironment<
					SolAsset,
					TokenEnvironment,
				>>::lookup(SolAsset::SolUsdc)
				.ok_or(SolanaTransactionBuildingError::CannotLookupTokenEnvironment(
					SolAsset::SolUsdc,
				))?;

				let ata =
					crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
						transfer_param.to,
						token_environment.token_mint_pubkey,
					)
					.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

				Ok(SolanaInstructionBuilder::ccm_transfer_token(
					ata.address,
					transfer_param.amount,
					transfer_param.to,
					source_chain,
					source_address,
					message,
					ccm_accounts,
					vault_program,
					vault_program_data_account,
					system_program_id,
					sys_var_instructions,
					token_environment.token_vault_pda_account,
					token_environment.token_vault_ata,
					token_environment.token_mint_pubkey,
					SolAddress::from_str(TOKEN_PROGRAM_ID)
						.expect("Preset Solana Token program ID must be valid."),
					agg_key,
					nonce_account,
					compute_price,
					SOL_USDC_DECIMAL,
				))
			},
		}?;

		let transaction = SolTransaction::new_unsigned(SolMessage::new_with_blockhash(
			&instruction_set,
			Some(&agg_key.into()),
			&durable_nonce.into(),
		));

		Ok(Self {
			call_type: SolanaTransactionType::CcmTransfer,
			transaction,
			_phantom: Default::default(),
		})
	}
}

impl<Env: 'static> ApiCall<SolanaCrypto> for SolanaApi<Env> {
	fn threshold_signature_payload(&self) -> <SolanaCrypto as ChainCrypto>::Payload {
		self.transaction.message().clone()
	}

	fn signed(mut self, signature: &<SolanaCrypto as ChainCrypto>::ThresholdSignature) -> Self {
		self.transaction.signatures = vec![*signature];
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.transaction.clone().finalize_and_serialize().unwrap_or_default()
	}

	fn is_signed(&self) -> bool {
		self.transaction.is_signed()
	}

	fn transaction_out_id(&self) -> <SolanaCrypto as ChainCrypto>::TransactionOutId {
		self.transaction.signatures.first().cloned().unwrap_or_default()
	}
}

impl<Env: 'static> ConsolidateCall<Solana> for SolanaApi<Env> {
	fn consolidate_utxos() -> Result<Self, ConsolidationError> {
		Err(ConsolidationError::NotRequired)
	}
}

impl<Env: 'static + SolanaEnvironment> SetAggKeyWithAggKey<SolanaCrypto> for SolanaApi<Env> {
	fn new_unsigned(
		_maybe_old_key: Option<<SolanaCrypto as ChainCrypto>::AggKey>,
		new_key: <SolanaCrypto as ChainCrypto>::AggKey,
	) -> Result<Option<Self>, crate::SetAggKeyWithAggKeyError> {
		Self::rotate_agg_key(new_key).map(Some).map_err(|e| {
			log::error!("Failed to construct Solana Rotate Agg key transaction! Error: {:?}", e);
			crate::SetAggKeyWithAggKeyError::Failed
		})
	}
}

impl<Env: 'static + SolanaEnvironment> ExecutexSwapAndCall<Solana> for SolanaApi<Env> {
	fn new_unsigned(
		transfer_param: TransferAssetParams<Solana>,
		source_chain: cf_primitives::ForeignChain,
		source_address: Option<ForeignChainAddress>,
		_gas_budget: <Solana as Chain>::ChainAmount,
		message: Vec<u8>,
		cf_parameters: Vec<u8>,
	) -> Result<Self, DispatchError> {
		Self::ccm_transfer(transfer_param, source_chain, source_address, message, cf_parameters)
			.map_err(|e| {
				log::error!("Failed to construct Solana CCM transfer transaction! Error: {:?}", e);
				e.into()
			})
	}
}

impl<Env: 'static + SolanaEnvironment> AllBatch<Solana> for SolanaApi<Env> {
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Solana>>,
		transfer_params: Vec<TransferAssetParams<Solana>>,
	) -> Result<Self, AllBatchError> {
		let _fetch_tx = Self::batch_fetch(fetch_params)?;
		let _transfer_txs = Self::transfer(transfer_params)?;

		Err(AllBatchError::DispatchError(DispatchError::Other("PRO-1348 This should be implemented after allowing Multiple transactions to be returned by this trait.")))
	}
}

impl<Env: 'static> TransferFallback<Solana> for SolanaApi<Env> {
	fn new_unsigned(_transfer_param: TransferAssetParams<Solana>) -> Result<Self, DispatchError> {
		Err(DispatchError::Other("Solana does not support TransferFallback."))
	}
}
