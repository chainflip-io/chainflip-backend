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

use cf_chains::dot::PolkadotTrackedData;
use subxt::events::Phase;

use super::super::common::{
	chain_source::Header, chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData,
};
use crate::{
	dot::{retry_rpc::DotRetryRpcApi, PolkadotHash},
	witness::dot::EventWrapper,
};

#[async_trait::async_trait]
impl<T: DotRetryRpcApi + Send + Sync + Clone>
	GetTrackedData<cf_chains::Polkadot, PolkadotHash, Vec<(Phase, EventWrapper)>> for T
{
	async fn get_tracked_data(
		&self,
		header: &Header<
			<cf_chains::Polkadot as cf_chains::Chain>::ChainBlockNumber,
			PolkadotHash,
			Vec<(Phase, EventWrapper)>,
		>,
	) -> Result<<cf_chains::Polkadot as cf_chains::Chain>::TrackedData, anyhow::Error> {
		let events = &header.data;

		let mut tips = Vec::new();
		for (phase, wrapped_event) in events.iter() {
			if let Phase::ApplyExtrinsic(_) = phase {
				if let EventWrapper::TransactionFeePaid { tip, .. } = wrapped_event {
					tips.push(*tip);
				}
			}
		}

		Ok(PolkadotTrackedData {
			median_tip: {
				tips.sort();
				tips.get(tips.len().saturating_sub(1) / 2).cloned().unwrap_or_default()
			},
			runtime_version: self.runtime_version(None).await,
		})
	}
}
