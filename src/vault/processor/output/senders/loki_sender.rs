use super::OutputSender;
use crate::{
    common::{LokiAmount, LokiWalletAddress},
    vault::blockchain_connection::loki_rpc::{self, TransferResponse},
    vault::config::LokiRpcConfig,
};
use chainflip_common::types::{
    chain::{Output, OutputSent, Validate},
    coin::Coin,
    Network, Timestamp, UUIDv4,
};
use std::str::FromStr;

/// Struct used for making Loki transactions
pub struct LokiSender {
    /// Loki rpc wallet configuration
    config: LokiRpcConfig,
    /// The network type of the sender
    network: Network,
}

async fn send_with_estimated_fee(
    port: u16,
    amount: &LokiAmount,
    address: &LokiWalletAddress,
    output_id: UUIDv4,
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
    pub fn new(config: LokiRpcConfig, network: Network) -> Self {
        Self { config, network }
    }

    async fn send_inner(&self, outputs: &[Output]) -> Vec<OutputSent> {
        let port: u16 = self.config.port;

        let mut sent_outputs = vec![];

        for output in outputs {
            assert_eq!(output.coin, Coin::LOKI);

            let amount = LokiAmount::from_atomic(output.amount);

            info!(
                "Sending Loki: {} (incuding fee) for output id: {}]",
                amount, output.id
            );

            let loki_address = match LokiWalletAddress::from_str(&output.address.to_string()) {
                Ok(addr) => addr,
                Err(err) => {
                    // TODO: we should probably mark the output as invalid so we don't
                    // keep trying to process it on every block
                    error!("Skipping invalid LOKI output: {}", err);
                    continue;
                }
            };

            match send_with_estimated_fee(port, &amount, &loki_address, output.id).await {
                Ok(res) => {
                    dbg!(&res);
                    let total_spent = res.amount.saturating_add(&res.fee);
                    debug!(
                        "Total Loki spent: {} for output id: {}",
                        total_spent, output.id
                    );

                    let sent = OutputSent {
                        id: UUIDv4::new(),
                        timestamp: Timestamp::now(),
                        outputs: vec![output.id],
                        coin: Coin::LOKI,
                        address: output.address.clone(),
                        amount: amount.to_atomic(),
                        fee: res.fee.to_atomic(),
                        transaction_id: (&res.tx_hash).into(),
                    };

                    if let Err(err) = sent.validate(self.network) {
                        panic!(
                            "Failed to create output tx for {:?} with hash {}: {}",
                            output.id, res.tx_hash, err
                        );
                    }

                    sent_outputs.push(sent);
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
    async fn send(&self, outputs: &[Output]) -> Vec<OutputSent> {
        self.send_inner(outputs).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_utils;
    use test_utils::data::TestData;

    #[tokio::test]
    #[ignore = "Custom environment setup required"]
    async fn it_sends() {
        test_utils::logging::init();

        // This test requires loki rpc wallet to run under the following port
        let config = LokiRpcConfig { port: 6935 };

        let loki = LokiSender::new(config, Network::Testnet);

        let output = TestData::output(
            Coin::LOKI,
            LokiAmount::from_decimal_string("1.25").to_atomic(),
        );

        let txs = loki.send_inner(&[output]).await;

        assert_eq!(txs.len(), 1);
    }
}
