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
use crate::rotation::*;
use cf_traits::{AuctionConfirmation, AuctionEvents, AuctionError, AuctionPenalty};
pub(super) mod time_source;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<MockRuntime>;
type Block = frame_system::mocking::MockBlock<MockRuntime>;

type Amount = u64;
type ValidatorId = u64;

use chains::ethereum;

thread_local! {
}

construct_runtime!(
	pub enum MockRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		EthereumChain: ethereum::{Module, Call, Config, Storage, Event<T>},
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

pub struct OtherChain;
impl Chain<RequestIndex, ValidatorId, RotationError<ValidatorId>> for OtherChain {
	fn chain_params() -> ChainParams {
		todo!()
	}

	fn try_start_vault_rotation(index: RequestIndex, new_public_key: NewPublicKey, validators: Vec<ValidatorId>) -> Result<(), RotationError<ValidatorId>> {
		todo!("mock other chain construction phase")
	}
}

// Our pallet is awaiting on completion
// impl ChainEvents<RequestIndex, ValidatorId, MockError> for MockRuntime {
// 	fn try_on_completion(index: RequestIndex, result: Result<ValidatorRotationRequest, ValidatorRotationError<ValidatorId>>) -> Result<(), MockError> {
// 		todo!("mock construction manager")
// 	}
// }

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

pub struct MockAuctionEvents<ValidatorId, Amount> {
	_v: PhantomData<ValidatorId>,
	_a: PhantomData<Amount>,
}

impl<ValidatorId, Amount> AuctionEvents<ValidatorId, Amount> for MockAuctionEvents<ValidatorId, Amount> {
	fn on_completed(winners: Vec<ValidatorId>, min_bid:Amount) -> Result<(), AuctionError> {
		Ok(())
	}
}

pub struct MockAuctionConfirmation;
impl AuctionConfirmation for MockAuctionConfirmation {
	fn try_confirmation() -> Result<(), AuctionError> {
		Ok(())
	}
}

pub struct MockAuctionPenalty<ValidatorId> {
	_a : PhantomData<ValidatorId>,
}

impl<ValidatorId> AuctionPenalty<ValidatorId> for MockAuctionPenalty<ValidatorId> {
	fn abort() {
		todo!()
	}

	fn penalise(bad_validators: Vec<ValidatorId>) {
		todo!()
	}
}

impl ChainFlip for MockRuntime {
	type Amount = Amount;
	type ValidatorId = ValidatorId;
}

impl AuctionManager<ValidatorId, Amount> for MockRuntime {
	type Penalty = MockAuctionPenalty<ValidatorId>;
	type Confirmation = MockAuctionConfirmation;
	type Events = MockAuctionEvents<ValidatorId, Amount>;
}

impl ethereum::Config for MockRuntime {
	type Event = Event;
	type Call = Call;
	type Vaults = EthereumVault;
	type EnsureWitnessed = MockEnsureWitness;
	type Witnesser = MockWitnesser;
	type Nonce = u64;
	type NonceProvider = EthereumVault;
}

// Our vault for Ethereum
impl pallet_cf_vaults::Config<Instance1> for MockRuntime {
	type Event = Event;
	type Call = Call;
	type Chain = EthereumChain;
	type EnsureWitnessed = MockEnsureWitness;
	type Witnesser = MockWitnesser;
	type Nonce = u64;
	type TimeSource = time_source::Mock;
}

// Another vault for OtherChain
impl pallet_cf_vaults::Config<Instance2> for MockRuntime {
	type Event = Event;
	type Call = Call;
	type Chain = OtherChain;
	type EnsureWitnessed = MockEnsureWitness;
	type Witnesser = MockWitnesser;
	type Nonce = u64;
	type TimeSource = time_source::Mock;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		ethereum: Default::default(),
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
