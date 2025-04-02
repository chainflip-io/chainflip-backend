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
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use super::*;

	// Renaming this to MinimumNetworkFee
	#[frame_support::storage_alias]
	pub type MinimumNetworkFeePerChunk<T: Config> =
		StorageValue<Pallet<T>, AssetAmount, ValueQuery>;
}

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let fee = crate::MinimumNetworkFee::<T>::get();
		Ok(fee.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let fee = old::MinimumNetworkFeePerChunk::<T>::take();
		crate::MinimumNetworkFee::<T>::put(fee);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let new_fee = crate::MinimumNetworkFee::<T>::get();
		let old_fee = AssetAmount::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;
		assert_eq!(new_fee, old_fee);

		Ok(())
	}
}
