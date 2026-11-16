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

mod old {
	use crate::{Config, Pallet};
	use cf_primitives::{Asset, AssetAmount};
	use frame_support::{storage_alias, Twox64Concat};

	#[storage_alias]
	pub type MaximumSwapAmount<T: Config> = StorageMap<Pallet<T>, Twox64Concat, Asset, AssetAmount>;
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let removed = old::MaximumSwapAmount::<T>::clear(u32::MAX, None).unique;
		T::DbWeight::get().reads_writes(removed.into(), removed.into())
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		frame_support::ensure!(
			old::MaximumSwapAmount::<T>::iter().next().is_none(),
			"MaximumSwapAmount storage should have been removed"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};
	use cf_primitives::Asset;

	#[test]
	fn removes_maximum_swap_amount_storage() {
		new_test_ext().execute_with(|| {
			old::MaximumSwapAmount::<Test>::insert(Asset::Btc, 100);
			old::MaximumSwapAmount::<Test>::insert(Asset::Eth, 200);

			Migration::<Test>::on_runtime_upgrade();

			assert!(old::MaximumSwapAmount::<Test>::iter().next().is_none());
		});
	}
}
