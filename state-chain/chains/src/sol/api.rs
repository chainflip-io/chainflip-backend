// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::{
	ccm_checker::VersionedSolanaCcmAdditionalData, Chain, RejectCall, RejectError,
	SetGovKeyWithAggKeyError,
};
use cf_runtime_utilities::log_or_panic;
use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::RuntimeDebug;
use sp_runtime::DispatchError;
use sp_std::{collections::btree_set::BTreeSet, vec, vec::Vec};

use crate::{
	ccm_checker::{check_ccm_for_blacklisted_accounts, CcmValidityError, DecodedCcmAdditionalData},
	sol::{
		sol_tx_core::{
			address_derivation::derive_associated_token_account, consts::SOL_USDC_DECIMAL,
		},
		transaction_builder::SolanaTransactionBuilder,
		SolAddress, SolAddressLookupTableAccount, SolAmount, SolApiEnvironment, SolAsset, SolHash,
		SolTrackedData, SolVersionedTransaction, SolanaCrypto,
	},
	AllBatch, AllBatchError, ApiCall, ChainCrypto, ChainEnvironment, ConsolidateCall,
	ConsolidationError, ExecutexSwapAndCall, ExecutexSwapAndCallError,
	FetchAndCloseSolanaVaultSwapAccounts, FetchAssetParams, ForeignChainAddress,
	SetAggKeyWithAggKey, SetGovKeyWithAggKey, Solana, TransferAssetParams, TransferFallback,
	TransferFallbackError,
};

use cf_primitives::{EgressId, ForeignChain, GasAmount};

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct ComputePrice;
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct DurableNonce;
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct AllNonceAccounts;
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct ApiEnvironment;
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct CurrentAggKey;
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct CurrentOnChainKey;

pub type DurableNonceAndAccount = (SolAddress, SolHash);

#[derive(
	Clone,
	Encode,
	Decode,
	PartialEq,
	RuntimeDebug,
	TypeInfo,
	Copy,
	Serialize,
	Deserialize,
	Ord,
	PartialOrd,
	Eq,
)]
pub struct VaultSwapAccountAndSender {
	pub vault_swap_account: SolAddress,
	pub swap_sender: SolAddress,
}

/// Super trait combining all Environment lookups required for the Solana chain.
/// Also contains some calls for easy data retrieval.
pub trait SolanaEnvironment:
	ChainEnvironment<ApiEnvironment, SolApiEnvironment>
	+ ChainEnvironment<CurrentAggKey, SolAddress>
	+ ChainEnvironment<CurrentOnChainKey, SolAddress>
	+ ChainEnvironment<ComputePrice, SolAmount>
	+ ChainEnvironment<DurableNonce, DurableNonceAndAccount>
	+ ChainEnvironment<AllNonceAccounts, Vec<DurableNonceAndAccount>>
	+ ChainEnvironment<
		BTreeSet<SolAddress>,
		AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>,
	> + RecoverDurableNonce
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

	fn current_on_chain_key() -> Result<SolAddress, SolanaTransactionBuildingError> {
		<Self as ChainEnvironment<CurrentOnChainKey, SolAddress>>::lookup(CurrentOnChainKey)
			.ok_or(SolanaTransactionBuildingError::CannotLookupCurrentAggKey)
	}

	fn all_nonce_accounts() -> Result<Vec<SolAddress>, SolanaTransactionBuildingError> {
		Self::lookup(AllNonceAccounts)
			.map(|nonces| nonces.into_iter().map(|(addr, _hash)| addr).collect::<Vec<_>>())
			.ok_or(SolanaTransactionBuildingError::NoNonceAccountsSet)
	}

	/// Get any user-defined Address lookup tables from the Environment.
	fn get_address_lookup_tables(
		alts: BTreeSet<SolAddress>,
	) -> Result<Vec<SolAddressLookupTableAccount>, SolanaTransactionBuildingError> {
		match Self::lookup(alts) {
			Some(AltWitnessingConsensusResult::Valid(witnessed_alts)) => Ok(witnessed_alts),
			Some(AltWitnessingConsensusResult::Invalid) =>
				Err(SolanaTransactionBuildingError::AltsInvalid),
			None => Err(SolanaTransactionBuildingError::AltsNotYetWitnessed),
		}
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
#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, TypeInfo, Debug)]
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
	DispatchError(DispatchError),
	AltsNotYetWitnessed,
	AltsInvalid,
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
	CloseEventAccounts,
	SetProgramSwapParameters,
	SetTokenSwapParameters,
	UpgradeProgram,
	Refund,
}

