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
use crate::nonce::NonceUnixTime;

thread_local! {
}

construct_runtime!(
	pub enum MockRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		EthereumVault: ethereum::{Module, Call, Config, Storage, Event<T>},
		Vaults: pallet_cf_vaults::{Module, Call, Storage, Event<T>, Config},
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
type RequestIndex = u64;
impl ChainVault<RequestIndex, Vec<u8>, ValidatorId, RotationError<ValidatorId>> for OtherChain {
	fn chain_params() -> ChainParams {
		todo!()
	}

	fn try_start_vault_rotation(index: RequestIndex, new_public_key: Vec<u8>, validators: Vec<ValidatorId>) -> Result<(), RotationError<ValidatorId>> {
		todo!("mock other chain construction phase")
	}

	fn vault_rotated(response: VaultRotationResponse) {
		todo!()
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
	type Vaults = Vaults;
	type EnsureWitnessed = MockEnsureWitness;
	type Witnesser = MockWitnesser;
	type Nonce = u64;
	type NonceProvider = NonceUnixTime<Self::Nonce, time_source::Mock>;
	type RequestIndex = u64;
	type PublicKey = Vec<u8>;
}

impl pallet_cf_vaults::Config for MockRuntime {
	type Event = Event;
	type Call = Call;
	type EthereumVault = EthereumVault;
	type EnsureWitnessed = MockEnsureWitness;
	type Witnesser = MockWitnesser;
	type RequestIndex = u64;
	type PublicKey = Vec<u8>;
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let config = GenesisConfig {
		frame_system: Default::default(),
		ethereum: Default::default(),
		pallet_cf_vaults: Some(VaultsConfig {
		}),
	};

	let mut ext: sp_io::TestExternalities = config.build_storage().unwrap().into();

	ext.execute_with(|| {
		System::set_block_number(1);
	});

	ext
}
