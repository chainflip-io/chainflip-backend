use super::*;
use crate::rotation::*;
use frame_support::{construct_runtime, parameter_types, traits::UnfilteredDispatchable};
use frame_system::{ensure_root, RawOrigin};
use sp_core::H256;
use sp_runtime::BuildStorage;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<MockRuntime>;
type Block = frame_system::mocking::MockBlock<MockRuntime>;

type Amount = u64;
type ValidatorId = u64;
type RequestIndex = u64;
use crate::nonce::NonceUnixTime;
use frame_support::pallet_prelude::{DispatchResultWithPostInfo, EnsureOrigin};

thread_local! {}

construct_runtime!(
	pub enum MockRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		EthereumPallet: ethereum::{Module, Call, Config, Storage, Event<T>},
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

	fn witness(_who: Self::AccountId, call: Self::Call) -> DispatchResultWithPostInfo {
		let result = call.dispatch_bypass_filter(frame_system::RawOrigin::Root.into());
		Ok(result.unwrap_or_else(|err| err.post_info))
	}
}

impl ChainFlip for MockRuntime {
	type Amount = Amount;
	type ValidatorId = ValidatorId;
}

impl ChainHandler for MockRuntime {
	type ValidatorId = ValidatorId;
	type Err = RotationError<ValidatorId>;

	fn try_complete_vault_rotation(
		_index: RequestIndex,
		result: Result<VaultRotationRequest, RotationError<Self::ValidatorId>>,
	) -> Result<(), Self::Err> {
		result.map(|_| ())
	}
}

impl ethereum::Config for MockRuntime {
	type Event = Event;
	type Vaults = Self;
	type EnsureWitnessed = MockEnsureWitness;
	type Nonce = u64;
	type NonceProvider = NonceUnixTime<Self::Nonce, cf_traits::mocks::time_source::Mock>;
	type RequestIndex = RequestIndex;
	type PublicKey = Vec<u8>;
}

pub const ALICE: <MockRuntime as frame_system::Config>::AccountId = 123u64;
pub const BOB: <MockRuntime as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <MockRuntime as frame_system::Config>::AccountId = 789u64;

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		ethereum: Default::default(),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
