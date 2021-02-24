use std::str::FromStr;

use super::OutputSender;
use crate::{
    common::OxenAmount,
    vault::blockchain_connection::oxen_rpc::{self, TransferResponse},
    vault::config::OxenRpcConfig,
};
use chainflip_common::types::{
    addresses::OxenAddress,
    chain::{Output, OutputSent, UniqueId, Validate},
    coin::Coin,
    unique_id::GetUniqueId,
    Network,
};

/// Struct used for making Oxen transactions
pub struct OxenSender {
    /// Oxen rpc wallet configuration
    config: OxenRpcConfig,
    /// The network type of the sender
    network: Network,
}

async fn send_with_estimated_fee(
    port: u16,
    amount: &OxenAmount,
    address: &OxenAddress,
    output_id: UniqueId,
) -> Result<TransferResponse, String> {
    let res = oxen_rpc::check_transfer_fee(port, &amount, &address).await?;

    debug!("Estimated fee: {} (output id: {})", res.fee, output_id);

    let amount = amount.checked_sub(&res.fee).ok_or("Fee exceeds amount")?;

    debug!(
        "Sending amount without fee: {} (output id: {})",
        amount, output_id
    );

    oxen_rpc::transfer(port, &amount, &address).await
}

impl OxenSender {
    /// Create instance from config
    pub fn new(config: OxenRpcConfig, network: Network) -> Self {
        Self { config, network }
    }

    async fn send_inner(&self, outputs: &[Output]) -> Vec<OutputSent> {
        let port: u16 = self.config.port;

        let mut sent_outputs = vec![];

        for output in outputs {
            assert_eq!(output.coin, Coin::OXEN);

            let amount = OxenAmount::from_atomic(output.amount);

            info!(
                "Sending Oxen: {} (incuding fee) for output id: {}]",
                amount,
                output.unique_id()
            );

            let oxen_address = match OxenAddress::from_str(&output.address.to_string()) {
                Ok(addr) => addr,
                Err(err) => {
                    // TODO: we should probably mark the output as invalid so we don't
                    // keep trying to process it on every block
                    error!("Skipping invalid OXEN output: {}", err);
                    continue;
                }
            };

            match send_with_estimated_fee(port, &amount, &oxen_address, output.unique_id()).await {
                Ok(res) => {
                    dbg!(&res);
                    let total_spent = res.amount.saturating_add(&res.fee);
                    debug!(
                        "Total Oxen spent: {} for output id: {}",
                        total_spent,
                        output.unique_id()
                    );

                    let sent = OutputSent {
                        outputs: vec![output.unique_id()],
                        coin: Coin::OXEN,
                        address: output.address.clone(),
                        amount: amount.to_atomic(),
                        fee: res.fee.to_atomic(),
                        transaction_id: (&res.tx_hash).into(),
                        event_number: None,
                    };

                    if let Err(err) = sent.validate(self.network) {
                        panic!(
                            "Failed to create output tx for {:?} with hash {}: {}",
                            output.unique_id(),
                            res.tx_hash,
                            err
                        );
                    }

                    sent_outputs.push(sent);
                }
                Err(err) => {
                    error!(
                        "Failed to send (output id: {}): {}",
                        output.unique_id(),
                        err
                    );
                }
            }
        }

        sent_outputs
    }
}

#[async_trait]
impl OutputSender for OxenSender {
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

        // This test requires oxen rpc wallet to run under the following port
        let config = OxenRpcConfig { port: 6935 };

        let oxen = OxenSender::new(config, Network::Testnet);

        let output = TestData::output(
            Coin::OXEN,
            OxenAmount::from_decimal_string("1.25").to_atomic(),
        );

        let txs = oxen.send_inner(&[output]).await;

        assert_eq!(txs.len(), 1);
    }
}
