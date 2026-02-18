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
	evm::{
		cached_rpc::{EvmCachingClient, EvmRetryRpcApiWithResult},
		event::{Event, EvmEventSource},
		rpc::{address_checker::AddressState, EvmRpcSigningClient},
	},
	witness::{
		common::{
			block_height_witnesser::witness_headers,
			block_witnesser::GenericBwVoter,
			traits::{WitnessClient, WitnessClientForBlockData},
		},
		eth::{
			sc_utils::{
				CallScFilter, DepositAndScCallFilter, DepositToScGatewayAndScCallFilter,
				DepositToVaultAndScCallFilter, ScUtilsEvents,
			},
			state_chain_gateway::{
				FundedFilter, RedemptionExecutedFilter, RedemptionExpiredFilter,
				StateChainGatewayEvents,
			},
		},
		evm::{
			contract_common::address_states,
			erc20_deposits::{
				flip::FlipEvents, usdc::UsdcEvents, usdt::UsdtEvents, wbtc::WbtcEvents,
			},
			key_manager::KeyManagerEvents,
			vault::VaultEvents,
			EvmAddressStateClient, EvmBlockQuery, EvmDepositChannelWitnessingConfig,
			EvmEventClient, EvmKeyManagerWitnessingConfig, EvmVoter, VaultDepositWitnessingConfig,
		},
	},
};
use cf_chains::{assets, eth::EthereumTrackedData, evm::ToAccountId32};
use cf_primitives::chains::assets::eth::Asset as EthAsset;
use cf_utilities::{
	context,
	task_scope::{self, Scope},
};
use engine_sc_client::{
	chain_api::ChainApi, electoral_api::ElectoralApi, extrinsic_api::signed::SignedExtrinsicApi,
	storage_api::StorageApi, STATE_CHAIN_CONNECTION,
};
use ethers::{
	abi::ethereum_types::BloomInput,
	types::{Block, Bloom},
};
use futures::FutureExt;
use itertools::Itertools;
use pallet_cf_elections::{
	electoral_systems::block_height_witnesser::{
		primitives::Header, ChainBlockHashOf, ChainBlockNumberOf, ChainTypes,
	},
	ElectoralSystemTypes, VoteOf,
};
use pallet_cf_funding::{EthereumDeposit, EthereumDepositAndSCCall};
use sp_core::{H160, H256};
use state_chain_runtime::{
	chainflip::witnessing::ethereum_elections::{
		EthereumBlockHeightWitnesserES, EthereumChain, EthereumElectoralSystemRunner,
		EthereumFeeTracking, EthereumLiveness, ScUtilsCall,
		StateChainGatewayEvent as ScGatewayEvent, ETHEREUM_MAINNET_SAFETY_BUFFER,
	},
	EthereumInstance,
};
use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};

// ------------ configuring header and block clients for ethereum ------------

#[derive(Clone, Debug)]
pub struct EvmSingleBlockQuery {
	pub block_height: u64,
	pub block_hash: H256,
	pub parent_hash: H256,
	pub bloom: Bloom,
}

impl EvmSingleBlockQuery {
	fn try_from_native_block(block: Block<H256>) -> Result<Self> {
		Ok(EvmSingleBlockQuery {
			block_height: block.number.ok_or_else(|| anyhow::anyhow!("No block number"))?.low_u64(),
			block_hash: block.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?,
			parent_hash: block.parent_hash,
			bloom: block.logs_bloom.unwrap_or(Bloom::repeat_byte(0xFFu8)),
		})
	}
}

impl EvmBlockQuery for EvmSingleBlockQuery {
	fn get_lowest_block_height_of_query(&self) -> u64 {
		self.block_height
	}
}
trait EvmSingleBlockChainTypes =
	ChainTypes<ChainBlockNumber = u64, ChainBlockHash = H256> + Sync + Send;

