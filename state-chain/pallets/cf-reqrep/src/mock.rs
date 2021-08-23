use crate::{self as pallet_cf_request_response, instances::BaseConfig};
use sp_core::H256;
use frame_support::parameter_types;
use frame_support::instances::Instance0;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup}, testing::Header,
};
use frame_system;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		PingPongRequestResponse: pallet_cf_request_response::<Instance0>::{Module, Call, Storage, Event<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

impl frame_system::Config for Test {
	type BaseCallFilter = ();
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type Origin = Origin;
	type Call = Call;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
}

impl BaseConfig for Test {
	type KeyId = u64;
	type ValidatorId = u64;
	type ChainId = u64;
}

pub(crate) mod ping_pong {
	use super::*;
	use codec::{Decode, Encode};

	/// Instance marker.
	#[derive(Clone, Copy, PartialEq, Eq, Debug)]
	pub struct Instance;
	
	#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
	pub struct Ping;

	#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
	pub struct Pong;

	impl pallet_cf_request_response::RequestResponse<Test> for Ping {
		type Response = Pong;

		fn on_response(&self, response: Self::Response) -> frame_support::dispatch::DispatchResult {
			assert_eq!(response, Pong);
			Ok(().into())
		}
	}
}

impl pallet_cf_request_response::Config<Instance0> for Test {
	type Event = Event;
	type Request = ping_pong::Ping;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap().into();
	
	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
