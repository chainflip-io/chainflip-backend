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

use std::{cmp, time::Duration};

use crate::{
	btc::rpc::{BtcRpcApi, BtcRpcClient},
	settings::HttpBasicAuthEndpoint,
};
use bitcoin::Txid;
use cf_chains::btc::BtcAmount;
use pallet_cf_elections::electoral_systems::oracle_price::primitives::compute_aggregated;
use tokio::time::sleep;

pub async fn predict_fees(
	client: &impl BtcRpcApi,
	target_tx_sample_count_per_block: u32,
) -> anyhow::Result<BtcAmount> {
	let info = client.mempool_info().await?;

	if info.size == 0 || info.bytes == 0 {
		return Ok(0);
	}

	// -- average vbytes per tx --
	let vbytes_per_tx = (info.bytes as f64) / (info.size as f64);
	println!("average vbytes/tx: {vbytes_per_tx}");

	// -- number of blocks in the mempool --
	let blocks_in_mempool = (info.bytes as f64) / 1000000.0;
	let txs_per_block = 1000000.0 / vbytes_per_tx;
	println!("there are ~{blocks_in_mempool} blocks in the mempool, with average {txs_per_block} txs per block");

	// -- calculate sample target --
	let sample_size = (target_tx_sample_count_per_block as f64) * blocks_in_mempool;
	println!("we have to download {sample_size} txs to have an average sample size of {target_tx_sample_count_per_block} per block");

	// ----------------------------------
	// downloading txs

	let tx_hashes: Vec<Txid> = client.raw_mempool().await?;
	println!("Got {} hashes.", tx_hashes.len());

	use rand::seq::SliceRandom;
	let sub_tx_hashes: Vec<Txid> = tx_hashes
		.choose_multiple(&mut rand::thread_rng(), sample_size as usize)
		.cloned()
		.collect();
	println!("Selected a subset of size {}", sub_tx_hashes.len());

	let tx_data = client.mempool_entries(sub_tx_hashes).await?;
	println!("Got data for {} txs", tx_data.len());

	let mut fees: Vec<_> = tx_data
		.into_iter()
		.filter_map(|a| {
			if a.vsize == 0 {
				return None;
			}
			Some((a.fees.base.to_sat() * 1000) / (a.vsize as u64))
		})
		.collect();
	fees.sort_unstable_by(|a, b| b.cmp(a)); // sort in descending order, we don't care about object identities
	let fees_next_block = &fees[0..cmp::min(target_tx_sample_count_per_block as usize, fees.len())];

	let fee_stats = compute_aggregated(fees_next_block.into_iter().cloned().collect());
	println!(
		"Got {} fees for the next block. Max: {}, min: {}, stats: {:?}",
		fees_next_block.len(),
		fees_next_block[0],
		fees_next_block[fees_next_block.len() - 1],
		fee_stats
	);

	Ok(0)
}

#[test]
fn mytestt() {
	let x = [0, 2];
	let y = &x[1..5];
	println!("{y:?}");
}

#[tokio::test]
async fn track_btc_fees() {
	let client = BtcRpcClient::new(
		HttpBasicAuthEndpoint {
			http_endpoint: "http://localhost:8332".into(),
			basic_auth_user: "flip".to_string(),
			basic_auth_password: "flip".to_string(),
		},
		None,
	)
	.unwrap()
	.await;

	loop {
		predict_fees(&client, 60).await.unwrap();
		sleep(Duration::from_secs(20)).await;
	}
}
