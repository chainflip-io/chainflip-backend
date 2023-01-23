use std::{sync::Arc, time::Duration};

use crate::{
	state_chain_observer::client::extrinsic_api::ExtrinsicApi,
	witnesser::{epoch_witnesser, EpochStart},
};

use super::rpc::EthRpcApi;

use cf_chains::eth::{Ethereum, TrackedData};

use state_chain_runtime::CfeSettings;
use tokio::sync::watch;
use utilities::{context, make_periodic_tick};
use web3::types::{BlockNumber, U256};

const ETH_CHAIN_TRACKING_POLL_INTERVAL: Duration = Duration::from_secs(4);

pub async fn start<StateChainClient, EthRpcClient>(
	eth_rpc: EthRpcClient,
	state_chain_client: Arc<StateChainClient>,
	epoch_start_receiver: async_broadcast::Receiver<EpochStart<Ethereum>>,
	cfe_settings_update_receiver: watch::Receiver<CfeSettings>,
	logger: &slog::Logger,
) -> anyhow::Result<()>
where
	EthRpcClient: 'static + EthRpcApi + Clone + Send + Sync,
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
{
	epoch_witnesser::start(
        "ETH-Chain-Data".to_string(),
        epoch_start_receiver,
        |epoch_start| epoch_start.current,
        TrackedData::<Ethereum>::default(),
        move |
            end_witnessing_signal,
            epoch_start,
            mut last_witnessed_data,
            logger
        | {
            let eth_rpc = eth_rpc.clone();
            let cfe_settings_update_receiver = cfe_settings_update_receiver.clone();

            let state_chain_client = state_chain_client.clone();
            async move {
                let mut poll_interval = make_periodic_tick(ETH_CHAIN_TRACKING_POLL_INTERVAL, false);

                loop {
                    if let Some(_end_block) = *end_witnessing_signal.lock().unwrap() {
                        break;
                    }

                    let priority_fee = cfe_settings_update_receiver
                            .borrow()
                            .eth_priority_fee_percentile;
                    let latest_data = get_tracked_data(
                        &eth_rpc,
                        priority_fee
                    ).await?;

                    if latest_data.block_height > last_witnessed_data.block_height || latest_data.base_fee != last_witnessed_data.base_fee {
                        let _result = state_chain_client
                            .submit_signed_extrinsic(
                                state_chain_runtime::RuntimeCall::Witnesser(pallet_cf_witnesser::Call::witness_at_epoch {
                                    call: Box::new(state_chain_runtime::RuntimeCall::EthereumChainTracking(
                                        pallet_cf_chain_tracking::Call::update_chain_state {
                                            state: latest_data,
                                        },
                                    )),
                                    epoch_index: epoch_start.epoch_index
                                }),
                                &logger,
                            )
                            .await;


                        last_witnessed_data = latest_data;

                    }

                    poll_interval.tick().await;
                }

                Ok(last_witnessed_data)
            }
        },
        logger,
    ).await
}

/// Queries the rpc node for the fee history and builds the `TrackedData` for Ethereum at the latest
/// block number.
async fn get_tracked_data<EthRpcClient: EthRpcApi + Send + Sync>(
	rpc: &EthRpcClient,
	priority_fee_percentile: u8,
) -> anyhow::Result<TrackedData<Ethereum>> {
	let fee_history = rpc
		.fee_history(
			U256::one(),
			BlockNumber::Latest,
			Some(vec![priority_fee_percentile as f64 / 100_f64]),
		)
		.await?;

	if let BlockNumber::Number(block_number) = fee_history.oldest_block {
		Ok(TrackedData::<Ethereum> {
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
			TrackedData {
				block_height: BLOCK_HEIGHT,
				base_fee: BASE_FEE,
				priority_fee: PRIORITY_FEE,
			}
		);
	}
}
