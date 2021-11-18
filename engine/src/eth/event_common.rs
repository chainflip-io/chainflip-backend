use anyhow::Result;

use std::fmt::Debug;
use web3::{
    ethabi::RawLog,
    types::{Log, H256},
};

/// Type for storing common (i.e. tx_hash) and specific event information
#[derive(Debug)]
pub struct EventWithCommon<EventParameters: Debug> {
    /// The transaction hash of the transaction that emitted this event
    pub tx_hash: [u8; 32],
    /// The block number at which the event occurred
    pub block_number: u64,
    /// The event specific parameters
    pub event_parameters: EventParameters,
}

impl<EventParameters: Debug> std::fmt::Display for EventWithCommon<EventParameters> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MultisigResult: {:?}; block_number: {}; tx_hash: 0x{}",
            self.event_parameters,
            self.block_number,
            hex::encode(self.tx_hash)
        )
    }
}

impl<EventParameters: Debug> EventWithCommon<EventParameters> {
    pub fn decode<LogDecoder: Fn(H256, RawLog) -> Result<EventParameters>>(
        decode_log: &LogDecoder,
        log: Log,
    ) -> Result<Self> {
        Ok(Self {
            tx_hash: log
                .transaction_hash
                .ok_or_else(|| anyhow::Error::msg("Could not get transaction hash from ETH log"))?
                .to_fixed_bytes(),
            block_number: log
                .block_number
                .expect("Should have a block number")
                .as_u64(),
            event_parameters: decode_log(
                *log.topics.first().ok_or_else(|| {
                    anyhow::Error::msg("Could not get event signature from ETH log")
                })?,
                RawLog {
                    topics: log.topics,
                    data: log.data.0,
                },
            )?,
        })
    }
}
