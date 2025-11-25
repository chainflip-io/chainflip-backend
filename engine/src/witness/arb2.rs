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
	evm::retry_rpc::node_interface::NodeInterfaceRetryRpcApiWithResult,
	witness::{
		common::block_height::{witness_headers, HeaderClient},
		evm::{
			contract_common::{address_states, events_at_block, query_election_block, Event},
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
use anyhow::anyhow;
use cf_chains::{
	address::{EncodedAddress, IntoForeignChainAddress},
	arb::ArbitrumTrackedData,
	evm::{DepositDetails, EvmTransactionMetadata, SchnorrVerificationComponents, TransactionFee},
	witness_period::{block_witness_range, block_witness_root, BlockWitnessRange, SaturatingStep},
	Arbitrum, CcmChannelMetadata, CcmDepositMetadata, ChainWitnessConfig, ForeignChain,
};
use cf_primitives::{chains::assets::arb::Asset as ArbAsset, Asset, AssetAmount};
use cf_utilities::task_scope::{self, Scope};
use ethers::types::{Bytes, TransactionReceipt};
use futures::FutureExt;
use itertools::Itertools;
use pallet_cf_elections::{
	electoral_systems::{
		block_height_witnesser::primitives::Header,
		block_witnesser::state_machine::BWElectionProperties,
	},
	ElectoralSystemTypes, VoteOf,
};
use pallet_cf_ingress_egress::{DepositWitness, VaultDepositWitness};
use sp_core::H160;
use state_chain_runtime::{
	chainflip::arbitrum_elections::{
		ArbitrumBlockHeightWitnesserES, ArbitrumChain, ArbitrumDepositChannelWitnessingES,
		ArbitrumElectoralSystemRunner, ArbitrumFeeTracking, ArbitrumKeyManagerWitnessingES,
		ArbitrumLiveness, ArbitrumVaultDepositWitnessingES, KeyManagerEvent as SCKeyManagerEvent,
		VaultEvents as SCVaultEvents, ARBITRUM_MAINNET_SAFETY_BUFFER,
	},
	ArbitrumInstance,
};
use std::{collections::HashMap, sync::Arc};

use crate::{
	elections::voter_api::{CompositeVoter, VoterApi},
	evm::{
		cached_rpc::{EvmCachingClient, EvmRetryRpcApiWithResult},
		rpc::EvmRpcSigningClient,
	},
	state_chain_observer::client::{
		chain_api::ChainApi, electoral_api::ElectoralApi,
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, STATE_CHAIN_CONNECTION,
	},
	witness::evm::erc20_deposits::usdc::UsdcEvents,
};

use super::evm::vault::vault_deposit_witness;

use anyhow::{Context, Result};

#[derive(Clone)]
pub struct ArbitrumBlockHeightWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}

#[async_trait::async_trait]
impl HeaderClient<ArbitrumChain, Arbitrum> for ArbitrumBlockHeightWitnesserVoter {
	async fn best_block_header(&self) -> anyhow::Result<Header<ArbitrumChain>> {
		let best_number = self.client.get_block_number().await?.low_u64();
		let range = block_witness_range(Arbitrum::WITNESS_PERIOD, best_number);
		let (start, end) = if *range.end() != best_number {
			(
				range.start().saturating_sub(Arbitrum::WITNESS_PERIOD),
				range.start().saturating_sub(1),
			)
		} else {
			(*range.start(), *range.end())
		};
		let futures = vec![self.client.block((start).into()), self.client.block((end).into())];
		let [block_start, block_end]: [_; 2] = futures::future::join_all(futures)
			.await
			.into_iter()
			.collect::<anyhow::Result<Vec<_>>>()?
			.try_into()
			.map_err(|_| anyhow::anyhow!("Failed to convert to array"))?;
		Ok(Header {
			block_height: BlockWitnessRange::try_new(start)
				.map_err(|_| anyhow!("Failed to create block witness range"))?,
			hash: block_end.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?,
			parent_hash: block_start.parent_hash,
		})
	}

	async fn block_header_by_height(
		&self,
		height: BlockWitnessRange<Arbitrum>,
	) -> anyhow::Result<Header<ArbitrumChain>> {
		let range = height.into_range_inclusive();
		let futures = vec![
			self.client.block((*range.start()).into()),
			self.client.block((*range.end()).into()),
		];
		let [block_start, block_end]: [_; 2] = futures::future::join_all(futures)
			.await
			.into_iter()
			.collect::<anyhow::Result<Vec<_>>>()?
			.try_into()
			.map_err(|_| anyhow::anyhow!("Failed to convert to array"))?;
		Ok(Header {
			block_height: height,
			hash: block_end.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?,
			parent_hash: block_start.parent_hash,
		})
	}
	async fn best_block_number(&self) -> anyhow::Result<BlockWitnessRange<Arbitrum>> {
		let best_block = self.client.get_block_number().await?.low_u64();
		let range = block_witness_range(Arbitrum::WITNESS_PERIOD, best_block);
		let block_witness_range =
			BlockWitnessRange::try_new(block_witness_root(Arbitrum::WITNESS_PERIOD, best_block))
				.map_err(|_| anyhow::anyhow!("Failed to build BlockWitnessRange"))?;
		if best_block == *range.end() {
			return Ok(block_witness_range);
		}
		Ok(block_witness_range.saturating_backward(1))
	}
}

#[async_trait::async_trait]
impl VoterApi<ArbitrumBlockHeightWitnesserES> for ArbitrumBlockHeightWitnesserVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <ArbitrumBlockHeightWitnesserES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumBlockHeightWitnesserES>>, anyhow::Error> {
		witness_headers::<ArbitrumBlockHeightWitnesserES, _, ArbitrumChain, Arbitrum>(
			self,
			properties,
			ARBITRUM_MAINNET_SAFETY_BUFFER,
			"ARB BHW",
		)
		.await
	}
}

