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

use crate::*;
use frame_support::traits::UncheckedOnRuntimeUpgrade;

#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct Migration<T>(core::marker::PhantomData<T>);

mod old {
	use super::*;

	// Note: stored values don't matter since all we do is kill the storage.

	#[frame_support::storage_alias]
	pub type BackupNodeEmissionInflation<T: Config> = StorageValue<Pallet<T>, ()>;
	#[frame_support::storage_alias]
	pub type BackupNodeEmissionPerBlock<T: Config> = StorageValue<Pallet<T>, ()>;
}

impl<T> UncheckedOnRuntimeUpgrade for Migration<T>
where
	T: Config,
{
	fn on_runtime_upgrade() -> frame_support::pallet_prelude::Weight {
		old::BackupNodeEmissionInflation::<T>::kill();
		old::BackupNodeEmissionPerBlock::<T>::kill();
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_: Vec<u8>) -> Result<(), frame_support::pallet_prelude::DispatchError> {
		assert!(!old::BackupNodeEmissionInflation::<T>::exists());
		assert!(!old::BackupNodeEmissionPerBlock::<T>::exists());
		Ok(())
	}
}
