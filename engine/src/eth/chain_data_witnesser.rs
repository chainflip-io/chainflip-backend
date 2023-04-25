use async_trait::async_trait;
use futures::StreamExt;
use std::{sync::Arc, time::Duration};
use tokio_stream::wrappers::IntervalStream;

use crate::{
	state_chain_observer::client::extrinsic_api::signed::SignedExtrinsicApi,
	witnesser::{
		epoch_process_runner::{
			self, start_epoch_process_runner, EpochProcessGenerator, EpochWitnesser,
			WitnesserInitResult,
		},
		EpochStart,
	},
};

use super::rpc::EthRpcApi;

use cf_chains::eth::{Ethereum, EthereumTrackedData};

use state_chain_runtime::CfeSettings;
use tokio::sync::{oneshot, watch, Mutex};
use tracing::{error, info_span, Instrument};
use utilities::{context, make_periodic_tick};
use web3::types::{BlockNumber, U256};

const ETH_CHAIN_TRACKING_POLL_INTERVAL: Duration = Duration::from_secs(4);

pub async fn start<StateChainClient, EthRpcClient>(
	eth_rpc: EthRpcClient,
	state_chain_client: Arc<StateChainClient>,
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Ethereum>>,
	cfe_settings_update_receiver: watch::Receiver<CfeSettings>,
) -> anyhow::Result<(), ()>
where
	EthRpcClient: 'static + EthRpcApi + Clone + Send + Sync,
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
{
	start_epoch_process_runner(
		Arc::new(Mutex::new(epoch_start_receiver)),
		ChainDataWitnesserGenerator { eth_rpc, state_chain_client, cfe_settings_update_receiver },
		EthereumTrackedData::default(),
	)
	.instrument(info_span!("Eth-Chain-Data-Witnesser"))
	.await
}

struct ChainDataWitnesser<StateChainClient, EthRpcClient> {
	state_chain_client: Arc<StateChainClient>,
	cfe_settings_update_receiver: watch::Receiver<CfeSettings>,
	eth_rpc: EthRpcClient,
	current_epoch: EpochStart<Ethereum>,
}

#[async_trait]
impl<StateChainClient, EthRpcClient> EpochWitnesser
	for ChainDataWitnesser<StateChainClient, EthRpcClient>
where
	EthRpcClient: EthRpcApi + 'static + Clone + Send + Sync,
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
{
	type Chain = Ethereum;
	type Data = ();
	type StaticState = EthereumTrackedData;

	const SHOULD_PROCESS_HISTORICAL_EPOCHS: bool = false;

	async fn run_witnesser(
		mut self,
		data_stream: std::pin::Pin<
			Box<dyn futures::Stream<Item = anyhow::Result<Self::Data>> + Send + 'static>,
		>,
		end_witnessing_receiver: oneshot::Receiver<
			<Ethereum as cf_chains::Chain>::ChainBlockNumber,
		>,
		state: Self::StaticState,
	) -> Result<Self::StaticState, ()> {
		epoch_process_runner::run_witnesser_data_stream(
			self,
			data_stream,
			end_witnessing_receiver,
			state,
		)
		.await
	}

	async fn do_witness(
		&mut self,
		_data: Self::Data,
		last_witnessed_data: &mut EthereumTrackedData,
	) -> anyhow::Result<()> {
		let priority_fee = self.cfe_settings_update_receiver.borrow().eth_priority_fee_percentile;
		let latest_data = get_tracked_data(&self.eth_rpc, priority_fee).await.map_err(|e| {
			error!("Failed to get tracked data: {e:?}");
			e
		})?;

		if latest_data.block_height > last_witnessed_data.block_height ||
			latest_data.base_fee != last_witnessed_data.base_fee
		{
			let _result = self
				.state_chain_client
				.submit_signed_extrinsic(state_chain_runtime::RuntimeCall::Witnesser(
					pallet_cf_witnesser::Call::witness_at_epoch {
						call: Box::new(state_chain_runtime::RuntimeCall::EthereumChainTracking(
							pallet_cf_chain_tracking::Call::update_chain_state {
								state: latest_data,
							},
						)),
						epoch_index: self.current_epoch.epoch_index,
					},
				))
				.await;

			*last_witnessed_data = latest_data;
		}

		Ok(())
	}
}

