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

use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

use crate::Runtime;

pub struct SafeModeMigration;

use crate::runtime_apis::custom_api::types::before_version_19::RuntimeSafeMode as OldRuntimeSafeMode;

impl OnRuntimeUpgrade for SafeModeMigration {
	fn on_runtime_upgrade() -> Weight {
		let _ = pallet_cf_environment::RuntimeSafeMode::<Runtime>::translate(
			|maybe_old: Option<OldRuntimeSafeMode>| maybe_old.map(Into::into),
		)
		.map_err(|_| {
			log::warn!(
				"Safe mode migration was not able to interpret the existing storage in the old format!"
			);
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: sp_std::vec::Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
		frame_support::ensure!(
			pallet_cf_environment::RuntimeSafeMode::<Runtime>::exists(),
			"RuntimeSafeMode should exist after the migration"
		);
		Ok(())
	}
}
