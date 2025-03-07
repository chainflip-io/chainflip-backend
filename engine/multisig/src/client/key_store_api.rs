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

use super::KeygenResultInfo;
use crate::{crypto::KeyId, ChainSigning};

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait KeyStoreAPI<C: ChainSigning>: Send + Sync {
	/// Get the key for the given key id
	fn get_key(&self, key_id: &KeyId) -> Option<KeygenResultInfo<C::CryptoScheme>>;

	/// Save or update the key data and write it to persistent memory
	fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo<C::CryptoScheme>);
}
