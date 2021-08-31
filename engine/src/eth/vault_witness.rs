//! Contains the information required to use the Vault contract as a source for
//! the EthEventStreamer

use std::sync::{Arc, Mutex};

use crate::{
    eth::{eth_event_streamer, utils, EventParseError, SignatureAndEvent},
    logging::COMPONENT_KEY,
    settings,
    state_chain::runtime::StateChainRuntime,
};

use substrate_subxt::{Client, PairSigner};

use web3::{
    ethabi::{self, RawLog},
    transports::WebSocket,
    types::{H160, H256},
    Web3,
};

use anyhow::Result;
use futures::{Future, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use slog::o;

/// Set up the eth event streamer for the vault contract, and start it
pub async fn start_vault_witness(
    web3: &Web3<WebSocket>,
    settings: &settings::Settings,
    _signer: Arc<Mutex<PairSigner<StateChainRuntime, sp_core::sr25519::Pair>>>,
    _subxt_client: Client<StateChainRuntime>,
    logger: &slog::Logger,
) -> Result<impl Future> {
    let logger = logger.new(o!(COMPONENT_KEY => "VaultWitness"));
    slog::info!(logger, "Starting Vault witness");

    slog::info!(logger, "Load Contract ABI");
    let vault_witness = VaultWitness::new(&settings)?;

    slog::info!(logger, "Creating Event Stream");
    let mut event_stream = vault_witness
        .event_stream(web3, settings.eth.from_block, &logger)
        .await?;

    Ok(async move {
        while let Some(result_event) = event_stream.next().await {
            match result_event.unwrap() {
                // TODO: Handle unwraps
                VaultEvent::TransferFailed { .. } => {
                    todo!();
                }
            }
        }
    })
}
#[derive(Clone)]
/// A wrapper for the Vault Ethereum contract.
pub struct VaultWitness {
    pub deployed_address: H160,
    contract: ethabi::Contract,
}

/// Represents the events that are expected from the Vault contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultEvent {
    /// The `Staked(nodeId, amount)` event.
    TransferFailed {
        /// The address of the recipient of the transfer
        recipient: ethabi::Address,
        /// The amount to transfer, in wei (uint)
        amount: u128,
        /// The data returned by the error
        low_level_data: ethabi::Bytes,
        /// Transaction hash that created the event
        tx_hash: [u8; 32],
    },
}

impl VaultWitness {
    /// Loads the contract abi to get event definitions
    pub fn new(settings: &settings::Settings) -> Result<Self> {
        let contract = ethabi::Contract::load(std::include_bytes!("abis/Vault.json").as_ref())?;
        Ok(Self {
            deployed_address: settings.eth.vault_contract_eth_address,
            contract,
        })
    }

    // TODO: Maybe try to factor this out (See StakeManager)
    pub async fn event_stream(
        &self,
        web3: &Web3<WebSocket>,
        from_block: u64,
        logger: &slog::Logger,
    ) -> Result<impl Stream<Item = Result<VaultEvent>>> {
        eth_event_streamer::new_eth_event_stream(
            web3,
            self.deployed_address,
            self.decode_log_closure()?,
            from_block,
            logger,
        )
        .await
    }

    pub fn decode_log_closure(&self) -> Result<impl Fn(H256, H256, RawLog) -> Result<VaultEvent>> {
        let transfer_failed = SignatureAndEvent::new(&self.contract, "TransferFailed")?;

        Ok(
            move |signature: H256, tx_hash: H256, raw_log: RawLog| -> Result<VaultEvent> {
                let tx_hash = tx_hash.to_fixed_bytes();
                if signature == transfer_failed.signature {
                    let log = transfer_failed.event.parse_log(raw_log)?;
                    let event = VaultEvent::TransferFailed {
                        recipient: utils::decode_log_param(&log, "recipient")?,
                        amount: utils::decode_log_param::<ethabi::Uint>(&log, "amount")?.as_u128(),
                        low_level_data: utils::decode_log_param(&log, "lowLevelData")?,
                        tx_hash,
                    };
                    Ok(event)
                } else {
                    Err(anyhow::Error::from(EventParseError::UnexpectedEvent(
                        signature,
                    )))
                }
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use web3::types::H256;

    use super::*;

    #[test]
    fn test_key_change_parsing() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let vault_witness = VaultWitness::new(&settings).unwrap();

        let decode_log = vault_witness.decode_log_closure().unwrap();

        let transfer_failed_event_signature =
            H256::from_str("0x6a67a47f2d3e3318710790c8238d45019beef95e77203aa85cf756eec2dc539e")
                .unwrap();

        let transaction_hash =
            H256::from_str("0x7e0ea5c0fa52294b74c12d8da0299476cffeaa82fff71a8d366dd2aa383a94a1")
                .unwrap();

        match decode_log(
            transfer_failed_event_signature,
            transaction_hash,
            RawLog {
                topics : vec![transfer_failed_event_signature],
                data : hex::decode("0000000000000000000000009e4c14403d7d9a8a782044e86a93cae09d7b2ac9000000000000000000000000000000000000000000000000016345785d8a000000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000000").unwrap()
            }
        ).expect("Failed parsing AGG_SET_AGG_LOG event") {
            VaultEvent::TransferFailed {
                recipient,
                amount,
                low_level_data,
                tx_hash,
            } => {
                assert_eq!(
                    recipient,
                    H160::from_str("0x9e4c14403d7d9a8a782044e86a93cae09d7b2ac9").unwrap()
                );
                assert_eq!(amount, 100000000000000000);
                assert!(low_level_data.is_empty());
                assert_eq!(
                    tx_hash,
                    H256::from_str(
                        "0x7e0ea5c0fa52294b74c12d8da0299476cffeaa82fff71a8d366dd2aa383a94a1",
                    )
                    .unwrap()
                    .to_fixed_bytes()
                );
            }
        }
    }
}
