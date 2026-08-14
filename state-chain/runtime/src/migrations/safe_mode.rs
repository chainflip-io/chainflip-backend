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

pub struct SafeModeMigration;

use crate::runtime_apis::custom_api::types::before_version_19::RuntimeSafeMode as OldRuntimeSafeMode;

impl OnRuntimeUpgrade for SafeModeMigration {
	fn on_runtime_upgrade() -> Weight {
		let storage_key = pallet_cf_environment::RuntimeSafeMode::<Runtime>::hashed_key();
		if unhashed::get_raw(&storage_key)
			.is_some_and(|encoded| RuntimeSafeMode::decode_all(&mut encoded.as_slice()).is_ok())
		{
			return Weight::zero()
		}

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
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::runtime_apis::custom_api::types::before_version_19::LiquidityProviderSafeMode as OldLiquidityProviderSafeMode;
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
					liquidity_provider: OldLiquidityProviderSafeMode {
						deposit_enabled: false,
						withdrawal_enabled: true,
						internal_swaps_enabled: false,
					},
					funding: pallet_cf_funding::PalletSafeMode::code_red(),
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
		});
	}

	#[test]
	fn leaves_current_format_untouched() {
		sp_io::TestExternalities::default().execute_with(|| {
			let already_migrated = RuntimeSafeMode::code_red();
			pallet_cf_environment::RuntimeSafeMode::<Runtime>::put(already_migrated.clone());

			SafeModeMigration::on_runtime_upgrade();

			assert_eq!(
				pallet_cf_environment::RuntimeSafeMode::<Runtime>::get(),
				already_migrated
			);
		});
	}
}