#[async_trait::async_trait]
impl<Chain: EvmSingleBlockChainTypes> WitnessClient<Chain>
	for EvmVoter<Chain, EvmSingleBlockQuery>
{
	type BlockQuery = EvmSingleBlockQuery;

	async fn best_block_number(&self) -> Result<u64> {
		Ok(self.client.get_block_number().await?.low_u64())
	}

	async fn best_block_header(&self) -> Result<Header<Chain>> {
		let best_number = self.client.get_block_number().await?;
		Ok(self.block_header_by_height(best_number.low_u64()).await?)
	}

	async fn block_header_by_height(&self, height: u64) -> Result<Header<Chain>> {
		let block = self.client.block(height.into()).await?;
		Ok(Header {
			block_height: block.number.ok_or_else(|| anyhow::anyhow!("No block number"))?.low_u64(),
			hash: block.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?,
			parent_hash: block.parent_hash,
		})
	}

	async fn block_query_from_hash_and_height(
		&self,
		hash: ChainBlockHashOf<Chain>,
		_height: ChainBlockNumberOf<Chain>,
	) -> Result<EvmSingleBlockQuery> {
		EvmSingleBlockQuery::try_from_native_block(self.client.block_by_hash(hash).await?)
	}

	async fn block_query_from_height(
		&self,
		height: Chain::ChainBlockNumber,
	) -> Result<Self::BlockQuery> {
		EvmSingleBlockQuery::try_from_native_block(self.client.block(height.into()).await?)
	}

	async fn block_query_and_hash_from_height(
		&self,
		height: Chain::ChainBlockNumber,
	) -> Result<(Self::BlockQuery, ChainBlockHashOf<EthereumChain>)> {
		let header = self.client.block(height.into()).await?;
		let hash = header.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?;
		let query = EvmSingleBlockQuery::try_from_native_block(header)?;
		Ok((query, hash))
	}
}

#[async_trait::async_trait]
impl<Chain: EvmSingleBlockChainTypes> EvmEventClient<Chain>
	for EvmVoter<Chain, EvmSingleBlockQuery>
{
	async fn events_from_block_query<Data: std::fmt::Debug>(
		&self,
		EvmEventSource { contract_address, event_type }: &EvmEventSource<Data>,
		query: Self::BlockQuery,
	) -> Result<Vec<Event<Data>>> {
		let mut contract_bloom = Bloom::default();
		contract_bloom.accrue(BloomInput::Raw(&contract_address.0));

		// if we have logs for this block, fetch them.
		let logs = if query.bloom.contains_bloom(&contract_bloom) {
			self.client.get_logs(query.block_hash, *contract_address).await?
		} else {
			// we know there won't be interesting logs, so don't fetch for events
			vec![]
		};
		Ok(logs
			.into_iter()
			.filter_map(|unparsed_log| -> Option<Event<Data>> {
				event_type
					.parse_log(unparsed_log)
					.map_err(|err| {
						tracing::error!(
						"event for contract {} could not be decoded in block {:?}. Error: {err}",
						contract_address,
						query.block_hash
					)
					})
					.ok()
			})
			.collect())
	}
}

#[async_trait::async_trait]
impl<Chain: EvmSingleBlockChainTypes> EvmAddressStateClient<Chain>
	for EvmVoter<Chain, EvmSingleBlockQuery>
{
	async fn address_states(
		&self,
		address_checker_address: H160,
		query: Self::BlockQuery,
		addresses: Vec<H160>,
	) -> Result<HashMap<H160, (AddressState, AddressState)>> {
		address_states(
			&self.client,
			address_checker_address,
			query.parent_hash,
			query.block_hash,
			addresses,
		)
		.await
	}
}

// --- block height witnessing ---

