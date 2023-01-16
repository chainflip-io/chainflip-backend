use anyhow::{anyhow, Result};

use std::fmt::Debug;
use web3::{
	ethabi::RawLog,
	types::{Log, H256, U256},
};

/// Type for storing common (i.e. tx_hash) and specific event information
#[derive(Debug, PartialEq, Eq)]
pub struct Event<EventParameters: Debug> {
	/// The transaction hash of the transaction that emitted this event
	pub tx_hash: H256,
	/// The index number of this particular log, in the list of logs emitted by the tx_hash
	pub log_index: U256,
	/// The event specific parameters
	pub event_parameters: EventParameters,
}

impl<EventParameters: Debug> std::fmt::Display for Event<EventParameters> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "EventParameters: {:?}; tx_hash: {:#x}", self.event_parameters, self.tx_hash)
	}
}

impl<EventParameters: Debug> Event<EventParameters> {
	pub fn new_from_unparsed_logs<LogDecoder>(decode_log: &LogDecoder, log: Log) -> Result<Self>
	where
		LogDecoder: Fn(H256, RawLog) -> Result<EventParameters>,
	{
		Ok(Self {
			tx_hash: log
				.transaction_hash
				.ok_or_else(|| anyhow!("Could not get transaction hash from ETH log"))?,
			log_index: log
				.log_index
				.ok_or_else(|| anyhow!("Could not get log index from ETH log"))?,
			event_parameters: decode_log(
				*log.topics
					.first()
					.ok_or_else(|| anyhow!("Could not get event signature from ETH log"))?,
				RawLog { topics: log.topics, data: log.data.0 },
			)?,
		})
	}
}

#[cfg(test)]
mod tests {

	use std::str::FromStr;

	use web3::types::H160;

	use crate::eth::{key_manager::KeyManager, EthContractWitnesser};

	use super::*;

	#[tokio::test]
	async fn common_event_info_decoded_correctly() {
		let key_manager = KeyManager::new(H160::default());

		let transaction_hash =
			H256::from_str("0x621aebbe0bb116ae98d36a195ad8df4c5e7c8785fae5823f5f1fe1b691e91bf2")
				.unwrap();

		let event = Event::new_from_unparsed_logs(
            &key_manager.decode_log_closure().unwrap(),
             web3::types::Log {
                address: H160::zero(),
                topics: vec![H256::from_str("0x5cba64f32f2576e404f74394dc04611cce7416e299c94db0667d4e315e852521")
                .unwrap()],
                data: web3::types::Bytes(hex::decode("31b2ba4b46201610901c5164f42edd1f64ce88076fde2e2c544f9dc3d7b350ae00000000000000000000000000000000000000000000000000000000000000011742daacd4dbfbe66d4c8965550295873c683cb3b65019d3a53975ba553cc31d0000000000000000000000000000000000000000000000000000000000000001").unwrap()),
                block_hash: None,
                block_number: None,
                transaction_hash: Some(transaction_hash),
                transaction_index: None,
                log_index: Some(U256::from(0)),
                transaction_log_index: None,
                log_type: None,
                removed: None,
            },
        ).unwrap();

		assert_eq!(event.tx_hash, transaction_hash);
	}
}
