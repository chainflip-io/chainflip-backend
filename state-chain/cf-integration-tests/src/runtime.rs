use cf_traits::chainflip::{ChainflipEpochTransitionHandler, ChainflipEpochTransitions};
use cf_traits::constants::{common::*, time::*};
use core::time::Duration;
use frame_support::sp_io::TestExternalities;
use frame_support::{construct_runtime, parameter_types};
use pallet_cf_flip::FlipSlasher;
use pallet_cf_reputation::ReputationPenalty;
use sp_core::storage::Storage;
use sp_core::H256;
use sp_runtime::{BuildStorage, MultiSignature};
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};
use cf_traits::Chainflip;
use sp_runtime::traits::{ConvertInto, IdentifyAccount, Verify};
use sp_runtime::impl_opaque_keys;
use sp_runtime::testing::UintAuthorityId;
use sp_runtime::Perbill;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;
type Block = frame_system::mocking::MockBlock<TestRuntime>;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;
/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

construct_runtime!(
	pub enum TestRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		Timestamp: pallet_timestamp::{Module, Call, Storage, Inherent},
		Session: pallet_session::{Module, Call, Storage, Event, Config<T>},
		Flip: pallet_cf_flip::{Module, Event<T>, Storage, Config<T>},
		Emissions: pallet_cf_emissions::{Module, Event<T>, Config<T>},
		Rewards: pallet_cf_rewards::{Module, Call, Event<T>},
		Staking: pallet_cf_staking::{Module, Call, Storage, Event<T>, Config<T>},
		Witnesser: pallet_cf_witnesser::{Module, Call, Event<T>, Origin},
		WitnesserApi: pallet_cf_witnesser_api::{Module, Call},
		Auctioneer: pallet_cf_auction::{Module, Call, Storage, Event<T>, Config<T>},
		Validator: pallet_cf_validator::{Module, Call, Storage, Event<T>, Config},
		Governance: pallet_cf_governance::{Module, Call, Storage, Event<T>, Config<T>, Origin},
		Vaults: pallet_cf_vaults::{Module, Call, Storage, Event<T>, Config<T>},
		Reputation: pallet_cf_reputation::{Module, Call, Storage, Event<T>, Config<T>},
	}
);

impl Chainflip for TestRuntime {
	type Amount = FlipBalance;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
}

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

impl frame_system::Config for TestRuntime {
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
	pub const MinimumPeriod: u64 = 5;
}

