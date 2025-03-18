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

use cf_chains::ChainCrypto;

use super::{MockPallet, MockPalletStorage};
use crate::EpochKey;
use std::marker::PhantomData;

#[derive(Default)]
pub struct MockKeyProvider<C: ChainCrypto>(PhantomData<C>);

impl<C: ChainCrypto> MockPallet for MockKeyProvider<C> {
	const PREFIX: &'static [u8] = b"MockKeyProvider::";
}

const EPOCH_KEY: &[u8] = b"EPOCH_KEY";

impl<C: ChainCrypto> MockKeyProvider<C> {
	pub fn set_key(key: C::AggKey) {
		Self::put_value(EPOCH_KEY, EpochKey { key, epoch_index: Default::default() });
	}
}

impl<C: ChainCrypto> crate::KeyProvider<C> for MockKeyProvider<C> {
	fn active_epoch_key() -> Option<EpochKey<C::AggKey>> {
		Self::get_value(EPOCH_KEY)
	}
}
