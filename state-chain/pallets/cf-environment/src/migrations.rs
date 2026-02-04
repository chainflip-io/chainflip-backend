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
#[cfg(feature = "try-runtime")]
use crate::{CurrentReleaseVersion, Get};
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::{migrations::VersionedMigration, traits::OnRuntimeUpgrade};
#[cfg(feature = "try-runtime")]
use frame_support::{pallet_prelude::DispatchError, sp_runtime};
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod wbtc_arbusdt_solusdt;
// NOTE: Do not remove this. This is used to update the on-chain version for CFE compatibility
// checks.
pub struct VersionUpdate<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for VersionUpdate<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(Default::default())
	}

	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		Pallet::<T>::update_current_release_version();
		frame_support::weights::Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		frame_support::ensure!(
			CurrentReleaseVersion::<T>::get() == <T as Config>::CurrentReleaseVersion::get(),
			"Expect storage to be the new version after upgrade."
		);

		Ok(())
	}
}

pub type PalletMigration<T> = (
	VersionedMigration<
		21,
		22,
		wbtc_arbusdt_solusdt::NewAssetsMigration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<22, Pallet<T>>,
);

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{CurrentReleaseVersion, SemVer};

	use crate::mock::{new_test_ext, Test};

	#[test]
	fn version_updates() {
		new_test_ext().execute_with(|| {
			let config_version = <Test as Config>::CurrentReleaseVersion::get();

			let old_version = SemVer { major: 1, minor: 0, patch: 0 };
			assert_ne!(old_version, config_version);
			CurrentReleaseVersion::<Test>::put(old_version);

			VersionUpdate::<Test>::on_runtime_upgrade();

			assert_eq!(CurrentReleaseVersion::<Test>::get(), config_version);
		});
	}
}
