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
		common::block_height_witnesser::{witness_headers, HeaderClient},
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
			contract_common::{events_at_block, query_election_block},
			erc20_deposits::Erc20Events,
			key_manager::{handle_key_manager_events, KeyManagerEventConfig, KeyManagerEvents},
			vault::{handle_vault_events, VaultEventConfig, VaultEvents},
		},
	},
};
use cf_chains::{eth::EthereumTrackedData, evm::ToAccountId32, Ethereum, ForeignChain};
use cf_primitives::{chains::assets::eth::Asset as EthAsset, Asset};
use cf_utilities::{
	context,
	task_scope::{self, Scope},
};
use engine_sc_client::{
	chain_api::ChainApi, electoral_api::ElectoralApi, extrinsic_api::signed::SignedExtrinsicApi,
	storage_api::StorageApi, STATE_CHAIN_CONNECTION,
};
use futures::FutureExt;
use itertools::Itertools;
use pallet_cf_elections::{
	electoral_systems::{
		block_height_witnesser::primitives::Header,
		block_witnesser::state_machine::BWElectionProperties,
	},
	ElectoralSystemTypes, VoteOf,
};
use pallet_cf_funding::{EthereumDeposit, EthereumDepositAndSCCall};
use sp_core::H160;
use state_chain_runtime::{
	chainflip::ethereum_elections::{
		EthereumBlockHeightWitnesserES, EthereumChain, EthereumDepositChannelWitnessingES,
		EthereumElectoralSystemRunner, EthereumFeeTracking, EthereumKeyManagerWitnessingES,
		EthereumLiveness, EthereumScUtilsWitnessingES, EthereumStateChainGatewayWitnessingES,
		EthereumVaultDepositWitnessingES, ScUtilsCall,
		StateChainGatewayEvent as SCStateChainGatewayEvent, ETHEREUM_MAINNET_SAFETY_BUFFER,
	},
	EthereumInstance,
};
use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};

#[derive(Clone)]
pub struct EthereumBlockHeightWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}

#[async_trait::async_trait]
impl HeaderClient<EthereumChain, Ethereum> for EthereumBlockHeightWitnesserVoter {
	async fn best_block_header(&self) -> anyhow::Result<Header<EthereumChain>> {
		let best_number = self.client.get_block_number().await?;
		let block = self.client.block(best_number).await?;
		Ok(Header {
			block_height: block.number.ok_or_else(|| anyhow::anyhow!("No block number"))?.low_u64(),
			hash: block.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?,
			parent_hash: block.parent_hash,
		})
	}

	async fn block_header_by_height(&self, height: u64) -> anyhow::Result<Header<EthereumChain>> {
		let block = self.client.block(height.into()).await?;
		Ok(Header {
			block_height: block.number.ok_or_else(|| anyhow::anyhow!("No block number"))?.low_u64(),
			hash: block.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?,
			parent_hash: block.parent_hash,
		})
	}
	async fn best_block_number(&self) -> anyhow::Result<u64> {
		Ok(self.client.get_block_number().await?.low_u64())
	}
}

#[async_trait::async_trait]
impl VoterApi<EthereumBlockHeightWitnesserES> for EthereumBlockHeightWitnesserVoter {
	async fn vote(
		&self,
		_settings: <EthereumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <EthereumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumBlockHeightWitnesserES>>, anyhow::Error> {
		witness_headers::<EthereumBlockHeightWitnesserES, _, EthereumChain, Ethereum>(
			self,
			properties,
			ETHEREUM_MAINNET_SAFETY_BUFFER,
			"ETH BHW",
		)
		.await
	}
}

#[derive(Clone)]
pub struct EthereumDepositChannelWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	address_checker_address: H160,
	vault_address: H160,
	usdc_contract_address: H160,
	usdt_contract_address: H160,
	flip_contract_address: H160,
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
		block_height: u64,
		block_hash: sp_core::H256,
	) -> Result<Option<Vec<crate::witness::evm::contract_common::Event<Erc20Events>>>> {
		use crate::witness::evm::{
			contract_common::events_at_block,
			erc20_deposits::{flip::FlipEvents, usdc::UsdcEvents, usdt::UsdtEvents},
		};

		let events = match asset {
			EthAsset::Usdc => events_at_block::<cf_chains::Ethereum, UsdcEvents, EthereumChain, _>(
				bloom,
				block_height,
				block_hash,
				self.usdc_contract_address,
				&self.client,
			)
			.await?
			.into_iter()
			.map(|event| crate::witness::evm::contract_common::Event {
				event_parameters: event.event_parameters.into(),
				tx_hash: event.tx_hash,
				log_index: event.log_index,
			})
			.collect::<Vec<_>>(),
			EthAsset::Flip => events_at_block::<cf_chains::Ethereum, FlipEvents, EthereumChain, _>(
				bloom,
				block_height,
				block_hash,
				self.flip_contract_address,
				&self.client,
			)
			.await?
			.into_iter()
			.map(|event| crate::witness::evm::contract_common::Event {
				event_parameters: event.event_parameters.into(),
				tx_hash: event.tx_hash,
				log_index: event.log_index,
			})
			.collect::<Vec<_>>(),
			EthAsset::Usdt => events_at_block::<cf_chains::Ethereum, UsdtEvents, EthereumChain, _>(
				bloom,
				block_height,
				block_hash,
				self.usdt_contract_address,
				&self.client,
			)
			.await?
			.into_iter()
			.map(|event| crate::witness::evm::contract_common::Event {
				event_parameters: event.event_parameters.into(),
				tx_hash: event.tx_hash,
				log_index: event.log_index,
			})
			.collect::<Vec<_>>(),
			_ => return Ok(None), // Skip unsupported assets
		};
		Ok(Some(events))
	}
}

