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

use super::RpcResult;
use async_trait::async_trait;

use super::stream_api::{StreamApi, FINALIZED, UNFINALIZED};

#[async_trait]
pub trait ChainApi {
	fn latest_finalized_block(&self) -> super::BlockInfo;
	fn latest_unfinalized_block(&self) -> super::BlockInfo;

	async fn finalized_block_stream(&self) -> Box<dyn StreamApi<FINALIZED>>;
	async fn unfinalized_block_stream(&self) -> Box<dyn StreamApi<UNFINALIZED>>;

	async fn block(&self, hash: state_chain_runtime::Hash) -> RpcResult<super::BlockInfo>;
}
