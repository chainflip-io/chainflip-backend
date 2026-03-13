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
	elections::voter_api::{CompositeVoter, VoterApi},
	evm::event::{Event, EvmEventSource},
	tron::{
		cached_rpc::{TronCachingClient, TronRetryRpcApiWithResult},
		rpc::TronRpcSigningClient,
		rpc_client_api::TransactionInfo,
	},
	witness::{
		common::{
			block_height_witnesser::witness_headers,
			block_witnesser::GenericBwVoter,
			traits::{WitnessClient, WitnessClientForBlockData},
		},
		eth_elections::EvmSingleBlockQuery,
		evm::{
			erc20_deposits::usdt::UsdtEvents,
			key_manager::{
				AggKeySetByGovKeyFilter, GovernanceActionFilter, KeyManagerEvents,
				SignatureAcceptedFilter,
			},
		},
		tron::{
			tron_deposits::witness_deposit_channels, vault_swaps_witnessing::witness_vault_swaps,
			TransferNativeFailedFilter, TransferTokenFailedFilter,
			TronDepositChannelWitnessingConfig, TronVaultEvents, VaultDepositWitnessingConfig,
		},
	},
};
use anyhow::{Context, Result};
use cf_chains::{
	evm::{AggKey, SchnorrVerificationComponents},
	tron::{TronTransactionFee, TronTransactionMetadata},
	Chain, DepositChannel, Tron,
};
use cf_primitives::chains::assets::tron::Asset as TronAsset;
use cf_utilities::task_scope::{self, Scope};
use engine_sc_client::{
	chain_api::ChainApi, electoral_api::ElectoralApi, extrinsic_api::signed::SignedExtrinsicApi,
	storage_api::StorageApi,
};
use ethers::types::H256;
use futures::FutureExt;
use itertools::Itertools;
use pallet_cf_broadcast::TransactionConfirmation;
use pallet_cf_elections::{
	electoral_systems::block_height_witnesser::{
		primitives::Header, ChainBlockHashOf, ChainBlockNumberOf,
	},
	ElectoralSystemTypes, VoteOf,
};
use pallet_cf_ingress_egress::{DepositWitness, TransferFailedWitness};
use pallet_cf_vaults::VaultKeyRotatedExternally;
use sp_core::H160;
use state_chain_runtime::{
	chainflip::witnessing::{
		pallet_hooks::{EvmKeyManagerEvent, EvmVaultContractEvent},
		tron_elections::{
			TronBlockHeightWitnesserES, TronChain, TronElectoralSystemRunner, TronLiveness,
			TRON_MAINNET_SAFETY_BUFFER,
		},
	},
	Runtime, TronInstance,
};
use std::{collections::HashMap, sync::Arc};

// ------------------------------------------
// ---        TronVoter struct            ---
// ------------------------------------------

#[derive(Clone)]
pub struct TronVoter {
	client: TronCachingClient<TronRpcSigningClient>,
}

impl TronVoter {
	pub fn new(client: TronCachingClient<TronRpcSigningClient>) -> Self {
		Self { client }
	}

	async fn events_from_block_query<Data: std::fmt::Debug>(
		&self,
		event_source: &EvmEventSource<Data>,
		query: &EvmSingleBlockQuery,
	) -> Result<Vec<Event<Data>>> {
		let logs = self.client.get_logs(query.block_hash, event_source.contract_address).await?;
		Ok(logs
			.into_iter()
			.filter_map(|log| {
				event_source
					.event_type
					.parse_log(log)
					.map_err(|err| {
						tracing::error!(
							"Event for contract {} could not be decoded in block {:?}. Error: {err}",
							event_source.contract_address,
							query.block_hash
						)
					})
					.ok()
			})
			.collect())
	}
}

// ------------------------------------------
// ---    WitnessClient<TronChain>        ---
// ------------------------------------------

#[async_trait::async_trait]
impl WitnessClient<TronChain> for TronVoter {
	type BlockQuery = EvmSingleBlockQuery;

