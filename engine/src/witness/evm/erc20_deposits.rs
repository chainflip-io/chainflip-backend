use std::collections::HashSet;

use cf_chains::instances::ChainInstanceFor;
use cf_primitives::EpochIndex;
use ethers::types::{Bloom, H160};
use futures_core::Future;
use pallet_cf_ingress_egress::DepositWitness;
use sp_core::{H256, U256};

use crate::{
	eth::retry_rpc::EthersRetryRpcApi,
	witness::common::{
		chunked_chain_source::chunked_by_vault::deposit_addresses::Addresses, RuntimeCallHasChain,
		RuntimeHasChain,
	},
};

use super::{
	super::common::{
		chain_source::Header,
		chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	},
	contract_common::events_at_block,
};

pub enum Erc20Events {
	TransferFilter { to: H160, from: H160, value: U256 },
	Other,
}

macro_rules! define_erc20 {
	($mod_name:ident, $name:ident, $contract_events_type:ident, $abi_path:literal) => {
		pub mod $mod_name {
			use super::Erc20Events;
			use ethers::prelude::abigen;

			abigen!($name, $abi_path);

			impl From<$contract_events_type> for Erc20Events {
				fn from(event: $contract_events_type) -> Self {
					match event {
						$contract_events_type::TransferFilter(TransferFilter {
							to,
							from,
							value,
						}) => Self::TransferFilter { to, from, value },
						_ => Self::Other,
					}
				}
			}
		}
	};
}

define_erc20!(
	flip,
	Flip,
	FlipEvents,
	"$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IFLIP.json"
);
define_erc20!(usdc, Usdc, UsdcEvents, "$CF_ETH_CONTRACT_ABI_ROOT/IUSDC.json");
define_erc20!(usdt, Usdt, UsdtEvents, "$CF_ETH_CONTRACT_ABI_ROOT/IUSDT.json");

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub async fn erc20_deposits<ProcessCall, ProcessingFut, EthRetryRpcClient, Events>(
		self,
		process_call: ProcessCall,
		eth_rpc: EthRetryRpcClient,
		asset: <Inner::Chain as cf_chains::Chain>::ChainAsset,
		asset_contract_address: H160,
	) -> Result<ChunkedByVaultBuilder<impl ChunkedByVault>, anyhow::Error>
	where
		Inner::Chain:
			cf_chains::Chain<ChainAmount = u128, DepositDetails = (), ChainAccount = H160>,
		Inner: ChunkedByVault<Index = u64, Hash = H256, Data = (Bloom, Addresses<Inner>)>,
		ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		ProcessingFut: Future<Output = ()> + Send + 'static,
		EthRetryRpcClient: EthersRetryRpcApi + Send + Sync + Clone,
		Events: std::fmt::Debug
			+ ethers::contract::EthLogDecode
			+ Send
			+ Sync
			+ Into<Erc20Events>
			+ 'static,
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		state_chain_runtime::RuntimeCall:
			RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
	{
		Ok(self.then(move |epoch, header| {
			let process_call = process_call.clone();
			let eth_rpc = eth_rpc.clone();
			async move {
				let addresses = header
					.data
					.1
					.into_iter()
					.filter(|deposit_channel| deposit_channel.deposit_channel.asset == asset)
					.map(|deposit_channel| deposit_channel.deposit_channel.address)
					.collect::<HashSet<_>>();

				let deposit_witnesses = events_at_block::<Events, _>(
					Header {
						index: header.index,
						hash: header.hash,
						parent_hash: header.parent_hash,
						data: header.data.0,
					},
					asset_contract_address,
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
					process_call(
						pallet_cf_ingress_egress::Call::<
							_,
							ChainInstanceFor<Inner::Chain>,
						>::process_deposits {
							deposit_witnesses,
							block_height: header.index,
						}
						.into(),
						epoch.index,
					)
					.await;
				}

				Ok::<(), anyhow::Error>(())
			}
		}))
	}
}
