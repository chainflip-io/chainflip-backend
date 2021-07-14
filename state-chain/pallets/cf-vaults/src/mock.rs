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
use crate::rotation::ChainParams::{Ethereum, Other};
use cf_traits::AuctionConfirmation;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<MockRuntime>;
type Block = frame_system::mocking::MockBlock<MockRuntime>;

type Amount = u64;
type ValidatorId = u64;

thread_local! {
}

construct_runtime!(
	pub enum MockRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		EthereumVault: pallet_cf_vaults::<Instance1>::{Module, Call, Storage, Event<T>, Config},
		OtherChainVault: pallet_cf_vaults::<Instance2>::{Module, Call, Storage, Event<T>, Config},
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

parameter_types! {
}

// This would be our chain, let's say Ethereum
// This would be implemented by the Ethereum instance
pub struct EthereumConstructor;
impl Construct<RequestIndex, ValidatorId> for EthereumConstructor {
	type Manager = MockRuntime;
	fn start_construction_phase(index: RequestIndex, response: KeygenResponse<ValidatorId>) {
		// We would complete the construction and then notify the completion
		Self::Manager::on_completion(index, Ok(
			ValidatorRotationRequest::new(Ethereum(vec![]))
		));
	}
}

pub struct OtherChainConstructor;
impl Construct<RequestIndex, ValidatorId> for OtherChainConstructor {
	type Manager = MockRuntime;
	fn start_construction_phase(index: RequestIndex, response: KeygenResponse<ValidatorId>) {
		// We would complete the construction and then notify the completion
		Self::Manager::on_completion(index, Ok(
			ValidatorRotationRequest::new(Other(vec![]))
		));
	}
}

// Our pallet is awaiting on completion
impl ConstructionManager<RequestIndex> for MockRuntime {
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

pub struct MockAuctionConfirmation;

impl AuctionConfirmation for MockAuctionConfirmation {
	fn awaiting_confirmation() -> bool {
		false
	}

	fn set_awaiting_confirmation(_waiting: bool) {

	}
}

impl ChainFlip for MockRuntime {
	type Amount = Amount;
	type ValidatorId = ValidatorId;
}

impl AuctionManager<ValidatorId> for MockRuntime {
	type AuctionPenalty = MockAuctionPenalty;
	type AuctionConfirmation = MockAuctionConfirmation;
}

// Our vault for Ethereum
impl Config<Instance1> for MockRuntime {
	type Event = Event;
	type Call = Call;
	type Constructor = EthereumConstructor;
	type EnsureWitnessed = MockEnsureWitness;
	type Witnesser = MockWitnesser;
}

// Another vault for OtherChain
impl Config<Instance2> for MockRuntime {
	type Event = Event;
	type Call = Call;
	type Constructor = OtherChainConstructor;
	type EnsureWitnessed = MockEnsureWitness;
	type Witnesser = MockWitnesser;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_vaults_Instance1: Some(EthereumVaultConfig {
		}),
		pallet_cf_vaults_Instance2: Some(OtherChainVaultConfig {
		}),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
