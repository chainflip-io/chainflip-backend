use core::marker::PhantomData;

use codec::{Decode, Encode};
use frame_support::{
	sp_runtime::DispatchError, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::{prelude::format, TypeInfo};
use sp_core::RuntimeDebug;
use sp_std::{boxed::Box, vec, vec::Vec};

use crate::{
	sol::{
		instruction_builder::SolanaInstructionBuilder, sol_tx_core::address_derivation, SolAddress,
		SolAmount, SolHash, SolMessage, SolTransaction, SolanaCrypto,
	},
	AllBatch, AllBatchError, ApiCall, Chain, ChainCrypto, ChainEnvironment, ConsolidateCall,
	ConsolidationError, DepositChannel, ExecutexSwapAndCall, FetchAssetParams, ForeignChainAddress,
	SetAggKeyWithAggKey, Solana, TransferAssetParams, TransferFallback,
};

/// Super trait combining all Environment lookups required for the Solana chain.
/// Also contains some calls for easy data retrieval.
pub trait SolanaEnvironment:
	ChainEnvironment<SolanaEnvAccountLookupKey, SolAddress>
	+ ChainEnvironment<(), SolAmount>
	+ ChainEnvironment<(), SolHash>
	+ ChainEnvironment<(), Vec<SolAddress>>
{
	fn compute_price() -> Result<SolAmount, SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<(), SolAmount>>::lookup(())
			.ok_or(SolanaTransactionBuildingError::CannotLookupComputePrice)
	}

	fn durable_nonce() -> Result<SolHash, SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<(), SolHash>>::lookup(())
			.ok_or(SolanaTransactionBuildingError::CannotLookupDurableNonce)
	}

	fn lookup_account(
		key: SolanaEnvAccountLookupKey,
	) -> Result<SolAddress, SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<SolanaEnvAccountLookupKey, SolAddress>>::lookup(key).ok_or(
			match key {
				SolanaEnvAccountLookupKey::AggKey =>
					SolanaTransactionBuildingError::CannotLookupAggKey,
				SolanaEnvAccountLookupKey::AvailableNonceAccount =>
					SolanaTransactionBuildingError::NoAvailableNonceAccount,
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
		<Self as ChainEnvironment<(), Vec<SolAddress>>>::lookup(())
			.ok_or(SolanaTransactionBuildingError::NoNonceAccountsSet)
	}
}

/// For looking up different accounts from the Solana Environment.
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum SolanaEnvAccountLookupKey {
	AggKey,
	AvailableNonceAccount,
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
	CannotLookupDurableNonce,
	CannotLookupUpgradeManagerProgramDataAccount,
	CannotLookupTokenMintPubkey,
	NoNonceAccountsSet,
	NoAvailableNonceAccount,
	FailedToDeriveAddress(crate::sol::AddressDerivationError),
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
		deposit_channels: Vec<DepositChannel<Solana>>,
	) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup the current Aggkey
		let agg_key = Environment::lookup_account(SolanaEnvAccountLookupKey::AggKey)?;

		// Build the instruction_set
		let instruction_set = SolanaInstructionBuilder::<Environment>::default()
			.fetch_from(deposit_channels)?
			.finalize()?;
		let transaction =
			SolTransaction::new_unsigned(SolMessage::new(&instruction_set, Some(&agg_key.into())));

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

		// Build the instruction_set
		let instruction_set = SolanaInstructionBuilder::<Environment>::default()
			.transfer(transfer_param)?
			.finalize()?;

		let transaction =
			SolTransaction::new_unsigned(SolMessage::new(&instruction_set, Some(&agg_key.into())));

		Ok(Self {
			call_type: SolanaTransactionType::Transfer,
			transaction,
			_phantom: Default::default(),
		})
	}

	pub fn rotate_agg_key(new_agg_key: SolAddress) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup the current Aggkey
		let agg_key = Environment::lookup_account(SolanaEnvAccountLookupKey::AggKey)?;

		// Build the instruction_set
		let instruction_set = SolanaInstructionBuilder::<Environment>::default()
			.rotate_agg_key(new_agg_key)?
			.finalize()?;

		let transaction =
			SolTransaction::new_unsigned(SolMessage::new(&instruction_set, Some(&agg_key.into())));

		Ok(Self {
			call_type: SolanaTransactionType::RotateAggKey,
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
		todo!("Double check on the transaction out ID")
	}
}

impl<Env: 'static> ConsolidateCall<Solana> for SolanaApi<Env> {
	fn consolidate_utxos() -> Result<Self, ConsolidationError> {
		Err(ConsolidationError::NotRequired)
	}
}

impl<Env: SolanaEnvironment> SetAggKeyWithAggKey<SolanaCrypto> for SolanaApi<Env> {
	fn new_unsigned(
		_maybe_old_key: Option<<SolanaCrypto as ChainCrypto>::AggKey>,
		new_key: <SolanaCrypto as ChainCrypto>::AggKey,
	) -> Result<Option<Self>, crate::SetAggKeyWithAggKeyError> {
		Self::rotate_agg_key(new_key).map(Some).map_err(|e| {
			log::error!("Failed to construct Rotate Agg key! {:?}", e);
			crate::SetAggKeyWithAggKeyError::Failed
		})
	}
}

impl<Env: 'static> ExecutexSwapAndCall<Solana> for SolanaApi<Env> {
	fn new_unsigned(
		_transfer_param: TransferAssetParams<Solana>,
		_source_chain: cf_primitives::ForeignChain,
		_source_address: Option<ForeignChainAddress>,
		_gas_budget: <Solana as Chain>::ChainAmount,
		_message: vec::Vec<u8>,
	) -> Result<Self, DispatchError> {
		todo!()
	}
}

impl<Env: SolanaEnvironment> AllBatch<Solana> for SolanaApi<Env> {
	fn new_unsigned(
		fetch_params: vec::Vec<FetchAssetParams<Solana>>,
		transfer_params: vec::Vec<TransferAssetParams<Solana>>,
	) -> Result<Self, AllBatchError> {
		let vault_program = Env::lookup_account(SolanaEnvAccountLookupKey::VaultProgram)
			.map_err(|e| AllBatchError::DispatchError(e.into()))?;
		let deposit_channels = fetch_params
			.into_iter()
			.map(|fetch_param| {
				address_derivation::derive_deposit_channel::<Env>(
					fetch_param.deposit_fetch_id,
					fetch_param.asset,
					vault_program,
				)
				.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)
			})
			.collect::<Result<Vec<_>, SolanaTransactionBuildingError>>()?;

		let _fetch_tx = Self::batch_fetch(deposit_channels)?;

		let _transfer_txs = transfer_params
			.into_iter()
			.map(|transfer_param| Self::transfer(transfer_param))
			.collect::<Result<Vec<_>, SolanaTransactionBuildingError>>()?;

		todo!("PRO-1348 This should be implemented after allowing Multiple transactions to be returned by this trait.")
	}
}

impl<Env: 'static> TransferFallback<Solana> for SolanaApi<Env> {
	fn new_unsigned(_transfer_param: TransferAssetParams<Solana>) -> Result<Self, DispatchError> {
		Err(DispatchError::Other("Solana does not support TransferFallback."))
	}
}
