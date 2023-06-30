use super::{event::Event, BlockWithItems, DecodeLogClosure, EthContractWitnesser};
use crate::{
	eth::EthRpcApi, state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
};
use anyhow::Result;
use async_trait::async_trait;
use cf_primitives::EpochIndex;
use ethers::{abi::RawLog, contract::EthLogDecode};
use std::sync::Arc;
use tracing::{info, trace};
use web3::types::H160;

use ethers::prelude::*;

abigen!(StateChainGateway, "eth-contract-abis/perseverance-rc17/IStateChainGateway.json");

pub struct StateChainGateway {
	pub deployed_address: H160,
}

#[async_trait]
impl EthContractWitnesser for StateChainGateway {
	type EventParameters = StateChainGatewayEvents;

	fn contract_name(&self) -> String {
		"StateChainGateway".to_string()
	}

	async fn handle_block_events<StateChainClient, EthRpcClient>(
		&mut self,
		epoch: EpochIndex,
		block_number: u64,
		block: BlockWithItems<Event<Self::EventParameters>>,
		state_chain_client: Arc<StateChainClient>,
		_eth_rpc: &EthRpcClient,
	) -> anyhow::Result<()>
	where
		EthRpcClient: EthRpcApi + Sync + Send,
		StateChainClient: SignedExtrinsicApi + Send + Sync,
	{
		for event in block.block_items {
			info!("Handling event: {event}");
			match event.event_parameters {
				StateChainGatewayEvents::FundedFilter(FundedFilter {
					node_id: account_id,
					amount,
					funder,
				}) => {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_funding::Call::funded {
									account_id: account_id.into(),
									amount: amount
										.try_into()
										.expect("Funded amount should fit in u128"),
									funder: funder.into(),
									tx_hash: event.tx_hash.into(),
								}
								.into(),
							),
							epoch_index: epoch,
						})
						.await;
				},
				StateChainGatewayEvents::RedemptionExecutedFilter(RedemptionExecutedFilter {
					node_id: account_id,
					amount,
				}) => {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_funding::Call::redeemed {
									account_id: account_id.into(),
									redeemed_amount: amount
										.try_into()
										.expect("Redemption amount should fit in u128"),
									tx_hash: event.tx_hash.to_fixed_bytes(),
								}
								.into(),
							),
							epoch_index: epoch,
						})
						.await;
				},
				StateChainGatewayEvents::RedemptionExpiredFilter(RedemptionExpiredFilter {
					node_id: account_id,
					amount: _,
				}) => {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_funding::Call::redemption_expired {
									account_id: account_id.into(),
									block_number,
								}
								.into(),
							),
							epoch_index: epoch,
						})
						.await;
				},
				_ => {
					trace!("Ignoring unused event: {event}");
				},
			}
		}

		Ok(())
	}

	fn contract_address(&self) -> H160 {
		self.deployed_address
	}

	fn decode_log_closure(&self) -> Result<DecodeLogClosure<Self::EventParameters>> {
		Ok(Box::new(move |raw_log: RawLog| -> Result<Self::EventParameters> {
			Ok(StateChainGatewayEvents::decode_log(&raw_log)?)
		}))
	}
}

impl StateChainGateway {
	pub fn new(deployed_address: H160) -> Self {
		Self { deployed_address }
	}
}
