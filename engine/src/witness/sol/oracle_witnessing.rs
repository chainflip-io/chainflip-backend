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
	program_instructions::{oracle_query_helpers::OracleQueryHelperProgram, InstructionExt},
	AccountMeta,
};

use base64::{prelude::BASE64_STANDARD, Engine};
use cf_chains::sol::{SolVersionedMessage, SolVersionedTransaction};
use std::str::FromStr;

// Simulating a tranasction requires a payer, even if the simulation doesn't require a tx fee.
// We use a prefunded account for this purpose. The keys have been burnt.
#[allow(dead_code)]
const PREFUNDED_ACCOUNT: SolAddress = const_address("CsS34ewTFLGqrpckPRww5hbWr4QJQ1J3ZA5D7WL4Ni3K");

#[allow(dead_code)]
pub struct PriceFeedData {
	pub round_id: u32,
	pub slot: u64,
	pub timestamp: u32,
	pub answer: i128,
	pub decimals: u8,
	pub description: String,
}

#[allow(dead_code)]
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

	let simulation_result =
		sol_client.simulate_transaction(serialized_transaction, min_context_slot).await;

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

// Expected returned data is Vec<PriceFeedData>
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

	// It should always have 8 bytes for the query timestamp, followed by 4 bytes for the vector
	// length.
	let mut offset = 12;
	if decoded_return_data.len() < offset {
		anyhow::bail!("Insufficient data length for decoding");
	}

	let query_timestamp = i64::from_le_bytes(decoded_return_data[0..8].try_into()?);

	// Manually deserialize return data - Vec<PriceFeedData>
	let num_entries = u32::from_le_bytes(decoded_return_data[8..offset].try_into()?);

	let mut results = Vec::new();
	for _ in 0..num_entries {
		if offset + 37 > decoded_return_data.len() {
			anyhow::bail!(anyhow::anyhow!("Insufficient data length"));
		}

		let round_id = u32::from_le_bytes(decoded_return_data[offset..offset + 4].try_into()?);
		let slot = u64::from_le_bytes(decoded_return_data[offset + 4..offset + 12].try_into()?);
		let timestamp =
			u32::from_le_bytes(decoded_return_data[offset + 12..offset + 16].try_into()?);
		let answer = i128::from_le_bytes(decoded_return_data[offset + 16..offset + 32].try_into()?);
		let decimals = u8::from_le_bytes(decoded_return_data[offset + 32..offset + 33].try_into()?);

		let string_length =
			u32::from_le_bytes(decoded_return_data[offset + 33..offset + 37].try_into()?);
		let string_start = offset + 37;
		let string_end = string_start + string_length as usize;
		if string_end > decoded_return_data.len() {
			anyhow::bail!("Insufficient data for string content");
		}
		let description =
			String::from_utf8(decoded_return_data[string_start..string_end].to_vec())?;

		results.push(PriceFeedData { round_id, slot, timestamp, answer, decimals, description });

		// Update offset for the next entry
		offset = string_end;
	}

	// Check for extra bytes
	if offset != decoded_return_data.len() {
		anyhow::bail!(
			"Unexpected trailing bytes: {} bytes remaining",
			decoded_return_data.len() - offset
		);
	}

	Ok((results, query_timestamp))
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

	// TODO: PRO-2320: Add same test for mainnet when the `oracle_query_helper` is deployed to make
	// sure it works and that the prefunded account is prefunded correctly

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn can_build_query_tx_and_simulate_devnet() {
		task_scope::task_scope(|scope| {
			async {
				let client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: HttpEndpoint {
							http_endpoint: "https://api.devnet.solana.com".into(),
						},
						backup: None,
					},
					Some(
						SolHash::from_str("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG").unwrap(),
					),
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				let oracle_program_id: SolAddress =
					const_address("HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny");
				let oracle_feed: SolAddress =
					const_address("6PxBx93S8x3tno1TsFZwT5VqP8drrRCbCXygEXYNkFJe");
				let oracle_query_helper: SolAddress =
					const_address("HaAGuDMxS56xgoy9vzm1NtESKftoqpiHCysvXRULk7K7");

				let serialized_transaction = build_and_serialize_query_transaction(
					oracle_query_helper,
					oracle_program_id,
					vec![oracle_feed],
				)
				.unwrap();

				let simulation_result =
					client.simulate_transaction(serialized_transaction, None).await;

				let slot = simulation_result.context.slot;
				println!("Simulation slot: {}", slot);

				let return_data = simulation_result
					.value
					.return_data
					.as_ref()
					.expect("Expected return data to be Some");

				let (price_feeds, query_timestamp) =
					decode_query_return_data(return_data, oracle_query_helper).unwrap();
				println!("Query Timestamp: {}", query_timestamp);

				let PriceFeedData { round_id, slot, timestamp, answer, decimals, description } =
					price_feeds.first().unwrap();

				println!(
					"Round ID: {}, Slot: {}, Timestamp: {}, Answer: {}, Decimals: {}, Description: {}",
					round_id, slot, timestamp, answer, decimals, description
				);

				assert_eq!(*decimals, 8);
				assert_eq!(description, "BTC / USD");

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn can_query_multiple_devnet() {
		task_scope::task_scope(|scope| {
			async {
				let client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: HttpEndpoint {
							http_endpoint: "https://api.devnet.solana.com".into(),
						},
						backup: None,
					},
					Some(
						SolHash::from_str("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG").unwrap(),
					),
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				let oracle_program_id: SolAddress =
					const_address("HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny");
				let oracle_feeds = vec![
					const_address("6PxBx93S8x3tno1TsFZwT5VqP8drrRCbCXygEXYNkFJe"),
					const_address("669U43LNHx7LsVj95uYksnhXUfWKDsdzVqev3V4Jpw3P"),
				];
				let oracle_query_helper: SolAddress =
					const_address("HaAGuDMxS56xgoy9vzm1NtESKftoqpiHCysvXRULk7K7");

				let serialized_transaction = build_and_serialize_query_transaction(
					oracle_query_helper,
					oracle_program_id,
					oracle_feeds,
				)
				.unwrap();

				let simulation_result =
					client.simulate_transaction(serialized_transaction, None).await;

				let return_data = simulation_result
					.value
					.return_data
					.as_ref()
					.expect("Expected return data to be Some");

				let (price_feeds, _) =
					decode_query_return_data(return_data, oracle_query_helper).unwrap();

				for (result_index, price_feed) in price_feeds.iter().enumerate() {
					let PriceFeedData { round_id, slot, timestamp, answer, decimals, description } =
						price_feed;

					println!(
						"Index {}: Description: {}, Round ID: {}, Slot: {}, Timestamp: {}, Answer: {}, Decimals: {}",
						result_index,
						round_id, slot, timestamp, answer, decimals, description
					);
					assert_eq!(*decimals, 8);
				}

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	#[ignore = "requires access to external RPC"]
	#[tokio::test]
	async fn can_build_query_tx_and_simulate_localnet() {
		task_scope::task_scope(|scope| {
			async {
				let client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: HttpEndpoint { http_endpoint: "http://127.0.0.1:8899".into() },
						backup: None,
					},
					None,
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				let oracle_program_id: SolAddress =
					const_address("DfYdrym1zoNgc6aANieNqj9GotPj2Br88rPRLUmpre7X");
				let oracle_feed: SolAddress =
					const_address("HDSV2wFxmsrmCwwY34QzaVkvmJpG7VF8S9fX2iThynjG");
				let oracle_query_helper: SolAddress =
					const_address("GXn7uzbdNgozXuS8fEbqHER1eGpD9yho7FHTeuthWU8z");

				let serialized_transaction = build_and_serialize_query_transaction(
					oracle_query_helper,
					oracle_program_id,
					vec![oracle_feed],
				)
				.unwrap();

				let simulation_result =
					client.simulate_transaction(serialized_transaction, None).await;
				let return_data = simulation_result
					.value
					.return_data
					.as_ref()
					.expect("Expected return data to be Some");

				let (price_feeds, _) =
					decode_query_return_data(return_data, oracle_query_helper).unwrap();

				let PriceFeedData { round_id, slot, timestamp, answer, decimals, description } =
					price_feeds.first().unwrap();

				println!(
					"Round ID: {}, Slot: {}, Timestamp: {}, Answer: {}, Decimals: {}, Description: {}",
					round_id, slot, timestamp, answer, decimals, description
				);

				assert_eq!(*decimals, 8);
				assert_eq!(description, "BTC / USD");

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
