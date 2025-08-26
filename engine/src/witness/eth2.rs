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
	evm::rpc::address_checker::AddressState,
	witness::{
		eth::state_chain_gateway::{
			FundedFilter, RedemptionExecutedFilter, RedemptionExpiredFilter,
			StateChainGatewayEvents,
		},
		evm::{
			contract_common::Event,
			erc20_deposits::Erc20Events,
			evm_deposits::eth_ingresses_at_block,
			key_manager::{
				AggKeySetByGovKeyFilter, GovernanceActionFilter, KeyManagerEvents,
				SignatureAcceptedFilter,
			},
			vault::{
				decode_cf_parameters, SwapNativeFilter, SwapTokenFilter,
				TransferNativeFailedFilter, TransferTokenFailedFilter, VaultEvents,
				XcallNativeFilter, XcallTokenFilter,
			},
		},
	},
};
use anyhow::{anyhow, ensure};
use cf_chains::{
	address::{EncodedAddress, IntoForeignChainAddress},
	eth::EthereumTrackedData,
	evm::{
		DepositDetails, EvmTransactionMetadata, SchnorrVerificationComponents, TransactionFee, H256,
	},
	witness_period::SaturatingStep,
	CcmChannelMetadata, CcmDepositMetadata, Ethereum, ForeignChain,
};
use cf_primitives::{chains::assets::eth::Asset as EthAsset, Asset, AssetAmount};
use cf_utilities::{
	context,
	task_scope::{self, Scope},
};
use ethbloom::Bloom;
use ethers::{
	abi::ethereum_types::BloomInput,
	types::{Block, TransactionReceipt},
};
use futures::{future, FutureExt};
use itertools::Itertools;
use pallet_cf_elections::{
	electoral_systems::{
		block_height_witnesser::{
			primitives::{Header, NonemptyContinuousHeaders},
			ChainTypes, HeightWitnesserProperties,
		},
		block_witnesser::state_machine::{BWElectionProperties, EngineElectionType},
	},
	ElectoralSystemTypes, VoteOf,
};
use pallet_cf_ingress_egress::{DepositWitness, VaultDepositWitness};
use sp_core::{bounded::alloc::collections::VecDeque, H160};
use state_chain_runtime::{
	chainflip::ethereum_elections::{
		EthereumBlockHeightWitnesserES, EthereumChain, EthereumDepositChannelWitnessingES,
		EthereumEgressWitnessingES, EthereumElectoralSystemRunner, EthereumFeeTracking,
		EthereumKeyManagerWitnessingES, EthereumLiveness, EthereumStateChainGatewayWitnessingES,
		EthereumVaultDepositWitnessingES, KeyManagerEvent as SCKeyManagerEvent,
		StateChainGatewayEvent as SCStateChainGatewayEvent, VaultEvents as SCVaultEvents,
		ETHEREUM_MAINNET_SAFETY_BUFFER,
	},
	EthereumInstance,
};
use std::{collections::HashMap, sync::Arc};

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	evm::{
		cached_rpc::{
			AddressCheckerRetryRpcApiWithResult, EvmCachingClient, EvmRetryRpcApiWithResult,
		},
		rpc::EvmRpcSigningClient,
	},
	state_chain_observer::client::{
		chain_api::ChainApi,
		electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		STATE_CHAIN_CONNECTION,
	},
	witness::evm::erc20_deposits::{flip::FlipEvents, usdc::UsdcEvents, usdt::UsdtEvents},
};

use super::{
	evm::vault::vault_deposit_witness,
};

use anyhow::{Context, Result};

