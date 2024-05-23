use core::{marker::PhantomData, str::FromStr};

use codec::{Decode, Encode};
use frame_support::{
	sp_runtime::DispatchError, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::{prelude::format, TypeInfo};
use sp_core::RuntimeDebug;
use sp_std::{boxed::Box, vec, vec::Vec};

use crate::{
	sol::{
		consts::SYSTEM_PROGRAM_ID, instruction_builder::SolanaInstructionBuilder, SolAddress,
		SolAmount, SolCcmAccounts, SolHash, SolMessage, SolTransaction, SolanaCrypto,
	},
	AllBatch, AllBatchError, ApiCall, Chain, ChainCrypto, ChainEnvironment, ConsolidateCall,
	ConsolidationError, ExecutexSwapAndCall, FetchAssetParams, ForeignChainAddress,
	SetAggKeyWithAggKey, Solana, TransferAssetParams, TransferFallback,
};

use cf_primitives::ForeignChain;

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
				SolanaEnvAccountLookupKey::TokenMintPubkey =>
					SolanaTransactionBuildingError::CannotLookupTokenMintPubkey,
			},
		)
	}

	fn all_nonce_accounts() -> Result<Vec<SolAddress>, SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<AllNonceAccounts, Vec<(SolAddress, SolHash)>>>::lookup(
			AllNonceAccounts,
		)
		.map(|nonces| nonces.into_iter().map(|(addr, _hash)| addr).collect::<Vec<_>>())
		.ok_or(SolanaTransactionBuildingError::NoNonceAccountsSet)
	}
}

/// For looking up different accounts from the Solana Environment.
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum SolanaEnvAccountLookupKey {
	AggKey,
	VaultProgram,
	VaultProgramDataAccount,
	UpgradeManagerProgramDataAccount,
	TokenMintPubkey,
}

/// Errors that can arise when building Solana Transactions.
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum SolanaTransactionBuildingError {
	CannotLookupAggKey,
	CannotLookupVaultProgram,
	CannotLookupVaultProgramDataAccount,
	CannotLookupComputePrice,
	CannotLookupUpgradeManagerProgramDataAccount,
	CannotLookupTokenMintPubkey,
	NoNonceAccountsSet,
	NoAvailableNonceAccount,
	FailedToDeriveAddress(crate::sol::AddressDerivationError),
	CannotDecodeCcmCfParam,
}

impl sp_std::fmt::Display for SolanaTransactionBuildingError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
			.expect("Preset System Program ID account must be valid.");
		let (nonce_account, durable_nonce) = Environment::nonce_account()?;
		let compute_price = Environment::compute_price()?;

		// Build the instruction_set
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			fetch_params,
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
		transfer_param: TransferAssetParams<Solana>,
	) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup the current Aggkey
		let agg_key = Environment::lookup_account(SolanaEnvAccountLookupKey::AggKey)?;
		let (nonce_account, durable_nonce) = Environment::nonce_account()?;
		let compute_price = Environment::compute_price()?;

		// Build the instruction_set
		let instruction_set = SolanaInstructionBuilder::transfer(
			transfer_param,
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
			call_type: SolanaTransactionType::Transfer,
			transaction,
			_phantom: Default::default(),
		})
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
		let instruction_set = SolanaInstructionBuilder::ccm_transfer(
			transfer_param,
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
		);

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

		let _transfer_txs = transfer_params
			.into_iter()
			.map(|transfer_param| Self::transfer(transfer_param))
			.collect::<Result<Vec<_>, SolanaTransactionBuildingError>>()?;

		Err(AllBatchError::DispatchError(DispatchError::Other("PRO-1348 This should be implemented after allowing Multiple transactions to be returned by this trait.")))
	}
}

impl<Env: 'static> TransferFallback<Solana> for SolanaApi<Env> {
	fn new_unsigned(_transfer_param: TransferAssetParams<Solana>) -> Result<Self, DispatchError> {
		Err(DispatchError::Other("Solana does not support TransferFallback."))
	}
}
