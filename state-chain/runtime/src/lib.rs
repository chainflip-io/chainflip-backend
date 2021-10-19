#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]
mod chainflip;
use core::time::Duration;
pub use frame_support::{
	construct_runtime, debug, parameter_types,
	traits::{KeyOwnerProofSystem, Randomness},
	weights::{
		constants::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight, WEIGHT_PER_SECOND},
		IdentityFee, Weight,
	},
	StorageValue,
};
use frame_system::offchain::SendTransactionTypes;
use pallet_cf_flip::FlipSlasher;
use pallet_cf_reputation::ReputationPenalty;
use pallet_grandpa::fg_primitives;
use pallet_grandpa::{AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList};
use pallet_session::historical as session_historical;
pub use pallet_timestamp::Call as TimestampCall;
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::traits::{
	AccountIdLookup, BlakeTwo256, Block as BlockT, IdentifyAccount, NumberFor, OpaqueKeys, Verify,
};
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, MultiSignature,
};
pub use sp_runtime::{Perbill, Permill};
use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

/// An index to a block.
pub type BlockNumber = u32;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// The type for looking up accounts. We don't expect more than 4 billion of them, but you
/// never know...
pub type AccountIndex = u32;

/// Index of a transaction in the chain.
pub type Index = u32;

/// Balance of an account.
pub type Balance = u128;

/// A hash of some data used by the chain.
pub type Hash = sp_core::H256;

/// Digest item type.
pub type DigestItem = generic::DigestItem<Hash>;

pub type FlipBalance = u128;

/// The type used as an epoch index.
pub type EpochIndex = u32;

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
	pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;

	use super::*;

	/// Opaque block header type.
	pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
	/// Opaque block type.
	pub type Block = generic::Block<Header, UncheckedExtrinsic>;
	/// Opaque block identifier type.
	pub type BlockId = generic::BlockId<Block>;

	impl_opaque_keys! {
		pub struct SessionKeys {
			pub aura: Aura,
			pub grandpa: Grandpa,
		}
	}
}

pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("state-chain-node"),
	impl_name: create_runtime_str!("state-chain-node"),
	authoring_version: 1,
	spec_version: 100,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 1,
};

/// This determines the average expected block time that we are targeting.
/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
/// up by `pallet_aura` to implement `fn slot_duration()`.
///
/// Change this to adjust the block time.
pub const MILLISECS_PER_BLOCK: u64 = 6000;

pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

// Time is measured by number of blocks.
pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
pub const HOURS: BlockNumber = MINUTES * 60;
pub const DAYS: BlockNumber = HOURS * 24;

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion {
		runtime_version: VERSION,
		can_author_with: Default::default(),
	}
}

parameter_types! {
	pub const MinAuctionSize: u32 = 2;
}

impl pallet_cf_auction::Config for Runtime {
	type Event = Event;
	type Amount = FlipBalance;
	type BidderProvider = pallet_cf_staking::Pallet<Self>;
	type AuctionIndex = u64;
	type Registrar = Session;
	type ValidatorId = AccountId;
	type MinAuctionSize = MinAuctionSize;
	type Handler = Vaults;
	type WeightInfo = pallet_cf_auction::weights::PalletWeight<Runtime>;
	type Online = Reputation;
}

// FIXME: These would be changed
parameter_types! {
	pub const MinEpoch: BlockNumber = 1;
}

impl pallet_cf_validator::Config for Runtime {
	type Event = Event;
	type MinEpoch = MinEpoch;
	type EpochTransitionHandler = chainflip::ChainflipEpochTransitions;
	type ValidatorWeightInfo = pallet_cf_validator::weights::PalletWeight<Runtime>;
	type EpochIndex = EpochIndex;
	type Amount = FlipBalance;
	type Auction = Auction;
}

impl pallet_cf_environment::Config for Runtime {
	type Event = Event;
}

impl pallet_cf_vaults::Config for Runtime {
	type Event = Event;
	type EpochInfo = pallet_cf_validator::Pallet<Self>;
	type RotationHandler = Auction;
	type OfflineReporter = Reputation;
	type SigningContext = chainflip::EthereumSigningContext;
	type ThresholdSigner = EthereumThresholdSigner;
}

impl<LocalCall> SendTransactionTypes<LocalCall> for Runtime
where
	Call: From<LocalCall>,
{
	type Extrinsic = UncheckedExtrinsic;
	type OverarchingCall = Call;
}

