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

use core::marker::PhantomData;

use cf_chains::Chain;

use crate::GetBlockHeight;

use super::MockPallet;
use crate::mocks::MockPalletStorage;

pub struct BlockHeightProvider<C: Chain>(PhantomData<C>);

impl<C: Chain> MockPallet for BlockHeightProvider<C> {
	const PREFIX: &'static [u8] = b"MockBlockHeightProvider";
}

const BLOCK_HEIGHT_KEY: &[u8] = b"BLOCK_HEIGHT";

impl<C: Chain> BlockHeightProvider<C> {
	pub fn set_block_height(height: C::ChainBlockNumber) {
		Self::put_value(BLOCK_HEIGHT_KEY, height);
	}

	pub fn increment_block_height() {
		Self::set_block_height(Self::get_block_height() + 1u32.into());
	}
}

const DEFAULT_BLOCK_HEIGHT: u32 = 1337;

impl<C: Chain> GetBlockHeight<C> for BlockHeightProvider<C> {
	fn get_block_height() -> C::ChainBlockNumber {
		Self::get_value(BLOCK_HEIGHT_KEY).unwrap_or(DEFAULT_BLOCK_HEIGHT.into())
	}
}