/// The Solana Api call. Contains a call_type and the actual Transaction itself.
#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub struct SolanaApi<Environment: 'static> {
	pub call_type: SolanaTransactionType,
	pub transaction: SolVersionedTransaction,
	pub signer: Option<SolAddress>,
	#[doc(hidden)]
	#[codec(skip)]
	pub _phantom: PhantomData<Environment>,
}

#[derive(
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
)]
pub enum SolanaGovCall {
	SetProgramSwapsParameters {
		min_native_swap_amount: u64,
		max_dst_address_len: u16,
		max_ccm_message_len: u32,
		max_cf_parameters_len: u32,
		max_event_accounts: u32,
	},
	SetTokenSwapParameters {
		min_swap_amount: u64,
		token_mint_pubkey: SolAddress,
	},
	UpgradeProgram {
		program_address: SolAddress,
		buffer_address: SolAddress,
		spill_address: SolAddress,
	},
}

impl SolanaGovCall {
	pub fn to_api_call<E: SolanaEnvironment>(
		&self,
	) -> Result<SolanaApi<E>, SolanaTransactionBuildingError> {
		match self {
			SolanaGovCall::SetProgramSwapsParameters {
				min_native_swap_amount,
				max_dst_address_len,
				max_ccm_message_len,
				max_cf_parameters_len,
				max_event_accounts,
			} => SolanaApi::set_program_swaps_parameters(
				*min_native_swap_amount,
				*max_dst_address_len,
				*max_ccm_message_len,
				*max_cf_parameters_len,
				*max_event_accounts,
			),
			SolanaGovCall::SetTokenSwapParameters { min_swap_amount, token_mint_pubkey } =>
				SolanaApi::set_token_swap_parameters(*min_swap_amount, *token_mint_pubkey),
			SolanaGovCall::UpgradeProgram { program_address, buffer_address, spill_address } =>
				SolanaApi::upgrade_program(*program_address, *buffer_address, *spill_address),
		}
	}
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
						let ata = derive_associated_token_account(
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
							vec![sol_api_environment.address_lookup_table_account.clone()],
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
			sol_api_environment.alt_manager_program,
			durable_nonce,
			compute_price,
			vec![sol_api_environment.address_lookup_table_account],
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
		gas_budget: GasAmount,
		message: Vec<u8>,
		ccm_additional_data: VersionedSolanaCcmAdditionalData,
	) -> Result<Self, SolanaTransactionBuildingError> {
		let sol_api_environment = Environment::api_environment()?;
		let agg_key = Environment::current_agg_key()?;

		// Ensure the CCM parameters do not contain blacklisted accounts.
		let ccm_accounts = ccm_additional_data.ccm_accounts();
		check_ccm_for_blacklisted_accounts(
			&ccm_accounts,
			vec![sol_api_environment.token_vault_pda_account.into(), agg_key.into()],
		)
		.map_err(SolanaTransactionBuildingError::InvalidCcm)?;

		let compute_price = Environment::compute_price()?;

		let durable_nonce = Environment::nonce_account()?;

		// Get the Address lookup tables. Chainflip's ALT is proceeded with the User's.
		let external_alts = ccm_additional_data
			.alt_addresses()
			.map(|alts| Environment::get_address_lookup_tables(BTreeSet::from_iter(alts)))
			.transpose()
			.inspect_err(|_| {
				// This can be removed once the transaction building is made transactional.
				// Recover the durable nonce when transaction building fails.
				Environment::recover_durable_nonce(durable_nonce.0);
			})?
			.unwrap_or_default();

		let address_lookup_tables =
			sp_std::iter::once(sol_api_environment.address_lookup_table_account)
				.chain(external_alts)
				.collect::<Vec<_>>();

		let compute_limit =
			SolTrackedData::calculate_ccm_compute_limit(gas_budget, transfer_param.asset);

		Ok(Self {
			call_type: SolanaTransactionType::CcmTransfer {
				fallback: TransferAssetParams {
					asset: transfer_param.asset,
					amount: transfer_param.amount,
					to: ccm_accounts.fallback_address.into(),
				},
			},
			transaction: match transfer_param.asset {
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
					compute_limit,
					address_lookup_tables,
				),
				SolAsset::SolUsdc => SolanaTransactionBuilder::ccm_transfer_token(
					derive_associated_token_account(
						transfer_param.to,
						sol_api_environment.usdc_token_mint_pubkey,
					)
					.map_err(SolanaTransactionBuildingError::FailedToDeriveAddress)?
					.address,
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
					compute_limit,
					address_lookup_tables,
				),
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
			})?,
			signer: None,
			_phantom: Default::default(),
		})
	}

	pub fn fetch_and_batch_close_vault_swap_accounts(
		vault_swap_accounts: Vec<VaultSwapAccountAndSender>,
	) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup environment variables, such as aggkey and durable nonce.
		let agg_key = Environment::current_agg_key()?;
		let sol_api_environment = Environment::api_environment()?;
		let compute_price = Environment::compute_price()?;
		let durable_nonce = Environment::nonce_account()?;

		// Build the transaction
		let transaction = SolanaTransactionBuilder::fetch_and_close_vault_swap_accounts(
			vault_swap_accounts,
			sol_api_environment.vault_program_data_account,
			sol_api_environment.swap_endpoint_program,
			sol_api_environment.swap_endpoint_program_data_account,
			agg_key,
			durable_nonce,
			compute_price,
			vec![sol_api_environment.address_lookup_table_account],
		)?;

		Ok(Self {
			call_type: SolanaTransactionType::CloseEventAccounts,
			transaction,
			signer: None,
			_phantom: Default::default(),
		})
	}

	pub fn set_program_swaps_parameters(
		min_native_swap_amount: u64,
		max_dst_address_len: u16,
		max_ccm_message_len: u32,
		max_cf_parameters_len: u32,
		max_event_accounts: u32,
	) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup environment variables, such as aggkey and durable nonce.
		let agg_key = Environment::current_agg_key()?;
		let sol_api_environment = Environment::api_environment()?;
		let compute_price = Environment::compute_price()?;
		let durable_nonce = Environment::nonce_account()?;

		// Build the transaction
		let transaction = SolanaTransactionBuilder::set_program_swaps_parameters(
			min_native_swap_amount,
			max_dst_address_len,
			max_ccm_message_len,
			max_cf_parameters_len,
			max_event_accounts,
			sol_api_environment.vault_program,
			sol_api_environment.vault_program_data_account,
			// Assumed that the agg_key is the gov_key in the on-chain programs.
			// This assumption is valid until we change the key to some independent Governance key.
			agg_key,
			durable_nonce,
			compute_price,
			vec![sol_api_environment.address_lookup_table_account],
		)?;

		Ok(Self {
			call_type: SolanaTransactionType::SetProgramSwapParameters,
			transaction,
			signer: None,
			_phantom: Default::default(),
		})
	}
	pub fn set_token_swap_parameters(
		min_swap_amount: u64,
		token_mint_pubkey: SolAddress,
	) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup environment variables, such as aggkey and durable nonce.
		let agg_key = Environment::current_agg_key()?;
		let sol_api_environment = Environment::api_environment()?;
		let compute_price = Environment::compute_price()?;
		let durable_nonce = Environment::nonce_account()?;

		// Build the transaction
		let transaction = SolanaTransactionBuilder::enable_token_support(
			min_swap_amount,
			sol_api_environment.vault_program,
			sol_api_environment.vault_program_data_account,
			token_mint_pubkey,
			// Assumed that the agg_key is the gov_key in the on-chain programs.
			// This assumption is valid until we change the key to some independent Governance key.
			agg_key,
			durable_nonce,
			compute_price,
			vec![sol_api_environment.address_lookup_table_account],
		)?;

		Ok(Self {
			call_type: SolanaTransactionType::SetTokenSwapParameters,
			transaction,
			signer: None,
			_phantom: Default::default(),
		})
	}

	pub fn upgrade_program(
		program_address: SolAddress,
		buffer_address: SolAddress,
		spill_address: SolAddress,
	) -> Result<Self, SolanaTransactionBuildingError> {
		// Lookup environment variables, such as aggkey and durable nonce.
		let agg_key = Environment::current_agg_key()?;
		let sol_api_environment = Environment::api_environment()?;
		let compute_price = Environment::compute_price()?;
		let durable_nonce = Environment::nonce_account()?;

		// Build the transaction
		let transaction = SolanaTransactionBuilder::upgrade_program(
			program_address,
			buffer_address,
			sol_api_environment.vault_program,
			sol_api_environment.vault_program_data_account,
			// Assumed that the agg_key is the gov_key in the on-chain programs.
			// This assumption is valid until we change the key to some independent Governance key.
			agg_key,
			spill_address,
			durable_nonce,
			compute_price,
			vec![sol_api_environment.address_lookup_table_account],
		)?;

		Ok(Self {
			call_type: SolanaTransactionType::UpgradeProgram,
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
	fn new_unsigned_impl(
		_maybe_old_key: Option<<SolanaCrypto as ChainCrypto>::AggKey>,
		new_key: <SolanaCrypto as ChainCrypto>::AggKey,
	) -> Result<Option<Self>, crate::SetAggKeyWithAggKeyError> {
		Self::rotate_agg_key(new_key).map(Some).map_err(|e| {
			log::warn!("Failed to construct Solana Rotate Agg key transaction: {:?}", e);
			crate::SetAggKeyWithAggKeyError::FinalTransactionExceededMaxLength
		})
	}
}

impl<Env: 'static + SolanaEnvironment> ExecutexSwapAndCall<Solana> for SolanaApi<Env> {
	fn new_unsigned_impl(
		transfer_param: TransferAssetParams<Solana>,
		source_chain: cf_primitives::ForeignChain,
		_source_address: Option<ForeignChainAddress>,
		gas_budget: GasAmount,
		message: Vec<u8>,
		ccm_additional_data: DecodedCcmAdditionalData,
	) -> Result<Self, ExecutexSwapAndCallError> {
		Self::ccm_transfer(
			transfer_param,
			source_chain,
			// Hardcoding this to None to gain extra bytes in Solana. Consider
			// reverting this when we implement versioned Transactions. PRO-2181.
			None,
			gas_budget,
			message,
			match ccm_additional_data {
				DecodedCcmAdditionalData::NotRequired => Err(ExecutexSwapAndCallError::Unsupported),
				DecodedCcmAdditionalData::Solana(versioned_solana_ccm_additional_data) =>
					Ok(versioned_solana_ccm_additional_data),
			}?,
		)
		.map_err(|e| {
			log::warn!("Failed to construct Solana CCM transfer transaction: {:?}", e);
			match e {
				SolanaTransactionBuildingError::AltsNotYetWitnessed =>
					ExecutexSwapAndCallError::AuxDataNotReady,
				_ => ExecutexSwapAndCallError::FailedToBuildCcmForSolana(e),
			}
		})
	}
}

impl<Env: 'static + SolanaEnvironment> AllBatch<Solana> for SolanaApi<Env> {
	fn new_unsigned_impl(
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
	fn new_unsigned_impl(
		_transfer_param: TransferAssetParams<Solana>,
	) -> Result<Self, TransferFallbackError> {
		Err(TransferFallbackError::Unsupported)
	}
}

impl<Env: 'static + SolanaEnvironment> FetchAndCloseSolanaVaultSwapAccounts for SolanaApi<Env> {
	fn new_unsigned_impl(
		accounts: Vec<VaultSwapAccountAndSender>,
	) -> Result<Self, SolanaTransactionBuildingError> {
		Self::fetch_and_batch_close_vault_swap_accounts(accounts)
	}
}

