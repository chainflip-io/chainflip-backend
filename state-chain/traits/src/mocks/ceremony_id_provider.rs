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

use cf_primitives::CeremonyId;
use codec::{Decode, Encode};
use scale_info::TypeInfo;

use frame_support::{storage, StorageHasher, Twox64Concat};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockCeremonyIdProvider;

impl MockCeremonyIdProvider {
	const STORAGE_KEY: &'static [u8] = b"MockCeremonyIdProvider::Counter";

	pub fn set(id: CeremonyId) {
		storage::hashed::put(&<Twox64Concat as StorageHasher>::hash, Self::STORAGE_KEY, &id)
	}

	pub fn get() -> CeremonyId {
		storage::hashed::get_or_default(&<Twox64Concat as StorageHasher>::hash, Self::STORAGE_KEY)
	}
}
