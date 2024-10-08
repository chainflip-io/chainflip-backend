#![cfg(test)]

use std::cell::RefCell;

use crate::{
	self as pallet_cf_broadcast, ChainBlockNumberFor, Instance1, PalletOffence, PalletSafeMode,
};
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
		liability_tracker::MockLiabilityTracker, signer_nomination::MockNominator,
		threshold_signer::MockThresholdSigner,
	},
	AccountRoleRegistry, DummyEgressSuccessWitnesser, OnBroadcastReady,
};
use codec::{Decode, Encode};
use frame_support::{derive_impl, parameter_types, traits::UnfilteredDispatchable};
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
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
	type EnsureThresholdSigned = FailOnNoneOrigin<Self>;
	type WeightInfo = ();
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = MockCallback;
	type SafeMode = MockRuntimeSafeMode;
	type BroadcastReadyProvider = MockBroadcastReadyProvider;
	type SafeModeBlockMargin = ConstU64<10>;
	type SafeModeChainBlockMargin = ConstU64<SAFEMODE_CHAINBLOCK_MARGIN>;
	type ChainTracking = BlockHeightProvider<MockEthereum>;
	type ElectionEgressWitnesser = DummyEgressSuccessWitnesser<MockEthereumChainCrypto>;
	type RetryPolicy = MockRetryPolicy;
	type LiabilityTracker = MockLiabilityTracker;
	type CfeBroadcastRequest = MockCfeInterface;
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
