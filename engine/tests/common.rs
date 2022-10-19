#![cfg(feature = "integration-test")]

use chainflip_engine::{
	eth::{
		block_events_stream_for_contract_from, event::Event, rpc::EthDualRpcClient,
		EthContractWitnesser,
	},
	settings::{CfSettings, CommandLineOptions, Settings},
};
use config::{Config, ConfigError, File};
use futures::stream::StreamExt;
use serde::Deserialize;
use sp_core::H160;

#[derive(Debug, Deserialize, Clone)]
pub struct IntegrationTestConfig {
	pub eth: Eth,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Eth {
	pub key_manager_address: H160,
	pub stake_manager_address: H160,
}

impl IntegrationTestConfig {
	/// Load integration test settings from a TOML file
	pub fn from_file(file: &str) -> Result<Self, ConfigError> {
		let s = Config::builder().add_source(File::with_name(file)).build()?.try_deserialize()?;

		Ok(s)
	}
}

pub async fn get_contract_events<ContractWitnesser>(
	contract_witnesser: ContractWitnesser,
	logger: slog::Logger,
) -> Vec<Event<<ContractWitnesser as EthContractWitnesser>::EventParameters>>
where
	ContractWitnesser: EthContractWitnesser + std::marker::Sync,
{
	let eth_dual_rpc = EthDualRpcClient::new_test(
		&<Settings as CfSettings>::load_settings_from_all_sources(
			"config/Testing.toml",
			None,
			CommandLineOptions::default(),
		)
		.unwrap()
		.eth,
		&logger,
	)
	.await
	.expect("Could not create EthDualRpcClient");

	// The stream is infinite unless we stop it after a short time
	// in which it should have already done it's job.
	let events = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        block_events_stream_for_contract_from(
            0,
            &contract_witnesser,
            eth_dual_rpc.clone(),
            &logger,
        ),
    )
    .await
    .expect("Timeout getting events. You might need to run hardhat with --config hardhat-interval-mining.config.js")
    .unwrap()
    .map(|block| futures::stream::iter(block.block_items))
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
