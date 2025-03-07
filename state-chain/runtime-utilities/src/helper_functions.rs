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

use frame_support::{migration::move_storage_from_pallet, traits::PalletInfoAccess};

/// Move storage between pallets.
pub fn move_pallet_storage<From: PalletInfoAccess, To: PalletInfoAccess>(storage_name: &[u8]) {
	log::info!(
		"⏫ Moving storage {} from {} to {}.",
		sp_std::str::from_utf8(storage_name).expect("storage names are all valid utf8"),
		From::name(),
		To::name(),
	);
	move_storage_from_pallet(storage_name, From::name().as_bytes(), To::name().as_bytes());
}
