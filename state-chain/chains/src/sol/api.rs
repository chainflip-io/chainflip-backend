use core::marker::PhantomData;
use sol_prim::consts::SOL_USDC_DECIMAL;

use codec::{Decode, Encode};
use frame_support::{
	sp_runtime::DispatchError, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;
use sp_std::{vec, vec::Vec};

use crate::{
	sol::{
		ccm_checker::SolanaCcmValidityChecker, instruction_builder::SolanaInstructionBuilder,
		SolAddress, SolAmount, SolApiEnvironment, SolAsset, SolCcmAccounts, SolHash, SolMessage,
		SolTransaction, SolanaCrypto,
	},
	AllBatch, AllBatchError, ApiCall, CcmChannelMetadata, CcmValidityChecker, CcmValidityError,
	Chain, ChainCrypto, ChainEnvironment, ConsolidateCall, ConsolidationError, ExecutexSwapAndCall,
	ExecutexSwapAndCallError, FetchAssetParams, ForeignChainAddress, SetAggKeyWithAggKey, Solana,
	TransferAssetParams, TransferFallback,
};

use cf_primitives::{EgressId, ForeignChain};

#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct ComputePrice;
#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct DurableNonce;
#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct AllNonceAccounts;
#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct ApiEnvironment;
#[derive(Clone, Encode, Decode, PartialEq, Debug, TypeInfo)]
pub struct CurrentAggKey;

pub type DurableNonceAndAccount = (SolAddress, SolHash);

/// Super trait combining all Environment lookups required for the Solana chain.
/// Also contains some calls for easy data retrieval.
pub trait SolanaEnvironment:
	ChainEnvironment<ApiEnvironment, SolApiEnvironment>
	+ ChainEnvironment<CurrentAggKey, SolAddress>
	+ ChainEnvironment<ComputePrice, SolAmount>
	+ ChainEnvironment<DurableNonce, DurableNonceAndAccount>
	+ ChainEnvironment<AllNonceAccounts, Vec<DurableNonceAndAccount>>
{
	fn compute_price() -> Result<SolAmount, SolanaTransactionBuildingError> {
		Self::lookup(ComputePrice).ok_or(SolanaTransactionBuildingError::CannotLookupComputePrice)
	}

	fn api_environment() -> Result<SolApiEnvironment, SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<ApiEnvironment, SolApiEnvironment>>::lookup(ApiEnvironment)
			.ok_or(SolanaTransactionBuildingError::CannotLookupApiEnvironment)
	}

	fn nonce_account() -> Result<(SolAddress, SolHash), SolanaTransactionBuildingError> {
		Self::lookup(DurableNonce).ok_or(SolanaTransactionBuildingError::NoAvailableNonceAccount)
	}

	fn current_agg_key() -> Result<SolAddress, SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<CurrentAggKey, SolAddress>>::lookup(CurrentAggKey)
			.ok_or(SolanaTransactionBuildingError::CannotLookupCurrentAggKey)
	}

	fn all_nonce_accounts() -> Result<Vec<SolAddress>, SolanaTransactionBuildingError> {
		Self::lookup(AllNonceAccounts)
			.map(|nonces| nonces.into_iter().map(|(addr, _hash)| addr).collect::<Vec<_>>())
			.ok_or(SolanaTransactionBuildingError::NoNonceAccountsSet)
	}
}

/// Errors that can arise when building Solana Transactions.
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum SolanaTransactionBuildingError {
	CannotLookupApiEnvironment,
	CannotLookupCurrentAggKey,
	CannotLookupComputePrice,
	NoNonceAccountsSet,
	NoAvailableNonceAccount,
	FailedToDeriveAddress(crate::sol::AddressDerivationError),
	CannotDecodeCcmCfParam,
	InvalidCcm(CcmValidityError),
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
	pub call_type: SolanaTransactionType,
	pub transaction: SolTransaction,
	#[doc(hidden)]
	#[codec(skip)]
	_phantom: PhantomData<Environment>,
}

impl<Environment: SolanaEnvironment> SolanaApi<Environment> {
	pub fn batch_fetch(
		fetch_params: Vec<FetchAssetParams<Solana>>,
	) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup environment variables, such as aggkey and durable nonce.
		let agg_key = Environment::current_agg_key()?;
		let sol_api_environment = Environment::api_environment()?;
		let (nonce_account, durable_nonce) = Environment::nonce_account()?;
		let compute_price = Environment::compute_price()?;

