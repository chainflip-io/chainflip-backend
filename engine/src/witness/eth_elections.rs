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
		rpc::EvmRpcSigningClient,
	},
	witness::{
		common::{
			block_height_witnesser::witness_headers,
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
			contract_common::{
				evm_event_type, evm_events_at_block, query_election_block, EvmEventType,
			},
			erc20_deposits::{
				flip::FlipEvents, usdc::UsdcEvents, usdt::UsdtEvents, wbtc::WbtcEvents, Erc20Events,
			},
			key_manager::{handle_key_manager_events, KeyManagerEventConfig, KeyManagerEvents},
			vault::{handle_vault_events, VaultEvents},
		},
	},
};
use cf_chains::{assets, eth::EthereumTrackedData, evm::ToAccountId32, Ethereum};
use cf_primitives::chains::assets::eth::Asset as EthAsset;
use cf_utilities::{
	context,
	task_scope::{self, Scope},
};
use engine_sc_client::{
	chain_api::ChainApi, electoral_api::ElectoralApi, extrinsic_api::signed::SignedExtrinsicApi,
	storage_api::StorageApi, STATE_CHAIN_CONNECTION,
};
use ethers::types::{Block, Bloom};
use futures::FutureExt;
use itertools::Itertools;
use pallet_cf_elections::{
	electoral_systems::{
		block_height_witnesser::{primitives::Header, ChainBlockHashOf, ChainBlockNumberOf},
		block_witnesser::state_machine::BWElectionProperties,
	},
	ElectoralSystemTypes, VoteOf,
};
use pallet_cf_funding::{EthereumDeposit, EthereumDepositAndSCCall};
use sp_core::{H160, H256};
use state_chain_runtime::{
	chainflip::witnessing::{
		ethereum_elections::{
			EthereumBlockHeightWitnesserES, EthereumChain, EthereumDepositChannelWitnessingES,
			EthereumElectoralSystemRunner, EthereumFeeTracking, EthereumKeyManagerWitnessingES,
			EthereumLiveness, EthereumScUtilsWitnessingES, EthereumStateChainGatewayWitnessingES,
			EthereumVaultDepositWitnessingES, ScUtilsCall,
			StateChainGatewayEvent as SCStateChainGatewayEvent, ETHEREUM_MAINNET_SAFETY_BUFFER,
		},
		pallet_hooks::EvmVaultContractEvent,
	},
	EthereumInstance, Runtime,
};
use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};

// ------------ configuring header and block clients for ethereum ------------

#[derive(Clone)]
pub struct EvmSingleBlockQueryWithBloom {
	pub block_height: u64,
	pub hash: H256,
	pub bloom: Bloom,
}

impl EvmSingleBlockQueryWithBloom {
	fn try_from_native_block(block: Block<H256>) -> Result<Self> {
		Ok(EvmSingleBlockQueryWithBloom {
			block_height: block.number.ok_or_else(|| anyhow::anyhow!("No block number"))?.low_u64(),
			hash: block.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?,
			bloom: block.logs_bloom.unwrap_or(Bloom::repeat_byte(0xFFu8)),
		})
	}
}

#[derive(Clone)]
pub struct EvmSingleBlockHeaderVoter<Config: Sync + Send> {
	client: EvmCachingClient<EvmRpcSigningClient>,
	config: Config,
}

#[async_trait::async_trait]
impl<Config: Sync + Send> WitnessClient<EthereumChain> for EvmSingleBlockHeaderVoter<Config> {
	type BlockQuery = EvmSingleBlockQueryWithBloom;

	async fn best_block_number(&self) -> Result<u64> {
		Ok(self.client.get_block_number().await?.low_u64())
	}

	async fn best_block_header(&self) -> Result<Header<EthereumChain>> {
		let best_number = self.client.get_block_number().await?;
		Ok(self.block_header_by_height(best_number.low_u64()).await?)
	}

