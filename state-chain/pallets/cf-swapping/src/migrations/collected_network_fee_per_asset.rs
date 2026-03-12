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

use cf_primitives::STABLE_ASSET;
use frame_support::{
	traits::{Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use cf_primitives::{Asset, AssetAmount};
#[cfg(feature = "try-runtime")]
use codec::Encode;
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use cf_primitives::AssetAmount;
	use frame_support::pallet_prelude::ValueQuery;

	#[frame_support::storage_alias]
	pub type CollectedNetworkFee<T: crate::Config> =
		StorageValue<crate::Pallet<T>, AssetAmount, ValueQuery>;
}

pub struct Migration<T>(PhantomData<T>);

impl<T: crate::Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let old_value = old::CollectedNetworkFee::<T>::take();

		if old_value > 0 {
			crate::CollectedNetworkFee::<T>::insert(STABLE_ASSET, old_value);
			log::info!(
				"✅ Migrated CollectedNetworkFee: {} USDC inserted into per-asset map.",
				old_value
			);
			T::DbWeight::get().reads_writes(1, 2)
		} else {
			log::info!("✅ CollectedNetworkFee was zero, no data to migrate.");
			T::DbWeight::get().reads(1)
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let old_value = old::CollectedNetworkFee::<T>::get();
		log::info!("Pre-upgrade: CollectedNetworkFee old value = {}", old_value);
		Ok(old_value.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use codec::Decode;
		use frame_support::ensure;

		let old_value = AssetAmount::decode(&mut &state[..])
			.map_err(|_| DispatchError::Other("Failed to decode pre-upgrade state"))?;

		let migrated = crate::CollectedNetworkFee::<T>::get(Asset::Usdc);

		ensure!(
			migrated == old_value,
			"Post-upgrade: CollectedNetworkFee for Usdc does not match pre-upgrade value"
		);

		ensure!(
			!old::CollectedNetworkFee::<T>::exists(),
			"Post-upgrade: old CollectedNetworkFee StorageValue was not removed"
		);

		log::info!("✅ Post-upgrade: CollectedNetworkFee migration verified. Usdc = {}.", migrated);

		Ok(())
	}
}
