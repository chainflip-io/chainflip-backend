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
	chainflip::{
		self,
		bitcoin_block_processor::BtcEvent,
		bitcoin_elections::{
			BitcoinBlockHeightWitnesserES, BitcoinDepositChannelWitnessingES,
			BitcoinEgressWitnessingES, BitcoinFeeSettings, BitcoinLiveness,
			BitcoinVaultDepositWitnessing, BitcoinVaultDepositWitnessingES,
		},
		elections::TypesFor,
	},
	BitcoinInstance, Runtime,
};
use cf_chains::{btc::BtcAmount, refund_parameters::ChannelRefundParameters, Chain};
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::{
	migrations::VersionedMigration, traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};
use pallet_cf_elections::{ElectoralSystemTypes, Pallet};
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

pub type Migration = (
	VersionedMigration<
		6,
		7,
		IngressEgressDelay,
		pallet_cf_elections::Pallet<Runtime, BitcoinInstance>,
		<Runtime as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<7, Pallet<Runtime, BitcoinInstance>>,
);

pub struct IngressEgressDelay;

impl UncheckedOnRuntimeUpgrade for IngressEgressDelay {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {

	}

	fn on_runtime_upgrade() -> Weight {


		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
	}
}
