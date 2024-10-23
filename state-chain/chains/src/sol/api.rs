use cf_runtime_utilities::log_or_panic;
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound};
use scale_info::TypeInfo;
use sol_prim::consts::SOL_USDC_DECIMAL;
use sp_core::RuntimeDebug;
use sp_std::{vec, vec::Vec};

use crate::{
	ccm_checker::{
		check_ccm_for_blacklisted_accounts, CcmValidityCheck, CcmValidityChecker, CcmValidityError,
		DecodedCfParameters,
	},
	sol::{
		transaction_builder::SolanaTransactionBuilder, SolAddress, SolAmount, SolApiEnvironment,
		SolAsset, SolHash, SolTransaction, SolanaCrypto,
	},
	AllBatch, AllBatchError, ApiCall, CcmChannelMetadata, Chain, ChainCrypto, ChainEnvironment,
	ConsolidateCall, ConsolidationError, ExecutexSwapAndCall, ExecutexSwapAndCallError,
	FetchAssetParams, ForeignChainAddress, SetAggKeyWithAggKey, SetGovKeyWithAggKey, Solana,
	TransferAssetParams, TransferFallback, TransferFallbackError,
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
	+ RecoverDurableNonce
{
	fn compute_price() -> Result<SolAmount, SolanaTransactionBuildingError> {
		Self::lookup(ComputePrice).ok_or(SolanaTransactionBuildingError::CannotLookupComputePrice)
	}

	fn api_environment() -> Result<SolApiEnvironment, SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<ApiEnvironment, SolApiEnvironment>>::lookup(ApiEnvironment)
			.ok_or(SolanaTransactionBuildingError::CannotLookupApiEnvironment)
	}

	fn nonce_account() -> Result<DurableNonceAndAccount, SolanaTransactionBuildingError> {
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

/// IMPORTANT: This should only be used if the nonce has not been used to sign a transaction.
///
/// Once a nonce is actually used, it should ONLY be recovered via Witnessing.
/// Only use this if you know what you are doing.
pub trait RecoverDurableNonce {
	/// Set a unused durable nonce back as available.
	fn recover_durable_nonce(_nonce_account: SolAddress) {}
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
	InvalidCcm(CcmValidityError),
	FailedToSerializeFinalTransaction,
	FinalTransactionExceededMaxLength(u32),
}

impl sp_std::fmt::Display for SolanaTransactionBuildingError {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		write!(f, "{:?}", self)
	}
}

impl From<SolanaTransactionBuildingError> for AllBatchError {
	fn from(err: SolanaTransactionBuildingError) -> AllBatchError {
		AllBatchError::FailedToBuildSolanaTransaction(err)
	}
}

/// Indicates the purpose of the Solana Api call.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
pub enum SolanaTransactionType {
	BatchFetch,
	Transfer,
	RotateAggKey,
	#[deprecated]
	CcmTransferLegacy,
	SetGovKeyWithAggKey,
	CcmTransfer {
		fallback: TransferAssetParams<Solana>,
	},
}

/// The Solana Api call. Contains a call_type and the actual Transaction itself.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub struct SolanaApi<Environment: 'static> {
	pub call_type: SolanaTransactionType,
	pub transaction: SolTransaction,
	pub signer: Option<SolAddress>,
	#[doc(hidden)]
	#[codec(skip)]
	pub _phantom: PhantomData<Environment>,
}

impl<Environment: SolanaEnvironment> SolanaApi<Environment> {
	pub fn batch_fetch(
		fetch_params: Vec<FetchAssetParams<Solana>>,
	) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup environment variables, such as aggkey and durable nonce.
		let agg_key = Environment::current_agg_key()?;
		let sol_api_environment = Environment::api_environment()?;
		let compute_price = Environment::compute_price()?;
		let durable_nonce = Environment::nonce_account()?;

		// Build the transaction
		let transaction = SolanaTransactionBuilder::fetch_from(
			fetch_params,
			sol_api_environment,
			agg_key,
			durable_nonce,
			compute_price,
		)?;

