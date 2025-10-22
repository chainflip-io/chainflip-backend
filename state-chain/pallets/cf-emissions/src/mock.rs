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

use crate::{self as pallet_cf_emissions, PalletSafeMode};
use cf_chains::{
	eth::api::StateChainGatewayAddressProvider,
	mocks::{MockEthereum, MockEthereumChainCrypto},
	ApiCall, ChainCrypto, Ethereum, UpdateFlipSupply,
};
use cf_primitives::FlipBalance;
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode, impl_mock_waived_fees,
	mocks::{
		broadcaster::MockBroadcaster, egress_handler::MockEgressHandler,
		flip_burn_info::MockFlipBurnOrMoveInfo,
	},
	Issuance, RewardsDistribution, WaivedFees,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{derive_impl, parameter_types};
use frame_system::{self as system};
use scale_info::TypeInfo;
use sp_arithmetic::Permill;

pub type AccountId = u64;

pub const FLIP_TO_BURN: FlipBalance = 10_000;
pub const SUPPLY_UPDATE_INTERVAL: u32 = 10;
pub const TOTAL_ISSUANCE: FlipBalance = 1_000_000_000;
pub const DAILY_SLASHING_RATE: Permill = Permill::from_perthousand(1);

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Flip: pallet_cf_flip,
		Emissions: pallet_cf_emissions,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type Block = Block;
}

impl_mock_chainflip!(Test);

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

parameter_types! {
	pub const HeartbeatBlockInterval: u64 = 150;
}

// Implement mock for RestrictionHandler
impl_mock_waived_fees!(AccountId, RuntimeCall);

impl pallet_cf_flip::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = FlipBalance;
	type BlocksPerDay = BlocksPerDay;
	type WeightInfo = ();
	type WaivedFees = WaivedFeesMock;
	type CallIndexer = ();
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockUpdateFlipSupply {
	pub new_total_supply: FlipBalance,
	pub block_number: u64,
}

impl UpdateFlipSupply<MockEthereumChainCrypto> for MockUpdateFlipSupply {
	fn new_unsigned(new_total_supply: FlipBalance, block_number: u64) -> Self {
		Self { new_total_supply, block_number }
	}
}

impl ApiCall<MockEthereumChainCrypto> for MockUpdateFlipSupply {
	fn threshold_signature_payload(&self) -> <MockEthereumChainCrypto as ChainCrypto>::Payload {
		[0xcf; 4]
	}

	fn signed(
		self,
		_threshold_signature: &<MockEthereumChainCrypto as ChainCrypto>::ThresholdSignature,
		_signer: <MockEthereumChainCrypto as ChainCrypto>::AggKey,
	) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}

	fn transaction_out_id(&self) -> <MockEthereumChainCrypto as ChainCrypto>::TransactionOutId {
		unimplemented!()
	}

	fn refresh_replay_protection(&mut self) {
		unimplemented!()
	}

	fn signer(&self) -> Option<<MockEthereumChainCrypto as ChainCrypto>::AggKey> {
		unimplemented!()
	}
}

pub struct MockStateChainGatewayProvider;

impl StateChainGatewayAddressProvider for MockStateChainGatewayProvider {
	fn state_chain_gateway_address() -> cf_chains::eth::Address {
		[0xcc; 20].into()
	}
}

impl_mock_runtime_safe_mode! { emissions: PalletSafeMode }

pub type MockEmissionsBroadcaster = MockBroadcaster<(MockUpdateFlipSupply, RuntimeCall)>;

// The Emissions pallet has access to the Flip pallet, so we don't need to mock it.
pub struct FlipDistribution;
impl RewardsDistribution for FlipDistribution {
	type Balance = FlipBalance;
	type AccountId = AccountId;

	fn distribute(reward_amount: Self::Balance, beneficiary: &Self::AccountId) {
		pallet_cf_flip::FlipIssuance::<Test>::mint(beneficiary, reward_amount);
	}
}

impl pallet_cf_emissions::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type HostChain = MockEthereum;
	type FlipBalance = FlipBalance;
	type ApiCall = MockUpdateFlipSupply;
	type Issuance = pallet_cf_flip::FlipIssuance<Test>;
	type CompoundingInterval = HeartbeatBlockInterval;
	type EthEnvironment = MockStateChainGatewayProvider;
	type RewardsDistribution = FlipDistribution;
	type Broadcaster = MockEmissionsBroadcaster;
	type FlipToBurn = MockFlipBurnOrMoveInfo;
	type SafeMode = MockRuntimeSafeMode;
	type EgressHandler = MockEgressHandler<Ethereum>;
	type WeightInfo = ();
}

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		flip: FlipConfig { total_issuance: TOTAL_ISSUANCE, daily_slashing_rate: DAILY_SLASHING_RATE },
		emissions: {
			EmissionsConfig {
				current_authority_emission_inflation: 2720,
				supply_update_interval: SUPPLY_UPDATE_INTERVAL,
				..Default::default()
			}
		},
	},
	|| {
		MockEpochInfo::add_authorities(1);
		MockEpochInfo::add_authorities(2);
		MockFlipBurnOrMoveInfo::set_flip_to_burn(FLIP_TO_BURN);
	}
}
