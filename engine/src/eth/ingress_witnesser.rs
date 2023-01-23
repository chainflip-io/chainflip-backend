use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use cf_chains::eth::Ethereum;
use sp_core::H160;
use state_chain_runtime::EthereumInstance;

use crate::{
	eth::{core_h160, core_h256},
	state_chain_observer::client::extrinsic_api::ExtrinsicApi,
	witnesser::EpochStart,
};

use super::{eth_block_witnessing::BlockProcessor, rpc::EthDualRpcClient, EthNumberBloom};

pub struct IngressWitnesser<StateChainClient> {
	rpc: EthDualRpcClient,
	state_chain_client: Arc<StateChainClient>,
	monitored_addresses: BTreeSet<H160>,
	eth_monitor_ingress_receiver: tokio::sync::mpsc::UnboundedReceiver<H160>,
	logger: slog::Logger,
}

impl<StateChainClient> IngressWitnesser<StateChainClient>
where
	StateChainClient: ExtrinsicApi + Send + Sync,
{
	pub fn new(
		state_chain_client: Arc<StateChainClient>,
		rpc: EthDualRpcClient,
		monitored_addresses: BTreeSet<H160>,
		eth_monitor_ingress_receiver: tokio::sync::mpsc::UnboundedReceiver<H160>,
		logger: &slog::Logger,
	) -> Self {
		Self {
			rpc,
			state_chain_client,
			monitored_addresses,
			eth_monitor_ingress_receiver,
			logger: logger.clone(),
		}
	}
}

#[async_trait]
impl<StateChainClient> BlockProcessor for IngressWitnesser<StateChainClient>
where
	StateChainClient: ExtrinsicApi + Send + Sync,
{
	async fn process_block(
		&mut self,
		epoch: &EpochStart<Ethereum>,
		block: &EthNumberBloom,
	) -> anyhow::Result<()> {
		use crate::eth::rpc::EthRpcApi;
		use cf_primitives::chains::assets::eth;
		use pallet_cf_ingress_egress::IngressWitness;

		let txs = self.rpc.block_with_txs(block.block_number).await?.transactions;

		// Before we process the transactions, check if
		// we have any new addresses to monitor
		while let Some(address) = self.eth_monitor_ingress_receiver.recv().await {
			self.monitored_addresses.insert(address);
		}

		let ingress_witnesses = txs
			.iter()
			.filter_map(|tx| {
				let to_addr = core_h160(tx.to?);
				if self.monitored_addresses.contains(&to_addr) {
					Some((tx, to_addr))
				} else {
					None
				}
			})
			.map(|(tx, to_addr)| IngressWitness {
				ingress_address: to_addr,
				asset: eth::Asset::Eth,
				amount: tx
					.value
					.try_into()
					.expect("Ingress witness transfer value should fit u128"),
				tx_id: core_h256(tx.hash),
			})
			.collect::<Vec<IngressWitness<Ethereum>>>();

		if !ingress_witnesses.is_empty() {
			let _result = self
				.state_chain_client
				.submit_signed_extrinsic(
					pallet_cf_witnesser::Call::witness_at_epoch {
						call: Box::new(
							pallet_cf_ingress_egress::Call::<_, EthereumInstance>::do_ingress {
								ingress_witnesses,
							}
							.into(),
						),
						epoch_index: epoch.epoch_index,
					},
					&self.logger,
				)
				.await;
		}

		Ok(())
	}
}