#[derive(Clone)]
pub struct EthereumBlockHeightWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}
#[async_trait::async_trait]
impl VoterApi<EthereumBlockHeightWitnesserES> for EthereumBlockHeightWitnesserVoter {
	async fn vote(
		&self,
		_settings: <EthereumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <EthereumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumBlockHeightWitnesserES>>, anyhow::Error> {
		tracing::debug!("ETH BHW: Block height tracking called properties: {:?}", properties);
		let HeightWitnesserProperties { witness_from_index } = properties;

		let header_from_eth_block = |header: Block<H256>| -> anyhow::Result<Header<EthereumChain>> {
			Ok(Header {
				block_height: header
					.number
					.ok_or_else(|| anyhow::anyhow!("No block number"))?
					.low_u64(),
				hash: header.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?,
				parent_hash: header.parent_hash,
			})
		};

		let best_block_number = self.client.get_block_number().await?;
		let best_block = self.client.block(best_block_number).await?;

		let best_block_header = header_from_eth_block(best_block)?;

		if best_block_header.block_height < witness_from_index {
			tracing::debug!("ETH BHW: no new blocks found since best block height is {} for witness_from={witness_from_index}", best_block_header.block_height);
			return Ok(None)
		} else {
			// The `latest_block_height == 0` is a special case for when starting up the
			// electoral system for the first time.
			let witness_from_index = if witness_from_index == 0 {
				tracing::debug!(
					"ETH BHW: election_property=0, best_block_height={}, submitting last 6 blocks.",
					best_block_header.block_height
				);
				best_block_header
					.block_height
					.saturating_sub(ETHEREUM_MAINNET_SAFETY_BUFFER as u64)
			} else {
				witness_from_index
			};

			// Compute the highest block height we want to fetch a header for,
			// since for performance reasons we're bounding the number of headers
			// submitted in one vote. We're submitting at most SAFETY_BUFFER headers.
			let highest_submitted_height = std::cmp::min(
				best_block_header.block_height,
				witness_from_index.saturating_forward(ETHEREUM_MAINNET_SAFETY_BUFFER as usize + 1),
			);

			// request headers for at most SAFETY_BUFFER heights, in parallel
			let requests = (witness_from_index..highest_submitted_height)
				.map(|index| async move {
					header_from_eth_block(self.client.block(index.into()).await?)
				})
				.collect::<Vec<_>>();
			let mut headers: VecDeque<_> =
				future::join_all(requests).await.into_iter().collect::<Result<_>>()?;

			// If we submitted all headers up the highest, we also append the highest
			if highest_submitted_height == best_block_header.block_height {
				headers.push_back(best_block_header);
			}

			let headers_len = headers.len();
			NonemptyContinuousHeaders::try_new(headers)
				.inspect(|_| tracing::debug!("ETH BHW: Submitting vote for (witness_from={witness_from_index})with {headers_len} headers",))
				.map(Some)
				.map_err(|err| anyhow::format_err!("ETH BHW: {err:?}"))
		}
	}
}

async fn query_election_block<C: ChainTypes<ChainBlockHash = H256, ChainBlockNumber = u64>>(
	client: &EvmCachingClient<EvmRpcSigningClient>,
	block_height: C::ChainBlockNumber,
	election_type: EngineElectionType<C>,
) -> Result<(Bloom, Option<C::ChainBlockHash>, C::ChainBlockHash, C::ChainBlockHash)> {
	match election_type {
		EngineElectionType::ByHash(hash) => {
			let block = client.block_by_hash(hash).await?;
			if let (Some(block_number), Some(block_hash)) = (block.number, block.hash) {
				assert_eq!(block_number.as_u64(), block_height);
				Ok((
					block.logs_bloom.unwrap_or(Bloom::repeat_byte(0xFFu8)),
					None,
					block_hash,
					block.parent_hash,
				))
			} else {
				Err(anyhow::anyhow!(
					"Block number or hash is none for block number: {}",
					block_height
				))
			}
		},
		EngineElectionType::BlockHeight { submit_hash } => {
			let block = client.block(block_height.into()).await?;
			if let (Some(block_number), Some(block_hash)) = (block.number, block.hash) {
				assert_eq!(block_number.as_u64(), block_height);
				Ok((
					block.logs_bloom.unwrap_or(Bloom::repeat_byte(0xFFu8)),
					if submit_hash { block.hash } else { None },
					block_hash,
					block.parent_hash,
				))
			} else {
				Err(anyhow::anyhow!(
					"Block number or hash is none for block number: {}",
					block_height
				))
			}
		},
	}
}

async fn address_states<EvmCachingClient>(
	eth_rpc: &EvmCachingClient,
	address_checker_address: H160,
	parent_hash: H256,
	hash: H256,
	addresses: Vec<H160>,
) -> Result<impl Iterator<Item = (H160, (AddressState, AddressState))>, anyhow::Error>
where
	EvmCachingClient: AddressCheckerRetryRpcApiWithResult + Send + Sync + Clone,
{
	let previous_address_states = eth_rpc
		.address_states(parent_hash, address_checker_address, addresses.clone())
		.await?;

	let address_states =
		eth_rpc.address_states(hash, address_checker_address, addresses.clone()).await?;

	ensure!(
		addresses.len() == previous_address_states.len() &&
			previous_address_states.len() == address_states.len()
	);

	Ok(addresses
		.into_iter()
		.zip(previous_address_states.into_iter().zip(address_states)))
}

pub async fn events_at_block<Chain, EventParameters, EvmCachingClient>(
	data: Bloom,
	block_number: Chain::ChainBlockNumber,
	block_hash: H256,
	contract_address: H160,
	eth_rpc: &EvmCachingClient,
) -> Result<Vec<Event<EventParameters>>>
where
	Chain: cf_chains::Chain<ChainBlockNumber = u64>,
	EventParameters: std::fmt::Debug + ethers::contract::EthLogDecode + Send + Sync + 'static,
	EvmCachingClient: EvmRetryRpcApiWithResult,
{
	assert!(Chain::is_block_witness_root(block_number));
	if Chain::WITNESS_PERIOD == 1 {
		let mut contract_bloom = Bloom::default();
		contract_bloom.accrue(BloomInput::Raw(&contract_address.0));

		// if we have logs for this block, fetch them.
		if data.contains_bloom(&contract_bloom) {
			eth_rpc.get_logs(block_hash, contract_address).await?
		} else {
			// we know there won't be interesting logs, so don't fetch for events
			vec![]
		}
	} else {
		eth_rpc
			.get_logs_range(Chain::block_witness_range(block_number), contract_address)
			.await?
	}
	.into_iter()
	.map(|unparsed_log| -> anyhow::Result<Event<EventParameters>> {
		Event::<EventParameters>::new_from_unparsed_logs(unparsed_log)
	})
	.collect::<anyhow::Result<Vec<_>>>()
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
impl VoterApi<EthereumDepositChannelWitnessingES> for EthereumDepositChannelWitnesserVoter {
	async fn vote(
		&self,
		_settings: <EthereumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <EthereumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumDepositChannelWitnessingES>>, anyhow::Error> {
		let BWElectionProperties {
			block_height, properties: deposit_addresses, election_type, ..
		} = properties;
		let data = query_election_block(&self.client, block_height, election_type).await?;
		let (eth_deposti_channels, erc20_deposit_channels): (Vec<_>, HashMap<_, Vec<_>>) =
			deposit_addresses.into_iter().fold(
				(Vec::new(), HashMap::new()),
				|(mut eth, mut erc20), deposit_channel| {
					let address = deposit_channel.address;
					if deposit_channel.asset == EthAsset::Eth {
						eth.push(address);
					} else {
						erc20.entry(deposit_channel.asset).or_insert_with(Vec::new).push(address);
					}
					(eth, erc20)
				},
			);

		let eth_ingresses = eth_ingresses_at_block(
			address_states(
				&self.client,
				self.address_checker_address,
				data.3,
				data.2,
				eth_deposti_channels.clone(),
			)
			.await?,
			events_at_block::<cf_chains::Ethereum, _, _>(
				data.0,
				block_height,
				data.2,
				self.vault_address,
				&self.client,
			)
			.await?
			.into_iter()
			.filter_map(|event| match event.event_parameters {
				VaultEvents::FetchedNativeFilter(inner_event) => Some((inner_event, event.tx_hash)),
				_ => None,
			})
			.collect(),
		)?;

		let mut erc20_ingresses: Vec<DepositWitness<cf_chains::Ethereum>> = Vec::new();

		// Handle each asset type separately with its specific event type
		for (asset, deposit_channels) in erc20_deposit_channels {
			let events = match asset {
				EthAsset::Usdc => events_at_block::<cf_chains::Ethereum, UsdcEvents, _>(
					data.0,
					block_height,
					data.2,
					self.usdc_contract_address,
					&self.client,
				)
				.await?
				.into_iter()
				.map(|event| Event {
					event_parameters: event.event_parameters.into(),
					tx_hash: event.tx_hash,
					log_index: event.log_index,
				})
				.collect::<Vec<_>>(),
				EthAsset::Flip => events_at_block::<cf_chains::Ethereum, FlipEvents, _>(
					data.0,
					block_height,
					data.2,
					self.flip_contract_address,
					&self.client,
				)
				.await?
				.into_iter()
				.map(|event| Event {
					event_parameters: event.event_parameters.into(),
					tx_hash: event.tx_hash,
					log_index: event.log_index,
				})
				.collect::<Vec<_>>(),
				EthAsset::Usdt => events_at_block::<cf_chains::Ethereum, UsdtEvents, _>(
					data.0,
					block_height,
					data.2,
					self.usdt_contract_address,
					&self.client,
				)
				.await?
				.into_iter()
				.map(|event| Event {
					event_parameters: event.event_parameters.into(),
					tx_hash: event.tx_hash,
					log_index: event.log_index,
				})
				.collect::<Vec<_>>(),
				_ => continue, // Skip unsupported assets
			};

			let asset_ingresses = events
				.into_iter()
				.filter_map(|event| {
					match event.event_parameters {
						Erc20Events::TransferFilter{to, value, from: _ } if deposit_channels.contains(&to) =>
							Some(pallet_cf_ingress_egress::DepositWitness {
								deposit_address: to,
								amount: value.try_into().expect(
									"Any ERC20 tokens we support should have amounts that fit into a u128",
								),
								asset,
								deposit_details: DepositDetails {
									tx_hashes: Some(vec![event.tx_hash]),
								},
							}),
						_ => None,
					}
				})
				.collect::<Vec<_>>();

			erc20_ingresses.extend(asset_ingresses);
		}

		Ok(Some((
			eth_ingresses
				.into_iter()
				.map(|(to_addr, value, tx_hashes)| pallet_cf_ingress_egress::DepositWitness {
					deposit_address: to_addr,
					asset: EthAsset::Eth,
					amount: value
						.try_into()
						.expect("Ingress witness transfer value should fit u128"),
					deposit_details: DepositDetails { tx_hashes },
				})
				.chain(erc20_ingresses)
				.collect(),
			data.1,
		)))
	}
}

fn try_into_primitive<Primitive: std::fmt::Debug + TryInto<CfType> + Copy, CfType>(
	from: Primitive,
) -> Result<CfType>
where
	<Primitive as TryInto<CfType>>::Error: std::fmt::Display,
{
	from.try_into().map_err(|err| {
		anyhow!("Failed to convert into {:?}: {err}", std::any::type_name::<CfType>(),)
	})
}

fn try_into_encoded_address(chain: ForeignChain, bytes: Vec<u8>) -> Result<EncodedAddress> {
	EncodedAddress::from_chain_bytes(chain, bytes)
		.map_err(|e| anyhow!("Failed to convert into EncodedAddress: {e}"))
}

#[derive(Clone)]
pub struct EthereumVaultDepositWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	vault_address: H160,
	supported_assets: HashMap<H160, Asset>,
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
		let data = query_election_block(&self.client, block_height, election_type).await?;

		let events = events_at_block::<cf_chains::Ethereum, VaultEvents, _>(
			data.0,
			block_height,
			data.2,
			self.vault_address,
			&self.client,
		)
		.await?;

		let mut result = Vec::new();
		for event in events {
			match event.event_parameters {
				VaultEvents::SwapNativeFilter(SwapNativeFilter {
					dst_chain,
					dst_address,
					dst_token,
					amount,
					sender: _,
					cf_parameters,
				}) => {
					let (vault_swap_parameters, ()) =
						decode_cf_parameters(&cf_parameters[..], block_height)?;

					result.push(SCVaultEvents::SwapNativeFilter(vault_deposit_witness!(
						Asset::Eth,
						try_into_primitive(amount)?,
						try_into_primitive(dst_token)?,
						try_into_encoded_address(
							try_into_primitive(dst_chain)?,
							dst_address.to_vec()
						)?,
						None,
						event.tx_hash,
						vault_swap_parameters
					)));
				},
				VaultEvents::SwapTokenFilter(SwapTokenFilter {
					dst_chain,
					dst_address,
					dst_token,
					src_token,
					amount,
					sender: _,
					cf_parameters,
				}) => {
					let (vault_swap_parameters, ()) =
						decode_cf_parameters(&cf_parameters[..], block_height)?;

					result.push(SCVaultEvents::SwapTokenFilter(vault_deposit_witness!(
						*(self
							.supported_assets
							.get(&src_token)
							.ok_or_else(|| anyhow!("Source token {src_token:?} not found"))?),
						try_into_primitive(amount)?,
						try_into_primitive(dst_token)?,
						try_into_encoded_address(
							try_into_primitive(dst_chain)?,
							dst_address.to_vec()
						)?,
						None,
						event.tx_hash,
						vault_swap_parameters
					)));
				},
				VaultEvents::XcallNativeFilter(XcallNativeFilter {
					dst_chain,
					dst_address,
					dst_token,
					amount,
					sender,
					message,
					gas_amount,
					cf_parameters,
				}) => {
					let (vault_swap_parameters, ccm_additional_data) =
						decode_cf_parameters(&cf_parameters[..], block_height)?;

					result.push(SCVaultEvents::XcallNativeFilter(vault_deposit_witness!(
						Asset::Eth,
						try_into_primitive(amount)?,
						try_into_primitive(dst_token)?,
						try_into_encoded_address(
							try_into_primitive(dst_chain)?,
							dst_address.to_vec()
						)?,
						Some(CcmDepositMetadata {
							source_chain: ForeignChain::Ethereum,
							source_address: Some(
								IntoForeignChainAddress::<Ethereum>::into_foreign_chain_address(
									sender
								)
							),
							channel_metadata: CcmChannelMetadata {
								message: message.to_vec().try_into().map_err(|_| anyhow!(
									"Failed to deposit CCM: `message` too long."
								))?,
								gas_budget: try_into_primitive(gas_amount)?,
								ccm_additional_data,
							},
						}),
						event.tx_hash,
						vault_swap_parameters
					)));
				},
				VaultEvents::XcallTokenFilter(XcallTokenFilter {
					dst_chain,
					dst_address,
					dst_token,
					src_token,
					amount,
					sender,
					message,
					gas_amount,
					cf_parameters,
				}) => {
					let (vault_swap_parameters, ccm_additional_data) =
						decode_cf_parameters(&cf_parameters[..], block_height)?;

					result.push(SCVaultEvents::XcallTokenFilter(vault_deposit_witness!(
						*(self
							.supported_assets
							.get(&src_token)
							.ok_or_else(|| anyhow!("Source token {src_token:?} not found"))?),
						try_into_primitive(amount)?,
						try_into_primitive(dst_token)?,
						try_into_encoded_address(
							try_into_primitive(dst_chain)?,
							dst_address.to_vec()
						)?,
						Some(CcmDepositMetadata {
							source_chain: ForeignChain::Ethereum,
							source_address: Some(
								IntoForeignChainAddress::<Ethereum>::into_foreign_chain_address(
									sender
								)
							),
							channel_metadata: CcmChannelMetadata {
								message: message.to_vec().try_into().map_err(|_| anyhow!(
									"Failed to deposit CCM. Message too long."
								))?,
								gas_budget: try_into_primitive(gas_amount)?,
								ccm_additional_data,
							},
						}),
						event.tx_hash,
						vault_swap_parameters
					)));
				},
				VaultEvents::TransferNativeFailedFilter(TransferNativeFailedFilter {
					recipient,
					amount,
				}) => {
					result.push(SCVaultEvents::TransferNativeFailedFilter {
						asset: cf_chains::assets::eth::Asset::Eth,
						amount: try_into_primitive::<_, AssetAmount>(amount)?,
						destination_address: recipient,
					});
				},
				VaultEvents::TransferTokenFailedFilter(TransferTokenFailedFilter {
					recipient,
					amount,
					token,
					reason: _,
				}) => {
					result.push(SCVaultEvents::TransferTokenFailedFilter {
						asset: (*(self
							.supported_assets
							.get(&token)
							.ok_or_else(|| anyhow!("Asset {token:?} not found"))?))
						.try_into()
						.expect(
							"Asset translated from EthereumAddress must be supported by the chain.",
						),
						amount: try_into_primitive(amount)?,
						destination_address: recipient,
					});
				},
				_ => {},
			}
		}
		Ok(Some((result, data.1)))
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
		let data = query_election_block(&self.client, block_height, election_type).await?;

		let events = events_at_block::<cf_chains::Ethereum, StateChainGatewayEvents, _>(
			data.0,
			block_height,
			data.2,
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
						tx_hash: event.tx_hash.to_fixed_bytes(),
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

		Ok(Some((result, data.1)))
	}
}

#[derive(Clone)]
pub struct EthereumKeyManagerWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	key_manager_address: H160,
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
		let data = query_election_block(&self.client, block_height, election_type).await?;

		let events = events_at_block::<cf_chains::Ethereum, KeyManagerEvents, _>(
			data.0,
			block_height,
			data.2,
			self.key_manager_address,
			&self.client,
		)
		.await?;

		let mut result: Vec<SCKeyManagerEvent> = Vec::new();

		for event in events {
			match event.event_parameters {
				KeyManagerEvents::AggKeySetByGovKeyFilter(AggKeySetByGovKeyFilter {
					new_agg_key,
					..
				}) => {
					result.push(SCKeyManagerEvent::AggKeySetByGovKey {
						new_public_key: cf_chains::evm::AggKey::from_pubkey_compressed(
							new_agg_key.serialize(),
						),
						block_number: block_height,
						tx_id: event.tx_hash,
					});
				},
				KeyManagerEvents::SignatureAcceptedFilter(SignatureAcceptedFilter {
					sig_data,
					..
				}) => {
					let TransactionReceipt { gas_used, effective_gas_price, from, to, .. } =
						self.client.transaction_receipt(event.tx_hash).await?;

					let gas_used = gas_used
						.ok_or_else(|| {
							anyhow::anyhow!(
								"No gas_used on Transaction receipt for tx_hash: {}",
								event.tx_hash
							)
						})?
						.try_into()
						.map_err(anyhow::Error::msg)?;
					let effective_gas_price = effective_gas_price
						.ok_or_else(|| {
							anyhow::anyhow!(
								"No effective_gas_price on Transaction receipt for tx_hash: {}",
								event.tx_hash
							)
						})?
						.try_into()
						.map_err(anyhow::Error::msg)?;

					let transaction = self.client.get_transaction(event.tx_hash).await?;
					let tx_metadata = EvmTransactionMetadata {
						contract: to.expect("To have a contract"),
						max_fee_per_gas: transaction.max_fee_per_gas,
						max_priority_fee_per_gas: transaction.max_priority_fee_per_gas,
						gas_limit: Some(transaction.gas),
					};
					result.push(SCKeyManagerEvent::SignatureAccepted {
						tx_out_id: SchnorrVerificationComponents {
							s: sig_data.sig.into(),
							k_times_g_address: sig_data.k_times_g_address.into(),
						},
						signer_id: from,
						tx_fee: TransactionFee { effective_gas_price, gas_used },
						tx_metadata,
						transaction_ref: transaction.hash,
					});
				},
				KeyManagerEvents::GovernanceActionFilter(GovernanceActionFilter { message }) => {
					result.push(SCKeyManagerEvent::GovernanceAction { call_hash: message });
				},
				_ => {},
			}
		}

		Ok(Some((result, data.1)))
	}
}

