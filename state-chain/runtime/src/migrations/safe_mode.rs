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

use codec::DecodeAll;
use frame_support::{storage::unhashed, traits::OnRuntimeUpgrade, weights::Weight};

use crate::{safe_mode::RuntimeSafeMode, Runtime};

use crate::runtime_apis::custom_api::types::before_version_22::RuntimeSafeMode as OldRuntimeSafeMode;

/// Drops the `emissions` entry from the stored runtime safe mode. Without this, the pre-upgrade
/// encoding would be decoded shifted by one byte, silently scrambling every pallet's safe mode.
pub struct SafeModeMigration;

impl OnRuntimeUpgrade for SafeModeMigration {
	fn on_runtime_upgrade() -> Weight {
		let storage_key = pallet_cf_environment::RuntimeSafeMode::<Runtime>::hashed_key();
		let Some(encoded) = unhashed::get_raw(&storage_key) else { return Weight::zero() };

		// Check the old format first: a shifted decode of the old encoding into the current
		// format is not guaranteed to fail.
		if let Ok(old) = OldRuntimeSafeMode::decode_all(&mut encoded.as_slice()) {
			pallet_cf_environment::RuntimeSafeMode::<Runtime>::put(RuntimeSafeMode::from(old));
		} else if RuntimeSafeMode::decode_all(&mut encoded.as_slice()).is_err() {
			log::warn!(
				"Safe mode migration was not able to interpret the existing storage in the old format!"
			);
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: sp_std::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let storage_key = pallet_cf_environment::RuntimeSafeMode::<Runtime>::hashed_key();
		if let Some(encoded) = unhashed::get_raw(&storage_key) {
			frame_support::ensure!(
				RuntimeSafeMode::decode_all(&mut encoded.as_slice()).is_ok(),
				"RuntimeSafeMode storage is not in the current format"
			);
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cf_traits::SafeMode;
	use codec::Encode;

	#[test]
	fn translates_pre_upgrade_storage() {
		sp_io::TestExternalities::default().execute_with(|| {
			unhashed::put_raw(
				&pallet_cf_environment::RuntimeSafeMode::<Runtime>::hashed_key(),
				&OldRuntimeSafeMode {
					// Deliberately neither code green nor code red: the storage item is
					// `ValueQuery`, so a migration that silently wiped it would read back as code
					// green and a uniform fixture couldn't tell the two apart.
					liquidity_provider: pallet_cf_lp::PalletSafeMode {
						deposit_enabled: false,
						withdrawal_enabled: true,
						internal_swaps_enabled: false,
						flip_to_on_chain_balance_enabled: true,
					},
					funding: pallet_cf_funding::PalletSafeMode::code_red(),
					witnesser: pallet_cf_witnesser::PalletSafeMode::code_green(),
					..Default::default()
				}
				.encode(),
			);

			SafeModeMigration::on_runtime_upgrade();

			let migrated = pallet_cf_environment::RuntimeSafeMode::<Runtime>::get();
			assert_eq!(
				migrated.liquidity_provider,
				pallet_cf_lp::PalletSafeMode {
					deposit_enabled: false,
					withdrawal_enabled: true,
					internal_swaps_enabled: false,
					flip_to_on_chain_balance_enabled: true,
				}
			);
			assert_eq!(migrated.funding, pallet_cf_funding::PalletSafeMode::code_red());
			assert_eq!(migrated.swapping, pallet_cf_swapping::PalletSafeMode::code_green());
			assert_eq!(migrated.witnesser, pallet_cf_witnesser::PalletSafeMode::code_green());
		});
	}

	#[test]
	fn leaves_current_format_untouched() {
		sp_io::TestExternalities::default().execute_with(|| {
			let already_migrated = RuntimeSafeMode::code_red();
			pallet_cf_environment::RuntimeSafeMode::<Runtime>::put(already_migrated.clone());

			SafeModeMigration::on_runtime_upgrade();

			assert_eq!(pallet_cf_environment::RuntimeSafeMode::<Runtime>::get(), already_migrated);
		});
	}
}
