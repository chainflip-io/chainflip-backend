use std::unimplemented;

use futures::{Stream, StreamExt};
use web3::{
    api::EthSubscribe,
    types::{BlockNumber, FilterBuilder, Log, H160},
};

struct StakingEventStreamer {
    client: ::web3::Web3<::web3::transports::WebSocket>,
}

pub enum StreamingError {
    Web3Error(::web3::error::Error),
}

impl From<::web3::error::Error> for StreamingError {
    fn from(e: ::web3::error::Error) -> Self {
        StreamingError::Web3Error(e)
    }
}

type Result<R> = std::result::Result<R, StreamingError>;

impl StakingEventStreamer {
    pub async fn new(url: &str) -> Result<Self> {
        let transport = ::web3::transports::WebSocket::new(url).await?;
        Ok(Self {
            client: ::web3::Web3::new(transport),
        })
    }

    /// Create a stream of Ethereum log events.
    pub async fn run(&self, addresses: Vec<H160>, block_height: BlockNumber) -> Result<()> {
        let filter = FilterBuilder::default()
            .address(addresses)
            .from_block(block_height)
            .build();

        let event_stream = self.client.eth_subscribe().subscribe_logs(filter).await?;

        event_stream.for_each(|log| async {
            match log {
                Ok(log) => {
                    self.process_log(log).await;
                }
                Err(e) => log::error!("Unable to parse Eth log event: {:?}", e),
            }
        });

        Ok(())
    }

    async fn process_log(&self, log: Log) {
        // Determine Event type and dispatch to Substrate.
        unimplemented!()
    }
}