	async fn best_block_number(&self) -> Result<u64> {
		Ok(self.client.get_block_number().await?.low_u64())
	}

	async fn best_block_header(&self) -> Result<Header<TronChain>> {
		let best_number = self.client.get_block_number().await?;
		self.block_header_by_height(best_number.low_u64()).await
	}

	async fn block_header_by_height(&self, height: u64) -> Result<Header<TronChain>> {
		let block = self.client.block(height.into()).await?;
		Ok(Header {
			block_height: block.number.ok_or_else(|| anyhow::anyhow!("No block number"))?.low_u64(),
			hash: block.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?,
			parent_hash: block.parent_hash,
		})
	}

	async fn block_query_from_hash_and_height(
		&self,
		hash: ChainBlockHashOf<TronChain>,
		_height: ChainBlockNumberOf<TronChain>,
	) -> Result<EvmSingleBlockQuery> {
		EvmSingleBlockQuery::try_from_native_block(self.client.block_by_hash(hash).await?)
	}

	async fn block_query_from_height(&self, height: u64) -> Result<EvmSingleBlockQuery> {
		EvmSingleBlockQuery::try_from_native_block(self.client.block(height.into()).await?)
	}

	async fn block_query_and_hash_from_height(
		&self,
		height: u64,
	) -> Result<(EvmSingleBlockQuery, ChainBlockHashOf<TronChain>)> {
		let block = self.client.block(height.into()).await?;
		let hash = block.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?;
		let query = EvmSingleBlockQuery::try_from_native_block(block)?;
		Ok((query, hash))
	}
}

// ------------------------------------------
// ---    TronRetryRpcApiWithResult       ---
// ---    delegation for TronVoter        ---
// ------------------------------------------

#[async_trait::async_trait]
impl TronRetryRpcApiWithResult for TronVoter {
	async fn get_transaction_info_by_id(
		&self,
		tx_id: &str,
	) -> anyhow::Result<crate::tron::rpc_client_api::TransactionInfo> {
		self.client.get_transaction_info_by_id(tx_id).await
	}

	async fn get_transaction_by_id(
		&self,
		tx_id: &str,
	) -> anyhow::Result<crate::tron::rpc_client_api::Transaction> {
		self.client.get_transaction_by_id(tx_id).await
	}

	async fn get_block_balances(
		&self,
		block_number: crate::tron::rpc_client_api::BlockNumber,
		hash: &str,
	) -> anyhow::Result<crate::tron::rpc_client_api::BlockBalance> {
		self.client.get_block_balances(block_number, hash).await
	}

	async fn chain_id(&self) -> anyhow::Result<ethers::types::U256> {
		self.client.chain_id().await
	}

	async fn get_logs(
		&self,
		block_hash: H256,
		contract_address: H160,
	) -> anyhow::Result<Vec<ethers::types::Log>> {
		self.client.get_logs(block_hash, contract_address).await
	}

	async fn transaction_receipt(
		&self,
		tx_hash: H256,
	) -> anyhow::Result<ethers::types::TransactionReceipt> {
		self.client.transaction_receipt(tx_hash).await
	}

	async fn block(
		&self,
		block_number: ethers::types::U64,
	) -> anyhow::Result<ethers::types::Block<H256>> {
		self.client.block(block_number).await
	}

	async fn block_by_hash(&self, block_hash: H256) -> anyhow::Result<ethers::types::Block<H256>> {
		self.client.block_by_hash(block_hash).await
	}

	async fn block_with_txs(
		&self,
		block_number: ethers::types::U64,
	) -> anyhow::Result<ethers::types::Block<ethers::types::Transaction>> {
		self.client.block_with_txs(block_number).await
	}

	async fn get_transaction(&self, tx_hash: H256) -> anyhow::Result<ethers::types::Transaction> {
		self.client.get_transaction(tx_hash).await
	}

