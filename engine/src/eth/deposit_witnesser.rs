use std::sync::Arc;

use async_trait::async_trait;
use cf_chains::eth::Ethereum;
use sp_core::H160;
use state_chain_runtime::EthereumInstance;
use tokio::sync::Mutex;
use utilities::ring_buffer::RingBuffer;

use crate::{
	eth::{core_h160, core_h256},
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
	witnesser::{AddressMonitor, EpochStart},
};

use super::{eth_block_witnessing::BlockProcessor, rpc::EthDualRpcClient, EthNumberBloom};

type AddressMonitorEth = AddressMonitor<H160, H160, ()>;
type BlockTransactions = Vec<web3::types::Transaction>;

pub struct DepositWitnesser<StateChainClient> {
	rpc: EthDualRpcClient,
	state_chain_client: Arc<StateChainClient>,
	block_cache: RingBuffer<BlockTransactions>,
	address_monitor: Arc<Mutex<AddressMonitorEth>>,
}

impl<StateChainClient> DepositWitnesser<StateChainClient>
where
	StateChainClient: SignedExtrinsicApi + Send + Sync,
{
	pub fn new(
		state_chain_client: Arc<StateChainClient>,
		rpc: EthDualRpcClient,
		address_monitor: Arc<Mutex<AddressMonitorEth>>,
	) -> Self {
		Self { rpc, state_chain_client, address_monitor, block_cache: RingBuffer::new(4) }
	}
}

use pallet_cf_ingress_egress::DepositWitness;

fn check_for_deposits_updating_cache(
	transactions_in_current_block: BlockTransactions,
	block_cache: &mut RingBuffer<BlockTransactions>,
	address_monitor: &mut AddressMonitorEth,
) -> Vec<DepositWitness<Ethereum>> {
	use cf_primitives::chains::assets::eth;
	// Before we process the transactions, check if
	// we have any new addresses to monitor
	let new_addresses = address_monitor.sync_addresses();

	let txs_from_cache = block_cache.iter().flatten().filter_map(|tx| {
		let to_addr = core_h160(tx.to?);

		new_addresses.contains(&to_addr).then(|| (tx, to_addr))
	});

	let txs_from_new_block = transactions_in_current_block.iter().filter_map(|tx| {
		let to_addr = core_h160(tx.to?);
		if address_monitor.contains(&to_addr) {
			Some((tx, to_addr))
		} else {
			None
		}
	});

	let deposit_witnesses = txs_from_new_block
		.chain(txs_from_cache)
		.map(|(tx, to_addr)| DepositWitness {
			deposit_address: to_addr,
			asset: eth::Asset::Eth,
			amount: tx.value.try_into().expect("Ingress witness transfer value should fit u128"),
			tx_id: core_h256(tx.hash),
		})
		.collect::<Vec<DepositWitness<Ethereum>>>();

	block_cache.push(transactions_in_current_block);

	deposit_witnesses
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

		let mut address_monitor =
			self.address_monitor.try_lock().expect("should have exclusive ownership");

		let transactions = self.rpc.successful_transactions(block.block_number).await?;

		let deposit_witnesses = check_for_deposits_updating_cache(
			transactions,
			&mut self.block_cache,
			&mut address_monitor,
		);

		if !deposit_witnesses.is_empty() {
			self.state_chain_client
				.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
					call: Box::new(
						pallet_cf_ingress_egress::Call::<_, EthereumInstance>::process_deposits {
							deposit_witnesses,
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

#[cfg(test)]
mod tests {

	use super::*;

	use web3::types::{Bytes, Transaction, H256, U256, U64};

	use crate::{
		eth::web3_h160,
		witnesser::{AddressMonitor, AddressMonitorCommand},
	};

	fn create_address() -> sp_core::H160 {
		use rand::Rng;
		let bytes: [u8; 20] = rand::thread_rng().gen();
		sp_core::H160::from(bytes)
	}

	fn create_tx(to: &sp_core::H160) -> Transaction {
		Transaction {
			hash: H256::from([0u8; 32]),
			nonce: U256::from(0),
			block_hash: None,
			block_number: None,
			transaction_index: Some(U64::from(0)),
			from: Some(web3_h160(create_address())),
			to: Some(web3_h160(to.clone())),
			value: 2000000.into(),
			gas_price: None,
			gas: 1000000.into(),
			input: Bytes(vec![]),
			v: None,
			r: None,
			s: None,
			raw: None,
			transaction_type: None,
			access_list: None,
			max_fee_per_gas: None,
			max_priority_fee_per_gas: None,
		}
	}

	#[test]
	fn deposit_witnessing_with_cache() {
		let address1 = create_address();
		let address2 = create_address();

		let tx1 = create_tx(&address1);
		let tx2 = create_tx(&address2);

		let mut block_cache = RingBuffer::new(4);

		// The next block will contain transactions for both Address 1
		// and Address 2. Only Address 1 is monitored initially, so only
		// one deposit will be witnessed at first:
		let (address_sender, mut address_monitor) =
			AddressMonitor::new([address1].into_iter().collect());
		{
			let next_block = vec![tx1, tx2];

			let deposits = check_for_deposits_updating_cache(
				next_block,
				&mut block_cache,
				&mut address_monitor,
			);

			assert_eq!(deposits.iter().next().unwrap().deposit_address, address1);
		}

		// Address 2 is received after the block mentioning it, but we should still
		// be able to witness the deposit due to the cache. Importantly no witness
		// for Address 1 should be generated:
		{
			address_sender.send(AddressMonitorCommand::Add(address2)).unwrap();

			let deposits =
				check_for_deposits_updating_cache(vec![], &mut block_cache, &mut address_monitor);

			assert_eq!(deposits.len(), 1);
			assert_eq!(deposits.iter().next().unwrap().deposit_address, address2);
		}

		// Both addresses have been witnessed, so no duplicate witnesses
		// should be generated:
		{
			let deposits =
				check_for_deposits_updating_cache(vec![], &mut block_cache, &mut address_monitor);

			assert!(deposits.is_empty());
		}
	}
}