	async fn block_header_by_height(&self, height: u64) -> Result<Header<EthereumChain>> {
		let block = self.client.block(height.into()).await?;
		Ok(Header {
			block_height: block.number.ok_or_else(|| anyhow::anyhow!("No block number"))?.low_u64(),
			hash: block.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?,
			parent_hash: block.parent_hash,
		})
	}

	async fn block_query_from_hash_and_height(
		&self,
		hash: ChainBlockHashOf<EthereumChain>,
		_height: ChainBlockNumberOf<EthereumChain>,
	) -> Result<EvmSingleBlockQueryWithBloom> {
		EvmSingleBlockQueryWithBloom::try_from_native_block(self.client.block_by_hash(hash).await?)
	}

	async fn block_query_from_height(
		&self,
		height: <EthereumChain as pallet_cf_elections::electoral_systems::block_height_witnesser::ChainTypes>::ChainBlockNumber,
	) -> Result<Self::BlockQuery> {
		EvmSingleBlockQueryWithBloom::try_from_native_block(self.client.block(height.into()).await?)
	}

	async fn block_query_and_hash_from_height(
		&self,
		height: <EthereumChain as pallet_cf_elections::electoral_systems::block_height_witnesser::ChainTypes>::ChainBlockNumber,
	) -> Result<(Self::BlockQuery, ChainBlockHashOf<EthereumChain>)> {
		let header = self.client.block(height.into()).await?;
		let hash = header.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?;
		let query = EvmSingleBlockQueryWithBloom::try_from_native_block(header)?;
		Ok((query, hash))
	}
}

struct VaultDepositWitnessingConfig {
	vault_address: H160,
	supported_assets: HashMap<H160, assets::eth::Asset>,
}

#[async_trait::async_trait]
impl WitnessClientForBlockData<EthereumChain, Vec<EvmVaultContractEvent<Runtime, EthereumInstance>>>
	for EvmSingleBlockHeaderVoter<VaultDepositWitnessingConfig>
{
	async fn block_data_from_query(
		&self,
		header: &EvmSingleBlockQueryWithBloom,
	) -> Result<Vec<EvmVaultContractEvent<Runtime, EthereumInstance>>> {
		let events = events_at_block::<cf_chains::Ethereum, VaultEvents, EthereumChain, _>(
			Some(header.bloom),
			header.block_height,
			header.hash,
			self.config.vault_address,
			&self.client,
		)
		.await?;

		let result =
			handle_vault_events(&self.config.supported_assets, events, header.block_height)?;
		Ok(result.into_iter().sorted().collect())
	}
}

#[async_trait::async_trait]
impl VoterApi<EthereumBlockHeightWitnesserES> for EvmSingleBlockHeaderVoter<()> {
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

// --- deposit channel witnessing ---

#[derive(Clone)]
pub struct EthereumDepositChannelWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	address_checker_address: H160,
	vault_address: H160,
	supported_asset_address_and_event_type:
		HashMap<assets::eth::Asset, (H160, Arc<dyn EvmEventType<Erc20Events>>)>,
}

#[async_trait::async_trait]
impl crate::witness::evm::contract_common::DepositChannelWitnesserConfig<Ethereum, EthereumChain>
	for EthereumDepositChannelWitnesserVoter
{
	fn client(&self) -> &EvmCachingClient<EvmRpcSigningClient> {
		&self.client
	}

	fn address_checker_address(&self) -> H160 {
		self.address_checker_address
	}

	fn vault_address(&self) -> H160 {
		self.vault_address
	}

	async fn get_events_for_erc20_asset(
		&self,
		asset: EthAsset,
		bloom: Option<ethers::types::Bloom>,
		_block_height: u64,
		block_hash: sp_core::H256,
	) -> Result<Option<Vec<crate::witness::evm::contract_common::Event<Erc20Events>>>> {
		let (contract_address, event_type) =
			self.supported_asset_address_and_event_type.get(&asset).ok_or_else(|| {
				anyhow::anyhow!("Tried to get erc20 events for unsupported asset: {asset:?}")
			})?;

		let events = evm_events_at_block(
			&self.client,
			event_type.clone(),
			*contract_address,
			block_hash,
			bloom,
		)
		.await?;

		return Ok(Some(events))
	}
}