impl pallet_session::Config for Runtime {
	type SessionHandler = <opaque::SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type ShouldEndSession = Validator;
	type SessionManager = Validator;
	type Event = Event;
	type Keys = opaque::SessionKeys;
	type NextSessionRotation = Validator;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = pallet_cf_validator::ValidatorOf<Self>;
	type DisabledValidatorsThreshold = ();
	type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
}

impl pallet_session::historical::Config for Runtime {
	type FullIdentification = ();
	type FullIdentificationOf = ();
}

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;
	pub const BlockHashCount: BlockNumber = 2400;
	/// We allow for 2 seconds of compute with a 6 second average block time.
	pub BlockWeights: frame_system::limits::BlockWeights = frame_system::limits::BlockWeights
		::with_sensible_defaults(2 * WEIGHT_PER_SECOND, NORMAL_DISPATCH_RATIO);
	pub BlockLength: frame_system::limits::BlockLength = frame_system::limits::BlockLength
		::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub const SS58Prefix: u8 = 42;
}

// Configure FRAME pallets to include in runtime.

impl frame_system::Config for Runtime {
	/// The basic call filter to use in dispatchable.
	type BaseCallFilter = ();
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = BlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = BlockLength;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The aggregated dispatch type that is available for extrinsics.
	type Call = Call;
	/// The lookup mechanism to get account ID from whatever is passed in dispatchers.
	type Lookup = AccountIdLookup<AccountId, ()>;
	/// The index type for storing how many extrinsics an account has signed.
	type Index = Index;
	/// The index type for blocks.
	type BlockNumber = BlockNumber;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The hashing algorithm used.
	type Hashing = BlakeTwo256;
	/// The header type.
	type Header = generic::Header<BlockNumber, BlakeTwo256>;
	/// The ubiquitous event type.
	type Event = Event;
	/// The ubiquitous origin type.
	type Origin = Origin;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// Converts a module to the index of the module in `construct_runtime!`.
	///
	/// This type is being generated by `construct_runtime!`.
	type PalletInfo = PalletInfo;
	/// What to do if a new account is created.
	type OnNewAccount = ();
	/// What to do if an account is fully reaped from the system.
	type OnKilledAccount = pallet_cf_flip::BurnFlipAccount<Self>;
	/// The data to be stored in an account.
	type AccountData = ();
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = ();
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
}

impl frame_system::offchain::SigningTypes for Runtime {
	type Public = <Signature as Verify>::Signer;
	type Signature = Signature;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
}

impl pallet_grandpa::Config for Runtime {
	type Event = Event;
	type Call = Call;

	type KeyOwnerProofSystem = Historical;

	type KeyOwnerProof =
		<Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(KeyTypeId, GrandpaId)>>::Proof;

	type KeyOwnerIdentification = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
		KeyTypeId,
		GrandpaId,
	)>>::IdentificationTuple;

	type HandleEquivocation = ();

	type WeightInfo = ();
}

parameter_types! {
	pub OffencesWeightSoftLimit: Weight = Perbill::from_percent(60) * BlockWeights::get().max_block;
}

impl pallet_offences::Config for Runtime {
	type Event = Event;
	type IdentificationTuple = pallet_session::historical::IdentificationTuple<Self>;
	type OnOffenceHandler = ();
	type WeightSoftLimit = OffencesWeightSoftLimit;
}

