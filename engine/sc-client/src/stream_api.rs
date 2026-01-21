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

use cf_utilities::cached_stream::CachedStream;
use futures::Stream;

use super::BlockInfo;

pub const FINALIZED: bool = true;
pub const UNFINALIZED: bool = false;

pub trait StreamApi<const IS_FINALIZED: bool>:
	CachedStream<Item = BlockInfo> + Send + Sync + Unpin + 'static
{
}

#[derive(Clone)]
#[pin_project::pin_project]
pub struct StateChainStream<const IS_FINALIZED: bool, S>(#[pin] S);

impl<const IS_FINALIZED: bool, S: CachedStream> StateChainStream<IS_FINALIZED, S> {
	pub fn new(inner: S) -> Self {
		Self(inner)
	}
}

impl<const IS_FINALIZED: bool, S: Stream> Stream for StateChainStream<IS_FINALIZED, S> {
	type Item = <S as Stream>::Item;

	fn poll_next(
		self: core::pin::Pin<&mut Self>,
		cx: &mut core::task::Context<'_>,
	) -> core::task::Poll<Option<Self::Item>> {
		self.project().0.poll_next(cx)
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.0.size_hint()
	}
}
impl<const IS_FINALIZED: bool, S> CachedStream for StateChainStream<IS_FINALIZED, S>
where
	S: CachedStream,
{
	fn cache(&self) -> &Self::Item {
		self.0.cache()
	}
}
impl<
		const IS_FINALIZED: bool,
		S: CachedStream<Item = BlockInfo> + Unpin + Send + Sync + 'static,
	> StreamApi<IS_FINALIZED> for StateChainStream<IS_FINALIZED, S>
{
}