impl pallet_timestamp::Config for TestRuntime {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

impl_opaque_keys! {
	pub struct MockSessionKeys {
		pub dummy: UintAuthorityId,
	}
}

parameter_types! {
	pub const DisabledValidatorsThreshold: Perbill = Perbill::from_percent(33);
}

impl pallet_session::Config for TestRuntime {
	type ShouldEndSession = Validator;
	type SessionManager = Validator;
	type SessionHandler = Validator;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = ConvertInto;
	type Keys = MockSessionKeys;
	type Event = Event;
	type DisabledValidatorsThreshold = DisabledValidatorsThreshold;
	type NextSessionRotation = ();
	type WeightInfo = ();
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 500;
	pub const BlocksPerDay: u32 = DAYS;
}

impl pallet_cf_flip::Config for TestRuntime {
	type Event = Event;
	type Balance = FlipBalance;
	type ExistentialDeposit = ExistentialDeposit;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type BlocksPerDay = BlocksPerDay;
}

parameter_types! {
	pub const MintInterval: u32 = 10 * MINUTES;
}

impl pallet_cf_emissions::Config for TestRuntime {
	type Event = Event;
	type FlipBalance = FlipBalance;
	type Surplus = pallet_cf_flip::Surplus<TestRuntime>;
	type Issuance = pallet_cf_flip::FlipIssuance<TestRuntime>;
	type RewardsDistribution = pallet_cf_rewards::OnDemandRewardsDistribution<TestRuntime>;
	type MintInterval = MintInterval;
}

impl pallet_cf_rewards::Config for TestRuntime {
	type Event = Event;
}

parameter_types! {
	/// 4 days. When a claim is signed, there needs to be enough time left to be able to cash it in.
	pub const MinClaimTTL: Duration = Duration::from_secs(2 * REGISTRATION_DELAY);
	/// 6 days.
	pub const ClaimTTL: Duration = Duration::from_secs(3 * REGISTRATION_DELAY);
}

impl pallet_cf_staking::Config for TestRuntime {
	type Event = Event;
	type Balance = FlipBalance;
	type Flip = Flip;
	type Nonce = u64;
	type EnsureWitnessed = pallet_cf_witnesser::EnsureWitnessed;
	type EpochInfo = pallet_cf_validator::Pallet<TestRuntime>;
	type TimeSource = Timestamp;
	type MinClaimTTL = MinClaimTTL;
	type ClaimTTL = ClaimTTL;
}

impl pallet_cf_witnesser::Config for TestRuntime {
	type Event = Event;
	type Origin = Origin;
	type Call = Call;
	type Epoch = EpochIndex;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type EpochInfo = pallet_cf_validator::Pallet<Self>;
	type Amount = FlipBalance;
}

impl pallet_cf_witnesser_api::Config for TestRuntime {
	type Call = Call;
	type Witnesser = Witnesser;
}

parameter_types! {
	pub const MinAuctionSize: u32 = 2;
}

impl pallet_cf_auction::Config for TestRuntime {
	type Event = Event;
	type Amount = FlipBalance;
	type BidderProvider = pallet_cf_staking::Pallet<Self>;
	type AuctionIndex = AuctionIndex;
	type Registrar = Session;
	type ValidatorId = AccountId;
	type MinAuctionSize = MinAuctionSize;
	type Handler = Vaults;
	type WeightInfo = pallet_cf_auction::weights::PalletWeight<Self>;
	type Online = Reputation;
}

parameter_types! {
	pub const MinEpoch: BlockNumber = 1;
}

impl ChainflipEpochTransitionHandler for TestRuntime {
	type Emissions = Emissions;
	type Rewards = Rewards;
	type Flip = Flip;
	type Reputation = Reputation;
	type Witnesser = Witnesser;
	type Amount = FlipBalance;
	type ValidatorId = AccountId;
}

impl pallet_cf_validator::Config for TestRuntime {
	type Event = Event;
	type MinEpoch = MinEpoch;
	type EpochTransitionHandler = ChainflipEpochTransitions<Self>;
	type ValidatorWeightInfo = pallet_cf_validator::weights::PalletWeight<Self>;
	type EpochIndex = EpochIndex;
	type Amount = FlipBalance;
	type Auction = Auctioneer;
}

impl pallet_cf_governance::Config for TestRuntime {
	type Origin = Origin;
	type Call = Call;
	type Event = Event;
	type TimeSource = Timestamp;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
}

parameter_types! {
	pub const HeartbeatBlockInterval: u32 = 150;
	pub const ReputationPointPenalty: ReputationPenalty<BlockNumber> = ReputationPenalty { points: 1, blocks: 10 };
	pub const ReputationPointFloorAndCeiling: (i32, i32) = (-2880, 2880);
	pub const EmergencyRotationPercentageTrigger: u8 = 80;
}

impl pallet_cf_vaults::Config for TestRuntime {
	type Event = Event;
	type EnsureWitnessed = pallet_cf_witnesser::EnsureWitnessed;
	type PublicKey = Vec<u8>;
	type TransactionHash = Vec<u8>;
	type RotationHandler = Auctioneer;
	type NonceProvider = Vaults;
	type EpochInfo = Validator;
}

impl pallet_cf_reputation::Config for TestRuntime {
	type Event = Event;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type Amount = FlipBalance;
	type HeartbeatBlockInterval = HeartbeatBlockInterval;
	type ReputationPointPenalty = ReputationPointPenalty;
	type ReputationPointFloorAndCeiling = ReputationPointFloorAndCeiling;
	type Slasher = FlipSlasher<Self>;
	type EpochInfo = pallet_cf_validator::Pallet<Self>;
	type EmergencyRotation = pallet_cf_validator::EmergencyRotationOf<Self>;
	type EmergencyRotationPercentageTrigger = EmergencyRotationPercentageTrigger;
}
