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

use crate::{Config, Pallet};
use cf_primitives::BroadcastId;
use frame_support::{pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade};

mod old {
	use super::*;

	// The value types don't matter for clearing; we only need the storage prefix to match.
	#[frame_support::storage_alias]
	pub type RequestSuccessCallbacks<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BroadcastId, ()>;

	#[frame_support::storage_alias]
	pub type RequestFailureCallbacks<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BroadcastId, ()>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let r1 = old::RequestSuccessCallbacks::<T, I>::clear(u32::MAX, None);
		let r2 = old::RequestFailureCallbacks::<T, I>::clear(u32::MAX, None);

		log::info!(
			"🧹 {}: Cleared {} RequestSuccessCallbacks and {} RequestFailureCallbacks entries.",
			<Pallet<T, I> as PalletInfoAccess>::name(),
			r1.unique,
			r2.unique,
		);

		Weight::zero()
	}
}
