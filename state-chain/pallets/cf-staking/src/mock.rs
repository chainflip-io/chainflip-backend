use crate as pallet_cf_funding;
use cf_chains::{ApiCall, Chain, ChainCrypto, Ethereum};
use cf_primitives::{BroadcastId, ThresholdSignatureRequestId};
use cf_traits::{
	impl_mock_callback, impl_mock_chainflip, impl_mock_waived_fees, mocks::time_source,
	AccountRoleRegistry, Broadcaster, WaivedFees,
};
use codec::{Decode, Encode, MaxEncodedLen};
use core::cell::RefCell;
use frame_support::{parameter_types, traits::UnfilteredDispatchable};
use scale_info::TypeInfo;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	AccountId32, BuildStorage,
};
use std::time::Duration;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;
// Use a realistic account id for compatibility with `RegisterRedemption`.
type AccountId = AccountId32;
type Balance = u128;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		Flip: pallet_cf_flip,
		Funding: pallet_cf_funding,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = sp_core::H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
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
	pub const CeremonyRetryDelay: <Test as frame_system::Config>::BlockNumber = 1;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 10;
}

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

// Implement mock for RestrictionHandler
impl_mock_waived_fees!(AccountId, RuntimeCall);

impl pallet_cf_flip::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = u128;
	type ExistentialDeposit = ExistentialDeposit;
	type BlocksPerDay = BlocksPerDay;
	type OnAccountFunded = MockOnAccountFunded;
	type WeightInfo = ();
	type WaivedFees = WaivedFeesMock;
}

cf_traits::impl_mock_ensure_witnessed_for_origin!(RuntimeOrigin);
cf_traits::impl_mock_on_account_funded!(AccountId, u128);

pub struct MockBroadcaster;

thread_local! {
	pub static REDEMPTION_BROADCAST_REQUESTS: RefCell<Vec<<Ethereum as Chain>::ChainAmount>> = RefCell::new(vec![]);
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockRegisterRedemption {
	amount: <Ethereum as Chain>::ChainAmount,
}

impl cf_chains::RegisterRedemption<Ethereum> for MockRegisterRedemption {
	fn new_unsigned(_node_id: &[u8; 32], amount: u128, _address: &[u8; 20], _expiry: u64) -> Self {
		Self { amount }
	}

	fn amount(&self) -> u128 {
		self.amount
	}
}

impl ApiCall<Ethereum> for MockRegisterRedemption {
	fn threshold_signature_payload(&self) -> <Ethereum as ChainCrypto>::Payload {
		unimplemented!()
	}

	fn signed(self, _threshold_signature: &<Ethereum as ChainCrypto>::ThresholdSignature) -> Self {
		unimplemented!()
	}

	fn chain_encoded(&self) -> Vec<u8> {
		unimplemented!()
	}

	fn is_signed(&self) -> bool {
		unimplemented!()
	}
}

impl MockBroadcaster {
	pub fn received_requests() -> Vec<<Ethereum as Chain>::ChainAmount> {
		REDEMPTION_BROADCAST_REQUESTS.with(|cell| cell.borrow().clone())
	}
}

impl_mock_callback!(RuntimeOrigin);

impl Broadcaster<Ethereum> for MockBroadcaster {
	type ApiCall = MockRegisterRedemption;
	type Callback = MockCallback;

	fn threshold_sign_and_broadcast(
		api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		REDEMPTION_BROADCAST_REQUESTS.with(|cell| {
			cell.borrow_mut().push(api_call.amount);
		});
		(0, 1)
	}

	fn threshold_sign_and_broadcast_with_callback(
		_api_call: Self::ApiCall,
		_callback: Self::Callback,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		unimplemented!()
	}
}

pub const REDEMPTION_DELAY_BUFFER_SECS: u64 = 10;

impl pallet_cf_funding::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type TimeSource = time_source::Mock;
	type Balance = u128;
	type Flip = Flip;
	type WeightInfo = ();
	type FunderId = AccountId;
	type Broadcaster = MockBroadcaster;
	type ThresholdCallable = RuntimeCall;
	type EnsureThresholdSigned = NeverFailingOriginCheck<Self>;
	type RegisterRedemption = MockRegisterRedemption;
}

pub const REDEMPTION_TTL_SECS: u64 = 10;

pub const ALICE: AccountId = AccountId32::new([0xa1; 32]);
pub const BOB: AccountId = AccountId32::new([0xb0; 32]);
// Used as genesis node for testing.
pub const CHARLIE: AccountId = AccountId32::new([0xc1; 32]);

pub const MIN_FUNDING: u128 = 10;
// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		system: Default::default(),
		flip: FlipConfig { total_issuance: 1_000_000 },
		funding: FundingConfig {
			genesis_validators: vec![(CHARLIE, MIN_FUNDING)],
			minimum_funding: MIN_FUNDING,
			redemption_ttl: Duration::from_secs(REDEMPTION_TTL_SECS),
			redemption_delay_buffer_seconds: REDEMPTION_DELAY_BUFFER_SECS,
		},
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		for id in &[ALICE, BOB, CHARLIE] {
			<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(id)
				.unwrap();
		}
		System::set_block_number(1);
	});

	ext
}
