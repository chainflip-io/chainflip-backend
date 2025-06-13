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

use anyhow::{Ok, Result};

use super::{EvmRpcClient, EvmRpcSigningClient};

abigen!(AddressChecker, "$CF_ETH_CONTRACT_ABI_ROOT/$CF_ETH_CONTRACT_ABI_TAG/IAddressChecker.json");

#[async_trait::async_trait]
pub trait AddressCheckerRpcApi {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>>;

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<U256>>;

	async fn query_price_feeds(
		&self,
		contract_address: H160,
		aggregator_addresses: Vec<H160>,
	) -> Result<(U256, U256, Vec<PriceFeedData>)>;
}

#[async_trait::async_trait]
impl AddressCheckerRpcApi for EvmRpcClient {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>> {
		Ok(AddressChecker::new(contract_address, self.provider.clone())
			.address_states(addresses)
			.block(BlockId::Hash(block_hash))
			.call()
			.await?)
	}

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<U256>> {
		Ok(AddressChecker::new(contract_address, self.provider.clone())
			.native_balances(addresses)
			.block(BlockId::Hash(block_hash))
			.call()
			.await?)
	}
	async fn query_price_feeds(
		&self,
		contract_address: H160,
		aggregator_addresses: Vec<H160>,
	) -> Result<(U256, U256, Vec<PriceFeedData>)> {
		let price_feed_data = AddressChecker::new(contract_address, self.provider.clone())
			.query_price_feeds(aggregator_addresses)
			.call()
			.await?;
		Ok(price_feed_data)
	}
}

#[async_trait::async_trait]
impl AddressCheckerRpcApi for EvmRpcSigningClient {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>> {
		self.rpc_client.address_states(block_hash, contract_address, addresses).await
	}

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Result<Vec<U256>> {
		self.rpc_client.balances(block_hash, contract_address, addresses).await
	}

	async fn query_price_feeds(
		&self,
		contract_address: H160,
		aggregator_addresses: Vec<H160>,
	) -> Result<(U256, U256, Vec<PriceFeedData>)> {
		self.rpc_client.query_price_feeds(contract_address, aggregator_addresses).await
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
	const ADDRESS_CHECKER_ETHEREUM_ADDRESS: &str = "0x0000000000000000000000000000000000000000"; // TODO: To add

	// ARBITRUM ADDRESSES
	const BTC_USD_AGGREGATOR_ARBITRUM_ADDRESS: &str = "0x6ce185860a4963106506C203335A2910413708e9"; // heartbeat: 86400s, Deviation 0.05%, Decimals 8
	const ETH_USD_AGGREGATOR_ARBITRUM_ADDRESS: &str = "0x639Fe6ab55C921f74e7fac1ee960C0B6293ba612"; // heartbeat: 86400s, Deviation 0.05%, Decimals 8
	const ADDRESS_CHECKER_ARBITRUM_ADDRESS: &str = "0x0000000000000000000000000000000000000000"; // TODO: To add

	const LOCALNET_ETH_PRICE_FEED_BTC: &str = "0x322813Fd9A801c5507c9de605d63CEA4f2CE6c44";
	const LOCALNET_ETH_PRICE_FEED_ETH: &str = "0xa85233C63b9Ee964Add6F2cffe00Fd84eb32338f";
	const LOCALNET_ARB_PRICE_FEED_BTC: &str = "0xa85233C63b9Ee964Add6F2cffe00Fd84eb32338f";
	const LOCALNET_ETH_ADDRESS_CHECKER: &str = "0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512";
	const LOCALNET_ARB_ADDRESS_CHECKER: &str = "0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0";

	fn print_round_data(chain_name: &str, query_result: (U256, U256, Vec<PriceFeedData>)) {
		let (query_block_number, query_block_timestamp, price_feeds_data) = query_result;
		println!("Price feed data for chain: {}", chain_name);
		println!("Query Block Number: {}", query_block_number);
		println!("Query Block Timestamp: {}", query_block_timestamp);
		for feed in price_feeds_data.iter() {
			println!("  Round ID: {}", feed.round_id);
			println!("  Answer: {}", feed.answer);
			println!("  Started At: {}", feed.started_at);
			println!("  Updated At: {}", feed.updated_at);
			println!("  Answered In Round: {}", feed.answered_in_round);
			println!("  Decimals: {}", feed.decimals);
			println!("  Description: {}", feed.description);
		}
	}

	#[tokio::test]
	#[ignore = "requires access to external RPC"]
	async fn eth_oracle_aggregator_test_mainnet() {
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
				.query_price_feeds(
					H160::from_str(ADDRESS_CHECKER_ETHEREUM_ADDRESS).unwrap(),
					vec![
						H160::from_str(BTC_USD_AGGREGATOR_ETHEREUM_ADDRESS).unwrap(),
						H160::from_str(ETH_USD_AGGREGATOR_ETHEREUM_ADDRESS).unwrap(),
					],
				)
				.await
				.unwrap(),
		);
		print_round_data(
			"Arbitrum",
			arb_client
				.query_price_feeds(
					H160::from_str(ADDRESS_CHECKER_ARBITRUM_ADDRESS).unwrap(),
					vec![
						H160::from_str(BTC_USD_AGGREGATOR_ARBITRUM_ADDRESS).unwrap(),
						H160::from_str(ETH_USD_AGGREGATOR_ARBITRUM_ADDRESS).unwrap(),
					],
				)
				.await
				.unwrap(),
		);
	}

