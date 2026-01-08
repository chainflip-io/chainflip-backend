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

use core::marker::PhantomData;
use std::collections::BTreeSet;

use crate::{self as pallet_cf_environment, Decode, Encode, TypeInfo};
use cf_chains::{
	btc::{BitcoinCrypto, BitcoinFeeInfo},
	eth,
	sol::{
		api::{
			AllNonceAccounts, AltWitnessingConsensusResult, ApiEnvironment, ComputePrice,
			CurrentAggKey, CurrentOnChainKey, DurableNonce, DurableNonceAndAccount,
			RecoverDurableNonce, SolanaApi, SolanaEnvironment,
		},
		SolAddress, SolAddressLookupTableAccount, SolAmount, SolApiEnvironment, SolHash,
	},
	Arbitrum, Assethub, Bitcoin, Chain, ChainEnvironment, Polkadot, Solana,
};
use cf_primitives::{BroadcastId, SemVer, ThresholdSignatureRequestId};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode, impl_pallet_safe_mode,
	mocks::key_provider::MockKeyProvider, Broadcaster, GetBitcoinFeeInfo, VaultKeyWitnessedHandler,
};
use frame_support::{
	derive_impl,
	pallet_prelude::{InvalidTransaction, TransactionValidityError},
	parameter_types,
	traits::UnfilteredDispatchable,
	DebugNoBound, DefaultNoBound,
};
use sp_core::{H160, H256};
use sp_runtime::{
	traits::{DispatchInfoOf, SignedExtension},
	DispatchError,
};

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Environment: pallet_cf_environment,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl_mock_chainflip!(Test);

pub struct MockPolkadotVaultKeyWitnessedHandler;
impl VaultKeyWitnessedHandler<Polkadot> for MockPolkadotVaultKeyWitnessedHandler {
	fn on_first_key_activated(
		_block_number: <Polkadot as Chain>::ChainBlockNumber,
	) -> frame_support::pallet_prelude::DispatchResult {
		Ok(())
	}
}

pub struct MockAssethubVaultKeyWitnessedHandler;
impl VaultKeyWitnessedHandler<Assethub> for MockAssethubVaultKeyWitnessedHandler {
	fn on_first_key_activated(
		_block_number: <Assethub as Chain>::ChainBlockNumber,
	) -> frame_support::pallet_prelude::DispatchResult {
		Ok(())
	}
}

pub struct MockBitcoinVaultKeyWitnessedHandler;
impl VaultKeyWitnessedHandler<Bitcoin> for MockBitcoinVaultKeyWitnessedHandler {
	fn on_first_key_activated(
		_block_number: <Bitcoin as Chain>::ChainBlockNumber,
	) -> frame_support::pallet_prelude::DispatchResult {
		Ok(())
	}
}

pub struct MockArbitrumVaultKeyWitnessedHandler;
impl VaultKeyWitnessedHandler<Arbitrum> for MockArbitrumVaultKeyWitnessedHandler {
	fn on_first_key_activated(
		_block_number: <Arbitrum as Chain>::ChainBlockNumber,
	) -> frame_support::pallet_prelude::DispatchResult {
		Ok(())
	}
}

pub struct MockSolanaVaultKeyWitnessedHandler;
impl VaultKeyWitnessedHandler<Solana> for MockSolanaVaultKeyWitnessedHandler {
	fn on_first_key_activated(
		_block_number: <Solana as Chain>::ChainBlockNumber,
	) -> frame_support::pallet_prelude::DispatchResult {
		Ok(())
	}
}

parameter_types! {
	pub CurrentReleaseVersion: SemVer = SemVer {
		major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap(),
		minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap(),
		patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().unwrap(),
	};
}

pub struct MockBitcoinFeeInfo;
impl GetBitcoinFeeInfo for MockBitcoinFeeInfo {
	fn bitcoin_fee_info() -> BitcoinFeeInfo {
		BitcoinFeeInfo::new(10 * 1000)
	}
}

