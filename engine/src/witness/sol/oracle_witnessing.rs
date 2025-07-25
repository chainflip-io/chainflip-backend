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

use crate::sol::{retry_rpc::SolRetryRpcApi, rpc_client_api::*};
use cf_chains::sol::SolAddress;
use sol_prim::{
	consts::const_address,
	program_instructions::{
		oracle_query_helpers::OracleQueryHelperProgram, InstructionExt, PriceFeedData,
		PriceFeedResponse,
	},
	AccountMeta,
};

use base64::{prelude::BASE64_STANDARD, Engine};
use cf_chains::sol::{SolVersionedMessage, SolVersionedTransaction};
use std::str::FromStr;

// Simulating a transaction requires a payer, even if the simulation doesn't require a tx fee.
// We use a prefunded account for this purpose. The keys have been burnt.
const PREFUNDED_ACCOUNT: SolAddress = const_address("CsS34ewTFLGqrpckPRww5hbWr4QJQ1J3ZA5D7WL4Ni3K");

pub async fn get_price_feeds<SolRetryRpcClient>(
	sol_client: &SolRetryRpcClient,
	oracle_query_helper: SolAddress,
	oracle_program_id: SolAddress,
	feed_addresses: Vec<SolAddress>,
	min_context_slot: Option<u64>,
) -> Result<(Vec<PriceFeedData>, i64, u64), anyhow::Error>
where
	SolRetryRpcClient: SolRetryRpcApi + Send + Sync + Clone,
{
	let serialized_transaction = build_and_serialize_query_transaction(
		oracle_query_helper,
		oracle_program_id,
		feed_addresses,
	)
	.map_err(|e| anyhow::anyhow!("Failed to build and serialize the query transaction: {:?}", e))?;

	let simulation_result = sol_client
		.simulate_transaction(serialized_transaction, min_context_slot)
		.await?;

	let query_slot = simulation_result.context.slot;

	let return_data = simulation_result
		.value
		.return_data
		.as_ref()
		.ok_or_else(|| anyhow::anyhow!("Expected return data to be Some"))?;

	let (price_feeds, query_timestamp) = decode_query_return_data(return_data, oracle_query_helper)
		.map_err(|e| anyhow::anyhow!("Failed to decode the query return data: {:?}", e))?;

	Ok((price_feeds, query_timestamp, query_slot))
}

// NOTE: This builds a transaction with the default compute units (200k). This should be enough for
// querying more than 10 feeds so we don't bother extending the compute budget. If that was not
// enough, the compute budget extension instruction needs to be added to the transaction before
// serialization.
fn build_and_serialize_query_transaction(
	oracle_query_helper: SolAddress,
	oracle_program_id: SolAddress,
	feed_addresses: Vec<SolAddress>,
) -> Result<Vec<u8>, anyhow::Error> {
	let price_feed_metas: Vec<AccountMeta> = feed_addresses
		.into_iter()
		.map(|feed_account| AccountMeta::new(feed_account.into(), false))
		.collect();
	let instructions = vec![OracleQueryHelperProgram::with_id(oracle_query_helper)
		.query_price_feeds(oracle_program_id)
		.with_additional_accounts(price_feed_metas)];

	let transaction = SolVersionedTransaction::new_unsigned(SolVersionedMessage::new(
		&instructions,
		Some(PREFUNDED_ACCOUNT.into()),
		None,
		&[],
	));
	transaction
		.clone()
		.finalize_and_serialize()
		.map_err(|e| anyhow::anyhow!("Failed to serialize oracle query transaction: {:?}", e))
}

fn decode_query_return_data(
	return_data: &UiTransactionReturnData,
	expected_program_id: SolAddress,
) -> Result<(Vec<PriceFeedData>, i64), anyhow::Error> {
	let decoded_return_data = BASE64_STANDARD.decode(return_data.data.0.clone())?;
	if return_data.data.1 != UiReturnDataEncoding::Base64 {
		anyhow::bail!("Expected return data encoding to be Base64, found {:?}", return_data.data.1);
	}
	let program_id = SolAddress::from_str(&return_data.program_id)?;
	if program_id != expected_program_id {
		anyhow::bail!(
			"Program ID mismatch: expected {}, found {}",
			expected_program_id,
			program_id
		);
	}

	let response = PriceFeedResponse::try_from(decoded_return_data).map_err(|e| {
		anyhow::anyhow!("Failed to decode PriceFeedResponse from return data: {:?}", e)
	})?;

	Ok((response.results, response.query_timestamp))
}

#[cfg(test)]
mod tests {
	use crate::{
		settings::{HttpEndpoint, NodeContainer},
		sol::retry_rpc::{SolRetryRpcApi, SolRetryRpcClient},
	};
	use cf_chains::{sol::SolHash, Chain, Solana};
	use cf_utilities::task_scope;
	use futures_util::FutureExt;
	use sol_prim::consts::const_address;
	use std::str::FromStr;

	use super::*;

