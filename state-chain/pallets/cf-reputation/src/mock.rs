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

use super::*;
use crate::{self as pallet_cf_reputation, PalletSafeMode};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode, mocks::flip_slasher::MockFlipSlasher,
	AccountRoleRegistry,
};
use frame_support::{assert_ok, construct_runtime, derive_impl, parameter_types};
use serde::{Deserialize, Serialize};
use sp_std::cell::RefCell;

type Block = frame_system::mocking::MockBlock<Test>;

type ValidatorId = u64;

thread_local! {
	pub static HEARTBEATS: RefCell<u32> = RefCell::new(Default::default());
}

construct_runtime!(
	pub struct Test {
		System: frame_system,
		ReputationPallet: pallet_cf_reputation,
	}
);

impl_mock_runtime_safe_mode! { reputation: PalletSafeMode }

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = ValidatorId;
	type Block = Block;
}

impl_mock_chainflip!(Test);

// A heartbeat interval in blocks
pub const HEARTBEAT_BLOCK_INTERVAL: u64 = 150;
pub const REPUTATION_PER_HEARTBEAT: ReputationPoints = 10;

pub const ACCRUAL_RATIO: (i32, u64) = (REPUTATION_PER_HEARTBEAT, HEARTBEAT_BLOCK_INTERVAL);

pub const MAX_ACCRUABLE_REPUTATION: ReputationPoints = 25;

pub const MISSED_HEARTBEAT_PENALTY_POINTS: ReputationPoints = 2;
pub const GRANDPA_EQUIVOCATION_PENALTY_POINTS: ReputationPoints = 50;
pub const GRANDPA_SUSPENSION_DURATION: u64 = HEARTBEAT_BLOCK_INTERVAL * 10;

parameter_types! {
	pub const HeartbeatBlockInterval: u64 = HEARTBEAT_BLOCK_INTERVAL;
	pub const ReputationPointFloorAndCeiling: (i32, i32) = (-2880, 2880);
	pub const MaximumAccruableReputation: ReputationPoints = MAX_ACCRUABLE_REPUTATION;
}

pub const ALICE: <Test as frame_system::Config>::AccountId = 100u64;
pub const BOB: <Test as frame_system::Config>::AccountId = 200u64;

#[derive(
	Serialize,
	Deserialize,
	Copy,
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum AllOffences {
	MissedHeartbeat,
	NotLockingYourComputer,
	ForgettingYourYubiKey,
	UpsettingGrandpa,
}

impl From<PalletOffence> for AllOffences {
	fn from(o: PalletOffence) -> Self {
		match o {
			PalletOffence::MissedHeartbeat => AllOffences::MissedHeartbeat,
		}
	}
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Offence = AllOffences;
	type HeartbeatBlockInterval = HeartbeatBlockInterval;
	type ReputationPointFloorAndCeiling = ReputationPointFloorAndCeiling;
	type Slasher = MockFlipSlasher<Test>;
	type WeightInfo = ();
	type MaximumAccruableReputation = MaximumAccruableReputation;
	type SafeMode = MockRuntimeSafeMode;
}

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		reputation_pallet: ReputationPalletConfig {
			accrual_ratio: ACCRUAL_RATIO,
			penalties: vec![
				(AllOffences::MissedHeartbeat, (MISSED_HEARTBEAT_PENALTY_POINTS, 0)),
				(AllOffences::ForgettingYourYubiKey, (15, HEARTBEAT_BLOCK_INTERVAL)),
				(AllOffences::NotLockingYourComputer, (15, HEARTBEAT_BLOCK_INTERVAL)),
				(
					AllOffences::UpsettingGrandpa,
					(GRANDPA_EQUIVOCATION_PENALTY_POINTS, GRANDPA_SUSPENSION_DURATION),
				),
			],
			genesis_validators: vec![ALICE],
		},
	},
	||{
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&ALICE));
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&BOB));
		<Test as Chainflip>::EpochInfo::next_epoch(Vec::from([ALICE]));
	}
}
