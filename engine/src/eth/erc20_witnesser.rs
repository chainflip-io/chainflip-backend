use std::sync::Arc;

use async_trait::async_trait;
use cf_primitives::{chains::assets::eth, EpochIndex};
use state_chain_runtime::EthereumInstance;
use tokio::sync::Mutex;
use web3::types::H160;

use ethers::{abi::RawLog, prelude::*};

use crate::{
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi, witnesser::ItemMonitor,
};

abigen!(Erc20, "eth-contract-abis/IERC20.json");

use super::{
	core_h256, event::Event, rpc::EthRpcApi, BlockWithItems, DecodeLogClosure, EthContractWitnesser,
};
use pallet_cf_ingress_egress::DepositWitness;

use anyhow::Result;

/// Can witness txs of a particular ERC20 token to any of the monitored addresses.
/// NB: Any tokens watched by this must *strictly* adhere to the ERC20 standard: https://eips.ethereum.org/EIPS/eip-20
pub struct Erc20Witnesser {
	pub deployed_address: H160,
	asset: eth::Asset,
	address_monitor: Arc<Mutex<ItemMonitor<sp_core::H160, sp_core::H160, ()>>>,
}

impl Erc20Witnesser {
	/// Loads the contract abi to get the event definitions
	pub fn new(
		deployed_address: H160,
		asset: eth::Asset,
		address_monitor: Arc<Mutex<ItemMonitor<sp_core::H160, sp_core::H160, ()>>>,
	) -> Self {
		Self { deployed_address, asset, address_monitor }
	}
}

#[async_trait]
impl EthContractWitnesser for Erc20Witnesser {
	type EventParameters = Erc20Events;

	fn contract_name(&self) -> String {
		format!("ERC20-{:?}", self.asset)
	}

	async fn handle_block_events<StateChainClient, EthRpcClient>(
		&mut self,
		epoch: EpochIndex,
		_block_number: u64,
		block: BlockWithItems<Event<Self::EventParameters>>,
		state_chain_client: Arc<StateChainClient>,
		_eth_rpc: &EthRpcClient,
	) -> Result<()>
	where
		EthRpcClient: EthRpcApi + Sync + Send,
		StateChainClient: SignedExtrinsicApi + Send + Sync,
	{
		let mut address_monitor =
			self.address_monitor.try_lock().expect("should have exclusive ownership");

		address_monitor.sync_items();

		let deposit_witnesses: Vec<_> = block
			.block_items
			.into_iter()
			.filter_map(|event| match event.event_parameters {
				Erc20Events::TransferFilter(TransferFilter { to, value, from: _ })
					if address_monitor.contains(&to) =>
					Some(DepositWitness {
						deposit_address: to,
						amount: value.try_into().expect(
							"Any ERC20 tokens we support should have amounts that fit into a u128",
						),
						asset: self.asset,
						tx_id: core_h256(event.tx_hash),
					}),
				_ => None,
			})
			.collect();

		if !deposit_witnesses.is_empty() {
			state_chain_client
				.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
					call: Box::new(
						pallet_cf_ingress_egress::Call::<_, EthereumInstance>::process_deposits {
							deposit_witnesses,
						}
						.into(),
					),
					epoch_index: epoch,
				})
				.await;
		}

		Ok(())
	}

	fn contract_address(&self) -> H160 {
		self.deployed_address
	}

	fn decode_log_closure(&self) -> DecodeLogClosure<Self::EventParameters> {
		Box::new(move |raw_log: RawLog| -> Result<Self::EventParameters> {
			Ok(Erc20Events::decode_log(&raw_log)?)
		})
	}
}
