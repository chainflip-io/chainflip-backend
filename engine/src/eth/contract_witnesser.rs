use std::sync::Arc;

use async_trait::async_trait;
use cf_chains::eth::Ethereum;
use sp_core::H160;

use crate::{
	eth::web3_h160, state_chain_observer::client::extrinsic_api::ExtrinsicApi,
	witnesser::EpochStart,
};

use super::{
	core_h160, eth_block_witnessing::BlockProcessor, event::Event, rpc::EthDualRpcClient,
	BlockWithItems, DecodeLogClosure, EthContractWitnesser, EthNumberBloom,
};

pub struct ContractWitnesser<Contract, StateChainClient> {
	contract: Contract,
	rpc: EthDualRpcClient,
	state_chain_client: Arc<StateChainClient>,
	should_witness_historical_epochs: bool,
	logger: slog::Logger,
}

impl<Contract, StateChainClient> ContractWitnesser<Contract, StateChainClient>
where
	Contract: EthContractWitnesser,
	StateChainClient: ExtrinsicApi + Send + Sync,
{
	pub fn new(
		contract: Contract,
		state_chain_client: Arc<StateChainClient>,
		rpc: EthDualRpcClient,
		should_witness_historical_epochs: bool,
		logger: &slog::Logger,
	) -> Self {
		Self {
			contract,
			rpc,
			state_chain_client,
			should_witness_historical_epochs,
			logger: logger.clone(),
		}
	}
}

#[async_trait]
impl<Contract, StateChainClient> BlockProcessor for ContractWitnesser<Contract, StateChainClient>
where
	Contract: EthContractWitnesser + Send + Sync,
	StateChainClient: ExtrinsicApi + Send + Sync,
{
	async fn process_block(
		&mut self,
		epoch: &EpochStart<Ethereum>,
		block: &EthNumberBloom,
	) -> anyhow::Result<()> {
		if !self.should_witness_historical_epochs && !epoch.current {
			return Ok(())
		}

		let contract_address = self.contract.contract_address();

		let events = block_to_events(
			block,
			core_h160(contract_address),
			&self.contract.decode_log_closure().unwrap(),
			&self.rpc,
		)
		.await?;

		self.contract
			.handle_block_events(
				epoch.epoch_index,
				block.block_number.as_u64(),
				BlockWithItems { block_number: block.block_number.as_u64(), block_items: events },
				// Can't this just take a reference?
				self.state_chain_client.clone(),
				&self.rpc,
				&self.logger,
			)
			.await?;

		Ok(())
	}
}

pub async fn block_to_events<'a, EventParameters>(
	header: &'a EthNumberBloom,
	contract_address: H160,
	decode_log_fn: &DecodeLogClosure<EventParameters>,
	eth_rpc: &'a EthDualRpcClient,
) -> anyhow::Result<Vec<Event<EventParameters>>>
where
	EventParameters: core::fmt::Debug + Send + Sync + 'static,
{
	use crate::eth::rpc::EthRpcApi;
	use ethbloom::{Bloom, Input};
	use web3::types::{BlockNumber, FilterBuilder};

	let mut contract_bloom = Bloom::default();
	contract_bloom.accrue(Input::Raw(&contract_address.0));

	let block_number = header.block_number;

	// if we have logs for this block, fetch them.
	if header.logs_bloom.contains_bloom(&contract_bloom) {
		let logs = eth_rpc
			.get_logs(
				FilterBuilder::default()
					// from_block *and* to_block are *inclusive*
					.from_block(BlockNumber::Number(block_number))
					.to_block(BlockNumber::Number(block_number))
					.address(vec![web3_h160(contract_address)])
					.build(),
			)
			.await?;

		logs.into_iter()
			.map(|unparsed_log| -> anyhow::Result<Event<EventParameters>> {
				Event::<EventParameters>::new_from_unparsed_logs(decode_log_fn, unparsed_log)
			})
			.collect::<anyhow::Result<Vec<_>>>()
	} else {
		// we know there won't be interesting logs, so don't fetch for events
		Ok(vec![])
	}
}
