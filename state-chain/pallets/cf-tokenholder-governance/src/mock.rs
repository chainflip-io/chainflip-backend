use crate::{self as pallet_cf_tokenholder_governance};
use cf_chains::{Chain, ChainCrypto, Ethereum, ForeignChain};
use cf_traits::{
	impl_mock_chainflip, impl_mock_ensure_witnessed_for_origin, impl_mock_on_account_funded,
	impl_mock_waived_fees, mocks::fee_payment::MockFeePayment, BroadcastAnyChainGovKey,
	CommKeyBroadcaster, WaivedFees,
};
use codec::{Decode, Encode};
use frame_support::{derive_impl, parameter_types, traits::HandleLifetime};
use frame_system as system;
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};

use system::pallet_prelude::BlockNumberFor;

type AccountId = u64;
type Balance = u128;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		TokenholderGovernance: pallet_cf_tokenholder_governance,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
	pub const VotingPeriod: BlockNumberFor<Test> = 10;
	pub const ProposalFee: Balance = 100;
	pub const EnactmentDelay: BlockNumberFor<Test> = 20;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
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
impl_mock_ensure_witnessed_for_origin!(RuntimeOrigin);

parameter_types! {
	pub const BlocksPerDay: u64 = 14400;
}

// Implement mock for RestrictionHandler
impl_mock_waived_fees!(AccountId, RuntimeCall);
impl_mock_on_account_funded!(AccountId, u128);

pub struct MockBroadcaster;

impl MockBroadcaster {
	pub fn set_behaviour(behaviour: MockBroadcasterBehaviour) {
		MockBroadcasterStorage::put(behaviour);
	}
	pub fn broadcasted_gov_key() -> Option<(ForeignChain, Option<Vec<u8>>, Vec<u8>)> {
		GovKeyBroadcasted::get()
	}
	fn is_govkey_compatible() -> bool {
		MockBroadcasterStorage::get().unwrap_or_default().key_compatible
	}
	fn broadcast_success() -> bool {
		MockBroadcasterStorage::get().unwrap_or_default().broadcast_success
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct MockBroadcasterBehaviour {
	pub key_compatible: bool,
	pub broadcast_success: bool,
}

impl Default for MockBroadcasterBehaviour {
	fn default() -> Self {
		Self { key_compatible: true, broadcast_success: true }
	}
}

#[frame_support::storage_alias]
type MockBroadcasterStorage = StorageValue<Mock, MockBroadcasterBehaviour>;

#[frame_support::storage_alias]
type GovKeyBroadcasted = StorageValue<Mock, (cf_chains::ForeignChain, Option<Vec<u8>>, Vec<u8>)>;

#[frame_support::storage_alias]
type CommKeyBroadcasted =
	StorageValue<Mock, <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::GovKey>;

impl BroadcastAnyChainGovKey for MockBroadcaster {
	fn broadcast_gov_key(
		chain: cf_chains::ForeignChain,
		old_key: Option<Vec<u8>>,
		new_key: Vec<u8>,
	) -> Result<(), ()> {
		if Self::broadcast_success() {
			GovKeyBroadcasted::put((chain, old_key, new_key));
			Ok(())
		} else {
			Err(())
		}
	}

	fn is_govkey_compatible(_chain: cf_chains::ForeignChain, _key: &[u8]) -> bool {
		Self::is_govkey_compatible()
	}
}

impl CommKeyBroadcaster for MockBroadcaster {
	fn broadcast(new_key: <<Ethereum as Chain>::ChainCrypto as cf_chains::ChainCrypto>::GovKey) {
		CommKeyBroadcasted::put(new_key);
	}
}

impl pallet_cf_tokenholder_governance::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type FeePayment = MockFeePayment<Self>;
	type CommKeyBroadcaster = MockBroadcaster;
	type AnyChainGovKeyBroadcaster = MockBroadcaster;
	type WeightInfo = ();
	type VotingPeriod = VotingPeriod;
	type EnactmentDelay = EnactmentDelay;
	type ProposalFee = ProposalFee;
}

// Accounts
pub const ALICE: AccountId = 123u64;
pub const BOB: AccountId = 456u64;
pub const CHARLES: AccountId = 789u64;
pub const EVE: AccountId = 987u64;
pub const BROKE_PAUL: AccountId = 1987u64;

cf_test_utilities::impl_test_helpers! {
	Test,
	RuntimeGenesisConfig::default(),
	|| {
		let account_balances = [
			(ALICE, 500),
			(BOB, 200),
			(CHARLES, 100),
			(EVE, 200),
			(BROKE_PAUL, ProposalFee::get() - 1),
		];
		for (account, _) in account_balances {
			frame_system::Provider::<Test>::created(&account).unwrap();
			assert!(frame_system::Pallet::<Test>::account_exists(&account));
		}
		MockFundingInfo::<Test>::set_balances(account_balances);
	},
}