		// Build the instruction_set
		let instruction_set = SolanaInstructionBuilder::fetch_from(
			fetch_params,
			sol_api_environment,
			agg_key,
			nonce_account,
			compute_price,
		)?;
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
		transfer_params: Vec<(TransferAssetParams<Solana>, EgressId)>,
	) -> Result<Vec<(Self, Vec<EgressId>)>, SolanaTransactionBuildingError> {
		// Lookup environment variables, such as aggkey and durable nonce.
		let agg_key = Environment::current_agg_key()?;
		let sol_api_environment = Environment::api_environment()?;
		let compute_price = Environment::compute_price()?;

		transfer_params
			.into_iter()
			.map(|(transfer_param, egress_id)| {
				let (nonce_account, durable_nonce) = Environment::nonce_account()?;
				let transfer_instruction_set = match transfer_param.asset {
					SolAsset::Sol => SolanaInstructionBuilder::transfer_native(
						transfer_param.amount,
						transfer_param.to,
						agg_key,
						nonce_account,
						compute_price,
					),
					SolAsset::SolUsdc => {
						let ata =
						crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
							transfer_param.to,
							sol_api_environment.usdc_token_mint_pubkey,
						)
						.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;
						SolanaInstructionBuilder::transfer_token(
							ata.address,
							transfer_param.amount,
							transfer_param.to,
							sol_api_environment.vault_program,
							sol_api_environment.vault_program_data_account,
							sol_api_environment.token_vault_pda_account,
							sol_api_environment.usdc_token_vault_ata,
							sol_api_environment.usdc_token_mint_pubkey,
							agg_key,
							nonce_account,
							compute_price,
							SOL_USDC_DECIMAL,
						)
					},
				};

				Ok((
					Self {
						call_type: SolanaTransactionType::Transfer,
						transaction: SolTransaction::new_unsigned(SolMessage::new_with_blockhash(
							&transfer_instruction_set,
							Some(&agg_key.into()),
							&durable_nonce.into(),
						)),
						_phantom: Default::default(),
					},
					vec![egress_id],
				))
			})
			.collect::<Result<Vec<_>, SolanaTransactionBuildingError>>()
	}

	pub fn rotate_agg_key(new_agg_key: SolAddress) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup environment variables, such as aggkey and durable nonce.
		let agg_key = Environment::current_agg_key()?;
		let sol_api_environment = Environment::api_environment()?;
		let nonce_accounts = Environment::all_nonce_accounts()?;
		let (nonce_account, durable_nonce) = Environment::nonce_account()?;
		let compute_price = Environment::compute_price()?;

		// Build the instruction_set
		let instruction_set = SolanaInstructionBuilder::rotate_agg_key(
			new_agg_key,
			nonce_accounts,
			sol_api_environment.vault_program,
			sol_api_environment.vault_program_data_account,
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
		// Verify the validity of the CCM message before building the call.
		SolanaCcmValidityChecker::<Environment>::is_valid(
			&CcmChannelMetadata {
				message: message
					.clone()
					.try_into()
					.expect("This is parsed from bounded vec, therefore the size must fit"),
				gas_budget: 0,
				cf_parameters: cf_parameters
					.clone()
					.try_into()
					.expect("This is parsed from bounded vec, therefore the size must fit"),
			},
			transfer_param.asset.into(),
		)
		.map_err(|e| {
			log::warn!(
				"Failed to build CCM API call. Transfer param: {:?}, Error: {:?}",
				transfer_param,
				e
			);
			SolanaTransactionBuildingError::InvalidCcm(e)
		})?;

		let ccm_accounts = SolCcmAccounts::decode(&mut &cf_parameters[..])
			.map_err(|_| SolanaTransactionBuildingError::CannotDecodeCcmCfParam)?;

		let sol_api_environment = Environment::api_environment()?;
		let agg_key = Environment::current_agg_key()?;
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
				sol_api_environment.vault_program,
				sol_api_environment.vault_program_data_account,
				agg_key,
				nonce_account,
				compute_price,
			)),
			SolAsset::SolUsdc => {
				let ata =
					crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
						transfer_param.to,
						sol_api_environment.usdc_token_mint_pubkey,
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
					sol_api_environment.vault_program,
					sol_api_environment.vault_program_data_account,
					sol_api_environment.token_vault_pda_account,
					sol_api_environment.usdc_token_vault_ata,
					sol_api_environment.usdc_token_mint_pubkey,
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

	fn refresh_replay_protection(&mut self) {
		todo!()
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
	) -> Result<Self, ExecutexSwapAndCallError> {
		Self::ccm_transfer(transfer_param, source_chain, source_address, message, cf_parameters)
			.map_err(|e| {
				log::error!("Failed to construct Solana CCM transfer transaction! Error: {:?}", e);
				ExecutexSwapAndCallError::FailedToBuildCcmForSolana(e)
			})
	}
}

impl<Env: 'static + SolanaEnvironment> AllBatch<Solana> for SolanaApi<Env> {
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Solana>>,
		transfer_params: Vec<(TransferAssetParams<Solana>, EgressId)>,
	) -> Result<Vec<(Self, Vec<EgressId>)>, AllBatchError> {
		let mut txs = Self::transfer(transfer_params)?;

		if !fetch_params.is_empty() {
			txs.push((Self::batch_fetch(fetch_params)?, vec![]));
		}

		Ok(txs)
	}
}

impl<Env: 'static> TransferFallback<Solana> for SolanaApi<Env> {
	fn new_unsigned(_transfer_param: TransferAssetParams<Solana>) -> Result<Self, DispatchError> {
		Err(DispatchError::Other("Solana does not support TransferFallback."))
	}
}
