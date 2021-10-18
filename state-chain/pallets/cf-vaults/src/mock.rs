use std::cell::RefCell;

use frame_support::{construct_runtime, parameter_types, traits::UnfilteredDispatchable};
use frame_system::{ensure_root, RawOrigin};
use sp_core::H256;
use sp_runtime::BuildStorage;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};

use crate as pallet_cf_vaults;

use super::*;
use cf_traits::{mocks, Chainflip, Nonce, NonceIdentifier};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<MockRuntime>;
type Block = frame_system::mocking::MockBlock<MockRuntime>;

type Amount = u64;
type ValidatorId = u64;

thread_local! {
	pub static OTHER_CHAIN_RESULT: RefCell<CeremonyId> = RefCell::new(0);
	pub static BAD_VALIDATORS: RefCell<Vec<ValidatorId>> = RefCell::new(vec![]);
	pub static GENESIS_ETHEREUM_AGG_PUB_KEY: RefCell<Vec<u8>> = RefCell::new(vec![0;33]);
}

construct_runtime!(
	pub enum MockRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		VaultsPallet: pallet_cf_vaults::{Module, Call, Storage, Event<T>, Config<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

impl frame_system::Config for MockRuntime {
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

parameter_types! {}

cf_traits::impl_mock_ensure_witnessed_for_origin!(Origin);

impl Chainflip for MockRuntime {
	type KeyId = u32;
	type ValidatorId = u64;
	type Amount = u128;
	type Call = Call;
	type EnsureWitnessed = MockEnsureWitnessed;
}

impl VaultRotationHandler for MockRuntime {
	type ValidatorId = u64;
	fn vault_rotation_aborted() {}

	fn penalise(bad_validators: &[Self::ValidatorId]) {
		BAD_VALIDATORS.with(|l| *l.borrow_mut() = bad_validators.to_vec());
	}
}

impl NonceProvider for MockRuntime {
	fn next_nonce(_identifier: NonceIdentifier) -> Nonce {
		0
	}
}

impl pallet_cf_vaults::Config for MockRuntime {
	type Event = Event;
	type PublicKey = Vec<u8>;
	type TransactionHash = Vec<u8>;
	type RotationHandler = Self;
	type NonceProvider = Self;
	type EpochInfo = cf_traits::mocks::epoch_info::Mock;
}

pub fn bad_validators() -> Vec<ValidatorId> {
	BAD_VALIDATORS.with(|l| l.borrow().to_vec())
}

pub const ALICE: <MockRuntime as frame_system::Config>::AccountId = 123u64;
pub const BOB: <MockRuntime as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <MockRuntime as frame_system::Config>::AccountId = 789u64;

pub fn ethereum_public_key() -> Vec<u8> {
	GENESIS_ETHEREUM_AGG_PUB_KEY.with(|l| l.borrow().to_vec())
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_vaults: Some(VaultsPalletConfig {
			ethereum_vault_key: ethereum_public_key(),
		}),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