	#[tokio::test]
	#[ignore = "requires access to external RPC"]
	async fn eth_oracle_aggregator_test_sepolia() {
		let settings = Settings::new_test().unwrap();

		let eth_client = EvmRpcSigningClient::new(
			settings.clone().eth.private_key_file,
			"https://sepolia.infura.io/v3/<YOUR_API_KEY>".into(),
			11155111u64,
			"Ethereum",
		)
		.unwrap()
		.await;

		print_round_data(
			"Ethereum",
			eth_client
				.query_price_feeds(
					H160::from_str("0xb421E1DEbd6803CcFdf09B4262F1bAEd4eAFD97b").unwrap(), // Deployed AddressChecker
					vec![
						H160::from_str("0x1b44F3514812d835EB1BDB0acB33d3fA3351Ee43").unwrap(), // Btc Sepolia
						H160::from_str("0x694AA1769357215DE4FAC081bf1f309aDC325306").unwrap(), // Eth Sepolia
					],
				)
				.await
				.unwrap(),
		);
	}

	#[tokio::test]
	#[ignore = "Requires connection to localnet"]
	async fn eth_oracle_aggregator_localnet() {
		let settings = Settings::new_test().unwrap();

		let eth_client = EvmRpcSigningClient::new(
			settings.clone().eth.private_key_file,
			"http://localhost:8545".into(), // Bouncer localnet
			10997u64,                       // Bouncer aggregator_addresses chain id
			// "http://127.0.0.1:8545".into(), // Hardhat node ("npx hardhat node")
			// 31337u64,                       // Hardhat node ("npx hardhat node")
			"Ethereum",
		)
		.unwrap()
		.await;

		print_round_data(
			"Ethereum",
			eth_client
				.query_price_feeds(
					H160::from_str(LOCALNET_ETH_ADDRESS_CHECKER).unwrap(),
					vec![
						H160::from_str(LOCALNET_ETH_PRICE_FEED_BTC).unwrap(),
						H160::from_str(LOCALNET_ETH_PRICE_FEED_ETH).unwrap(),
					],
				)
				.await
				.unwrap(),
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
				.query_price_feeds(
					H160::from_str(LOCALNET_ARB_ADDRESS_CHECKER).unwrap(),
					vec![H160::from_str(LOCALNET_ARB_PRICE_FEED_BTC).unwrap()],
				)
				.await
				.unwrap(),
		);
	}
}
