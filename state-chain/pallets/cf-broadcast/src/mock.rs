#![cfg(test)]

use std::cell::RefCell;

use crate::{self as pallet_cf_broadcast, Instance1, PalletOffence, PalletSafeMode};
use cf_chains::{
	eth::Ethereum,
	evm::EvmCrypto,
	mocks::{MockApiCall, MockEthereum, MockEthereumChainCrypto, MockTransactionBuilder},
	Chain, ChainCrypto, RetryPolicy,
};
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode,
	mocks::{
		block_height_provider::BlockHeightProvider, cfe_interface_mock::MockCfeInterface,
		signer_nomination::MockNominator, threshold_signer::MockThresholdSigner,
	},
	AccountRoleRegistry, OnBroadcastReady,
};
use codec::{Decode, Encode};
use frame_support::{parameter_types, traits::UnfilteredDispatchable};
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use sp_core::{ConstU64, H256};
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Broadcaster: pallet_cf_broadcast::<Instance1>,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

thread_local! {
	pub static VALIDKEY: std::cell::RefCell<bool> = RefCell::new(true);
}

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<5>;
}

pub const BROADCAST_EXPIRY_BLOCKS: BlockNumberFor<Test> = 4;

parameter_types! {
	pub const BroadcastTimeout: BlockNumberFor<Test> = BROADCAST_EXPIRY_BLOCKS;
}

pub type MockOffenceReporter =
	cf_traits::mocks::offence_reporting::MockOffenceReporter<u64, PalletOffence>;

thread_local! {
	pub static SIGNATURE_REQUESTS: RefCell<Vec<<<Ethereum as Chain>::ChainCrypto as ChainCrypto>::Payload>> = RefCell::new(vec![]);
	pub static CALLBACK_CALLED: RefCell<bool> = RefCell::new(false);
	pub static VALID_METADATA: RefCell<bool> = RefCell::new(true);
}

pub type EthMockThresholdSigner = MockThresholdSigner<EvmCrypto, crate::mock::RuntimeCall>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockCallback;

impl MockCallback {
	pub fn was_called() -> bool {
		CALLBACK_CALLED.with(|cell| *cell.borrow())
	}
}

impl UnfilteredDispatchable for MockCallback {
	type RuntimeOrigin = RuntimeOrigin;

	fn dispatch_bypass_filter(
		self,
		_origin: Self::RuntimeOrigin,
	) -> frame_support::pallet_prelude::DispatchResultWithPostInfo {
		CALLBACK_CALLED.with(|cell| *cell.borrow_mut() = true);
		Ok(().into())
	}
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

impl pallet_cf_broadcast::Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Offence = PalletOffence;
	type TargetChain = MockEthereum;
	type ApiCall = MockApiCall<MockEthereumChainCrypto>;
	type TransactionBuilder = MockTransactionBuilder<Self::TargetChain, Self::ApiCall>;
	type ThresholdSigner = MockThresholdSigner<MockEthereumChainCrypto, RuntimeCall>;
	type BroadcastSignerNomination = MockNominator;
	type OffenceReporter = MockOffenceReporter;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type BroadcastTimeout = BroadcastTimeout;
	type WeightInfo = ();
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = MockCallback;
	type SafeMode = MockRuntimeSafeMode;
	type BroadcastReadyProvider = MockBroadcastReadyProvider;
	type SafeModeBlockMargin = ConstU64<10>;
	type ChainTracking = BlockHeightProvider<MockEthereum>;
	type RetryPolicy = MockRetryPolicy;
	type CfeBroadcastRequest = MockCfeInterface;
}

impl_mock_chainflip!(Test);
cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
		MockEpochInfo::next_epoch((0..151).collect());
		MockNominator::use_current_authorities_as_nominees::<MockEpochInfo>();
		for id in &MockEpochInfo::current_authorities() {
			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(id).unwrap();
		}
	}
}
