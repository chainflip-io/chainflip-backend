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

use crate::{BscInstance, Runtime};

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// Bsc was integrated without setting `WitnessSafetyMargin`, so `cf_witness_safety_margin` returns
/// `None` while the margin actually in effect is the one in the Bsc elections settings. Copying
/// that value keeps the effective margin unchanged.
pub struct BscWitnessSafetyMargin;

impl OnRuntimeUpgrade for BscWitnessSafetyMargin {
	fn on_runtime_upgrade() -> Weight {
		if pallet_cf_ingress_egress::WitnessSafetyMargin::<Runtime, BscInstance>::get().is_none() {
			if let Some(settings) =
				pallet_cf_elections::ElectoralUnsynchronisedSettings::<Runtime, BscInstance>::get()
			{
				pallet_cf_ingress_egress::WitnessSafetyMargin::<Runtime, BscInstance>::put(
					settings.1.safety_margin as u64,
				);
			}
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		if let Some(settings) =
			pallet_cf_elections::ElectoralUnsynchronisedSettings::<Runtime, BscInstance>::get()
		{
			frame_support::ensure!(
				pallet_cf_ingress_egress::WitnessSafetyMargin::<Runtime, BscInstance>::get() ==
					Some(settings.1.safety_margin as u64),
				"Bsc WitnessSafetyMargin does not match the elections safety margin"
			);
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::chainflip::witnessing::bsc_elections;

	#[test]
	fn sets_margin_from_election_settings() {
		sp_io::TestExternalities::default().execute_with(|| {
			let settings = bsc_elections::initial_state().unsynchronised_settings;
			let expected = settings.1.safety_margin as u64;
			pallet_cf_elections::ElectoralUnsynchronisedSettings::<Runtime, BscInstance>::put(
				settings,
			);

			BscWitnessSafetyMargin::on_runtime_upgrade();

			assert_eq!(
				pallet_cf_ingress_egress::WitnessSafetyMargin::<Runtime, BscInstance>::get(),
				Some(expected)
			);
		});
	}

	#[test]
	fn does_not_overwrite_existing_margin() {
		sp_io::TestExternalities::default().execute_with(|| {
			pallet_cf_elections::ElectoralUnsynchronisedSettings::<Runtime, BscInstance>::put(
				bsc_elections::initial_state().unsynchronised_settings,
			);
			pallet_cf_ingress_egress::WitnessSafetyMargin::<Runtime, BscInstance>::put(5);

			BscWitnessSafetyMargin::on_runtime_upgrade();

			assert_eq!(
				pallet_cf_ingress_egress::WitnessSafetyMargin::<Runtime, BscInstance>::get(),
				Some(5)
			);
		});
	}
}