		Ok(Self {
			call_type: SolanaTransactionType::BatchFetch,
			transaction,
			signer: None,
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
				let durable_nonce = Environment::nonce_account()?;
				let transaction = match transfer_param.asset {
					SolAsset::Sol => SolanaTransactionBuilder::transfer_native(
						transfer_param.amount,
						transfer_param.to,
						agg_key,
						durable_nonce,
						compute_price,
					),
					SolAsset::SolUsdc => {
						let ata =
						crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
							transfer_param.to,
							sol_api_environment.usdc_token_mint_pubkey,
						)
						.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;
						SolanaTransactionBuilder::transfer_token(
							ata.address,
							transfer_param.amount,
							transfer_param.to,
							sol_api_environment.vault_program,
							sol_api_environment.vault_program_data_account,
							sol_api_environment.token_vault_pda_account,
							sol_api_environment.usdc_token_vault_ata,
							sol_api_environment.usdc_token_mint_pubkey,
							agg_key,
							durable_nonce,
							compute_price,
							SOL_USDC_DECIMAL,
						)
					},
				}?;

				Ok((
					Self {
						call_type: SolanaTransactionType::Transfer,
						transaction,
						signer: None,
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
		let compute_price = Environment::compute_price()?;
		let durable_nonce = Environment::nonce_account()?;

		// Build the transaction
		let transaction = SolanaTransactionBuilder::rotate_agg_key(
			new_agg_key,
			nonce_accounts,
			sol_api_environment.vault_program,
			sol_api_environment.vault_program_data_account,
			agg_key,
			durable_nonce,
			compute_price,
		)
		.inspect_err(|e| {
			// Vault Rotation call building NOT transactional - meaning when this fails,
			// storage is not rolled back. We must recover the durable nonce here,
			// since it has been taken from storage but not actually used.
			log::error!(
				"Solana RotateAggKey call building failed. Nonce recovered. Error: {:?}
				new aggkey: {:?}
				Nonce recovered: {:?}",
				e,
				new_agg_key,
				durable_nonce
			);
			Environment::recover_durable_nonce(durable_nonce.0);
		})?;

		Ok(Self {
			call_type: SolanaTransactionType::RotateAggKey,
			transaction,
			signer: None,
			_phantom: Default::default(),
		})
	}

	pub fn ccm_transfer(
		transfer_param: TransferAssetParams<Solana>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: <Solana as Chain>::ChainAmount,
		message: Vec<u8>,
		cf_parameters: Vec<u8>,
	) -> Result<Self, SolanaTransactionBuildingError> {
		// For extra safety, re-verify the validity of the CCM message here
		// and extract the decoded `ccm_accounts` from `cf_parameters`.
		let decoded_cf_params = CcmValidityChecker::check_and_decode(
			&CcmChannelMetadata {
				message: message
					.clone()
					.try_into()
					.expect("This is parsed from bounded vec, therefore the size must fit"),
				gas_budget: 0, // This value is un-used by Solana
				cf_parameters: cf_parameters
					.clone()
					.try_into()
					.expect("This is parsed from bounded vec, therefore the size must fit"),
			},
			transfer_param.asset.into(),
		)
		.map_err(SolanaTransactionBuildingError::InvalidCcm)?;

		// Always expects the `DecodedCfParameters::Solana(..)` variant of the decoded cf params.
		let ccm_accounts = if let DecodedCfParameters::Solana(ccm_accounts) = decoded_cf_params {
			Ok(ccm_accounts)
		} else {
			Err(SolanaTransactionBuildingError::InvalidCcm(
				CcmValidityError::CannotDecodeCfParameters,
			))
		}?;

		let sol_api_environment = Environment::api_environment()?;
		let agg_key = Environment::current_agg_key()?;

		// Ensure the CCM parameters do not contain blacklisted accounts.
		check_ccm_for_blacklisted_accounts(
			&ccm_accounts,
			vec![sol_api_environment.token_vault_pda_account.into(), agg_key.into()],
		)
		.map_err(SolanaTransactionBuildingError::InvalidCcm)?;

		let compute_price = Environment::compute_price()?;
		let durable_nonce = Environment::nonce_account()?;

		let fallback = TransferAssetParams {
			asset: transfer_param.asset,
			amount: transfer_param.amount,
			to: ccm_accounts.fallback_address.into(),
		};

		// Build the transaction
		let transaction = match transfer_param.asset {
			SolAsset::Sol => SolanaTransactionBuilder::ccm_transfer_native(
				transfer_param.amount,
				transfer_param.to,
				source_chain,
				source_address,
				message,
				ccm_accounts,
				sol_api_environment.vault_program,
				sol_api_environment.vault_program_data_account,
				agg_key,
				durable_nonce,
				compute_price,
				gas_budget,
			),
			SolAsset::SolUsdc => {
				let ata =
					crate::sol::sol_tx_core::address_derivation::derive_associated_token_account(
						transfer_param.to,
						sol_api_environment.usdc_token_mint_pubkey,
					)
					.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?;

				SolanaTransactionBuilder::ccm_transfer_token(
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
					durable_nonce,
					compute_price,
					SOL_USDC_DECIMAL,
					gas_budget,
				)
			},
		}
		.inspect_err(|e| {
			// CCM call building is NOT transactional - meaning when this fails,
			// storage is not rolled back. We must recover the durable nonce here,
			// since it has been taken from storage but not actually used.
			log::error!(
				"CCM building failed. Nonce recovered. Error: {:?}
				Transfer param: {:?}
				Nonce recovered: {:?}",
				e,
				transfer_param,
				durable_nonce
			);
			Environment::recover_durable_nonce(durable_nonce.0);
		})?;

		Ok(Self {
			call_type: SolanaTransactionType::CcmTransfer { fallback },
			transaction,
			signer: None,
			_phantom: Default::default(),
		})
	}
}

impl<Env: 'static> ApiCall<SolanaCrypto> for SolanaApi<Env> {
	fn threshold_signature_payload(&self) -> <SolanaCrypto as ChainCrypto>::Payload {
		self.transaction.message().clone()
	}

	fn signed(
		mut self,
		threshold_signature: &<SolanaCrypto as ChainCrypto>::ThresholdSignature,
		signer: <SolanaCrypto as ChainCrypto>::AggKey,
	) -> Self {
		self.transaction.signatures = vec![*threshold_signature];
		self.signer = Some(signer);
		self
	}

	fn chain_encoded(&self) -> Vec<u8> {
		self.transaction.clone().finalize_and_serialize().unwrap_or_else(|err| {
			log_or_panic!(
				"Failed to serialize Solana Transaction {:?}. Error: {:?}",
				self.transaction,
				err
			);
			Vec::default()
		})
	}

	fn is_signed(&self) -> bool {
		self.transaction.is_signed()
	}

	fn transaction_out_id(&self) -> <SolanaCrypto as ChainCrypto>::TransactionOutId {
		self.transaction.signatures.first().cloned().unwrap_or_default()
	}

	fn refresh_replay_protection(&mut self) {
		// No replay protection refresh for Solana.
	}

	fn signer(&self) -> Option<<SolanaCrypto as ChainCrypto>::AggKey> {
		self.signer
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
			crate::SetAggKeyWithAggKeyError::FinalTransactionExceededMaxLength
		})
	}
}