parameter_types! {
	pub static SolanaCallBroadcasted: Option<SolanaApi<MockSolEnvironment>> = None;
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockSolEnvironment;
impl ChainEnvironment<ApiEnvironment, SolApiEnvironment> for MockSolEnvironment {
	fn lookup(_s: ApiEnvironment) -> Option<SolApiEnvironment> {
		Some(SolApiEnvironment {
			vault_program: SolAddress([0x00; 32]),
			vault_program_data_account: SolAddress([0x00; 32]),
			token_vault_pda_account: SolAddress([0x00; 32]),
			usdc_token_mint_pubkey: SolAddress([0x00; 32]),
			usdc_token_vault_ata: SolAddress([0x00; 32]),
			swap_endpoint_program: SolAddress([0x00; 32]),
			swap_endpoint_program_data_account: SolAddress([0x00; 32]),
			alt_manager_program: SolAddress([0x00; 32]),
			address_lookup_table_account: SolAddressLookupTableAccount {
				key: SolAddress([0x00; 32]).into(),
				addresses: vec![],
			},
		})
	}
}
impl ChainEnvironment<CurrentAggKey, SolAddress> for MockSolEnvironment {
	fn lookup(_s: CurrentAggKey) -> Option<SolAddress> {
		Some(SolAddress([0x00; 32]))
	}
}
impl ChainEnvironment<CurrentOnChainKey, SolAddress> for MockSolEnvironment {
	fn lookup(_s: CurrentOnChainKey) -> Option<SolAddress> {
		Some(SolAddress([0x00; 32]))
	}
}
impl ChainEnvironment<ComputePrice, SolAmount> for MockSolEnvironment {
	fn lookup(_s: ComputePrice) -> Option<u64> {
		Some(0u64)
	}
}
impl ChainEnvironment<DurableNonce, DurableNonceAndAccount> for MockSolEnvironment {
	fn lookup(_s: DurableNonce) -> Option<DurableNonceAndAccount> {
		Some((SolAddress([0x00; 32]), SolHash([0x00; 32])))
	}
}
impl ChainEnvironment<AllNonceAccounts, Vec<DurableNonceAndAccount>> for MockSolEnvironment {
	fn lookup(_s: AllNonceAccounts) -> Option<Vec<DurableNonceAndAccount>> {
		Some(vec![(SolAddress([0x00; 32]), SolHash([0x00; 32]))])
	}
}
impl RecoverDurableNonce for MockSolEnvironment {
	fn recover_durable_nonce(_nonce_account: SolAddress) {
		unimplemented!();
	}
}

impl
	ChainEnvironment<
		BTreeSet<SolAddress>,
		AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>,
	> for MockSolEnvironment
{
	fn lookup(
		_alts: BTreeSet<SolAddress>,
	) -> Option<AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>> {
		None
	}
}

impl SolanaEnvironment for MockSolEnvironment {}

pub struct MockSolanaBroadcaster<C>(PhantomData<C>);
impl<C: UnfilteredDispatchable> Broadcaster<Solana> for MockSolanaBroadcaster<C> {
	type ApiCall = cf_chains::sol::api::SolanaApi<MockSolEnvironment>;
	type Callback = C;

	fn threshold_sign_and_broadcast(
		api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		SolanaCallBroadcasted::set(Some(api_call));
		(1, 2)
	}

	fn threshold_sign_and_broadcast_with_callback(
		_api_call: Self::ApiCall,
		_success_callback: Option<Self::Callback>,
		_failed_callback_generator: impl FnOnce(BroadcastId) -> Option<Self::Callback>,
	) -> BroadcastId {
		unimplemented!()
	}

	fn threshold_sign_and_broadcast_rotation_tx(
		_api_call: Self::ApiCall,
		_new_key: SolAddress,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		unimplemented!()
	}

	fn re_sign_broadcast(
		_broadcast_id: BroadcastId,
		_request_broadcast: bool,
		_refresh_replay_protection: bool,
	) -> Result<ThresholdSignatureRequestId, DispatchError> {
		unimplemented!()
	}

	fn threshold_sign(_api_call: Self::ApiCall) -> (BroadcastId, ThresholdSignatureRequestId) {
		unimplemented!()
	}

	fn expire_broadcast(_broadcast_id: BroadcastId) {
		unimplemented!()
	}
}

impl_pallet_safe_mode!(MockPalletSafeMode; flag1, flag2);
impl_mock_runtime_safe_mode!(mock: MockPalletSafeMode);

pub type MockBitcoinKeyProvider = MockKeyProvider<BitcoinCrypto>;

/// A Mock payment extension that simply checks whether the account exists in the system.
#[derive(Clone, DebugNoBound, DefaultNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct MockPayment<T>(PhantomData<T>);

impl<T: frame_system::Config + Send + Sync + 'static> SignedExtension for MockPayment<T> {
	type AccountId = T::AccountId;
	type AdditionalSigned = ();
	type Call = T::RuntimeCall;
	type Pre = ();
	const IDENTIFIER: &'static str = "UnitSignedExtension";
	fn additional_signed(&self) -> Result<(), TransactionValidityError> {
		Ok(())
	}
	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		if frame_system::Account::<T>::contains_key(who) {
			Ok(())
		} else {
			Err(TransactionValidityError::Invalid(InvalidTransaction::Payment))
		}
	}
}