#[async_trait::async_trait]
impl VoterApi<EthereumDepositChannelWitnessingES> for EthereumDepositChannelWitnesserVoter {
	async fn vote(
		&self,
		_settings: <EthereumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <EthereumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumDepositChannelWitnessingES>>, anyhow::Error> {
		use state_chain_runtime::chainflip::witnessing::ethereum_elections::EthereumChain;

		let BWElectionProperties {
			block_height, properties: deposit_addresses, election_type, ..
		} = properties;

		let (witnesses, return_block_hash) =
			crate::witness::evm::contract_common::witness_deposit_channels_generic::<
				cf_chains::Ethereum,
				EthereumChain,
				_,
			>(self, block_height, election_type, deposit_addresses)
			.await?;

		Ok(Some((witnesses, return_block_hash)))
	}
}

#[derive(Clone)]
pub struct EthereumVaultDepositWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	vault_address: H160,
	supported_assets: HashMap<H160, assets::eth::Asset>,
}

#[async_trait::async_trait]
impl VoterApi<EthereumVaultDepositWitnessingES> for EthereumVaultDepositWitnesserVoter {
	async fn vote(
		&self,
		_settings: <EthereumVaultDepositWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <EthereumVaultDepositWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumVaultDepositWitnessingES>>, anyhow::Error> {
		let BWElectionProperties { block_height, properties: _vault, election_type, .. } =
			properties;
		let (block, return_block_hash) =
			query_election_block::<_, Ethereum>(&self.client, block_height, election_type).await?;

		let events = evm_events_at_block::<VaultEvents>(
			&self.client,
			evm_event_type::<VaultEvents, VaultEvents>(),
			self.vault_address,
			block.hash,
			block.bloom,
		)
		.await?;

		let result = handle_vault_events(&self.supported_assets, events, block_height)?;

		Ok(Some((result.into_iter().sorted().collect(), return_block_hash)))
	}
}

#[derive(Clone)]
pub struct EthereumStateChainGatewayWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	state_chain_gateway_address: H160,
}
#[async_trait::async_trait]
impl VoterApi<EthereumStateChainGatewayWitnessingES> for EthereumStateChainGatewayWitnesserVoter {
	async fn vote(
		&self,
		_settings: <EthereumStateChainGatewayWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <EthereumStateChainGatewayWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumStateChainGatewayWitnessingES>>, anyhow::Error>
	{
		let BWElectionProperties {
			block_height,
			properties: _state_chain_gateway,
			election_type,
			..
		} = properties;
		let (block, return_block_hash) =
			query_election_block::<_, Ethereum>(&self.client, block_height, election_type).await?;

		let events = evm_events_at_block::<StateChainGatewayEvents>(
			&self.client,
			evm_event_type::<StateChainGatewayEvents, StateChainGatewayEvents>(),
			self.state_chain_gateway_address,
			block.hash,
			block.bloom,
		)
		.await?;

		let mut result: Vec<SCStateChainGatewayEvent> = Vec::new();
		for event in events {
			match event.event_parameters {
				StateChainGatewayEvents::FundedFilter(FundedFilter {
					node_id: account_id,
					amount,
					funder,
				}) => {
					result.push(SCStateChainGatewayEvent::Funded {
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
					result.push(SCStateChainGatewayEvent::RedemptionExecuted {
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
					result.push(SCStateChainGatewayEvent::RedemptionExpired {
						account_id: account_id.into(),
						block_number: block_height,
						tx_hash: event.tx_hash.into(),
					});
				},
				_ => {},
			}
		}

		Ok(Some((result.into_iter().sorted().collect(), return_block_hash)))
	}
}

#[derive(Clone)]
pub struct EthereumKeyManagerWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	key_manager_address: H160,
}

impl KeyManagerEventConfig for EthereumKeyManagerWitnesserVoter {
	type Chain = Ethereum;
	type Instance = EthereumInstance;

	fn client(&self) -> &EvmCachingClient<EvmRpcSigningClient> {
		&self.client
	}
}

#[async_trait::async_trait]
impl VoterApi<EthereumKeyManagerWitnessingES> for EthereumKeyManagerWitnesserVoter {
	async fn vote(
		&self,
		_settings: <EthereumKeyManagerWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <EthereumKeyManagerWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumKeyManagerWitnessingES>>, anyhow::Error> {
		let BWElectionProperties { block_height, properties: _key_manager, election_type, .. } =
			properties;
		let (block, return_block_hash) =
			query_election_block::<_, Ethereum>(&self.client, block_height, election_type).await?;

		let events = evm_events_at_block::<KeyManagerEvents>(
			&self.client,
			evm_event_type::<KeyManagerEvents, KeyManagerEvents>(),
			self.key_manager_address,
			block.hash,
			block.bloom,
		)
		.await?;

		let result = handle_key_manager_events(self, events, block_height).await?;

		Ok(Some((result.into_iter().sorted().collect(), return_block_hash)))
	}
}

#[derive(Clone)]
pub struct EthereumScUtilsVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	sc_utils_address: H160,
	supported_assets: HashMap<H160, assets::eth::Asset>,
}
#[async_trait::async_trait]
impl VoterApi<EthereumScUtilsWitnessingES> for EthereumScUtilsVoter {
	async fn vote(
		&self,
		_settings: <EthereumScUtilsWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <EthereumScUtilsWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumScUtilsWitnessingES>>, anyhow::Error> {
		let BWElectionProperties { block_height, properties: _sc_utils, election_type, .. } =
			properties;
		let (block, return_block_hash) =
			query_election_block::<_, Ethereum>(&self.client, block_height, election_type).await?;

		let events = evm_events_at_block::<ScUtilsEvents>(
			&self.client,
			evm_event_type::<ScUtilsEvents, ScUtilsEvents>(),
			self.sc_utils_address,
			block.hash,
			block.bloom,
		)
		.await?;

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
					if let Some(asset) = self.supported_assets.get(&token) {
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
					if let Some(asset) = self.supported_assets.get(&token) {
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

		Ok(Some((result.into_iter().sorted().collect(), return_block_hash)))
	}
}
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
		(EthAsset::Usdc, (usdc_contract_address, evm_event_type::<UsdcEvents, Erc20Events>())),
		(EthAsset::Usdt, (usdt_contract_address, evm_event_type::<UsdtEvents, Erc20Events>())),
		(EthAsset::Flip, (flip_contract_address, evm_event_type::<FlipEvents, Erc20Events>())),
		(EthAsset::Wbtc, (wbtc_contract_address, evm_event_type::<WbtcEvents, Erc20Events>())),
	]
	.into_iter()
	.collect();

	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<EthereumElectoralSystemRunner, _>::new((
						EvmSingleBlockHeaderVoter { client: client.clone(), config: () },
						EthereumDepositChannelWitnesserVoter {
							client: client.clone(),
							address_checker_address,
							vault_address,
							supported_asset_address_and_event_type,
						},
						EthereumVaultDepositWitnesserVoter {
							client: client.clone(),
							vault_address,
							supported_assets: supported_erc20_tokens.clone(),
						},
						EthereumKeyManagerWitnesserVoter {
							client: client.clone(),
							key_manager_address,
						},
						EthereumFeeVoter { client: client.clone() },
						EthereumLivenessVoter { client: client.clone() },
						EthereumStateChainGatewayWitnesserVoter {
							client: client.clone(),
							state_chain_gateway_address,
						},
						EthereumScUtilsVoter {
							client: client.clone(),
							sc_utils_address,
							supported_assets: supported_erc20_tokens,
						},
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
