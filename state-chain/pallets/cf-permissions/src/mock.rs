use super::{PermissionVerifier, Config};
use crate as pallet_cf_permissions;
use sp_core::{H256};

use sp_runtime::{
	traits::{
		BlakeTwo256,
		IdentityLookup,
	},
	testing::{
		Header,
	},
};
use frame_support::{parameter_types, construct_runtime, traits::GenesisBuild};
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		PermissionsManager: pallet_cf_permissions::{Module, Call, Storage},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}
impl frame_system::Config for Test {
	type BaseCallFilter = ();
	type BlockWeights = ();
	type BlockLength = ();
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
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
}

pub const BAD_ACTOR: u64 = 200;

pub struct Verifier;
impl PermissionVerifier for Verifier {
	type AccountId = u64;
	type Scope = u64;

	fn verify_scope(account: &Self::AccountId, _scope: &Self::Scope) -> bool {
		*account != BAD_ACTOR
	}
}

impl Config for Test {
	type Scope = u64;
	type Verifier = Verifier;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
	pallet_cf_permissions::GenesisConfig::<Test>{
		scopes: vec![],
	}.assimilate_storage(&mut t).unwrap();
	t.into()
}
