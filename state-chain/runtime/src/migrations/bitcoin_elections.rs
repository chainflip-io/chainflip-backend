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

use crate::{
	chainflip::bitcoin_elections::{
		BitcoinBlockHeightWitnesserES, BitcoinDepositChannelWitnessingES,
		BitcoinEgressWitnessingES, BitcoinLiveness, BitcoinVaultDepositWitnessingES,
	},
	BitcoinInstance, Runtime,
};
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::{
	migrations::VersionedMigration,
	traits::{OnRuntimeUpgrade, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use pallet_cf_elections::{ElectoralSystemTypes, Pallet};
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
use sp_std::vec::Vec;

pub type Migration = (
	VersionedMigration<
		6,
		7,
		BitcoinElectionMigration,
		pallet_cf_elections::Pallet<Runtime, BitcoinInstance>,
		<Runtime as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<7, Pallet<Runtime, BitcoinInstance>>,
);

pub struct BitcoinElectionMigration;

mod old {

	use super::*;
	use cf_chains::btc::BtcAmount;
	use frame_support::pallet_prelude::OptionQuery;
	use pallet_cf_elections::Config;

	pub type CompositeElectoralUnsynchronisedSettings = (
		<BitcoinBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		<BitcoinDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		<BitcoinVaultDepositWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		<BitcoinEgressWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		BtcAmount,
		<BitcoinLiveness as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
	);

	pub type CompositeElectoralUnsynchronisedState = (
		<BitcoinBlockHeightWitnesserES as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		<BitcoinDepositChannelWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		<BitcoinVaultDepositWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		<BitcoinEgressWitnessingES as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		BtcAmount,
		<BitcoinLiveness as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
	);

	#[frame_support::storage_alias]
	pub type ElectoralUnsynchronisedSettings<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, CompositeElectoralUnsynchronisedSettings, OptionQuery>;

	#[frame_support::storage_alias]
	pub type ElectoralUnsynchronisedState<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, CompositeElectoralUnsynchronisedState, OptionQuery>;
}

impl UncheckedOnRuntimeUpgrade for BitcoinElectionMigration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		Ok(Vec::new())
	}

	fn on_runtime_upgrade() -> Weight {
		log::info!("🍩 Migration for BTC Election started");

		// migrating unsynchronised state
		{
			let optional_storage =
				old::ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::get();
			let (a, b, c, d, current_btc_fee, f) =
				optional_storage.expect("Should contain something");

			pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::put((
				a,
				b,
				c,
				d,
				(current_btc_fee, 0), // last election concluded at block 0
				f,
			));
		}

		// migrating unsynchronised settings
		{
			let optional_storage =
				old::ElectoralUnsynchronisedSettings::<Runtime, BitcoinInstance>::get();
			let (a, b, c, d, _old_settings_amount, f) =
				optional_storage.expect("Should contain something");

			pallet_cf_elections::ElectoralUnsynchronisedSettings::<Runtime, BitcoinInstance>::put(
				(
					a, b, c, d, 10u32, // fee witnessing should happen every 10 SC blocks
					f,
				),
			);
		}

		log::info!("🍩 Migration for BTC Election completed");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		let current_state =
			pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime, BitcoinInstance>::get()
				.unwrap()
				.4;

		assert_eq!(current_state.1, 0);

		let current_settings =
			pallet_cf_elections::ElectoralUnsynchronisedSettings::<Runtime, BitcoinInstance>::get()
				.unwrap()
				.4;

		assert_eq!(current_settings, 10);

		Ok(())
	}
}

pub struct MyDebugMigration;
impl OnRuntimeUpgrade for MyDebugMigration {
	fn on_runtime_upgrade() -> Weight {
		panic!("let's try to see if this can fail?");
		log::info!("$$$ Running debug migration $$$$");
		Weight::zero()
	}
}