impl pallet_cf_environment::Config for Test {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type PolkadotVaultKeyWitnessedHandler = MockPolkadotVaultKeyWitnessedHandler;
	type BitcoinVaultKeyWitnessedHandler = MockBitcoinVaultKeyWitnessedHandler;
	type ArbitrumVaultKeyWitnessedHandler = MockArbitrumVaultKeyWitnessedHandler;
	type SolanaVaultKeyWitnessedHandler = MockSolanaVaultKeyWitnessedHandler;
	type AssethubVaultKeyWitnessedHandler = MockAssethubVaultKeyWitnessedHandler;
	type SolanaNonceWatch = ();
	type BitcoinFeeInfo = MockBitcoinFeeInfo;
	type BitcoinKeyProvider = MockBitcoinKeyProvider;
	type RuntimeSafeMode = MockRuntimeSafeMode;
	type CurrentReleaseVersion = CurrentReleaseVersion;
	type SolEnvironment = MockSolEnvironment;
	type SolanaBroadcaster = MockSolanaBroadcaster<RuntimeCall>;
	type TransactionPayments = MockPayment<Self>;
	type GetTransactionPayments = ();
	type WeightInfo = ();
}

pub const STATE_CHAIN_GATEWAY_ADDRESS: eth::Address = H160([0u8; 20]);
pub const ETH_KEY_MANAGER_ADDRESS: eth::Address = H160([1u8; 20]);
pub const ETH_VAULT_ADDRESS: eth::Address = H160([2u8; 20]);
pub const ETH_ADDRESS_CHECKER_ADDRESS: eth::Address = H160([3u8; 20]);
pub const ETH_CHAIN_ID: u64 = 1;

pub const ARB_KEY_MANAGER_ADDRESS: eth::Address = H160([4u8; 20]);
pub const ARB_VAULT_ADDRESS: eth::Address = H160([5u8; 20]);
pub const ARB_USDC_TOKEN_ADDRESS: eth::Address = H160([6u8; 20]);
pub const ARB_USDT_TOKEN_ADDRESS: eth::Address = H160([7u8; 20]);
pub const ARB_ADDRESS_CHECKER_ADDRESS: eth::Address = H160([8u8; 20]);
pub const ARB_CHAIN_ID: u64 = 2;

