// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use ethers::prelude::*;
use sp_core::H160;

use anyhow::{Ok, Result};

use super::{EvmRpcClient, EvmRpcSigningClient};

abigen!(AggregatorV3Interface, "$CF_ETH_CONTRACT_ABI_ROOT/AggregatorV3Interface.json");

// TODO: In the SC / elections check that current time < `updated_at` + `heartbeat` => Staleness
// Check price increase between the SC price and new price to be within a reasonable margin before
// update Check if the price has moved a lot in a period of updates.
// Check that the "Updated At" is monotonically increasing.
// Don't care about RoundId, Started At and `Answered in round``.

#[async_trait::async_trait]
pub trait AggregatorV3InterfaceRpcApi {
	async fn latest_round_data(
		&self,
		aggregator_address: H160,
	) -> Result<(u128, I256, U256, U256, u128)>;

	async fn decimals(&self, aggregator_address: H160) -> Result<u8>;

	async fn description(&self, aggregator_address: H160) -> Result<String>;
}

#[async_trait::async_trait]
impl AggregatorV3InterfaceRpcApi for EvmRpcClient {
	// This call will only get the price of one asset, we can't batch them. If we want to reduce
	// one to a single call we'll need to write a small program that makes all the calls and
	// returns all the data as one. Since we use EVM as a fallback it's probably not necessary.
	async fn latest_round_data(
		&self,
		aggregator_address: H160,
	) -> Result<(u128, I256, U256, U256, u128)> {
		let (round_id, answer, started_at, updated_at, answered_in_round) =
			AggregatorV3Interface::new(aggregator_address, self.provider.clone())
				.latest_round_data()
				.call()
				.await?;

		Ok((round_id, answer, started_at, updated_at, answered_in_round))
	}

	async fn decimals(&self, aggregator_address: H160) -> Result<u8> {
		let aggregator = AggregatorV3Interface::new(aggregator_address, self.provider.clone());
		let decimals = aggregator.decimals().call().await?;
		Ok(decimals)
	}

	async fn description(&self, aggregator_address: H160) -> Result<String> {
		let aggregator = AggregatorV3Interface::new(aggregator_address, self.provider.clone());
		let description = aggregator.description().call().await?;
		Ok(description)
	}
}

#[async_trait::async_trait]
impl AggregatorV3InterfaceRpcApi for EvmRpcSigningClient {
	async fn latest_round_data(
		&self,
		aggregator_address: H160,
	) -> Result<(u128, I256, U256, U256, u128)> {
		self.rpc_client.latest_round_data(aggregator_address).await
	}

	async fn decimals(&self, aggregator_address: H160) -> Result<u8> {
		self.rpc_client.decimals(aggregator_address).await
	}

	async fn description(&self, aggregator_address: H160) -> Result<String> {
		self.rpc_client.description(aggregator_address).await
	}
}

#[cfg(test)]
mod tests {

	use crate::settings::Settings;

	use super::*;
	use std::str::FromStr;

	// ETHERUM MAINNET ADDRESSES
	const BTC_USD_AGGREGATOR_ETHEREUM_ADDRESS: &str = "0xF4030086522a5bEEa4988F8cA5B36dbC97BeE88c"; // heartbeat: 3600s, Deviation 0.5%, Decimals 8
	const ETH_USD_AGGREGATOR_ETHEREUM_ADDRESS: &str = "0x5f4eC3Df9cbd43714FE2740f5E3616155c5b8419"; // heartbeat: 3600s, Deviation 0.5%, Decimals 8

	// ARBITRUM ADDRESSES
	const BTC_USD_AGGREGATOR_ARBITRUM_ADDRESS: &str = "0x6ce185860a4963106506C203335A2910413708e9"; // heartbeat: 86400s, Deviation 0.05%, Decimals 8
	const ETH_USD_AGGREGATOR_ARBITRUM_ADDRESS: &str = "0x639Fe6ab55C921f74e7fac1ee960C0B6293ba612"; // heartbeat: 86400s, Deviation 0.05%, Decimals 8

	const LOCALNET_ETH_PRICE_FEED: &str = "0x322813Fd9A801c5507c9de605d63CEA4f2CE6c44";
	const LOCALNET_ARB_PRICE_FEED: &str = "0xa85233C63b9Ee964Add6F2cffe00Fd84eb32338f";

	fn print_round_data(chain_name: &str, round_data: (u128, I256, U256, U256, u128)) {
		println!(
			"{} - Round ID: {}, Answer: {}, Started At: {}, Updated At: {}, Answered In Round: {}",
			chain_name, round_data.0, round_data.1, round_data.2, round_data.3, round_data.4
		);
	}

