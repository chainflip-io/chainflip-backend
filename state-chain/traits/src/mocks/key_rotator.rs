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

use crate::{mocks::MockPalletStorage, AsyncResult, KeyRotationStatusOuter, KeyRotator};
use cf_primitives::EpochIndex;
use sp_std::collections::btree_set::BTreeSet;

use super::MockPallet;

const ROTATION_OUTCOME: &[u8] = b"ROTATION_OUTCOME";

macro_rules! mock_key_rotator {
	($rotator_name:ident) => {
		pub struct $rotator_name;

		impl MockPallet for $rotator_name {
			const PREFIX: &'static [u8] = stringify!($rotator_name).as_bytes();
		}

		impl $rotator_name {
			pub fn keygen_success() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Ready(
						KeyRotationStatusOuter::KeygenComplete,
					),
				);
			}

			pub fn key_handover_success() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Ready(
						KeyRotationStatusOuter::KeyHandoverComplete,
					),
				);
			}

			pub fn keys_activated() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Ready(
						KeyRotationStatusOuter::RotationComplete,
					),
				);
			}

			pub fn failed<O: IntoIterator<Item = u64>>(offenders: O) {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Ready(
						KeyRotationStatusOuter::Failed(offenders.into_iter().collect()),
					),
				);
			}

			pub fn pending() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Pending,
				)
			}
		}

		impl KeyRotator for $rotator_name {
			type ValidatorId = u64;

			fn keygen(_candidates: BTreeSet<Self::ValidatorId>, _new_epoch_index: EpochIndex) {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Pending,
				);
			}

			fn key_handover(
				_old_participants: BTreeSet<Self::ValidatorId>,
				_new_candidates: BTreeSet<Self::ValidatorId>,
				_epoch_index: EpochIndex,
			) {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Pending,
				);
			}

			fn status() -> AsyncResult<KeyRotationStatusOuter<Self::ValidatorId>> {
				Self::get_value(ROTATION_OUTCOME).unwrap_or_default()
			}

			fn activate_keys() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Pending,
				);
			}

			fn reset_key_rotation() {
				Self::put_value(ROTATION_OUTCOME, AsyncResult::<KeyRotationStatusOuter<u64>>::Void);
			}

			#[cfg(feature = "runtime-benchmarks")]
			fn set_status(_outcome: AsyncResult<KeyRotationStatusOuter<Self::ValidatorId>>) {
				unimplemented!()
			}
		}
	};
}

mock_key_rotator!(MockKeyRotatorA);
mock_key_rotator!(MockKeyRotatorB);
mock_key_rotator!(MockKeyRotatorC);
