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

use bitcoin::BlockHash;

use crate::btc::retry_rpc::BtcRetryRpcApi;
use cf_chains::btc::{BitcoinFeeInfo, BitcoinTrackedData};

use super::super::common::{
	chain_source::Header, chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData,
};

#[async_trait::async_trait]
impl<T: BtcRetryRpcApi + Send + Sync + Clone> GetTrackedData<cf_chains::Bitcoin, BlockHash, ()>
	for T
{
	async fn get_tracked_data(
		&self,
		header: &Header<<cf_chains::Bitcoin as cf_chains::Chain>::ChainBlockNumber, BlockHash, ()>,
	) -> Result<<cf_chains::Bitcoin as cf_chains::Chain>::TrackedData, anyhow::Error> {
		let fee_rate = if let Some(next_block_fee_rate) = self.next_block_fee_rate().await {
			next_block_fee_rate
		} else {
			self.average_block_fee_rate(header.hash).await
		};

		Ok(BitcoinTrackedData { btc_fee_info: BitcoinFeeInfo::new(fee_rate) })
	}
}
