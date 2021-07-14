use super::*;
use crate as pallet_cf_vaults;
use frame_support::{construct_runtime, parameter_types};
use frame_system::{ensure_root, RawOrigin};
use sp_core::H256;
use sp_runtime::BuildStorage;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};
use std::cell::RefCell;
use crate::rotation::*;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

type Amount = u64;
type ValidatorId = u64;

thread_local! {
}

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		VaultsPallet: pallet_cf_vaults::{Module, Call, Storage, Event<T>, Config},
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

parameter_types! {
}

// This would be our chain, let's say Ethereum
// This would be implemented by the Ethereum instance
pub struct MockConstructor;
impl Construct<RequestIndex, ValidatorId> for MockConstructor {
	type Manager = MockConstructorHandler;
	fn start_construction_phase(index: RequestIndex, response: KeygenResponse<ValidatorId>) {
		// We would complete the construction and then notify the completion
		Self::Manager::on_completion(index, true);
	}
}

pub struct MockConstructorHandler;
impl ConstructionManager<RequestIndex> for MockConstructorHandler {
	fn on_completion(index: RequestIndex, result: Result<ValidatorRotationRequest, ValidatorRotationError>) {

	}
}

pub struct MockEnsureWitness;

impl EnsureOrigin<Origin> for MockEnsureWitness {
	type Success = ();

	fn try_origin(o: Origin) -> Result<Self::Success, Origin> {
		ensure_root(o).or(Err(RawOrigin::None.into()))
	}
}

pub struct MockWitnesser;

impl cf_traits::Witnesser for MockWitnesser {
	type AccountId = u64;
	type Call = Call;

	fn witness(_who: Self::AccountId, _call: Self::Call) -> DispatchResultWithPostInfo {
		// We don't intend to test this, it's just to keep the compiler happy.
		unimplemented!()
	}
}

pub struct MockAuctionPenalty;

impl AuctionPenalty<ValidatorId> for MockAuctionPenalty {
	fn penalise(bad_validators: crate::rotation::BadValidators<ValidatorId>) {
		todo!()
	}
}

impl Config for Test {
	type Event = Event;
	type Call = Call;
	type Amount = Amount;
	type ValidatorId = ValidatorId;
	type Constructor = MockConstructor;
	type EnsureWitnessed = MockEnsureWitness;
	type Witnesser = MockWitnesser;
	type AuctionPenalty = MockAuctionPenalty;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_vaults: Some(VaultsPalletConfig {
		}),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