	#[tokio::test]
	#[ignore = "requires access to external RPC"]
	async fn eth_oracle_aggregator_test() {
		let settings = Settings::new_test().unwrap();

		let eth_client = EvmRpcSigningClient::new(
			settings.clone().eth.private_key_file,
			"https://mainnet.infura.io/v3/<YOUR_API_KEY>".into(),
			1u64,
			"Ethereum",
		)
		.unwrap()
		.await;

		let arb_client = EvmRpcSigningClient::new(
			settings.eth.private_key_file,
			"https://arbitrum-mainnet.infura.io/v3/<YOUR_API_KEY>".into(),
			42161u64,
			"Arbitrum",
		)
		.unwrap()
		.await;
		print_round_data(
			"Ethereum",
			eth_client
				.latest_round_data(H160::from_str(BTC_USD_AGGREGATOR_ETHEREUM_ADDRESS).unwrap())
				.await
				.unwrap(),
		);
		print_round_data(
			"Arbitrum",
			arb_client
				.latest_round_data(H160::from_str(BTC_USD_AGGREGATOR_ARBITRUM_ADDRESS).unwrap())
				.await
				.unwrap(),
		);
		print_round_data(
			"Ethereum",
			eth_client
				.latest_round_data(H160::from_str(ETH_USD_AGGREGATOR_ETHEREUM_ADDRESS).unwrap())
				.await
				.unwrap(),
		);
		print_round_data(
			"Arbitrum",
			arb_client
				.latest_round_data(H160::from_str(ETH_USD_AGGREGATOR_ARBITRUM_ADDRESS).unwrap())
				.await
				.unwrap(),
		);

		println!(
			"{} - Decimals {}",
			"Ethereum",
			eth_client
				.decimals(H160::from_str(BTC_USD_AGGREGATOR_ETHEREUM_ADDRESS).unwrap())
				.await
				.unwrap()
		);
		println!(
			"{} - Decimals {}",
			"Arbitrum",
			arb_client
				.decimals(H160::from_str(BTC_USD_AGGREGATOR_ARBITRUM_ADDRESS).unwrap())
				.await
				.unwrap()
		);

		println!(
			"{} - Description {}",
			"Ethereum",
			eth_client
				.description(H160::from_str(ETH_USD_AGGREGATOR_ETHEREUM_ADDRESS).unwrap())
				.await
				.unwrap()
		);
		println!(
			"{} - Description {}",
			"Arbitrum",
			arb_client
				.description(H160::from_str(ETH_USD_AGGREGATOR_ARBITRUM_ADDRESS).unwrap())
				.await
				.unwrap()
		);
	}

	#[tokio::test]
	#[ignore = "Requires connection to localnet"]
	async fn eth_oracle_aggregator_localnet() {
		let settings = Settings::new_test().unwrap();

		let eth_client = EvmRpcSigningClient::new(
			settings.clone().eth.private_key_file,
			"http://localhost:8545".into(), // Bouncer localnet
			10997u64,                       // Bouncer localnet chain id
			// "http://127.0.0.1:8545".into(), // Hardhat node ("npx hardhat node")
			// 31337u64,                       // Hardhat node ("npx hardhat node")
			"Ethereum",
		)
		.unwrap()
		.await;

		print_round_data(
			"Ethereum",
			eth_client
				.latest_round_data(H160::from_str(LOCALNET_ETH_PRICE_FEED).unwrap())
				.await
				.unwrap(),
		);

		println!(
			"{} - Description {}",
			"Ethereum",
			eth_client
				.description(H160::from_str(LOCALNET_ETH_PRICE_FEED).unwrap())
				.await
				.unwrap()
		);
	}

	#[tokio::test]
	#[ignore = "Requires connection to localnet"]
	async fn arb_oracle_aggregator_localnet() {
		let settings = Settings::new_test().unwrap();

		let eth_client = EvmRpcSigningClient::new(
			settings.clone().eth.private_key_file,
			"http://localhost:8547".into(),
			412346u64,
			"Arbitrum",
		)
		.unwrap()
		.await;

		print_round_data(
			"Arbitrum",
			eth_client
				.latest_round_data(H160::from_str(LOCALNET_ARB_PRICE_FEED).unwrap())
				.await
				.unwrap(),
		);

		println!(
			"{} - Description {}",
			"Arbitrum",
			eth_client
				.description(H160::from_str(LOCALNET_ARB_PRICE_FEED).unwrap())
				.await
				.unwrap()
		);
	}
}
