use std::{collections::HashSet, sync::Arc};

use cf_chains::Ethereum;
use cf_primitives::chains::assets;
use ethers::types::{Bloom, H160};
use pallet_cf_ingress_egress::{DepositChannelDetails, DepositWitness};
use sp_core::{H256, U256};
use state_chain_runtime::EthereumInstance;

use crate::{
	eth::retry_rpc::EthersRetryRpcApi,
	state_chain_observer::client::{
		chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
};

use super::{
	chain_source::Header,
	chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	common::STATE_CHAIN_CONNECTION,
	contract_common::events_at_block,
};

pub enum Erc20Events {
	TransferFilter { to: H160, from: H160, value: U256 },
	Other,
}

macro_rules! define_erc20 {
	($mod_name:ident, $name:ident, $event_name:ident, $path:literal) => {
		pub mod $mod_name {
			use super::Erc20Events;
			use ethers::prelude::abigen;

			abigen!($name, $path);

			impl From<$event_name> for Erc20Events {
				fn from(event: $event_name) -> Self {
					match event {
						$event_name::TransferFilter(TransferFilter { to, from, value }) =>
							Self::TransferFilter { to, from, value },
						_ => Self::Other,
					}
				}
			}
		}
	};
}

define_erc20!(flip, Flip, FlipEvents, "eth-contract-abis/perseverance-rc17/IFLIP.json");
define_erc20!(usdc, Usdc, UsdcEvents, "eth-contract-abis/IUSDC.json");

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub async fn erc20_deposits<StateChainClient, EthRetryRpcClient, Events>(
		self,
		state_chain_client: Arc<StateChainClient>,
		eth_rpc: EthRetryRpcClient,
		asset: assets::eth::Asset,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner: ChunkedByVault<
			Index = u64,
			Hash = H256,
			Data = (Bloom, Vec<DepositChannelDetails<Ethereum>>),
			Chain = Ethereum,
		>,
		StateChainClient: SignedExtrinsicApi + StorageApi + ChainApi + Send + Sync + 'static,
		EthRetryRpcClient: EthersRetryRpcApi + Send + Sync + Clone,
		Events: std::fmt::Debug
			+ ethers::contract::EthLogDecode
			+ Send
			+ Sync
			+ Into<Erc20Events>
			+ 'static,
	{
		let erc20_contract_address = state_chain_client
			.storage_map_entry::<pallet_cf_environment::EthereumSupportedAssets<state_chain_runtime::Runtime>>(
				state_chain_client.latest_finalized_hash(),
				&asset,
			)
			.await
			.expect(STATE_CHAIN_CONNECTION)
			.unwrap()
			.into();

		self.then(move |epoch, header| {
			let state_chain_client = state_chain_client.clone();
			let eth_rpc = eth_rpc.clone();
			async move {
				let addresses = header
					.data
					.1
					.into_iter()
					.map(|deposit_channel| deposit_channel.deposit_channel.address)
					.collect::<HashSet<_>>();

				let deposit_witnesses = events_at_block::<Events, _>(
					Header {
						index: header.index,
						hash: header.hash,
						parent_hash: header.parent_hash,
						data: header.data.0,
					},
					erc20_contract_address,
					&eth_rpc,
				)
				.await?
				.into_iter()
				.filter_map(|event| {
					match event.event_parameters.into() {
						Erc20Events::TransferFilter{to, value, from: _ } if addresses.contains(&to) =>
							Some(DepositWitness {
								deposit_address: to,
								amount: value.try_into().expect(
									"Any ERC20 tokens we support should have amounts that fit into a u128",
								),
								asset,
								deposit_details: (),
							}),
						_ => None,
				}
				})
				.collect::<Vec<_>>();

				if !deposit_witnesses.is_empty() {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_ingress_egress::Call::<_, EthereumInstance>::process_deposits {
									deposit_witnesses,
									block_height: header.index,
								}
								.into(),
							),
							epoch_index: epoch.index,
						})
						.await;
				}

				Ok::<(), anyhow::Error>(())
			}
		})
	}
}
