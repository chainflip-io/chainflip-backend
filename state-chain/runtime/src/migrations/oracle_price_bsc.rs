// Copyright 2026 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

//! Resets the generic oracle elections with the BSC price source enabled.

use crate::{
	chainflip::witnessing::generic_elections::{
		initial_state,
		oracle_feeds::{self, OracleFeeds},
		ChainlinkOraclePriceSettings,
	},
	Runtime,
};
use cf_primitives::ChainflipNetwork;
#[cfg(feature = "try-runtime")]
use frame_support::traits::GetStorageVersion;
use frame_support::{
	migrations::VersionedMigration as FrameVersionedMigration, traits::UncheckedOnRuntimeUpgrade,
	weights::Weight,
};
use pallet_cf_elections::InitialStateOf;
#[cfg(feature = "try-runtime")]
use pallet_cf_elections::UniqueMonotonicIdentifier;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub type VersionedMigration = FrameVersionedMigration<
	9,
	10,
	Migration,
	pallet_cf_elections::Pallet<Runtime>,
	<Runtime as frame_system::Config>::DbWeight,
>;

pub struct Migration;

impl UncheckedOnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		let initial =
			new_initial_state(pallet_cf_environment::ChainflipNetworkName::<Runtime>::get());

		pallet_cf_elections::Pallet::<Runtime>::reset();
		if let Err(error) = pallet_cf_elections::Pallet::<Runtime>::internally_initialize(initial) {
			cf_runtime_utilities::log_or_panic!(
				"Failed to initialize generic oracle elections after reset: {error:?}"
			);
		}

		log::info!("Reset generic oracle elections with the BSC price source enabled");
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let expected =
			new_initial_state(pallet_cf_environment::ChainflipNetworkName::<Runtime>::get());
		frame_support::ensure!(
			pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime>::get() ==
				Some(expected.unsynchronised_state),
			"Generic oracle state was not reset to its initial value"
		);
		frame_support::ensure!(
			pallet_cf_elections::ElectoralUnsynchronisedSettings::<Runtime>::get() ==
				Some(expected.unsynchronised_settings),
			"Generic oracle settings were not reset to their initial value"
		);

		let (expected_electoral_settings,) = expected.settings;
		let (electoral_settings,) = pallet_cf_elections::ElectoralSettings::<Runtime>::get(
			UniqueMonotonicIdentifier::default(),
		)
		.ok_or("Reset oracle feed settings are missing")?;
		frame_support::ensure!(
			electoral_settings == expected_electoral_settings,
			"Oracle feed settings differ from the chain-spec values"
		);
		frame_support::ensure!(
			pallet_cf_elections::ElectoralSettings::<Runtime>::iter().count() == 1,
			"Old oracle settings boundaries were not cleared"
		);
		frame_support::ensure!(
			election_storage_is_empty(),
			"Old oracle elections were not cleared"
		);
		frame_support::ensure!(
			pallet_cf_elections::NextElectionIdentifier::<Runtime>::get() ==
				UniqueMonotonicIdentifier::default(),
			"Oracle election identifiers were not reset"
		);
		frame_support::ensure!(
			pallet_cf_elections::ContributingAuthorities::<Runtime>::iter_keys()
				.next()
				.is_none(),
			"Contributing oracle authorities were not cleared"
		);
		frame_support::ensure!(
			pallet_cf_elections::Status::<Runtime>::get() ==
				Some(pallet_cf_elections::ElectionPalletStatus::Running),
			"Generic oracle elections were not initialized"
		);
		frame_support::ensure!(
			pallet_cf_elections::SharedDataReferenceLifetime::<Runtime>::get() ==
				expected.shared_data_reference_lifetime,
			"Oracle shared data reference lifetime was not initialized"
		);
		frame_support::ensure!(
			pallet_cf_elections::Pallet::<Runtime>::on_chain_storage_version() ==
				pallet_cf_elections::STORAGE_VERSION,
			"Generic Elections storage version was not updated"
		);
		pallet_cf_elections::Pallet::<Runtime>::do_try_state()?;

		Ok(())
	}
}

fn new_initial_state(network: ChainflipNetwork) -> InitialStateOf<Runtime, ()> {
	let OracleFeeds { arbitrum, ethereum, bsc } = oracle_feeds::for_network(network);
	initial_state(ChainlinkOraclePriceSettings {
		arb_address_checker: pallet_cf_environment::ArbitrumAddressCheckerAddress::<Runtime>::get(),
		arb_oracle_feeds: arbitrum.into(),
		eth_address_checker: pallet_cf_environment::EthereumAddressCheckerAddress::<Runtime>::get(),
		eth_oracle_feeds: ethereum.into(),
		bsc_address_checker: pallet_cf_environment::BscAddressCheckerAddress::<Runtime>::get(),
		bsc_oracle_feeds: bsc.into(),
	})
}

#[cfg(feature = "try-runtime")]
fn election_storage_is_empty() -> bool {
	pallet_cf_elections::SharedDataReferenceCount::<Runtime>::iter()
		.next()
		.is_none() &&
		pallet_cf_elections::SharedData::<Runtime>::iter().next().is_none() &&
		pallet_cf_elections::BitmapComponents::<Runtime>::iter().next().is_none() &&
		pallet_cf_elections::IndividualComponents::<Runtime>::iter().next().is_none() &&
		pallet_cf_elections::ElectoralUnsynchronisedStateMap::<Runtime>::iter()
			.next()
			.is_none() &&
		pallet_cf_elections::ElectionProperties::<Runtime>::iter().next().is_none() &&
		pallet_cf_elections::ElectionState::<Runtime>::iter().next().is_none() &&
		pallet_cf_elections::ElectionConsensusHistory::<Runtime>::iter()
			.next()
			.is_none() &&
		pallet_cf_elections::ElectionConsensusHistoryUpToDate::<Runtime>::iter()
			.next()
			.is_none()
}
