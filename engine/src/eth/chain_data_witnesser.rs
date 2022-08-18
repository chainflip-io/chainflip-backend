use std::{sync::Arc, time::Duration};

use crate::state_chain_observer::client::SubmitSignedExtrinsic;
use web3::types::Address;

use super::{rpc::EthRpcApi, EpochStart};

use anyhow::Context;
use cf_chains::{eth::TrackedData, Ethereum};

use sp_core::U256;
use state_chain_runtime::CfeSettings;
use tokio::sync::{broadcast, watch};
use utilities::{context, make_periodic_tick};
use web3::types::{BlockNumber, U64};

pub const ETH_CHAIN_TRACKING_POLL_INTERVAL: Duration = Duration::from_secs(4);

pub struct TransactionParticipants {
    pub from: Address,
    pub to: Address,
}

pub trait TransactionParticipantProvider {
    fn get_transaction_participants(&self) -> Vec<TransactionParticipants>;
}

pub struct NoTransactionParticipants {}

impl TransactionParticipantProvider for NoTransactionParticipants {
    fn get_transaction_participants(&self) -> Vec<TransactionParticipants> {
        Vec::default()
    }
}

pub async fn start<StateChainClient, EthRpcClient, TxParticipantProvider>(
    eth_rpc: EthRpcClient,
    state_chain_client: Arc<StateChainClient>,
    epoch_start_receiver: broadcast::Receiver<EpochStart>,
    cfe_settings_update_receiver: watch::Receiver<CfeSettings>,
    tx_participant_provider: Arc<TxParticipantProvider>,
    poll_interval: Duration,
    logger: &slog::Logger,
) -> anyhow::Result<()>
where
    StateChainClient: 'static + SubmitSignedExtrinsic + Sync + Send,
    EthRpcClient: 'static + EthRpcApi + Clone + Send + Sync,
    TxParticipantProvider: 'static + TransactionParticipantProvider + Send + Sync,
{
    super::epoch_witnesser::start(
        "ETH-Chain-Data",
        epoch_start_receiver,
        |epoch_start| epoch_start.current,
        None,
        move |end_witnessing_signal, _epoch_start, mut last_witnessed_block_hash, logger| {
            let eth_rpc = eth_rpc.clone();
            let cfe_settings_update_receiver = cfe_settings_update_receiver.clone();

            let state_chain_client = state_chain_client.clone();
            let tx_participant_provider = tx_participant_provider.clone();

            async move {
                let mut poll_interval = make_periodic_tick(poll_interval, false);

                loop {
                    if let Some(_end_block) = *end_witnessing_signal.lock().unwrap() {
                        break;
                    }

                    let block_number = eth_rpc.block_number().await?;
                    let block = eth_rpc.block(block_number).await?;
                    let block_hash = block.hash
                        .context(format!("Missing hash for block {}.", block_number))?;
                    if last_witnessed_block_hash == Some(block_hash) {
                        continue;
                    }

                    let priority_fee = cfe_settings_update_receiver
                        .borrow()
                        .eth_priority_fee_percentile;
                    match get_tracked_data(&eth_rpc, block_number.as_u64(), priority_fee).await
                    {
                        Ok(tracked_data) => {
                            state_chain_client
                                .submit_signed_extrinsic(
                                    state_chain_runtime::Call::Witnesser(pallet_cf_witnesser::Call::witness {
                                        call: Box::new(state_chain_runtime::Call::EthereumChainTracking(
                                            pallet_cf_chain_tracking::Call::update_chain_state {
                                                state: tracked_data,
                                            },
                                        )),
                                    }),
                                    &logger,
                                )
                                .await
                                .context("Failed to submit signed extrinsic")?;
                            last_witnessed_block_hash = Some(block_hash);
                        }
                        Err(e) => {
                            slog::error!(&logger, "Failed to get tracked data: {:?}", e);
                        }
                    }

                    // Observe transactions
                    for tx_hash in &block.transactions {
                        let tx = eth_rpc.transaction_receipt(*tx_hash).await?;
                        if let Some(tx_to) = tx.to {
                            tx_participant_provider.get_transaction_participants().iter().for_each(|tx_participants|{
                                if tx.from == tx_participants.from && tx_to == tx_participants.to {
                                    slog::info!(&logger, "Observed transaction from {:?} to {:?}", tx_participants.from, tx_participants.to);
                                }
                            });
                        }
                    }

                    poll_interval.tick().await;
                }

                Ok(last_witnessed_block_hash)
            }
        },
        logger,
    )
    .await
}

/// Queries the rpc node and builds the `TrackedData` for Ethereum at the requested block number.
///
/// Value in Wei is rounded to nearest Gwei in an effort to ensure agreement between nodes in the presence of floating
/// point / rounding error. This approach is still vulnerable when the true value is near the rounding boundary.
///
/// See: https://github.com/chainflip-io/chainflip-backend/issues/1803
async fn get_tracked_data<EthRpcClient: EthRpcApi + Send + Sync>(
    rpc: &EthRpcClient,
    block_number: u64,
    priority_fee_percentile: u8,
) -> anyhow::Result<TrackedData<Ethereum>> {
    let fee_history = rpc
        .fee_history(
            U256::one(),
            BlockNumber::Number(U64::from(block_number)),
            Some(vec![priority_fee_percentile as f64 / 100_f64]),
        )
        .await?;

    Ok(TrackedData::<Ethereum> {
        block_height: block_number,
        base_fee: context!(fee_history.base_fee_per_gas.first())?.as_u128(),
        priority_fee: context!(context!(context!(fee_history.reward)?.first())?.first())?.as_u128(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_tracked_data() {
        use crate::eth::rpc::MockEthRpcApi;

        const BLOCK_HEIGHT: u64 = 42;
        const BASE_FEE: u128 = 40_000_000_000;
        const PRIORITY_FEE: u128 = 5_000_000_000;

        let mut rpc = MockEthRpcApi::new();

        // ** Rpc Api Assumptions **
        rpc.expect_fee_history()
            .once()
            .returning(|_, block_number, _| {
                Ok(web3::types::FeeHistory {
                    oldest_block: block_number,
                    base_fee_per_gas: vec![U256::from(BASE_FEE)],
                    gas_used_ratio: vec![],
                    reward: Some(vec![vec![U256::from(PRIORITY_FEE)]]),
                })
            });
        // ** Rpc Api Assumptions **

        assert_eq!(
            get_tracked_data(&rpc, BLOCK_HEIGHT, 50).await.unwrap(),
            TrackedData {
                block_height: BLOCK_HEIGHT,
                base_fee: BASE_FEE,
                priority_fee: PRIORITY_FEE,
            }
        );
    }
}