	struct TestConfig {
		endpoint: &'static str,
		sol_hash: Option<&'static str>,
		oracle_program_id: &'static str,
		oracle_feeds: Vec<&'static str>,
		oracle_query_helper: &'static str,
		expected_description: Vec<&'static str>,
	}

	async fn run_query_test(config: TestConfig) -> Result<(), anyhow::Error> {
		task_scope::task_scope(|scope| {
			async {
				let sol_hash =
					config.sol_hash.map(|hash| SolHash::from_str(hash).expect("Invalid SolHash"));

				let client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: HttpEndpoint { http_endpoint: config.endpoint.into() },
						backup: None,
					},
					sol_hash,
					Solana::WITNESS_PERIOD,
				)
				.await?;

				let oracle_program_id = const_address(config.oracle_program_id);
				let oracle_feeds =
					config.oracle_feeds.clone().into_iter().map(const_address).collect::<Vec<_>>();
				let oracle_query_helper = const_address(config.oracle_query_helper);

				let serialized_transaction = build_and_serialize_query_transaction(
					oracle_query_helper,
					oracle_program_id,
					oracle_feeds,
				)?;

				let simulation_result =
					client.simulate_transaction(serialized_transaction, None).await?;
				let return_data = simulation_result
					.value
					.return_data
					.as_ref()
					.expect("Expected return data to be Some");

				let (price_feeds, query_timestamp) =
					decode_query_return_data(return_data, oracle_query_helper)?;

				println!("Query Timestamp: {}", query_timestamp);
				// Ensure the number of price feeds matches the number of expected descriptions
				assert_eq!(
					price_feeds.len(),
					config.expected_description.len(),
					"Mismatch between number of price feeds and expected descriptions"
				);

				for (result_index, (price_feed, expected_desc)) in
					price_feeds.iter().zip(config.expected_description.iter()).enumerate()
				{
					let PriceFeedData { round_id, slot, timestamp, answer, decimals, description } =
						price_feed;

					println!(
						"Index {}: Description: {}, Round ID: {}, Slot: {}, Timestamp: {}, Answer: {}, Decimals: {}",
						result_index, description, round_id, slot, timestamp, answer, decimals
					);
					assert_eq!(*decimals, 8);
					assert_eq!(
						description, expected_desc,
						"Description mismatch at index {}",
						result_index
					);
				}

				Ok(())
			}
			.boxed()
		})
		.await
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn can_build_query_tx_and_simulate_mainnet() {
		run_query_test(TestConfig {
			endpoint: "https://api.mainnet-beta.solana.com",
			sol_hash: Some("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"),
			oracle_program_id: "HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny",
			oracle_feeds: vec!["Cv4T27XbjVoKUYwP72NQQanvZeA7W4YF9L4EnYT9kx5o"],
			oracle_query_helper: "5Vg6D87L4LMDoyze9gU56NhvcRKWrwbJMquF2tj4vnuX",
			expected_description: vec!["BTC / USD"],
		})
		.await
		.unwrap();
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn can_build_query_tx_and_simulate_devnet() {
		run_query_test(TestConfig {
			endpoint: "https://api.devnet.solana.com",
			sol_hash: Some("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"),
			oracle_program_id: "HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny",
			oracle_feeds: vec!["6PxBx93S8x3tno1TsFZwT5VqP8drrRCbCXygEXYNkFJe"],
			oracle_query_helper: "5Vg6D87L4LMDoyze9gU56NhvcRKWrwbJMquF2tj4vnuX",
			expected_description: vec!["BTC / USD"],
		})
		.await
		.unwrap();
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn can_query_multiple_devnet() {
		run_query_test(TestConfig {
			endpoint: "https://api.devnet.solana.com",
			sol_hash: Some("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"),
			oracle_program_id: "HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny",
			oracle_feeds: vec![
				"6PxBx93S8x3tno1TsFZwT5VqP8drrRCbCXygEXYNkFJe",
				"669U43LNHx7LsVj95uYksnhXUfWKDsdzVqev3V4Jpw3P",
			],
			oracle_query_helper: "5Vg6D87L4LMDoyze9gU56NhvcRKWrwbJMquF2tj4vnuX",
			expected_description: vec!["BTC / USD", "ETH / USD"],
		})
		.await
		.unwrap();
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn can_build_query_tx_and_simulate_localnet() {
		run_query_test(TestConfig {
			endpoint: "http://127.0.0.1:8899",
			sol_hash: None,
			oracle_program_id: "DfYdrym1zoNgc6aANieNqj9GotPj2Br88rPRLUmpre7X",
			oracle_feeds: vec!["HDSV2wFxmsrmCwwY34QzaVkvmJpG7VF8S9fX2iThynjG"],
			oracle_query_helper: "GXn7uzbdNgozXuS8fEbqHER1eGpD9yho7FHTeuthWU8z",
			expected_description: vec!["BTC / USD"],
		})
		.await
		.unwrap();
	}
}
