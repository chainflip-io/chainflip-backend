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
use crate::{Config, *};
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

pub struct VoteStorageMigration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for VoteStorageMigration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		// We can simply delete all the current votes (for all elections),
		// this will cause the validator to re-vote for the same elections again,
		// which is fine since the worst it can happen is some delayed consensus which is fine since
		// we are in safe mode
		let _ = SharedDataReferenceCount::<T, I>::clear(u32::MAX, None);
		let _ = SharedData::<T, I>::clear(u32::MAX, None);
		let _ = BitmapComponents::<T, I>::clear(u32::MAX, None);
		let _ = IndividualComponents::<T, I>::clear(u32::MAX, None);
		let _ = ElectionConsensusHistory::<T, I>::clear(u32::MAX, None);
		let _ = ElectionConsensusHistoryUpToDate::<T, I>::clear(u32::MAX, None);

		Weight::zero()
	}
}
