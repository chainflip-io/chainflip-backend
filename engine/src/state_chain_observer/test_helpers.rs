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

use sp_core::H256;

use super::client::BlockInfo;

pub fn test_header(number: u32, parent_hash: Option<H256>) -> BlockInfo {
	BlockInfo {
		number,
		parent_hash: parent_hash.unwrap_or_default(),
		hash: H256::from_low_u64_le(number.into()),
	}
}
