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

use crate::{BrokerRecoveryApi, FailedDepositRecovery, FailedDepositRecoveryOutcome};
use cf_primitives::{Asset, AssetAmount};
use frame_support::sp_runtime::DispatchError;
use sp_std::vec::Vec;

use super::{MockPallet, MockPalletStorage};

pub struct MockBrokerRecoveryApi;

impl MockPallet for MockBrokerRecoveryApi {
	const PREFIX: &'static [u8] = b"MockBrokerRecoveryApi";
}

const RECOVERY_ACCOUNTS: &[u8] = b"RECOVERY_ACCOUNTS";

impl MockBrokerRecoveryApi {
	pub fn link(broker: u64, recovery_account: u64) {
		Self::put_storage(RECOVERY_ACCOUNTS, broker, recovery_account);
	}
}

impl BrokerRecoveryApi for MockBrokerRecoveryApi {
	type AccountId = u64;

	fn recovery_account(broker: &Self::AccountId) -> Option<Self::AccountId> {
		Self::get_storage(RECOVERY_ACCOUNTS, broker)
	}
}

/// Credits the recovery account of any broker linked in [MockBrokerRecoveryApi], and records each
/// recovery.
pub struct MockFailedDepositRecovery;

impl MockPallet for MockFailedDepositRecovery {
	const PREFIX: &'static [u8] = b"MockFailedDepositRecovery";
}

const RECOVERIES: &[u8] = b"RECOVERIES";

impl MockFailedDepositRecovery {
	pub fn recoveries() -> Vec<(u64, Asset, AssetAmount)> {
		Self::get_storage(RECOVERIES, ()).unwrap_or_default()
	}
}

impl FailedDepositRecovery for MockFailedDepositRecovery {
	type AccountId = u64;

	fn route_failed_deposit(
		broker: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> Result<(Self::AccountId, FailedDepositRecoveryOutcome), DispatchError> {
		let recovery_account = MockBrokerRecoveryApi::recovery_account(broker)
			.ok_or(DispatchError::Other("no recovery account"))?;
		Self::mutate_storage::<(), _, _, _, _>(RECOVERIES, &(), |recoveries| {
			recoveries.get_or_insert_with(Vec::new).push((recovery_account, asset, amount))
		});
		Ok((recovery_account, FailedDepositRecoveryOutcome::Credited))
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup_benchmark(broker: &Self::AccountId) {
		MockBrokerRecoveryApi::link(*broker, broker + 1);
	}
}
