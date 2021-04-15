use super::*;
use crate as pallet_cf_validator;
use codec::{Encode, Decode};
use sp_io::hashing::blake2_256;
use sp_core::{crypto::key_types::DUMMY, H256};
use sp_runtime::{
    Perbill,
    impl_opaque_keys,
    traits::{
        BlakeTwo256,
        IdentityLookup,
        OpaqueKeys
    },
    testing::{
        Header,
        UintAuthorityId
    },
    RuntimeAppPublic
};
use frame_support::{parameter_types, construct_runtime, traits::{OnInitialize, OnFinalize}};
use pallet_session::SessionHandler;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub struct TestSessionHandler;
impl SessionHandler<u64> for TestSessionHandler {
    const KEY_TYPE_IDS: &'static [sp_runtime::KeyTypeId] = &[UintAuthorityId::ID];
    fn on_genesis_session<T: OpaqueKeys>(_validators: &[(u64, T)]) {}
    fn on_new_session<T: OpaqueKeys>(
        _changed: bool,
        _validators: &[(u64, T)],
        _queued_validators: &[(u64, T)],
    ) {
        // SESSION_CHANGED.with(|l| *l.borrow_mut() = changed);
        // AUTHORITIES.with(|l|
        //     *l.borrow_mut() = validators.iter()
        //         .map(|(_, id)| id.get::<UintAuthorityId>(DUMMY).unwrap_or_default())
        //         .collect()
        // );
    }
    fn on_disabled(_validator_index: usize) {
        //DISABLED.with(|l| *l.borrow_mut() = true)
    }
    fn on_before_session_ending() {
        //BEFORE_SESSION_END_CALLED.with(|b| *b.borrow_mut() = true);
    }
}

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		ValidatorManager: pallet_cf_validator::{Module, Call, Storage, Event<T>},
        Session: pallet_session::{Module, Call, Storage, Event, Config<T>},
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

impl_opaque_keys! {
	pub struct MockSessionKeys {
		pub dummy: UintAuthorityId,
	}
}

parameter_types! {
	pub const DisabledValidatorsThreshold: Perbill = Perbill::from_percent(33);
}

impl pallet_session::Config for Test {
    type ShouldEndSession = ValidatorManager;
    type SessionManager = ValidatorManager;
    type SessionHandler = TestSessionHandler;
    type ValidatorId = u64;
    type ValidatorIdOf = pallet_cf_validator::ValidatorOf<Self>;
    type Keys = MockSessionKeys;
    type Event = Event;
    type DisabledValidatorsThreshold = DisabledValidatorsThreshold;
    type NextSessionRotation = ();
    type WeightInfo = ();
}

pub struct TestValidatorProvider;

fn account<AccountId: Decode + Default>(name: &'static str, index: u32, seed: u32) -> AccountId {
    let entropy = (name, index, seed).using_encoded(blake2_256);
    AccountId::decode(&mut &entropy[..]).unwrap_or_default()
}

impl<T: Config> ValidatorProvider<T> for TestValidatorProvider {
    fn get_validators(index: SessionIndex) -> Option<Vec<T::AccountId>> {
        Some(vec![account("ALICE", 0, index),
                  account("BOB", 1, index),
                  account("CHARLIE", 2, index)])
    }

    fn session_ending(_index: SessionIndex) {
        // Get ready for next set to be called in get_validators()
    }

    fn session_starting(_index: SessionIndex) {
        // New session starting
    }
}
parameter_types! {
	pub const MinEpoch: u64 = 1;
	pub const MinValidatorSetSize: u64 = 2;
}

impl Config for Test {
    type Event = Event;
    type MinEpoch = MinEpoch;
    type MinValidatorSetSize = MinValidatorSetSize;
    type ValidatorProvider = TestValidatorProvider;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
    frame_system::GenesisConfig::default().assimilate_storage::<Test>(&mut t).unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn run_to_block(n: u64) {
    while System::block_number() < n {
        Session::on_finalize(System::block_number());
        System::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        Session::on_initialize(System::block_number());
    }
}
