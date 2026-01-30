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

use super::*;
use crate as pallet_cf_vaults;
use cf_chains::{
	mocks::{MockEthereum, MockEthereumChainCrypto},
	ApiCall, SetAggKeyWithAggKeyError,
};
use cf_traits::{
	impl_mock_chainflip,
	mocks::{
		block_height_provider::BlockHeightProvider, broadcaster::MockBroadcaster,
		cfe_interface_mock::MockCfeInterface,
	},
};
use frame_support::{construct_runtime, derive_impl, parameter_types};

thread_local! {
	pub static SET_AGG_KEY_WITH_AGG_KEY_REQUIRED: RefCell<bool> = const { RefCell::new(true) };
}

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub struct Test {
		System: frame_system,
		VaultsPallet: pallet_cf_vaults,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl_mock_chainflip!(Test);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockSetAggKeyWithAggKey {
	old_key: <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	new_key: <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::AggKey,
}

impl MockSetAggKeyWithAggKey {
	pub fn set_required(required: bool) {
		SET_AGG_KEY_WITH_AGG_KEY_REQUIRED.with(|cell| {
			*cell.borrow_mut() = required;
		});
	}
}

impl SetAggKeyWithAggKey<MockEthereumChainCrypto> for MockSetAggKeyWithAggKey {
	fn new_unsigned(
		old_key: Option<<<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::AggKey>,
		new_key: <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	) -> Result<Option<Self>, SetAggKeyWithAggKeyError> {
		if !SET_AGG_KEY_WITH_AGG_KEY_REQUIRED.with(|cell| *cell.borrow()) {
			return Ok(None)
		}

		Ok(Some(Self { old_key: old_key.ok_or(SetAggKeyWithAggKeyError::Failed)?, new_key }))
	}

	fn new_unsigned_impl(
		old_key: Option<<<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::AggKey>,
		new_key: <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	) -> Result<Option<Self>, SetAggKeyWithAggKeyError> {
		Self::new_unsigned(old_key, new_key)
	}
}

impl ApiCall<MockEthereumChainCrypto> for MockSetAggKeyWithAggKey {
	fn threshold_signature_payload(
		&self,
	) -> <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(
		self,
		_threshold_signature: &<<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature,
		_signer: <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::AggKey,
	) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}

	fn transaction_out_id(
		&self,
	) -> <<MockEthereum as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId {
		unimplemented!()
	}

	fn refresh_replay_protection(&mut self) {
		unimplemented!()
	}

	fn signer(&self) -> Option<<MockEthereumChainCrypto as ChainCrypto>::AggKey> {
		unimplemented!()
	}
}

parameter_types! {
	pub const KeygenResponseGracePeriod: u64 = 25;
}

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum MockRuntimeSafeMode {
	CodeRed,
	CodeGreen,
}

impl SafeMode for MockRuntimeSafeMode {
	fn code_green() -> Self {
		MockRuntimeSafeMode::CodeGreen
	}
	fn code_red() -> Self {
		MockRuntimeSafeMode::CodeRed
	}
}

thread_local! {
	pub static SAFE_MODE: RefCell<MockRuntimeSafeMode> = const { RefCell::new(MockRuntimeSafeMode::CodeGreen) };
}

//pub struct MockRuntimeSafeMode;
impl SetSafeMode<MockRuntimeSafeMode> for MockRuntimeSafeMode {
	fn set_safe_mode(mode: MockRuntimeSafeMode) {
		SAFE_MODE.with(|safe_mode| *(safe_mode.borrow_mut()) = mode);
	}
}

impl Get<MockRuntimeSafeMode> for MockRuntimeSafeMode {
	fn get() -> MockRuntimeSafeMode {
		SAFE_MODE.with(|safe_mode| safe_mode.borrow().clone())
	}
}

impl pallet_cf_vaults::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = MockEthereum;
	type SetAggKeyWithAggKey = MockSetAggKeyWithAggKey;
	type WeightInfo = ();
	type Broadcaster = MockBroadcaster<(MockSetAggKeyWithAggKey, RuntimeCall)>;
	type SafeMode = MockRuntimeSafeMode;
	type ChainTracking = BlockHeightProvider<MockEthereum>;
	type CfeMultisigRequest = MockCfeInterface;
}

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		vaults_pallet: VaultsPalletConfig {
			deployment_block: Some(0),
			chain_initialized: true,
		},
	},
	|| {},
}

pub(crate) fn new_test_ext_no_key() -> TestRunner<()> {
	TestRunner::<()>::new(RuntimeGenesisConfig::default())
}