impl<Env: 'static + SolanaEnvironment> ExecutexSwapAndCall<Solana> for SolanaApi<Env> {
	fn new_unsigned(
		transfer_param: TransferAssetParams<Solana>,
		source_chain: cf_primitives::ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: <Solana as Chain>::ChainAmount,
		message: Vec<u8>,
		cf_parameters: Vec<u8>,
	) -> Result<Self, ExecutexSwapAndCallError> {
		Self::ccm_transfer(
			transfer_param,
			source_chain,
			source_address,
			gas_budget,
			message,
			cf_parameters,
		)
		.map_err(|e| {
			log::error!("Failed to construct Solana CCM transfer transaction! \nError: {:?}", e);
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
	fn new_unsigned(
		_transfer_param: TransferAssetParams<Solana>,
	) -> Result<Self, TransferFallbackError> {
		Err(TransferFallbackError::Unsupported)
	}
}

impl<Environment: SolanaEnvironment> SetGovKeyWithAggKey<SolanaCrypto> for SolanaApi<Environment> {
	fn new_unsigned(
		_maybe_old_key: Option<<SolanaCrypto as ChainCrypto>::GovKey>,
		new_gov_key: <SolanaCrypto as ChainCrypto>::GovKey,
	) -> Result<Self, ()> {
		// Lookup environment variables, such as aggkey and durable nonce.
		let agg_key = Environment::current_agg_key().map_err(|_e| ())?;
		let sol_api_environment = Environment::api_environment().map_err(|_e| ())?;
		let compute_price = Environment::compute_price().map_err(|_e| ())?;
		let durable_nonce = Environment::nonce_account().map_err(|_e| ())?;

		// Build the transaction
		let transaction = SolanaTransactionBuilder::set_gov_key_with_agg_key(
			new_gov_key,
			sol_api_environment.vault_program,
			sol_api_environment.vault_program_data_account,
			agg_key,
			durable_nonce,
			compute_price,
		)
		.map_err(|e| {
			// SetGovKeyWithAggKey call building NOT transactional - meaning when this fails,
			// storage is not rolled back. We must recover the durable nonce here,
			// since it has been taken from storage but not actually used.
			log::error!(
				"Solana SetGovKeyWithAggKey call building failed. Nonce recovered. Error: {:?}
				Nonce recovered: {:?}",
				e,
				durable_nonce
			);
			Environment::recover_durable_nonce(durable_nonce.0);
		})?;

		Ok(Self {
			call_type: SolanaTransactionType::SetGovKeyWithAggKey,
			transaction,
			signer: None,
			_phantom: Default::default(),
		})
	}
}