parameter_types! {
	pub const MinimumPeriod: u64 = SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

parameter_types! {
	/// The number of blocks back we should accept uncles
	pub const UncleGenerations: BlockNumber = 5;
}

impl pallet_authorship::Config for Runtime {
	type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
	type UncleGenerations = UncleGenerations;
	type FilterUncle = ();
	type EventHandler = ();
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 500;
	pub const BlocksPerDay: u32 = DAYS;
}

impl pallet_cf_flip::Config for Runtime {
	type Event = Event;
	type Balance = FlipBalance;
	type ExistentialDeposit = ExistentialDeposit;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type BlocksPerDay = BlocksPerDay;
}

impl pallet_cf_witnesser::Config for Runtime {
	type Event = Event;
	type Origin = Origin;
	type Call = Call;
	type Epoch = EpochIndex;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type EpochInfo = pallet_cf_validator::Pallet<Self>;
	type Amount = FlipBalance;
}

/// Claims go live 48 hours after registration, so we need to allow enough time beyond that.
const SECS_IN_AN_HOUR: u64 = 3600;
const REGISTRATION_DELAY: u64 = 48 * SECS_IN_AN_HOUR;

parameter_types! {
	/// 4 days. When a claim is signed, there needs to be enough time left to be able to cash it in.
	pub const MinClaimTTL: Duration = Duration::from_secs(2 * REGISTRATION_DELAY);
	/// 6 days.
	pub const ClaimTTL: Duration = Duration::from_secs(3 * REGISTRATION_DELAY);
}

impl pallet_cf_staking::Config for Runtime {
	type Event = Event;
	type Balance = FlipBalance;
	type AccountId = AccountId;
	type Flip = Flip;
	type EpochInfo = pallet_cf_validator::Pallet<Runtime>;
	type NonceProvider = Vaults;
	type SigningContext = chainflip::EthereumSigningContext;
	type ThresholdSigner = EthereumThresholdSigner;
	type TimeSource = Timestamp;
	type MinClaimTTL = MinClaimTTL;
	type ClaimTTL = ClaimTTL;
}

impl pallet_cf_governance::Config for Runtime {
	type Origin = Origin;
	type Call = Call;
	type Event = Event;
	type TimeSource = Timestamp;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
}

parameter_types! {
	pub const MintInterval: u32 = 10 * MINUTES;
}

impl pallet_cf_emissions::Config for Runtime {
	type Event = Event;
	type FlipBalance = FlipBalance;
	type Surplus = pallet_cf_flip::Surplus<Runtime>;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;
	type RewardsDistribution = pallet_cf_rewards::OnDemandRewardsDistribution<Runtime>;
	type MintInterval = MintInterval;
}

impl pallet_cf_rewards::Config for Runtime {
	type Event = Event;
}

parameter_types! {
	pub const TransactionByteFee: FlipBalance = 1_000_000;
}

impl pallet_transaction_payment::Config for Runtime {
	type OnChargeTransaction = pallet_cf_flip::FlipTransactionPayment<Self>;
	type TransactionByteFee = TransactionByteFee;
	type WeightToFee = IdentityFee<FlipBalance>;
	type FeeMultiplierUpdate = ();
}

impl pallet_cf_witnesser_api::Config for Runtime {
	type Call = Call;
	type Witnesser = Witnesser;
}

parameter_types! {
	pub const HeartbeatBlockInterval: u32 = 150;
	pub const ReputationPointPenalty: ReputationPenalty<BlockNumber> = ReputationPenalty { points: 1, blocks: 10 };
	pub const ReputationPointFloorAndCeiling: (i32, i32) = (-2880, 2880);
	pub const EmergencyRotationPercentageTrigger: u8 = 80;
}

impl pallet_cf_reputation::Config for Runtime {
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

use frame_support::instances::Instance0;

impl pallet_cf_threshold_signature::Config<Instance0> for Runtime {
	type Event = Event;
	type SignerNomination = chainflip::BasicSignerNomination;
	type TargetChain = cf_chains::Ethereum;
	type SigningContext = chainflip::EthereumSigningContext;
	type KeyProvider = chainflip::EthereumKeyProvider;
	type OfflineReporter = Reputation;
}

parameter_types! {
	pub const EthereumSigningTimeout: BlockNumber = 5;
	pub const EthereumTransmissionTimeout: BlockNumber = 10 * MINUTES;
}

impl pallet_cf_broadcast::Config<Instance0> for Runtime {
	type Event = Event;
	type TargetChain = cf_chains::Ethereum;
	type BroadcastConfig = chainflip::EthereumBroadcastConfig;
	type SignerNomination = chainflip::BasicSignerNomination;
	type OfflineReporter = Reputation;
	type SigningTimeout = EthereumSigningTimeout;
	type TransmissionTimeout = EthereumTransmissionTimeout;
}

construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = opaque::Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		System: frame_system::{Module, Call, Config, Storage, Event<T>},
		RandomnessCollectiveFlip: pallet_randomness_collective_flip::{Module, Call, Storage},
		Timestamp: pallet_timestamp::{Module, Call, Storage, Inherent},
		Environment: pallet_cf_environment::{Module, Call, Event<T>, Config},
		Flip: pallet_cf_flip::{Module, Event<T>, Storage, Config<T>},
		Emissions: pallet_cf_emissions::{Module, Event<T>, Config<T>},
		Rewards: pallet_cf_rewards::{Module, Call, Event<T>},
		Staking: pallet_cf_staking::{Module, Call, Storage, Event<T>, Config<T>},
		TransactionPayment: pallet_transaction_payment::{Module, Storage},
		Session: pallet_session::{Module, Storage, Event, Config<T>},
		Historical: session_historical::{Module},
		Witnesser: pallet_cf_witnesser::{Module, Call, Event<T>, Origin},
		WitnesserApi: pallet_cf_witnesser_api::{Module, Call},
		Auction: pallet_cf_auction::{Module, Call, Storage, Event<T>, Config<T>},
		Validator: pallet_cf_validator::{Module, Call, Storage, Event<T>, Config},
		Aura: pallet_aura::{Module, Config<T>},
		Authorship: pallet_authorship::{Module, Call, Storage, Inherent},
		Grandpa: pallet_grandpa::{Module, Call, Storage, Config, Event},
		Offences: pallet_offences::{Module, Call, Storage, Event},
		Governance: pallet_cf_governance::{Module, Call, Storage, Event<T>, Config<T>, Origin},
		Vaults: pallet_cf_vaults::{Module, Call, Storage, Event<T>, Config},
		Reputation: pallet_cf_reputation::{Module, Call, Storage, Event<T>, Config<T>},
		EthereumThresholdSigner: pallet_cf_threshold_signature::<Instance0>::{Module, Call, Storage, Event<T>},
		EthereumBroadcaster: pallet_cf_broadcast::<Instance0>::{Module, Call, Storage, Event<T>},
	}
);

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;
/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;
/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
);
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<Address, Call, Signature, SignedExtra>;
/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, Call, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllModules,
>;

