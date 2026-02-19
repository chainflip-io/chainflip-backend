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

#![cfg(test)]

use std::cell::RefCell;

use crate::{
	self as pallet_cf_broadcast, ChainBlockNumberFor, Instance1, PalletOffence, PalletSafeMode,
};
use cf_chains::{
	eth::Ethereum,
	mocks::{MockApiCall, MockEthereum, MockEthereumChainCrypto, MockTransactionBuilder},
	Chain, ChainCrypto, RetryPolicy,
};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		block_height_provider::BlockHeightProvider,
		broadcast_outcome_handler::MockBroadcastOutcomeHandler,
		cfe_interface_mock::MockCfeInterface, liability_tracker::MockLiabilityTracker,
		signer_nomination::MockNominator, threshold_signer::MockThresholdSigner,
	},
	AccountRoleRegistry, ChainflipWithTargetChain, DummyEgressSuccessWitnesser, OnBroadcastReady,
};
use frame_support::{derive_impl, parameter_types};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::ConstU64;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Broadcaster: pallet_cf_broadcast::<Instance1>,
	}
);

thread_local! {
	pub static VALIDKEY: std::cell::RefCell<bool> = RefCell::new(true);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

pub const BROADCAST_EXPIRY_BLOCKS: BlockNumberFor<Test> = 4;
pub const SAFEMODE_CHAINBLOCK_MARGIN: ChainBlockNumberFor<Test, Instance1> = 10;

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<u64, PalletOffence>;

thread_local! {
	pub static SIGNATURE_REQUESTS: RefCell<Vec<<<Ethereum as Chain>::ChainCrypto as ChainCrypto>::Payload>> = RefCell::new(vec![]);
	pub static VALID_METADATA: RefCell<bool> = RefCell::new(true);
}

pub struct MockBroadcastReadyProvider;
impl OnBroadcastReady<MockEthereum> for MockBroadcastReadyProvider {
	type ApiCall = MockApiCall<MockEthereumChainCrypto>;
}

pub struct MockRetryPolicy;

parameter_types! {
	pub static BroadcastDelay: Option<BlockNumberFor<Test>> = None;
}

impl RetryPolicy for MockRetryPolicy {
	type BlockNumber = u64;
	type AttemptCount = u32;

	fn next_attempt_delay(_retry_attempts: Self::AttemptCount) -> Option<Self::BlockNumber> {
		BroadcastDelay::get()
	}
}

impl_mock_runtime_safe_mode! { broadcast: PalletSafeMode<Instance1> }

impl ChainflipWithTargetChain<Instance1> for Test {
	type TargetChain = MockEthereum;
}

impl pallet_cf_broadcast::Config<Instance1> for Test {
	type RuntimeCall = RuntimeCall;
	type Offence = PalletOffence;
	type ApiCall = MockApiCall<MockEthereumChainCrypto>;
	type TransactionBuilder = MockTransactionBuilder<Self::TargetChain, Self::ApiCall>;
	type ThresholdSigner = MockThresholdSigner<MockEthereumChainCrypto, RuntimeCall>;
	type BroadcastSignerNomination = MockNominator;
	type OffenceReporter = MockOffenceReporter;
	type EnsureThresholdSigned = FailOnNoneOrigin<Self>;
	type WeightInfo = ();
	type RuntimeOrigin = RuntimeOrigin;
	type SafeMode = MockRuntimeSafeMode;
	type BroadcastReadyProvider = MockBroadcastReadyProvider;
	type SafeModeBlockMargin = ConstU64<10>;
	type SafeModeChainBlockMargin = ConstU64<SAFEMODE_CHAINBLOCK_MARGIN>;
	type ChainTracking = BlockHeightProvider<MockEthereum>;
	type ElectionEgressWitnesser = DummyEgressSuccessWitnesser<MockEthereumChainCrypto>;
	type RetryPolicy = MockRetryPolicy;
	type LiabilityTracker = MockLiabilityTracker;
	type CfeBroadcastRequest = MockCfeInterface;
	type BroadcastOutcomeHandler = MockBroadcastOutcomeHandler<MockEthereum>;
}

impl_mock_chainflip!(Test);
cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		broadcaster: pallet_cf_broadcast::GenesisConfig {
			broadcast_timeout: 4,
		},
		..Default::default()
	},
	|| {
		MockEpochInfo::next_epoch((0..151).collect());
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		for id in &MockEpochInfo::current_authorities() {
			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(id).unwrap();
		}
	}
}
