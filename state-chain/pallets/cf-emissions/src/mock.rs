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
		flip_burn_info::MockFlipBurnInfo,
	},
	Issuance, RewardsDistribution, WaivedFees,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{derive_impl, parameter_types, traits::Imbalance};
use frame_system as system;
use scale_info::TypeInfo;
use sp_arithmetic::Permill;

pub type AccountId = u64;

pub const FLIP_TO_BURN: u128 = 10_000;
pub const SUPPLY_UPDATE_INTERVAL: u32 = 10;
pub const TOTAL_ISSUANCE: u128 = 1_000_000_000;
pub const DAILY_SLASHING_RATE: Permill = Permill::from_perthousand(1);

cf_traits::impl_mock_on_account_funded!(AccountId, u128);
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
	type Balance = u128;
	type BlocksPerDay = BlocksPerDay;
	type OnAccountFunded = MockOnAccountFunded;
	type WeightInfo = ();
	type WaivedFees = WaivedFeesMock;
	type CallIndexer = ();
}

pub const EMISSION_RATE: u128 = 10;
pub struct MockRewardsDistribution;

impl RewardsDistribution for MockRewardsDistribution {
	type Balance = u128;
	type Issuance = pallet_cf_flip::FlipIssuance<Test>;

	fn distribute() {
		let deposit =
			Flip::deposit_reserves(*b"RSVR", Emissions::current_authority_emission_per_block());
		let amount = deposit.peek();
		let _result = deposit.offset(Self::Issuance::mint(amount));
	}
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockUpdateFlipSupply {
	pub new_total_supply: u128,
	pub block_number: u64,
}

impl UpdateFlipSupply<MockEthereumChainCrypto> for MockUpdateFlipSupply {
	fn new_unsigned(new_total_supply: u128, block_number: u64) -> Self {
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

impl pallet_cf_emissions::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type HostChain = MockEthereum;
	type FlipBalance = FlipBalance;
	type ApiCall = MockUpdateFlipSupply;
	type Surplus = pallet_cf_flip::Surplus<Test>;
	type Issuance = pallet_cf_flip::FlipIssuance<Test>;
	type RewardsDistribution = MockRewardsDistribution;
	type CompoundingInterval = HeartbeatBlockInterval;
	type EthEnvironment = MockStateChainGatewayProvider;
	type Broadcaster = MockEmissionsBroadcaster;
	type FlipToBurn = MockFlipBurnInfo;
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
				backup_node_emission_inflation: 284,
				supply_update_interval: SUPPLY_UPDATE_INTERVAL,
				..Default::default()
			}
		},
	},
	|| {
		MockEpochInfo::add_authorities(1);
		MockEpochInfo::add_authorities(2);
		MockFlipBurnInfo::set_flip_to_burn(FLIP_TO_BURN);
	}
}
