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

use std::cmp;

use crate::btc::rpc::BtcRpcApi;
use anyhow::anyhow;
use bitcoin::Txid;
use cf_chains::btc::BtcAmount;
use pallet_cf_elections::electoral_systems::oracle_price::primitives::compute_median;

pub async fn predict_fees(
	client: &impl BtcRpcApi,
	tx_sample_count_per_mempool_block: u32,
) -> anyhow::Result<BtcAmount> {
	let info = client.mempool_info().await?;

	if info.size == 0 || info.bytes == 0 {
		return Err(anyhow!("mempool is empty"));
	}

	// -- average vbytes per tx --
	let vbytes_per_tx = (info.bytes as f64) / (info.size as f64);
	tracing::debug!("average vbytes/tx: {vbytes_per_tx}");

	// -- number of blocks in the mempool --
	let blocks_in_mempool = (info.bytes as f64) / 1000000.0;
	let txs_per_block = 1000000.0 / vbytes_per_tx;
	tracing::debug!("there are ~{blocks_in_mempool} blocks in the mempool, with average {txs_per_block} txs per block");

	// -- calculate sample target --
	let sample_size = if blocks_in_mempool >= 1.0 {
		let sample_size = (tx_sample_count_per_mempool_block as f64) * blocks_in_mempool;
		tracing::debug!("we have to download {sample_size} txs to have an average sample size of {tx_sample_count_per_mempool_block} per block");
		sample_size as usize
	} else {
		tracing::debug!("there is less than a single mempool block, falling back to downloading at most {tx_sample_count_per_mempool_block} transactions");
		tx_sample_count_per_mempool_block as usize
	};

	// ----------------------------------
	// downloading txs

	let tx_hashes: Vec<Txid> = client.raw_mempool().await?;
	tracing::debug!("Got {} hashes.", tx_hashes.len());

	use rand::seq::SliceRandom;
	let sub_tx_hashes: Vec<Txid> = tx_hashes
		.choose_multiple(&mut rand::thread_rng(), cmp::min(sample_size, tx_hashes.len()))
		.cloned()
		.collect();
	tracing::debug!("Selected a subset of size {}", sub_tx_hashes.len());

	let tx_data = client.mempool_entries(sub_tx_hashes).await?;
	tracing::debug!("Got data for {} txs", tx_data.len());

	let mut fees: Vec<_> = tx_data
		.into_iter()
		.filter_map(|a| {
			if a.vsize == 0 {
				return None;
			}
			// we multiply by 1000 because our unit on the statechain is sat/vkilobyte,
			// also this means that we don't need rationals or floating points to get
			// good enough precision
			Some((a.fees.base.to_sat() * 1000) / (a.vsize as u64))
		})
		.collect();
	fees.sort_unstable_by(|a, b| b.cmp(a)); // sort in descending order, we don't care about object identities
	let fees_next_block_len = cmp::min(tx_sample_count_per_mempool_block as usize, fees.len());
	let fees_next_block = &mut fees[0..fees_next_block_len];

	// make sure that we exit with an error if for some reason we don't have at least half the
	// sample size that we wanted
	if fees_next_block_len < (tx_sample_count_per_mempool_block as usize / 2) {
		return Err(anyhow!(
			"We got only {fees_next_block_len} fee samples, this is less than half our target sample count."
		));
	} else {
		tracing::debug!("Computing median fee rate of {} values", fees_next_block_len);
	}

	// compute and return fee
	let median_fee = compute_median(fees_next_block)
		.ok_or_else(|| anyhow!("Not enough values to compute median for fee estimation."))?;
	tracing::debug!("Estimated median fee rate is: {median_fee} sat/vkilobyte");

	Ok(*median_fee)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{btc::rpc::BtcRpcClient, settings::HttpBasicAuthEndpoint};
	use std::time::Duration;
	use tokio::time::sleep;

	#[ignore = "requires a running localnet"]
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
}