#[async_trait::async_trait]
impl VoterApi<EthereumDepositChannelWitnessingES> for EthereumDepositChannelWitnesserVoter {
	async fn vote(
		&self,
		_settings: <EthereumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <EthereumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumDepositChannelWitnessingES>>, anyhow::Error> {
		use state_chain_runtime::chainflip::ethereum_elections::EthereumChain;

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
	supported_assets: HashMap<H160, Asset>,
}

impl VaultEventConfig for EthereumVaultDepositWitnesserVoter {
	type Chain = Ethereum;
	type Instance = EthereumInstance;

	const FOREIGN_CHAIN: ForeignChain = ForeignChain::Ethereum;

	fn supported_assets(&self) -> &HashMap<H160, Asset> {
		&self.supported_assets
	}
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

		let events = events_at_block::<cf_chains::Ethereum, VaultEvents, EthereumChain, _>(
			block.bloom,
			block_height,
			block.hash,
			self.vault_address,
			&self.client,
		)
		.await?;

		let result = handle_vault_events(self, events, block_height)?;

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

		let events =
			events_at_block::<cf_chains::Ethereum, StateChainGatewayEvents, EthereumChain, _>(
				block.bloom,
				block_height,
				block.hash,
				self.state_chain_gateway_address,
				&self.client,
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
					});
				},
				StateChainGatewayEvents::RedemptionExpiredFilter(RedemptionExpiredFilter {
					node_id: account_id,
					amount: _,
				}) => {
					result.push(SCStateChainGatewayEvent::RedemptionExpired {
						account_id: account_id.into(),
						block_number: block_height,
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

		let events = events_at_block::<cf_chains::Ethereum, KeyManagerEvents, EthereumChain, _>(
			block.bloom,
			block_height,
			block.hash,
			self.key_manager_address,
			&self.client,
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
	supported_assets: HashMap<H160, Asset>,
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

		let events = events_at_block::<cf_chains::Ethereum, ScUtilsEvents, EthereumChain, _>(
			block.bloom,
			block_height,
			block.hash,
			self.sc_utils_address,
			&self.client,
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
									asset: (*asset).try_into().expect("we expect the asset to be an Eth Asset"),
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
									asset: (*asset).try_into().expect("we expect the asset to be an Eth Asset"),
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

	let supported_erc20_tokens: HashMap<H160, cf_primitives::Asset> = supported_erc20_tokens
		.into_iter()
		.map(|(asset, address)| (address, asset.into()))
		.collect();

	let sc_utils_address = state_chain_client
		.storage_value::<pallet_cf_environment::EthereumScUtilsAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect("Failed to get Sc Utils contract address from SC");

	scope.spawn(async move {
		task_scope::task_scope(|scope| {
			async {
				crate::elections::Voter::new(
					scope,
					state_chain_client,
					CompositeVoter::<EthereumElectoralSystemRunner, _>::new((
						EthereumBlockHeightWitnesserVoter { client: client.clone() },
						EthereumDepositChannelWitnesserVoter {
							client: client.clone(),
							address_checker_address,
							vault_address,
							usdc_contract_address,
							usdt_contract_address,
							flip_contract_address,
						},
						EthereumVaultDepositWitnesserVoter {
							client: client.clone(),
							vault_address,
							supported_assets: supported_erc20_tokens.clone(),
						},
						EthereumStateChainGatewayWitnesserVoter {
							client: client.clone(),
							state_chain_gateway_address,
						},
						EthereumKeyManagerWitnesserVoter {
							client: client.clone(),
							key_manager_address,
						},
						EthereumScUtilsVoter {
							client: client.clone(),
							sc_utils_address,
							supported_assets: supported_erc20_tokens,
						},
						EthereumFeeVoter { client: client.clone() },
						EthereumLivenessVoter { client: client.clone() },
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
