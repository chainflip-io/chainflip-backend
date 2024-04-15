use crate as pallet_cf_funding;
use crate::PalletSafeMode;
use cf_chains::{evm::EvmCrypto, ApiCall, Chain, ChainCrypto, Ethereum};
use cf_primitives::FlipBalance;
use cf_traits::{
	impl_mock_chainflip, impl_mock_runtime_safe_mode, impl_mock_waived_fees,
	mocks::{broadcaster::MockBroadcaster, time_source},
	AccountRoleRegistry, RedemptionCheck, WaivedFees,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{derive_impl, parameter_types};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	AccountId32, DispatchError, DispatchResult, Permill,
};
use std::time::Duration;

// Use a realistic account id for compatibility with `RegisterRedemption`.
type AccountId = AccountId32;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Flip: pallet_cf_flip,
		Funding: pallet_cf_funding,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = sp_core::H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
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

impl_mock_chainflip!(Test);

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

// Implement mock for RestrictionHandler
impl_mock_waived_fees!(AccountId, RuntimeCall);

impl pallet_cf_flip::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = FlipBalance;
	type BlocksPerDay = BlocksPerDay;
	type OnAccountFunded = MockOnAccountFunded;
	type WeightInfo = ();
	type WaivedFees = WaivedFeesMock;
}

cf_traits::impl_mock_ensure_witnessed_for_origin!(RuntimeOrigin);
cf_traits::impl_mock_on_account_funded!(AccountId, u128);

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockRegisterRedemption {
	amount: <Ethereum as Chain>::ChainAmount,
}

impl cf_chains::RegisterRedemption for MockRegisterRedemption {
	fn new_unsigned(
		_node_id: &[u8; 32],
		amount: u128,
		_address: &[u8; 20],
		_expiry: u64,
		_executor: Option<cf_chains::eth::Address>,
	) -> Self {
		Self { amount }
	}
}

impl ApiCall<EvmCrypto> for MockRegisterRedemption {
	fn threshold_signature_payload(
		&self,
	) -> <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(
		self,
		_threshold_signature: &<<Ethereum as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature,
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
	) -> <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId {
		todo!()
	}
}

parameter_types! {
	pub static CanRedeem: bool = true;
}

pub const BIDDING_ERR: DispatchError =
	DispatchError::Other("The given validator is an active bidder");
pub struct MockRedemptionChecker;

impl RedemptionCheck for MockRedemptionChecker {
	type ValidatorId = AccountId;

	fn ensure_can_redeem(_validator_id: &Self::ValidatorId) -> DispatchResult {
		frame_support::ensure!(CanRedeem::get(), BIDDING_ERR);
		Ok(())
	}
}

impl_mock_runtime_safe_mode! { funding: PalletSafeMode }

pub type MockFundingBroadcaster = MockBroadcaster<(MockRegisterRedemption, RuntimeCall)>;

impl pallet_cf_funding::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type TimeSource = time_source::Mock;
	type Flip = Flip;
	type WeightInfo = ();
	type FunderId = AccountId;
	type Broadcaster = MockFundingBroadcaster;
	type ThresholdCallable = RuntimeCall;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type RedemptionChecker = MockRedemptionChecker;
	type SafeMode = MockRuntimeSafeMode;
	type RegisterRedemption = MockRegisterRedemption;
}

pub const REDEMPTION_TTL_SECS: u64 = 10;

pub const ALICE: AccountId = AccountId32::new([0xa1; 32]);
pub const BOB: AccountId = AccountId32::new([0xb0; 32]);
// Used as genesis node for testing.
pub const CHARLIE: AccountId = AccountId32::new([0xc1; 32]);

pub const MIN_FUNDING: u128 = 10;
pub const REDEMPTION_TAX: u128 = MIN_FUNDING / 2;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig {
		system: Default::default(),
		flip: FlipConfig { total_issuance: 1_000_000, daily_slashing_rate: Permill::from_perthousand(1)},
		funding: FundingConfig {
			genesis_accounts: vec![(CHARLIE, MIN_FUNDING)],
			redemption_tax: REDEMPTION_TAX,
			minimum_funding: MIN_FUNDING,
			redemption_ttl: Duration::from_secs(REDEMPTION_TTL_SECS),
		},
	},
	|| {
		<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(&CHARLIE)
			.unwrap();
	}
}
