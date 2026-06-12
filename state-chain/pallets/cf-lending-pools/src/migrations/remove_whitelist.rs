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

use crate::Config;
use frame_support::{
	traits::{Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// The lending whitelist has been removed; this migration deletes the now-orphaned
/// `Whitelist` storage value.
mod old {
	use crate::{Config, Pallet};
	use frame_support::storage_alias;

	// The actual stored type is irrelevant for removal, so we use `()` to avoid depending on
	// the deleted `WhitelistStatus` type. The storage prefix matches the original item.
	#[storage_alias]
	pub type Whitelist<T: Config> = StorageValue<Pallet<T>, ()>;
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		old::Whitelist::<T>::kill();
		T::DbWeight::get().writes(1)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		frame_support::ensure!(
			!old::Whitelist::<T>::exists(),
			"Whitelist storage should have been removed"
		);
		Ok(())
	}
}