impl<Environment: SolanaEnvironment> SetGovKeyWithAggKey<SolanaCrypto> for SolanaApi<Environment> {
	fn new_unsigned_impl(
		_maybe_old_key: Option<<SolanaCrypto as ChainCrypto>::GovKey>,
		new_gov_key: <SolanaCrypto as ChainCrypto>::GovKey,
	) -> Result<Self, SetGovKeyWithAggKeyError> {
		// Lookup environment variables, such as aggkey and durable nonce.
		let agg_key = Environment::current_agg_key()
			.map_err(|_e| SetGovKeyWithAggKeyError::CurrentAggKeyUnavailable)?;
		let sol_api_environment = Environment::api_environment()
			.map_err(|_e| SetGovKeyWithAggKeyError::SolApiEnvironmentUnavailable)?;
		let compute_price = Environment::compute_price()
			.map_err(|_e| SetGovKeyWithAggKeyError::ComputePriceUnavailable)?;
		let durable_nonce = Environment::nonce_account()
			.map_err(|_e| SetGovKeyWithAggKeyError::NonceUnavailable)?;

		// Build the transaction
		let transaction = SolanaTransactionBuilder::set_gov_key_with_agg_key(
			new_gov_key,
			sol_api_environment.vault_program,
			sol_api_environment.vault_program_data_account,
			agg_key,
			durable_nonce,
			compute_price,
			vec![sol_api_environment.address_lookup_table_account],
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
			SetGovKeyWithAggKeyError::FailedToBuildAPICall
		})?;

