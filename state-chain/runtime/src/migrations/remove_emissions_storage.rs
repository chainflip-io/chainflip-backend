// Copyright 2026 Chainflip Labs GmbH
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

use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

/// The emissions pallet was removed from `construct_runtime!` now that FLIP 2.1 is
/// unconditionally active. Removing a pallet doesn't clear its storage, so this clears the
/// orphaned `Emissions` prefix directly. The pallet only ever had a handful of `StorageValue`
/// items, so a single unbounded `clear_prefix` is safe here.
pub struct RemoveEmissionsStorage;

impl OnRuntimeUpgrade for RemoveEmissionsStorage {
	fn on_runtime_upgrade() -> Weight {
		let _ = frame_support::storage::unhashed::clear_prefix(
			&sp_core::hashing::twox_128(b"Emissions"),
			None,
			None,
		);

		Weight::zero()
	}
}