struct ChainDataWitnesserGenerator<StateChainClient, EthRpcClient> {
	state_chain_client: Arc<StateChainClient>,
	cfe_settings_update_receiver: watch::Receiver<CfeSettings>,
	eth_rpc: EthRpcClient,
}

#[async_trait]
impl<StateChainClient, EthRpcClient> EpochProcessGenerator
	for ChainDataWitnesserGenerator<StateChainClient, EthRpcClient>
where
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
	EthRpcClient: EthRpcApi + 'static + Send + Sync + Clone,
{
	type Witnesser = ChainDataWitnesser<StateChainClient, EthRpcClient>;
	async fn init(
		&mut self,
		epoch: EpochStart<Ethereum>,
	) -> anyhow::Result<WitnesserInitResult<ChainDataWitnesser<StateChainClient, EthRpcClient>>> {
		let witnesser = ChainDataWitnesser {
			state_chain_client: self.state_chain_client.clone(),
			cfe_settings_update_receiver: self.cfe_settings_update_receiver.clone(),
			eth_rpc: self.eth_rpc.clone(),
			current_epoch: epoch,
		};

		let poll_interval =
			IntervalStream::new(make_periodic_tick(ETH_CHAIN_TRACKING_POLL_INTERVAL, true))
				.map(|_| Ok(()));

		Ok(WitnesserInitResult::Created((witnesser, Box::pin(poll_interval))))
	}
}

/// Queries the rpc node for the fee history and builds the `TrackedData` for Ethereum at the latest
/// block number.
async fn get_tracked_data<EthRpcClient: EthRpcApi + Send + Sync>(
	rpc: &EthRpcClient,
	priority_fee_percentile: u8,
) -> anyhow::Result<EthereumTrackedData> {
	let fee_history = rpc
		.fee_history(
			U256::one(),
			BlockNumber::Latest,
			Some(vec![priority_fee_percentile as f64 / 100_f64]),
		)
		.await?;

	if let BlockNumber::Number(block_number) = fee_history.oldest_block {
		Ok(EthereumTrackedData {
			block_height: block_number.as_u64(),
			base_fee: (*context!(fee_history.base_fee_per_gas.first())?)
				.try_into()
				.expect("Base fee should fit u128"),
			priority_fee: (*context!(context!(context!(fee_history.reward)?.first())?.first())?)
				.try_into()
				.expect("Priority fee should fit u128"),
		})
	} else {
		Err(anyhow::anyhow!("fee_history did not return `oldest_block` as a number"))
	}
}

#[cfg(test)]
mod tests {
	use web3::types::U64;

	use super::*;

	#[tokio::test]
	async fn test_get_tracked_data() {
		use crate::eth::rpc::MockEthRpcApi;

		const BLOCK_HEIGHT: u64 = 42;
		const BASE_FEE: u128 = 40_000_000_000;
		const PRIORITY_FEE: u128 = 5_000_000_000;

		let mut rpc = MockEthRpcApi::new();

		// ** Rpc Api Assumptions **
		rpc.expect_fee_history().once().returning(|_, _, _| {
			Ok(web3::types::FeeHistory {
				oldest_block: BlockNumber::Number(U64::from(BLOCK_HEIGHT)),
				base_fee_per_gas: vec![U256::from(BASE_FEE)],
				gas_used_ratio: vec![],
				reward: Some(vec![vec![U256::from(PRIORITY_FEE)]]),
			})
		});
		// ** Rpc Api Assumptions **

		assert_eq!(
			get_tracked_data(&rpc, 50).await.unwrap(),
			EthereumTrackedData {
				block_height: BLOCK_HEIGHT,
				base_fee: BASE_FEE,
				priority_fee: PRIORITY_FEE,
			}
		);
	}
}