#[derive(Clone)]
pub struct ArbitrumDepositChannelWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	address_checker_address: H160,
	vault_address: H160,
	usdc_contract_address: H160,
}
#[async_trait::async_trait]
impl VoterApi<ArbitrumDepositChannelWitnessingES> for ArbitrumDepositChannelWitnesserVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <ArbitrumDepositChannelWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumDepositChannelWitnessingES>>, anyhow::Error> {
		let BWElectionProperties {
			block_height, properties: deposit_addresses, election_type, ..
		} = properties;
		let (block, return_block_hash) =
			query_election_block::<_, Arbitrum>(&self.client, block_height, election_type).await?;
		let (eth_deposit_channels, erc20_deposit_channels): (Vec<_>, HashMap<_, Vec<_>>) =
			deposit_addresses.into_iter().fold(
				(Vec::new(), HashMap::new()),
				|(mut eth, mut erc20), deposit_channel| {
					if deposit_channel.asset == ArbAsset::ArbEth {
						eth.push(deposit_channel.address);
					} else {
						erc20
							.entry(deposit_channel.asset)
							.or_insert_with(Vec::new)
							.push(deposit_channel.address);
					}
					(eth, erc20)
				},
			);

		let eth_ingresses = eth_ingresses_at_block(
			address_states(
				&self.client,
				self.address_checker_address,
				block.parent_hash,
				block.hash,
				eth_deposit_channels.clone(),
			)
			.await?,
			events_at_block::<cf_chains::Arbitrum, _, _>(
				block.bloom,
				*block_height.root(),
				block.hash,
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

		let mut erc20_ingresses: Vec<DepositWitness<cf_chains::Arbitrum>> = Vec::new();

		// Handle each asset type separately with its specific event type
		for (asset, deposit_channels) in erc20_deposit_channels {
			let events = match asset {
				ArbAsset::ArbUsdc => events_at_block::<cf_chains::Arbitrum, UsdcEvents, _>(
					block.bloom,
					*block_height.root(),
					block.hash,
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
					asset: ArbAsset::ArbEth,
					amount: value
						.try_into()
						.expect("Ingress witness transfer value should fit u128"),
					deposit_details: DepositDetails { tx_hashes },
				})
				.chain(erc20_ingresses)
				.sorted_by_key(|deposit_witness| deposit_witness.deposit_address)
				.collect(),
			return_block_hash,
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
pub struct ArbitrumVaultDepositWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	vault_address: H160,
	supported_assets: HashMap<H160, Asset>,
}
#[async_trait::async_trait]
impl VoterApi<ArbitrumVaultDepositWitnessingES> for ArbitrumVaultDepositWitnesserVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumVaultDepositWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <ArbitrumVaultDepositWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumVaultDepositWitnessingES>>, anyhow::Error> {
		let BWElectionProperties { block_height, properties: _vault, election_type, .. } =
			properties;
		let (block, return_block_hash) =
			query_election_block::<_, Arbitrum>(&self.client, block_height, election_type).await?;

		let events = events_at_block::<cf_chains::Arbitrum, VaultEvents, _>(
			block.bloom,
			*block_height.root(),
			block.hash,
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
						decode_cf_parameters(&cf_parameters[..], *block_height.root())?;

					result.push(SCVaultEvents::SwapNativeFilter(vault_deposit_witness!(
						Asset::ArbEth,
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
						decode_cf_parameters(&cf_parameters[..], *block_height.root())?;

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
						decode_cf_parameters(&cf_parameters[..], *block_height.root())?;

					result.push(SCVaultEvents::XcallNativeFilter(vault_deposit_witness!(
						Asset::ArbEth,
						try_into_primitive(amount)?,
						try_into_primitive(dst_token)?,
						try_into_encoded_address(
							try_into_primitive(dst_chain)?,
							dst_address.to_vec()
						)?,
						Some(CcmDepositMetadata {
							source_chain: ForeignChain::Arbitrum,
							source_address: Some(
								IntoForeignChainAddress::<Arbitrum>::into_foreign_chain_address(
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
						decode_cf_parameters(&cf_parameters[..], *block_height.root())?;

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
							source_chain: ForeignChain::Arbitrum,
							source_address: Some(
								IntoForeignChainAddress::<Arbitrum>::into_foreign_chain_address(
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
						asset: ArbAsset::ArbEth,
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
							"Asset translated from ArbitrumAddress must be supported by the chain.",
						),
						amount: try_into_primitive(amount)?,
						destination_address: recipient,
					});
				},
				_ => {},
			}
		}
		Ok(Some((result.into_iter().sorted().collect(), return_block_hash)))
	}
}

#[derive(Clone)]
pub struct ArbitrumKeyManagerWitnesserVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
	key_manager_address: H160,
}
#[async_trait::async_trait]
impl VoterApi<ArbitrumKeyManagerWitnessingES> for ArbitrumKeyManagerWitnesserVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumKeyManagerWitnessingES as ElectoralSystemTypes>::ElectoralSettings,
		properties: <ArbitrumKeyManagerWitnessingES as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumKeyManagerWitnessingES>>, anyhow::Error> {
		let BWElectionProperties { block_height, properties: _key_manager, election_type, .. } =
			properties;
		let (block, return_block_hash) =
			query_election_block::<_, Arbitrum>(&self.client, block_height, election_type).await?;

		let events = events_at_block::<cf_chains::Arbitrum, KeyManagerEvents, _>(
			block.bloom,
			*block_height.root(),
			block.hash,
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
						block_number: *block_height.root(),
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

		Ok(Some((result.into_iter().sorted().collect(), return_block_hash)))
	}
}

#[derive(Clone)]
pub struct ArbitrumFeeVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}
#[async_trait::async_trait]
impl VoterApi<ArbitrumFeeTracking> for ArbitrumFeeVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumFeeTracking as ElectoralSystemTypes>::ElectoralSettings,
		_properties: <ArbitrumFeeTracking as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumFeeTracking>>, anyhow::Error> {
		let (_, _, l2_base_fee, l1_base_fee_estimate) = self
			.client
			.gas_estimate_components(
				// Using zero address as a proxy destination address for the gas estimation.
				H160::default(),
				false,
				// Using empty data for the gas estimation
				Bytes::default(),
			)
			.await?;

		Ok(Some(ArbitrumTrackedData {
			base_fee: l2_base_fee.try_into().expect("Base fee should fit u128"),
			l1_base_fee_estimate: l1_base_fee_estimate
				.try_into()
				.expect("L1 base fee should fit u128"),
		}))
	}
}

#[derive(Clone)]
pub struct ArbitrumLivenessVoter {
	client: EvmCachingClient<EvmRpcSigningClient>,
}
#[async_trait::async_trait]
impl VoterApi<ArbitrumLiveness> for ArbitrumLivenessVoter {
	async fn vote(
		&self,
		_settings: <ArbitrumLiveness as ElectoralSystemTypes>::ElectoralSettings,
		properties: <ArbitrumLiveness as ElectoralSystemTypes>::ElectionProperties,
	) -> std::result::Result<Option<VoteOf<ArbitrumLiveness>>, anyhow::Error> {
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
		+ ElectoralApi<ArbitrumInstance>
		+ 'static
		+ Send
		+ Sync,
{
	tracing::debug!("Starting ARB witness");

	let key_manager_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumKeyManagerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get KeyManager address from SC")?;

	let vault_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumVaultAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to get Vault contract address from SC")?;

	let address_checker_address = state_chain_client
		.storage_value::<pallet_cf_environment::ArbitrumAddressCheckerAddress<state_chain_runtime::Runtime>>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.expect(STATE_CHAIN_CONNECTION);

	let supported_erc20_tokens: HashMap<ArbAsset, H160> = state_chain_client
		.storage_map::<pallet_cf_environment::ArbitrumSupportedAssets<state_chain_runtime::Runtime>, _>(
			state_chain_client.latest_finalized_block().hash,
		)
		.await
		.context("Failed to fetch Arbitrum supported assets")?;

	let usdc_contract_address =
		*supported_erc20_tokens.get(&ArbAsset::ArbUsdc).context("USDC not supported")?;

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
					CompositeVoter::<ArbitrumElectoralSystemRunner, _>::new((
						ArbitrumBlockHeightWitnesserVoter { client: client.clone() },
						ArbitrumDepositChannelWitnesserVoter {
							client: client.clone(),
							address_checker_address,
							vault_address,
							usdc_contract_address,
						},
						ArbitrumVaultDepositWitnesserVoter {
							client: client.clone(),
							vault_address,
							supported_assets: supported_erc20_tokens.clone(),
						},
						ArbitrumKeyManagerWitnesserVoter {
							client: client.clone(),
							key_manager_address,
						},
						ArbitrumFeeVoter { client: client.clone() },
						ArbitrumLivenessVoter { client: client.clone() },
					)),
					Some(client.cache_invalidation_senders),
					"Arbitrum",
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