		Ok(Self {
			call_type: SolanaTransactionType::SetGovKeyWithAggKey,
			transaction,
			signer: None,
			_phantom: Default::default(),
		})
	}
}

impl<Environment: 'static + SolanaEnvironment> RejectCall<Solana> for SolanaApi<Environment> {
	fn new_unsigned(
		_deposit_details: <Solana as Chain>::DepositDetails,
		refund_address: <Solana as Chain>::ChainAccount,
		refund_amount: Option<<Solana as Chain>::ChainAmount>,
		asset: <Solana as Chain>::ChainAsset,
		deposit_fetch_id: Option<<Solana as Chain>::DepositFetchId>,
	) -> Result<Self, RejectError> {
		// TODO: I think we should probably fetch (for deposit channels) if the amount
		// is zero as we might still need to fetch an amount that has been taken as fees
		let amount = refund_amount.ok_or(RejectError::NotRequired)?;
		if amount == 0_u64 {
			return Err(RejectError::NotRequired);
		}

		// Lookup environment variables
		let agg_key = Environment::current_agg_key().map_err(|_| RejectError::Other)?;
		let sol_api_environment = Environment::api_environment().map_err(|_| RejectError::Other)?;
		let compute_price = Environment::compute_price().map_err(|_| RejectError::Other)?;
		let durable_nonce = Environment::nonce_account().map_err(|_| RejectError::Other)?;

		let transaction = match (deposit_fetch_id, asset) {
			(Some(fetch_id), SolAsset::Sol) => {
				let fetch_param = FetchAssetParams { deposit_fetch_id: fetch_id, asset };
				SolanaTransactionBuilder::refund_native(
					fetch_param,
					amount,
					refund_address,
					sol_api_environment,
					agg_key,
					durable_nonce,
					compute_price,
				)
			},
			(Some(fetch_id), SolAsset::SolUsdc) => {
				let ata = derive_associated_token_account(
					refund_address,
					sol_api_environment.usdc_token_mint_pubkey,
				)
				.map_err(|_| RejectError::Other)?;
				let fetch_param = FetchAssetParams { deposit_fetch_id: fetch_id, asset };
				SolanaTransactionBuilder::refund_token(
					fetch_param,
					ata.address,
					amount,
					refund_address,
					sol_api_environment.vault_program,
					sol_api_environment.vault_program_data_account,
					sol_api_environment.token_vault_pda_account,
					sol_api_environment.usdc_token_vault_ata,
					sol_api_environment.usdc_token_mint_pubkey,
					SOL_USDC_DECIMAL,
					sol_api_environment.clone(),
					agg_key,
					durable_nonce,
					compute_price,
					vec![sol_api_environment.clone().address_lookup_table_account],
				)
			},
			// TODO: This will be the case for Vault swaps that don't require
			// fetching but unclear if we will get no fetch_id or we need to
			// figure it out from the deposit details.
			(None, SolAsset::Sol) => {
				SolanaTransactionBuilder::transfer_native(
					amount,
					refund_address,
					agg_key,
					durable_nonce,
					compute_price,
				)
			},
			(None, SolAsset::SolUsdc) => {
				let ata = derive_associated_token_account(
					refund_address,
					sol_api_environment.usdc_token_mint_pubkey,
				)
				.map_err(|_| RejectError::Other)?;
				SolanaTransactionBuilder::transfer_token(
					ata.address,
					amount,
					refund_address,
					sol_api_environment.vault_program,
					sol_api_environment.vault_program_data_account,
					sol_api_environment.token_vault_pda_account,
					sol_api_environment.usdc_token_vault_ata,
					sol_api_environment.usdc_token_mint_pubkey,
					agg_key,
					durable_nonce,
					compute_price,
					SOL_USDC_DECIMAL,
					vec![sol_api_environment.address_lookup_table_account.clone()],
				)
			},
		}
		.map_err(|_| RejectError::Other)?;

		Ok(Self {
			call_type: SolanaTransactionType::Refund,
			transaction,
			signer: None,
			_phantom: Default::default(),
		})
	}
}

#[derive(
	Serialize,
	Deserialize,
	RuntimeDebug,
	PartialEq,
	Eq,
	Clone,
	Encode,
	Decode,
	TypeInfo,
	Ord,
	PartialOrd,
)]
pub enum AltWitnessingConsensusResult<T> {
	Valid(T),
	Invalid,
}
