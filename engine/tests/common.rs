#![cfg(feature = "integration-test")]

use anyhow::Result;
use std::{fmt::Debug, pin::Pin};
use tracing::info;

use chainflip_engine::{
	eth::{
		contract_witnesser::block_to_events,
		core_h160,
		event::Event,
		rpc::{EthHttpRpcClient, EthWsRpcClient},
		safe_block_subscription_from, BlockWithProcessedItems, EthContractWitnesser,
	},
	settings::{CfSettings, CommandLineOptions, Settings},
};
use config::{Config, ConfigError, File};
use futures::{stream::StreamExt, Stream};
use serde::Deserialize;

use web3::types::H160;

#[derive(Debug, Deserialize, Clone)]
pub struct IntegrationTestConfig {
	pub eth: Eth,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Eth {
	pub key_manager_address: H160,
	pub state_chain_gateway_address: H160,
	pub vault_address: H160,
}

impl IntegrationTestConfig {
	/// Load integration test settings from a TOML file
	pub fn from_file(file: &str) -> Result<Self, ConfigError> {
		let s = Config::builder().add_source(File::with_name(file)).build()?.try_deserialize()?;

		Ok(s)
	}
}

/// Get a block events stream for the contract, returning the stream only if the head of the stream
/// is ahead of from_block (otherwise it will wait until we have reached from_block).
async fn block_events_stream_for_contract_from<EventParameters, ContractWitnesser>(
	from_block: u64,
	contract_witnesser: ContractWitnesser,
	eth_http_rpc: EthHttpRpcClient,
	eth_ws_rpc: EthWsRpcClient,
) -> Result<
	Pin<Box<dyn Stream<Item = BlockWithProcessedItems<Event<EventParameters>>> + Send + 'static>>,
>
where
	EventParameters: Debug + Send + Sync + 'static,
	ContractWitnesser:
		EthContractWitnesser<EventParameters = EventParameters> + Send + Sync + 'static,
{
	let contract_address = contract_witnesser.contract_address();
	info!(
		"Subscribing to ETH events from contract at address: {:?}",
		hex::encode(contract_address)
	);

	let safe_header_stream =
		safe_block_subscription_from(from_block, eth_ws_rpc.clone(), eth_http_rpc.clone()).await?;

	Ok(Box::pin(safe_header_stream.then({
		move |block| {
			let rpc = eth_http_rpc.clone();
			let decode_log_closure = contract_witnesser.decode_log_closure().unwrap();
			async move {
				let processed_block_items =
					block_to_events(&block, core_h160(contract_address), &decode_log_closure, &rpc)
						.await;

				BlockWithProcessedItems {
					block_number: block.block_number.as_u64(),
					processed_block_items,
				}
			}
		}
	})))
}

pub async fn get_contract_events<ContractWitnesser>(
	contract_witnesser: ContractWitnesser,
) -> Vec<Event<<ContractWitnesser as EthContractWitnesser>::EventParameters>>
where
	ContractWitnesser: EthContractWitnesser + std::marker::Sync + Send + 'static,
{
	let settings = <Settings as CfSettings>::load_settings_from_all_sources(
		"config/testing/".to_owned(),
		CommandLineOptions::default(),
	)
	.unwrap()
	.eth;

	let eth_http_rpc = EthHttpRpcClient::new(&settings, None)
		.await
		.expect("Failed to create EthWsRpcClient");

	let eth_ws_rpc = EthWsRpcClient::new(&settings, None)
		.await
		.expect("Failed to create EthWsRpcClient");

	// The stream is infinite unless we stop it after a short time
	// in which it should have already done it's job.
	let events = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        block_events_stream_for_contract_from(
            0,
            contract_witnesser,
            eth_http_rpc.clone(),
            eth_ws_rpc.clone(),
        ),
    )
    .await
    .expect("Timeout getting events. You might need to run hardhat with --config hardhat-interval-mining.config.js")
    .unwrap()
    .map(|block| futures::stream::iter(block.processed_block_items.expect("should have fetched events")))
    .flatten()
    .take_until(tokio::time::sleep(std::time::Duration::from_millis(1000)))
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Vec<_>>();

	assert!(
		!events.is_empty(),
		"{}",
		r#"
    Event stream was empty.
    - Have you run the setup script to deploy/run the contracts? (tests/scripts/setup.sh)
    - Are you pointing to the correct contract address? (tests/config.toml)
    - The deploy script was ran at too large of a block height for this test to reach before the timeout (this test starts at block 0).
    "#
	);

	events
}