// TODO to be removed, egresses are part of key_manager witnessing
#[derive(Clone)]
pub struct EthereumEgressWitnesserVoter {
	_client: EvmCachingClient<EvmRpcSigningClient>,
}
#[async_trait::async_trait]
impl VoterApi<EthereumEgressWitnessingES> for EthereumEgressWitnesserVoter {
	async fn vote(
		&self,
		_settings: <EthereumEgressWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		_properties: <EthereumEgressWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumEgressWitnessingES>>, anyhow::Error> {
		Ok(None)
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
		_settings: <EthereumFeeTracking as ElectoralSystemTypes>::ElectoralSettings,
		_properties: <EthereumFeeTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<EthereumFeeTracking>>, anyhow::Error> {
		const FEE_HISTORY_WINDOW: u64 = 5;
		// We take the latest base fee. Assuming this will be most likely to be closest to the next
		// base fee. Then we take the lowest priority fee, which is not limited,
		// to protect against upward spikes in the priority fee. We only take the last 2 blocks so
		// we don't lag too much.
		const PRIORITY_FEE_PERCENTILE: f64 = 50.0;

		let best_block_number = self.client.get_block_number().await?;
		let fee_history = self
			.client
			.fee_history(
				FEE_HISTORY_WINDOW.into(),
				best_block_number.low_u64().into(),
				vec![PRIORITY_FEE_PERCENTILE],
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
							supported_assets: supported_erc20_tokens,
						},
						EthereumStateChainGatewayWitnesserVoter {
							client: client.clone(),
							state_chain_gateway_address,
						},
						EthereumKeyManagerWitnesserVoter {
							client: client.clone(),
							key_manager_address,
						},
						EthereumEgressWitnesserVoter { _client: client.clone() },
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
