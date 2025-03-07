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

use super::{
	chunked_by_time::{builder::ChunkedByTimeBuilder, ChunkedByTime},
	chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
};

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn logging(self, log_prefix: &'static str) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner: ChunkedByVault,
	{
		self.then(move |epoch, header| async move {
			tracing::info!(
				"{} | {} processed: epoch index: {:?}, block index {:?}, hash {:?}",
				<Inner::Chain as cf_chains::Chain>::NAME,
				log_prefix,
				epoch.index,
				header.index,
				header.hash
			);
			Ok::<_, anyhow::Error>(header.data)
		})
	}
}

impl<Inner: ChunkedByTime> ChunkedByTimeBuilder<Inner> {
	pub fn logging(self, log_prefix: &'static str) -> ChunkedByTimeBuilder<impl ChunkedByTime>
	where
		Inner: ChunkedByTime,
	{
		self.then(move |epoch, header| async move {
			tracing::info!(
				"{} | {} processed: epoch index: {:?}, block index {:?}, hash {:?}",
				<Inner::Chain as cf_chains::Chain>::NAME,
				log_prefix,
				epoch.index,
				header.index,
				header.hash
			);
			Ok::<_, anyhow::Error>(header.data)
		})
	}
}
