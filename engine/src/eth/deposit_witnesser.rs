use std::sync::Arc;

use async_trait::async_trait;
use cf_chains::eth::Ethereum;
use sp_core::H160;
use state_chain_runtime::EthereumInstance;
use tokio::sync::Mutex;

use crate::{
	eth::core_h160,
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
	witnesser::{EpochStart, ItemMonitor},
};

use super::{
	eth_block_witnessing::BlockProcessor,
	rpc::{EthHttpRpcClient, EthRpcApi},
	EthNumberBloom,
};

pub struct DepositWitnesser<StateChainClient> {
	rpc: EthHttpRpcClient,
	state_chain_client: Arc<StateChainClient>,
	address_monitor: Arc<Mutex<ItemMonitor<H160, H160, ()>>>,
}

impl<StateChainClient> DepositWitnesser<StateChainClient>
where
	StateChainClient: SignedExtrinsicApi + Send + Sync,
{
	pub fn new(
		state_chain_client: Arc<StateChainClient>,
		rpc: EthHttpRpcClient,
		address_monitor: Arc<Mutex<ItemMonitor<H160, H160, ()>>>,
	) -> Self {
		Self { rpc, state_chain_client, address_monitor }
	}
}

async fn filter_successful_txs<Rpc>(
	txs: Vec<(web3::types::Transaction, sp_core::H160)>,
	eth_rpc: &Rpc,
) -> anyhow::Result<impl Iterator<Item = (web3::types::Transaction, sp_core::H160)>>
where
	Rpc: EthRpcApi,
{
	use futures::StreamExt;

	const MAX_CONCURRENT_REQUESTS: usize = 10;

	let futures = txs.iter().map(|(tx, _)| eth_rpc.transaction_receipt(tx.hash));
	let receipts = utilities::assert_stream_send(futures::stream::iter(futures))
		.buffered(MAX_CONCURRENT_REQUESTS)
		.collect::<Vec<_>>()
		.await
		.into_iter()
		.collect::<Result<Vec<_>, _>>()?;

	// Note that we abort in case any of the receipts are missing a status:
	let statuses = receipts
		.into_iter()
		.map(|receipt| {
			receipt.status.ok_or_else(|| anyhow::anyhow!("Receipt did not contain status"))
		})
		.collect::<Result<Vec<_>, anyhow::Error>>()?;

	Ok(txs.into_iter().zip(statuses.into_iter()).filter_map(|(tx, status)| {
		if status.as_u64() == 1 {
			Some(tx)
		} else {
			None
		}
	}))
}

#[async_trait]
impl<StateChainClient> BlockProcessor for DepositWitnesser<StateChainClient>
where
	StateChainClient: SignedExtrinsicApi + Send + Sync,
{
	async fn process_block(
		&mut self,
		epoch: &EpochStart<Ethereum>,
		block: &EthNumberBloom,
	) -> anyhow::Result<()> {
		use crate::eth::rpc::EthRpcApi;
		use cf_primitives::chains::assets::eth;
		use itertools::Itertools;
		use pallet_cf_ingress_egress::DepositWitness;

		let mut address_monitor =
			self.address_monitor.try_lock().expect("should have exclusive ownership");

		let unchecked_txs = self.rpc.block_with_txs(block.block_number).await?.transactions;

		// Before we process the transactions, check if
		// we have any new addresses to monitor
		address_monitor.sync_items();

		let interesting_txs = unchecked_txs
			.into_iter()
			.filter_map(|tx| {
				let to_addr = core_h160(tx.to?);
				if address_monitor.contains(&to_addr) {
					Some((tx, to_addr))
				} else {
					None
				}
			})
			.collect::<Vec<_>>();

		let successful_txs = filter_successful_txs(interesting_txs, &self.rpc).await?;

		let deposit_witnesses = successful_txs
			.unique_by(|(tx, _)| tx.hash)
			.map(|(tx, to_addr)| DepositWitness {
				deposit_address: to_addr,
				asset: eth::Asset::Eth,
				amount: tx
					.value
					.try_into()
					.expect("Ingress witness transfer value should fit u128"),
				deposit_details: (),
			})
			.collect::<Vec<DepositWitness<Ethereum>>>();

		if !deposit_witnesses.is_empty() {
			self.state_chain_client
				.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
					call: Box::new(
						pallet_cf_ingress_egress::Call::<_, EthereumInstance>::process_deposits {
							deposit_witnesses,
							block_height: block.block_number.as_u64(),
						}
						.into(),
					),
					epoch_index: epoch.epoch_index,
				})
				.await;
		}

		Ok(())
	}
}

#[tokio::test]
async fn test_successful_tx_filter() {
	use web3::types::{TransactionReceipt, H256, U64};
	let mut rpc = crate::eth::rpc::MockEthRpcApi::default();

	// A tx with default hash is not successful, anything else is:
	rpc.expect_transaction_receipt().returning(|tx_hash| {
		if tx_hash == Default::default() {
			Ok(TransactionReceipt { status: Some(U64::from(0)), ..Default::default() })
		} else {
			Ok(TransactionReceipt { status: Some(U64::from(1)), ..Default::default() })
		}
	});

	// tx1 and tx3 are successful, tx2 is not:
	let tx1 = (
		web3::types::Transaction { hash: H256::from([0x1; 32]), ..Default::default() },
		Default::default(),
	);
	let tx2 = (
		web3::types::Transaction { hash: H256::default(), ..Default::default() },
		Default::default(),
	);
	let tx3 = (
		web3::types::Transaction { hash: H256::from([0x3; 32]), ..Default::default() },
		Default::default(),
	);

	let txs = vec![tx1.clone(), tx2, tx3.clone()];

	assert_eq!(filter_successful_txs(txs, &rpc).await.unwrap().collect::<Vec<_>>(), vec![tx1, tx3]);
}

#[tokio::test]
async fn successful_tx_filter_on_no_status() {
	use web3::types::{TransactionReceipt, H256};
	let mut rpc = crate::eth::rpc::MockEthRpcApi::default();

	// Return receipts with no status (which is unexpected)
	rpc.expect_transaction_receipt()
		.returning(|_| Ok(TransactionReceipt { status: None, ..Default::default() }));

	let txs = vec![(
		web3::types::Transaction { hash: H256::from([0x1; 32]), ..Default::default() },
		Default::default(),
	)];

	// We expect error due to missing status:
	assert!(filter_successful_txs(txs, &rpc).await.is_err());
}
