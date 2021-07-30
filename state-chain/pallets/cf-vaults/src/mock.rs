use super::*;
use crate as pallet_cf_vaults;
use frame_support::{construct_runtime, parameter_types, traits::UnfilteredDispatchable};
use frame_system::{ensure_root, RawOrigin};
use sp_core::H256;
use sp_runtime::BuildStorage;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};
use std::cell::RefCell;

pub(super) mod time_source;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<MockRuntime>;
type Block = frame_system::mocking::MockBlock<MockRuntime>;

type Amount = u64;
type ValidatorId = u64;

thread_local! {
	pub static OTHER_CHAIN_RESULT: RefCell<RequestIndex> = RefCell::new(0);
	pub static BAD_VALIDATORS: RefCell<Vec<ValidatorId>> = RefCell::new(vec![]);
}

construct_runtime!(
	pub enum MockRuntime where
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

pub struct OtherChain;
type RequestIndex = u64;
impl ChainVault<RequestIndex, Vec<u8>, ValidatorId, RotationError<ValidatorId>> for OtherChain {
	fn chain_params() -> ChainParams {
		ChainParams::Other(vec![])
	}

	fn try_start_vault_rotation(
		index: RequestIndex,
		_new_public_key: Vec<u8>,
		_validators: Vec<ValidatorId>,
	) -> Result<(), RotationError<ValidatorId>> {
		OTHER_CHAIN_RESULT.with(|l| *l.borrow_mut() = index);
		Ok(())
	}

	fn vault_rotated(_response: VaultRotationResponse) {}
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

	fn witness(_who: Self::AccountId, call: Self::Call) -> DispatchResultWithPostInfo {
		let result = call.dispatch_bypass_filter(frame_system::RawOrigin::Root.into());
		Ok(result.unwrap_or_else(|err| err.post_info))
	}
}

impl ChainFlip for MockRuntime {
	type Amount = Amount;
	type ValidatorId = ValidatorId;
}

impl AuctionPenalty<ValidatorId> for MockRuntime {
	fn abort() {}

	fn penalise(bad_validators: Vec<ValidatorId>) {
		BAD_VALIDATORS.with(|l| *l.borrow_mut() = bad_validators);
	}
}

impl pallet_cf_vaults::Config for MockRuntime {
	type Event = Event;
	type Call = Call;
	type EthereumVault = OtherChain;
	type EnsureWitnessed = MockEnsureWitness;
	type Witnesser = MockWitnesser;
	type RequestIndex = u64;
	type PublicKey = Vec<u8>;
	type Penalty = Self;
	type Handler = VaultsPallet;
}

pub fn bad_validators() -> Vec<ValidatorId> {
	BAD_VALIDATORS.with(|l| l.borrow().to_vec())
}

pub const ALICE: <MockRuntime as frame_system::Config>::AccountId = 123u64;
pub const BOB: <MockRuntime as frame_system::Config>::AccountId = 456u64;
pub const CHARLIE: <MockRuntime as frame_system::Config>::AccountId = 789u64;

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		pallet_cf_vaults: Some(VaultsPalletConfig {}),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
