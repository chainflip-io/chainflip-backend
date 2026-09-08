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

/// Every authority now contributes to consensus unconditionally; this migration deletes the
/// now-orphaned `ContributingAuthorities` entries.
mod old {
	use crate::{Config, Pallet};
	use cf_traits::Chainflip;
	use frame_support::{pallet_prelude::OptionQuery, storage_alias, Identity};

	#[storage_alias]
	pub type ContributingAuthorities<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Identity, <T as Chainflip>::ValidatorId, (), OptionQuery>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let result = old::ContributingAuthorities::<T, I>::clear(u32::MAX, None);
		T::DbWeight::get().writes(result.unique.into())
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		frame_support::ensure!(
			old::ContributingAuthorities::<T, I>::iter_keys().next().is_none(),
			"ContributingAuthorities storage should have been removed"
		);
		Ok(())
	}
}
