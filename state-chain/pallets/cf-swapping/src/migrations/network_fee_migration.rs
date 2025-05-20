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
use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::Config;

use crate::*;
use frame_support::pallet_prelude::Weight;

pub mod old {
	use super::*;

	#[frame_support::storage_alias]
	pub type MinimumNetworkFee<T: Config> = StorageValue<Pallet<T>, AssetAmount, ValueQuery>;

	#[frame_support::storage_alias]
	pub type InternalSwapNetworkFee<T: Config> = StorageValue<Pallet<T>, Permill, ValueQuery>;

	#[frame_support::storage_alias]
	pub type InternalSwapMinimumNetworkFee<T: Config> =
		StorageValue<Pallet<T>, AssetAmount, ValueQuery>;
}

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	// Migrating from separate storage items for rate and minimum to a single item of type
	// FeeRateAndMinimum. Also changing the normal network fee rate from a constant to a storage
	// item.
	fn on_runtime_upgrade() -> Weight {
		NetworkFee::<T>::set(FeeRateAndMinimum {
			minimum: old::MinimumNetworkFee::<T>::take(),
			// The old fee was a constant value, so just setting it manually here.
			rate: Permill::from_perthousand(1),
		});
		InternalSwapNetworkFee::<T>::set(FeeRateAndMinimum {
			minimum: old::InternalSwapMinimumNetworkFee::<T>::take(),
			rate: old::InternalSwapNetworkFee::<T>::take(),
		});
		Weight::zero()
	}
}