impl_runtime_apis! {

	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			Executive::execute_block(block)
		}

		fn initialize_block(header: &<Block as BlockT>::Header) {
			Executive::initialize_block(header)
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			Runtime::metadata().into()
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
			Executive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> <Block as BlockT>::Header {
			Executive::finalize_block()
		}

		fn inherent_extrinsics(data: sp_inherents::InherentData) ->
			Vec<<Block as BlockT>::Extrinsic> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			data.check_extrinsics(&block)
		}

		fn random_seed() -> <Block as BlockT>::Hash {
			RandomnessCollectiveFlip::random_seed()
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
		) -> TransactionValidity {
			Executive::validate_transaction(source, tx)
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> u64 {
			Aura::slot_duration()
		}

		fn authorities() -> Vec<AuraId> {
			Aura::authorities()
		}
	}

	impl sp_session::SessionKeys<Block> for Runtime {
		fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
			opaque::SessionKeys::generate(seed)
		}

		fn decode_session_keys(
			encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
			opaque::SessionKeys::decode_into_raw_public_keys(&encoded)
		}
	}

	impl fg_primitives::GrandpaApi<Block> for Runtime {
		fn grandpa_authorities() -> GrandpaAuthorityList {
			Grandpa::grandpa_authorities()
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			equivocation_proof: fg_primitives::EquivocationProof<
				<Block as BlockT>::Hash,
				NumberFor<Block>,
			>,
			key_owner_proof: fg_primitives::OpaqueKeyOwnershipProof,
		) -> Option<()> {
			let key_owner_proof = key_owner_proof.decode()?;

			Grandpa::submit_unsigned_equivocation_report(
				equivocation_proof,
				key_owner_proof,
			)
		}

		fn generate_key_ownership_proof(
			_set_id: fg_primitives::SetId,
			authority_id: GrandpaId,
		) -> Option<fg_primitives::OpaqueKeyOwnershipProof> {
			use codec::Encode;

			Historical::prove((fg_primitives::KEY_TYPE, authority_id))
				.map(|p| p.encode())
				.map(fg_primitives::OpaqueKeyOwnershipProof::new)
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Index> for Runtime {
		fn account_nonce(account: AccountId) -> Index {
			System::account_nonce(account)
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{Benchmarking, BenchmarkBatch, add_benchmark, TrackedStorageKey};

			use frame_system_benchmarking::Module as SystemBench;
			impl frame_system_benchmarking::Config for Runtime {}

			let whitelist: Vec<TrackedStorageKey> = vec![
				// Block Number
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac").to_vec().into(),
				// Total Issuance
				hex_literal::hex!("c2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80").to_vec().into(),
				// Execution Phase
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a").to_vec().into(),
				// Event Count
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850").to_vec().into(),
				// System Events
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7").to_vec().into(),
			];

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);

			add_benchmark!(params, batches, frame_system, SystemBench::<Runtime>);
			add_benchmark!(params, batches, pallet_timestamp, Timestamp);
			add_benchmark!(params, batches, pallet_cf_validator, Validator);
			add_benchmark!(params, batches, pallet_cf_auction, Auction);

			if batches.is_empty() { return Err("Benchmark not found for this pallet.".into()) }
			Ok(batches)
		}
	}
}