pub const ETH_SC_UTILS_ADDRESS: eth::Address = H160([9u8; 20]);

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		environment: EnvironmentConfig {
			state_chain_gateway_address: STATE_CHAIN_GATEWAY_ADDRESS,
			eth_key_manager_address: ETH_KEY_MANAGER_ADDRESS,
			ethereum_chain_id: ETH_CHAIN_ID,
			eth_vault_address: ETH_VAULT_ADDRESS,
			eth_address_checker_address: ETH_ADDRESS_CHECKER_ADDRESS,
			eth_sc_utils_address: ETH_SC_UTILS_ADDRESS,
			arb_key_manager_address: ARB_KEY_MANAGER_ADDRESS,
			arb_vault_address: ARB_VAULT_ADDRESS,
			arb_address_checker_address: ARB_ADDRESS_CHECKER_ADDRESS,
			arb_usdc_address: ARB_USDC_TOKEN_ADDRESS,
			arb_usdt_address: ARB_USDT_TOKEN_ADDRESS,
			arbitrum_chain_id: ARB_CHAIN_ID,
			flip_token_address: [0u8; 20].into(),
			eth_usdc_address: [0x2; 20].into(),
			eth_usdt_address: [0x2; 20].into(),
			eth_wbtc_address: [0x2; 20].into(),
			polkadot_genesis_hash: H256([0u8; 32]),
			polkadot_vault_account_id: None,
			assethub_genesis_hash: H256([0u8; 32]),
			assethub_vault_account_id: None,
			sol_genesis_hash: None,
			..Default::default()
		},
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarks_mock {
	use cf_traits::WaivedFees;
	use sp_core::ConstU64;

	use super::*;

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
	impl frame_system::Config for BenchmarksTest {
		type Block = Block;
	}

	// Configure a mock runtime to test the pallet.
	frame_support::construct_runtime!(
		pub enum BenchmarksTest {
			System: frame_system,
			Environment: pallet_cf_environment,
			Flip: pallet_cf_flip,
		}
	);

	impl_mock_chainflip!(BenchmarksTest);

	impl pallet_cf_environment::Config for BenchmarksTest {
		type RuntimeOrigin = RuntimeOrigin;
		type RuntimeCall = RuntimeCall;
		type RuntimeEvent = RuntimeEvent;
		type PolkadotVaultKeyWitnessedHandler = MockPolkadotVaultKeyWitnessedHandler;
		type BitcoinVaultKeyWitnessedHandler = MockBitcoinVaultKeyWitnessedHandler;
		type ArbitrumVaultKeyWitnessedHandler = MockArbitrumVaultKeyWitnessedHandler;
		type SolanaVaultKeyWitnessedHandler = MockSolanaVaultKeyWitnessedHandler;
		type AssethubVaultKeyWitnessedHandler = MockAssethubVaultKeyWitnessedHandler;
		type SolanaNonceWatch = ();
		type BitcoinFeeInfo = MockBitcoinFeeInfo;
		type BitcoinKeyProvider = MockBitcoinKeyProvider;
		type RuntimeSafeMode = MockRuntimeSafeMode;
		type CurrentReleaseVersion = CurrentReleaseVersion;
		type SolEnvironment = MockSolEnvironment;
		type SolanaBroadcaster = MockSolanaBroadcaster<RuntimeCall>;
		type TransactionPayments = MockPayment<Self>;
		type GetTransactionPayments = ();
		type WeightInfo = ();
	}

	pub struct MockWaivedFees;
	impl WaivedFees for MockWaivedFees {
		type AccountId = u64;
		type RuntimeCall = RuntimeCall;
		fn should_waive_fees(_call: &Self::RuntimeCall, _caller: &Self::AccountId) -> bool {
			false
		}
	}

	impl pallet_cf_flip::Config for BenchmarksTest {
		type RuntimeEvent = RuntimeEvent;
		type Balance = u128;
		type BlocksPerDay = ConstU64<14400>;
		type WeightInfo = ();
		type WaivedFees = MockWaivedFees;
		type CallIndexer = ();
	}

	cf_test_utilities::impl_test_helpers! {
		BenchmarksTest,
		RuntimeGenesisConfig {
			system: Default::default(),
			environment: EnvironmentConfig {
				state_chain_gateway_address: STATE_CHAIN_GATEWAY_ADDRESS,
				eth_key_manager_address: ETH_KEY_MANAGER_ADDRESS,
				ethereum_chain_id: ETH_CHAIN_ID,
				eth_vault_address: ETH_VAULT_ADDRESS,
				eth_address_checker_address: ETH_ADDRESS_CHECKER_ADDRESS,
				eth_sc_utils_address: ETH_SC_UTILS_ADDRESS,
				arb_key_manager_address: ARB_KEY_MANAGER_ADDRESS,
				arb_vault_address: ARB_VAULT_ADDRESS,
				arb_address_checker_address: ARB_ADDRESS_CHECKER_ADDRESS,
				arb_usdc_address: ARB_USDC_TOKEN_ADDRESS,
				arbitrum_chain_id: ARB_CHAIN_ID,
				flip_token_address: [0u8; 20].into(),
				eth_usdc_address: [0x2; 20].into(),
				eth_usdt_address: [0x2; 20].into(),
				polkadot_genesis_hash: H256([0u8; 32]),
				polkadot_vault_account_id: None,
				assethub_genesis_hash: H256([0u8; 32]),
				assethub_vault_account_id: None,
				sol_genesis_hash: None,
				..Default::default()
			},
			flip: Default::default(),
		},
	}
}
