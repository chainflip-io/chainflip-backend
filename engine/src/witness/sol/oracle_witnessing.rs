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

use crate::sol::rpc_client_api::*;

use cf_chains::sol::SolAddress;
use sol_prim::{AccountMeta, Instruction};

use base64::{prelude::BASE64_STANDARD, Engine};
use cf_chains::sol::{SolVersionedMessage, SolVersionedTransaction};

// TODO: We could consider hardcoding the serialized transaction so we don't have to serialize it
// every time.
// TODO: Simulating a transation will only return the return data of the last instruction. This
// means we'lll need an rpc call per asset. If we want to reduce one to a single call we'll need to
// write a small program that makes all the CPI calls and returns all the data as one.
// Since we use Solana as the main chain it might be worth it.
fn build_and_serialize_query_transaction(
	payer: SolAddress,
	chainlink_program_id: SolAddress,
	feed_address: SolAddress,
) -> Result<Vec<u8>, anyhow::Error> {
	let account_metas = vec![AccountMeta::new_readonly(feed_address.into(), false)];

	// Stands for "sha256("global:latest_round_data")[0:8]"
	// // const QUERY_INSTRUCTION_DISCRIMINATOR = Buffer.from([
	// //   0x27, 0xfb, 0x82, 0x9f, 0x2e, 0x88, 0xa4, 0xa9,
	// // ]);

	// // enum Query {
	// //     Version,
	// //     Decimals,
	// //     Description,
	// //     RoundData { round_id: u32 },
	// //     LatestRoundData,
	// //     Aggregator,
	// // }
	// Buffer.concat([QUERY_INSTRUCTION_DISCRIMINATOR, queryByte])
	let data: [u8; 9] = [0x27, 0xfb, 0x82, 0x9f, 0x2e, 0x88, 0xa4, 0xa9, 0x04];

	let instructions =
		vec![Instruction::new_with_bincode(chainlink_program_id.into(), &data, account_metas)];
	println!("instructions: {:?}", instructions);

	let transaction = SolVersionedTransaction::new_unsigned(SolVersionedMessage::new(
		&instructions,
		Some(payer.into()),
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
) -> Result<(u32, u64, u32, i128), anyhow::Error> {
	// TODO: We could also assert that the return_data.program_id matches the programID we have
	// serialized-encoded.
	let decoded_return_data = BASE64_STANDARD.decode(return_data.data.0.clone())?;
	assert_eq!(return_data.data.1, UiReturnDataEncoding::Base64);

	// Verify length (expect 32 bytes)
	assert_eq!(decoded_return_data.len(), 32);

	let round_id = u32::from_le_bytes(decoded_return_data[0..4].try_into()?);
	let slot = u64::from_le_bytes(decoded_return_data[4..12].try_into()?);
	let timestamp = u32::from_le_bytes(decoded_return_data[12..16].try_into()?);
	let answer = i128::from_le_bytes(decoded_return_data[16..32].try_into()?);

	Ok((round_id, slot, timestamp, answer))
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

				let payer: SolAddress =
					const_address("5GaMJ6MMdjCtSBADfWjYSupzk3voYbpGnfi7dkZY9S6a");
				let chainlink_program_id: SolAddress =
					const_address("HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny");
				let chainlink_feed: SolAddress =
					const_address("6PxBx93S8x3tno1TsFZwT5VqP8drrRCbCXygEXYNkFJe");

				let serialized_transaction = build_and_serialize_query_transaction(
					payer,
					chainlink_program_id,
					chainlink_feed,
				)
				.unwrap();

				let simulation_result = client.simulate_transaction(serialized_transaction).await;

				let return_data = simulation_result
					.value
					.return_data
					.as_ref()
					.expect("Expected return data to be Some");

				let (round_id, slot, timestamp, answer) =
					decode_query_return_data(return_data).unwrap();

				println!(
					"Round ID: {}, Slot: {}, Timestamp: {}, Answer: {}",
					round_id, slot, timestamp, answer
				);

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

				let payer: SolAddress =
					const_address("HfasueN6RNPjSM6rKGH5dga6kS2oUF8siGH3m4MXPURp");
				let chainlink_program_id: SolAddress =
					const_address("DfYdrym1zoNgc6aANieNqj9GotPj2Br88rPRLUmpre7X");
				let chainlink_feed: SolAddress =
					const_address("GRZmvuxuxCXyrabSuMdqwbn53Bht9wDRMqitgL49nNFK");

				let serialized_transaction = build_and_serialize_query_transaction(
					payer,
					chainlink_program_id,
					chainlink_feed,
				)
				.unwrap();

				let simulation_result = client.simulate_transaction(serialized_transaction).await;

				let return_data = simulation_result
					.value
					.return_data
					.as_ref()
					.expect("Expected return data to be Some");

				let (round_id, slot, timestamp, answer) =
					decode_query_return_data(return_data).unwrap();

				println!(
					"Round ID: {}, Slot: {}, Timestamp: {}, Answer: {}",
					round_id, slot, timestamp, answer
				);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