#[async_trait::async_trait]
impl VoterApi<EthereumBlockHeightWitnesserES> for EvmVoter<EthereumChain, EvmSingleBlockQuery> {
	async fn vote(
		&self,
		_settings: <EthereumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <EthereumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumBlockHeightWitnesserES>>, anyhow::Error> {
		witness_headers::<EthereumBlockHeightWitnesserES, _, EthereumChain>(
			self,
			properties,
			ETHEREUM_MAINNET_SAFETY_BUFFER,
			"ETH BHW",
		)
		.await
	}
}

// --- statechain gateway witnessing ---

#[derive(Clone)]
pub struct EthereumStateChainGatewayWitnessingConfig {
	state_chain_gateway: EvmEventSource<StateChainGatewayEvents>,
}

#[async_trait::async_trait]
impl WitnessClientForBlockData<EthereumChain, Vec<ScGatewayEvent>>
	for EvmVoter<EthereumChain, EvmSingleBlockQuery>
{
	type Config = EthereumStateChainGatewayWitnessingConfig;
	async fn block_data_from_query(
		&self,
		config: &Self::Config,
		_properties: &(),
		query: &Self::BlockQuery,
	) -> Result<Vec<ScGatewayEvent>> {
		let events =
			self.events_from_block_query(&config.state_chain_gateway, query.clone()).await?;

		let mut result: Vec<ScGatewayEvent> = Vec::new();
		for event in events {
			match event.event_parameters {
				StateChainGatewayEvents::FundedFilter(FundedFilter {
					node_id: account_id,
					amount,
					funder,
				}) => {
					result.push(ScGatewayEvent::Funded {
						account_id: account_id.into(),
						amount: amount.try_into().expect("Funded amount should fit in u128"),
						funder,
						tx_hash: event.tx_hash.into(),
					});
				},
				StateChainGatewayEvents::RedemptionExecutedFilter(RedemptionExecutedFilter {
					node_id: account_id,
					amount,
				}) => {
					result.push(ScGatewayEvent::RedemptionExecuted {
						account_id: account_id.into(),
						redeemed_amount: amount
							.try_into()
							.expect("Redemption amount should fit in u128"),
						tx_hash: event.tx_hash.into(),
					});
				},
				StateChainGatewayEvents::RedemptionExpiredFilter(RedemptionExpiredFilter {
					node_id: account_id,
					amount: _,
				}) => {
					result.push(ScGatewayEvent::RedemptionExpired {
						account_id: account_id.into(),
						block_number: query.block_height,
						tx_hash: event.tx_hash.into(),
					});
				},
				_ => {},
			}
		}

		Ok(result.into_iter().sorted().collect())
	}
}

// --- sc utils witnessing ---
#[derive(Clone)]
pub struct EthereumScUtilsWitnessingConfig {
	sc_utils: EvmEventSource<ScUtilsEvents>,
	supported_assets: HashMap<H160, assets::eth::Asset>,
}
#[async_trait::async_trait]
impl WitnessClientForBlockData<EthereumChain, Vec<ScUtilsCall>>
	for EvmVoter<EthereumChain, EvmSingleBlockQuery>
{
	type Config = EthereumScUtilsWitnessingConfig;
	async fn block_data_from_query(
		&self,
		config: &Self::Config,
		_properties: &(),
		query: &Self::BlockQuery,
	) -> Result<Vec<ScUtilsCall>> {
		let events = self.events_from_block_query(&config.sc_utils, query.clone()).await?;

		let mut result: Vec<ScUtilsCall> = Vec::new();
		for event in events {
			match event.event_parameters {
				ScUtilsEvents::DepositToScGatewayAndScCallFilter(
					DepositToScGatewayAndScCallFilter {
						sender,    // eth_address to attribute the FLIP to
						signer: _, // `tx.origin``. Not to be used for now
						amount,    // FLIP amount deposited
						sc_call,
					},
				) => result.push(ScUtilsCall {
					deposit_and_call: EthereumDepositAndSCCall {
						deposit: EthereumDeposit::FlipToSCGateway {
							amount: amount.try_into().expect("the amount should fit into u128 since all eth assets we support have max amounts smaller than u128::MAX"),
						},
						call: sc_call.to_vec(),
					},
					caller: sender,
					// use 0 padded ethereum address as account_id which the flip funds
					// are associated with on SC
					caller_account_id: sender.into_account_id_32(),
					eth_tx_hash: event.tx_hash.to_fixed_bytes(),
				}),
				ScUtilsEvents::DepositToVaultAndScCallFilter(
					DepositToVaultAndScCallFilter {
						sender,
						signer: _,
						amount,
						token,
						sc_call,
					},
				) => {
					if let Some(asset) = config.supported_assets.get(&token) {
						result.push(ScUtilsCall {
							deposit_and_call: EthereumDepositAndSCCall {
								deposit: EthereumDeposit::Vault {
									asset: *asset,
									amount: amount.try_into().expect("the amount should fit into u128 since all eth assets we support have max amounts smaller than u128::MAX"),
								},
								call: sc_call.to_vec(),
							},
							caller: sender,
							// use 0 padded ethereum address as account_id which the
							// flip funds are associated with on SC
							caller_account_id: sender.into_account_id_32(),
							eth_tx_hash: event.tx_hash.to_fixed_bytes(),
						});
					} else {
						continue;
					}
				},

				ScUtilsEvents::DepositAndScCallFilter(DepositAndScCallFilter {
					sender,
					signer: _,
					amount,
					token,
					to,
					sc_call,
				}) => {
					if let Some(asset) = config.supported_assets.get(&token) {
						result.push(ScUtilsCall {
							deposit_and_call: EthereumDepositAndSCCall {
								deposit: EthereumDeposit::Transfer {
									asset: *asset,
									amount: amount.try_into().expect("the amount should fit into u128 since all eth assets we support have max amounts smaller than u128::MAX"),
									destination: to,
								},
								call: sc_call.to_vec(),
							},
							caller: sender,
							// use 0 padded ethereum address as account_id which the
							// flip funds are associated with on SC
							caller_account_id: sender.into_account_id_32(),
							eth_tx_hash: event.tx_hash.to_fixed_bytes(),
						});
					} else {
						continue;
					}
				},

				ScUtilsEvents::CallScFilter(CallScFilter {
					sender,
					signer: _,
					sc_call,
				}) => result.push(ScUtilsCall {
					deposit_and_call: EthereumDepositAndSCCall {
						deposit: EthereumDeposit::NoDeposit,
						call: sc_call.to_vec(),
					},
					caller: sender,
					// use 0 padded ethereum address as account_id which the
					// flip funds are associated with on SC
					caller_account_id: sender.into_account_id_32(),
					eth_tx_hash: event.tx_hash.to_fixed_bytes(),
				}),
			}
		}

		Ok(result.into_iter().sorted().collect())
	}
}

// --- fee witnessing ---
#[derive(Clone)]
pub struct EthereumFeeVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}
#[async_trait::async_trait]
impl VoterApi<EthereumFeeTracking> for EthereumFeeVoter {
	async fn vote(
		&self,
		settings: <EthereumFeeTracking as ElectoralSystemTypes>::ElectoralSettings,
		_properties: <EthereumFeeTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumFeeTracking>>, anyhow::Error> {
		let (fee_history_window, priority_fee_percentile) = settings;

		let best_block_number = self.client.get_block_number().await?;
		let fee_history = self
			.client
			.fee_history(
				fee_history_window.into(),
				best_block_number.low_u64().into(),
				vec![priority_fee_percentile as f64],
			)
			.await?;

		Ok(Some(EthereumTrackedData {
			base_fee: (*context!(fee_history.base_fee_per_gas.last())?)
				.try_into()
				.expect("Base fee should fit u128"),
			priority_fee: context!(fee_history.reward.into_iter().flatten().min())?
				.try_into()
				.expect("Priority fee should fit u128"),
		}))
	}
}

// --- liveness witnessing ---
#[derive(Clone)]
pub struct EthereumLivenessVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}
#[async_trait::async_trait]
impl VoterApi<EthereumLiveness> for EthereumLivenessVoter {
	async fn vote(
		&self,
		_settings: <EthereumLiveness as ElectoralSystemTypes>::ElectoralSettings,
		properties: <EthereumLiveness as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumLiveness>>, anyhow::Error> {
		let block = self.client.block(properties.into()).await?;
		Ok(Some(block.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?))
	}
}

// ------------------------------------------
// ---    starting all ethereum voters    ---
// ------------------------------------------

pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	client: EvmCachingClient<EvmRpcSigningClient>,
	state_chain_client: Arc<StateChainClient>,
) -> Result<()>
where
	StateChainClient: StorageApi
		+ ChainApi
		+ SignedExtrinsicApi
		+ ElectoralApi<EthereumInstance>
		+ 'static
		+ Send
		+ Sync,
{
	tracing::debug!("Starting ETH witness");
	let state_chain_gateway_address = state_chain_client
        .storage_value::<pallet_cf_environment::EthereumStateChainGatewayAddress<state_chain_runtime::Runtime>>(
            state_chain_client.latest_finalized_block().hash,
        )
        .await
        .context("Failed to get StateChainGateway address from SC")?;

	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumKeyManagerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get KeyManager address from SC")?;

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let address_checker_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumAddressCheckerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect(STATE_CHAIN_CONNECTION);

	let supported_erc20_tokens: HashMap<EthAsset, H160> = state_chain_client
		.storage_map::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>, _>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to fetch Ethereum supported assets")?;

	let usdc_contract_address =
		*supported_erc20_tokens.get(&EthAsset::Usdc).context("USDC not supported")?;

	let flip_contract_address =
		*supported_erc20_tokens.get(&EthAsset::Flip).context("FLIP not supported")?;

	let usdt_contract_address =
		*supported_erc20_tokens.get(&EthAsset::Usdt).context("USDT not supported")?;

	let wbtc_contract_address =
		*supported_erc20_tokens.get(&EthAsset::Wbtc).context("WBTC not supported")?;

	let supported_erc20_tokens: HashMap<H160, assets::eth::Asset> = supported_erc20_tokens
		.into_iter()
		.map(|(asset, address)| (address, asset))
		.collect();

	let sc_utils_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumScUtilsAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect("Failed to get Sc Utils contract address from SC");

	let supported_asset_address_and_event_type = [
		(EthAsset::Usdc, EvmEventSource::new::<UsdcEvents>(usdc_contract_address)),
		(EthAsset::Usdt, EvmEventSource::new::<UsdtEvents>(usdt_contract_address)),
		(EthAsset::Flip, EvmEventSource::new::<FlipEvents>(flip_contract_address)),
		(EthAsset::Wbtc, EvmEventSource::new::<WbtcEvents>(wbtc_contract_address)),
	]
	.into_iter()
	.collect();

	let vault_event_source = EvmEventSource::new::<VaultEvents>(vault_address);
	let key_manager_event_source = EvmEventSource::new::<KeyManagerEvents>(key_manager_address);
	let sc_gateway_event_source =
		EvmEventSource::new::<StateChainGatewayEvents>(state_chain_gateway_address);
	let sc_utils_event_source = EvmEventSource::new::<ScUtilsEvents>(sc_utils_address);

	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<EthereumElectoralSystemRunner, _>::new((
						EvmVoter { client: client.clone(), _phantom: Default::default() },
						GenericBwVoter::new(
							EvmVoter::new(client.clone()),
							EvmDepositChannelWitnessingConfig {
								address_checker_address,
								vault_contract: vault_event_source.clone(),
								supported_assets: supported_asset_address_and_event_type,
							},
						),
						GenericBwVoter::new(
							EvmVoter::new(client.clone()),
							VaultDepositWitnessingConfig {
								vault: vault_event_source,
								supported_assets: supported_erc20_tokens.clone(),
							},
						),
						GenericBwVoter::new(
							EvmVoter::new(client.clone()),
							EvmKeyManagerWitnessingConfig { key_manager: key_manager_event_source },
						),
						EthereumFeeVoter { client: client.clone() },
						EthereumLivenessVoter { client: client.clone() },
						GenericBwVoter::new(
							EvmVoter::new(client.clone()),
							EthereumStateChainGatewayWitnessingConfig {
								state_chain_gateway: sc_gateway_event_source,
							},
						),
						GenericBwVoter::new(
							EvmVoter::new(client.clone()),
							EthereumScUtilsWitnessingConfig {
								sc_utils: sc_utils_event_source,
								supported_assets: supported_erc20_tokens,
							},
						),
					)),
					Some(client.cache_invalidation_senders),
					"Ethereum",
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