	async fn get_block_number(&self) -> anyhow::Result<ethers::types::U64> {
		self.client.get_block_number().await
	}
}

// ------------------------------------------
// ---    Block height witnessing         ---
// ------------------------------------------

#[async_trait::async_trait]
impl VoterApi<TronBlockHeightWitnesserES> for TronVoter {
	async fn vote(
		&self,
		_settings: <TronBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <TronBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<TronBlockHeightWitnesserES>>> {
		witness_headers::<TronBlockHeightWitnesserES, _, TronChain>(
			self,
			properties,
			TRON_MAINNET_SAFETY_BUFFER,
			"TRON BHW",
		)
		.await
	}
}

// ------------------------------------------
// ---    Deposit channel witnessing      ---
// ------------------------------------------

#[async_trait::async_trait]
impl WitnessClientForBlockData<TronChain, Vec<DepositWitness<Tron>>> for TronVoter {
	type ElectionProperties = Vec<DepositChannel<Tron>>;
	type Config = TronDepositChannelWitnessingConfig;

	async fn block_data_from_query(
		&self,
		config: &TronDepositChannelWitnessingConfig,
		deposit_channels: &Vec<DepositChannel<Tron>>,
		query: &EvmSingleBlockQuery,
	) -> Result<Vec<DepositWitness<Tron>>> {
		witness_deposit_channels(self, config, query, deposit_channels.clone()).await
	}
}

// ------------------------------------------
// ---    Vault deposit witnessing        ---
// ------------------------------------------

#[async_trait::async_trait]
impl WitnessClientForBlockData<TronChain, Vec<EvmVaultContractEvent<Runtime, TronInstance>>>
	for TronVoter
{
	type Config = VaultDepositWitnessingConfig;

	async fn block_data_from_query(
		&self,
		config: &VaultDepositWitnessingConfig,
		_properties: &(),
		query: &EvmSingleBlockQuery,
	) -> Result<Vec<EvmVaultContractEvent<Runtime, TronInstance>>> {
		let vault_swaps = witness_vault_swaps(self, config, query).await?;

		let mut result: Vec<EvmVaultContractEvent<Runtime, TronInstance>> = vault_swaps
			.into_iter()
			.map(|deposit| EvmVaultContractEvent::VaultDeposit(Box::new(deposit)))
			.collect();

		// Fetch vault contract events for TransferFailed witnessing. This is different from other
		// EVMs because Vault Swaps in TRON don't use events.
		let events = self.events_from_block_query(&config.vault_events, query).await?;

		for event in events {
			match event.event_parameters {
				TronVaultEvents::TransferNativeFailedFilter(TransferNativeFailedFilter {
					recipient,
					amount,
				}) => {
					result.push(EvmVaultContractEvent::TransferFailed(TransferFailedWitness {
						asset: <Tron as Chain>::GAS_ASSET,
						amount: amount.as_u128(),
						destination_address: recipient,
					}));
				},
				TronVaultEvents::TransferTokenFailedFilter(TransferTokenFailedFilter {
					recipient,
					amount,
					token,
					reason: _,
				}) => {
					if let Some((&asset, _)) =
						config.supported_assets.iter().find(|(_, es)| es.contract_address == token)
					{
						result.push(EvmVaultContractEvent::TransferFailed(TransferFailedWitness {
							asset,
							amount: amount.as_u128(),
							destination_address: recipient,
						}));
					} else {
						tracing::warn!("Unknown token {token:?} in Tron TransferTokenFailed event");
					}
				},
				// Skip all other vault events
				_ => {},
			}
		}

		Ok(result.into_iter().sorted().collect())
	}
}

// ------------------------------------------
// ---    Key manager witnessing          ---
// ------------------------------------------

#[derive(Clone)]
pub struct TronKeyManagerWitnessingConfig {
	pub key_manager: EvmEventSource<KeyManagerEvents>,
}

fn parse_tron_tx_fee(tx_info: &TransactionInfo) -> TronTransactionFee {
	let receipt = &tx_info.receipt;

	// Transaction is queried if an event is emitted so it should always succed.
	// We just log it as a warning because rpcs are not very reliable.
	if let Some(result) = &tx_info.result {
		if result != "SUCCESS" {
			tracing::warn!("Transaction result is not SUCCESS: {result}");
		}
	}

	// Pass all the relevant information to the State Chain in case we want to use it to adjust
	// energy estimates in the future.
	TronTransactionFee {
		fee: tx_info.fee.unwrap_or(0).try_into().unwrap_or(0),
		energy_usage: Some(receipt.energy_usage.unwrap_or(0).try_into().unwrap_or(0)),
		energy_fee: Some(receipt.energy_fee.unwrap_or(0).try_into().unwrap_or(0)),
		origin_energy_usage: Some(receipt.origin_energy_usage.unwrap_or(0).try_into().unwrap_or(0)),
		energy_usage_total: Some(receipt.energy_usage_total.unwrap_or(0).try_into().unwrap_or(0)),
		net_usage: Some(receipt.net_usage.unwrap_or(0).try_into().unwrap_or(0)),
		net_fee: Some(receipt.net_fee.unwrap_or(0).try_into().unwrap_or(0)),
		energy_penalty_total: Some(
			receipt.energy_penalty_total.unwrap_or(0).try_into().unwrap_or(0),
		),
	}
}

#[async_trait::async_trait]
impl WitnessClientForBlockData<TronChain, Vec<EvmKeyManagerEvent<Runtime, TronInstance>>>
	for TronVoter
{
	type Config = TronKeyManagerWitnessingConfig;

	async fn block_data_from_query(
		&self,
		config: &TronKeyManagerWitnessingConfig,
		_properties: &(),
		query: &EvmSingleBlockQuery,
	) -> Result<Vec<EvmKeyManagerEvent<Runtime, TronInstance>>> {
		let block_height = query.block_height;

		let events = self.events_from_block_query(&config.key_manager, query).await?;

		let mut result = Vec::new();

		// Not reusing the other EVM KeyManager logic because the fee/energy data differs.
		for event in events {
			let tx_hash = event.tx_hash;
			let km_event = match event.event_parameters {
				KeyManagerEvents::AggKeySetByGovKeyFilter(AggKeySetByGovKeyFilter {
					new_agg_key,
					..
				}) => Some(EvmKeyManagerEvent::AggKeySetByGovKey(VaultKeyRotatedExternally {
					new_public_key: AggKey::from_pubkey_compressed(new_agg_key.serialize()),
					block_number: block_height,
					tx_id: tx_hash,
				})),
				KeyManagerEvents::SignatureAcceptedFilter(SignatureAcceptedFilter {
					sig_data,
					..
				}) => {
					let tx_hash_str = format!("{:x}", tx_hash);

					let tx_receipt =
						self.client.transaction_receipt(tx_hash).await.with_context(|| {
							format!("Failed to get receipt for tx {tx_hash_str}")
						})?;

					// Compared to other EVMs we need to get the energy (fees) information via
					// `get_transaction_info_by_id` and the `get_transaction_by_id` to get the
					// `energy_limit` metadata.
					let tx_info =
						self.client.get_transaction_info_by_id(&tx_hash_str).await.with_context(
							|| format!("Failed to get transaction info for tx {tx_hash_str}"),
						)?;
					let tron_tx =
						self.client.get_transaction_by_id(&tx_hash_str).await.with_context(
							|| format!("Failed to get transaction by id for tx {tx_hash_str}"),
						)?;

					let signer_id = tx_receipt.from;
					let contract = tx_receipt.to.ok_or_else(|| {
						anyhow::anyhow!("No to address in receipt for tx {tx_hash_str}")
					})?;
					let fee_limit = tron_tx.raw_data.fee_limit.and_then(|fee| fee.try_into().ok());

					let tx_fee = parse_tron_tx_fee(&tx_info);
					let tx_metadata = TronTransactionMetadata { contract, fee_limit };

					Some(EvmKeyManagerEvent::SignatureAccepted(TransactionConfirmation {
						tx_out_id: SchnorrVerificationComponents {
							s: sig_data.sig.to_big_endian(),
							k_times_g_address: sig_data.k_times_g_address.into(),
						},
						signer_id,
						tx_fee,
						tx_metadata,
						transaction_ref: tx_hash,
					}))
				},
				KeyManagerEvents::GovernanceActionFilter(GovernanceActionFilter {
					message: call_hash,
				}) => Some(EvmKeyManagerEvent::SetWhitelistedCallHash(call_hash)),
				_ => None,
			};
			if let Some(km_event) = km_event {
				result.push(km_event);
			}
		}

		Ok(result.into_iter().sorted().collect())
	}
}

// ------------------------------------------
// ---    Liveness witnessing             ---
// ------------------------------------------

#[derive(Clone)]
pub struct TronLivenessVoter {
	client: TronCachingClient<TronRpcSigningClient>,
}

#[async_trait::async_trait]
impl VoterApi<TronLiveness> for TronLivenessVoter {
	async fn vote(
		&self,
		_settings: <TronLiveness as ElectoralSystemTypes>::ElectoralSettings,
		properties: <TronLiveness as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<TronLiveness>>> {
		let block = self.client.block(properties.into()).await?;
		Ok(Some(block.hash.ok_or_else(|| anyhow::anyhow!("No block hash for liveness block"))?))
	}
}

// ------------------------------------------
// ---    starting all tron voters        ---
// ------------------------------------------

pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	client: TronCachingClient<TronRpcSigningClient>,
	state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	StateChainClient: StorageApi
		+ ChainApi
		+ SignedExtrinsicApi
		+ ElectoralApi<TronInstance>
		+ 'static
		+ Send
		+ Sync,
{
	tracing::debug!("Starting TRON witness");

	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::TronKeyManagerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get KeyManager address from SC")?;

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::TronVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let supported_erc20_tokens: HashMap<TronAsset, H160> = state_chain_client
		.storage_map::<pallet_cf_environment::TronSupportedAssets<state_chain_runtime::Runtime>, _>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to fetch Tron supported assets")?;

	let usdt_contract_address =
		*supported_erc20_tokens.get(&TronAsset::TronUsdt).context("USDT not supported")?;

	let usdt_event_source = EvmEventSource::new::<UsdtEvents>(usdt_contract_address);

	let vault_event_source = EvmEventSource::new::<TronVaultEvents>(vault_address);

	let deposit_channel_config = TronDepositChannelWitnessingConfig {
		vault_contract: vault_event_source.clone(),
		supported_assets: [(TronAsset::TronUsdt, usdt_event_source.clone())].into_iter().collect(),
	};

	let vault_deposit_config = VaultDepositWitnessingConfig {
		vault: vault_address,
		vault_events: vault_event_source,
		supported_assets: [(TronAsset::TronUsdt, usdt_event_source)].into_iter().collect(),
	};

	let key_manager_config = TronKeyManagerWitnessingConfig {
		key_manager: EvmEventSource::new::<KeyManagerEvents>(key_manager_address),
	};

	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<TronElectoralSystemRunner, _>::new((
						TronVoter::new(client.clone()),
						GenericBwVoter::new(TronVoter::new(client.clone()), deposit_channel_config),
						GenericBwVoter::new(TronVoter::new(client.clone()), vault_deposit_config),
						GenericBwVoter::new(TronVoter::new(client.clone()), key_manager_config),
						TronLivenessVoter { client: client.clone() },
					)),
					Some(client.cache_invalidation_senders),
					"Tron",
				)
				.continuously_vote()
				.await;

				Ok(())
			}
			.boxed()
		})
		.await
	});

	Ok(())
}
