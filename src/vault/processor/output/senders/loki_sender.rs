use std::convert::TryInto;

use uuid::Uuid;

use crate::{
    common::{Coin, LokiAmount, LokiWalletAddress, Timestamp},
    transactions::{OutputSentTx, OutputTx},
    vault::blockchain_connection::loki_rpc::{self, TransferResponse},
    vault::config::LokiRpcConfig,
};

use super::OutputSender;

/// Struct used for making Loki transactions
pub struct LokiSender {
    /// Loki rpc wallet configuration
    config: LokiRpcConfig,
}

async fn send_with_estimated_fee(
    port: u16,
    amount: &LokiAmount,
    address: &LokiWalletAddress,
    output_id: Uuid,
) -> Result<TransferResponse, String> {
    let res = loki_rpc::check_transfer_fee(port, &amount, &address).await?;

    debug!("Estimated fee: {} (output id: {})", res.fee, output_id);

    let amount = amount.checked_sub(&res.fee).ok_or("Fee exceeds amount")?;

    debug!(
        "Sending amount without fee: {} (output id: {})",
        amount, output_id
    );

    loki_rpc::transfer(port, &amount, &address).await
}

impl LokiSender {
    /// Create instance from config
    pub fn new(config: LokiRpcConfig) -> Self {
        Self { config }
    }

    async fn send_inner(&self, outputs: &[OutputTx]) -> Vec<OutputSentTx> {
        let port: u16 = self.config.port;

        let mut sent_outputs = vec![];

        for output in outputs {
            assert_eq!(output.coin, Coin::LOKI);

            let amount = LokiAmount::from_atomic(output.amount);

            info!(
                "Sending Loki: {} (incuding fee) for output id: {}]",
                amount, output.id
            );

            let address: LokiWalletAddress = match output.address.clone().try_into() {
                Ok(addr) => addr,
                Err(err) => {
                    // TODO: we should probably mark the output as invalid so we don't
                    // keep trying to process it on every block
                    error!("Skipping invalid LOKI output: {}", err);
                    continue;
                }
            };

            match send_with_estimated_fee(port, &amount, &address, output.id).await {
                Ok(res) => {
                    dbg!(&res);
                    let total_spent = res.amount.saturating_add(&res.fee);
                    debug!(
                        "Total Loki spent: {} for output id: {}",
                        total_spent, output.id
                    );

                    let tx = match OutputSentTx::new(
                        Timestamp::now(),
                        vec![output.id],
                        Coin::LOKI,
                        address.into(),
                        amount.to_atomic(),
                        res.fee.to_atomic(),
                        res.tx_hash.clone(),
                    ) {
                        Ok(tx) => tx,
                        Err(err) => panic!(
                            "Failed to create output sent tx for output {} with hash {}: {}",
                            output.id, res.tx_hash, err
                        ),
                    };

                    sent_outputs.push(tx);
                }
                Err(err) => {
                    error!("Failed to send (output id: {}): {}", output.id, err);
                }
            }
        }

        sent_outputs
    }
}

#[async_trait]
impl OutputSender for LokiSender {
    async fn send(&self, outputs: &[OutputTx]) -> Vec<OutputSentTx> {
        self.send_inner(outputs).await
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        common::Coin,
        common::WalletAddress,
        utils::test_utils::{self, create_fake_output_tx},
    };

    use super::*;

    #[tokio::test]
    #[ignore = "Custom environment setup required"]
    async fn it_sends() {
        test_utils::logging::init();

        // This test requires loki rpc wallet to run under the following port
        let config = LokiRpcConfig { port: 6935 };

        let loki = LokiSender::new(config);

        let mut output = create_fake_output_tx(Coin::LOKI);

        output.address = WalletAddress::new("T6T6otxMejTKavFEQP66VufY9y8vr2Z6RMzoQ95BZx7KWy6zCngrfh39dUVtrF3crtLRFdXpmgjjH7658C74NoJ91imYo7zMk");
        output.amount = LokiAmount::from_decimal_string("1.25").to_atomic();

        let txs = loki.send_inner(&[output]).await;

        assert_eq!(txs.len(), 1);
    }
}
