#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]
mod chainflip;
pub mod constants;
#[cfg(test)]
mod tests;
use core::time::Duration;
pub use frame_support::{
	construct_runtime, debug, parameter_types,
	traits::{KeyOwnerProofSystem, Randomness, StorageInfo},
	weights::{
		constants::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight, WEIGHT_PER_SECOND},
		IdentityFee, Weight,
	},
	StorageValue,
};
use frame_system::offchain::SendTransactionTypes;
use pallet_grandpa::{
	fg_primitives, AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList,
};
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

use crate::chainflip::{
	ChainflipEpochTransitions, ChainflipHeartbeat, ChainflipStakeHandler,
	ChainflipVaultRotationHandler, OfflinePenalty,
};
use cf_traits::ChainflipAccountData;
pub use cf_traits::{BlockNumber, FlipBalance};
use constants::common::*;
use pallet_cf_broadcast::AttemptCount;
use pallet_cf_flip::FlipSlasher;
use pallet_cf_reputation::ReputationPenalty;

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Index of a transaction in the chain.
pub type Index = u32;

/// Balance of an account.
pub type Balance = u128;

/// A hash of some data used by the chain.
pub type Hash = sp_core::H256;

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
// To learn more about runtime versioning and what each of the following value means:
//   https://substrate.dev/docs/en/knowledgebase/runtime/upgrades#runtime-versioning
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("chainflip-node"),
	impl_name: create_runtime_str!("chainflip-node"),
	authoring_version: 1,
	spec_version: 106,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 1,
};

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

parameter_types! {
	pub const MinValidators: u32 = 1;
	pub const ActiveToBackupValidatorRatio: u32 = 3;
	pub const PercentageOfBackupValidatorsInEmergency: u32 = 30;
}

impl pallet_cf_auction::Config for Runtime {
	type Event = Event;
	type Amount = FlipBalance;
	type BidderProvider = pallet_cf_staking::Pallet<Self>;
	type Registrar = Session;
	type ValidatorId = AccountId;
	type MinValidators = MinValidators;
	type Handler = Vaults;
	type WeightInfo = pallet_cf_auction::weights::PalletWeight<Runtime>;
	type Online = Online;
	type PeerMapping = pallet_cf_validator::Pallet<Self>;
	type ChainflipAccount = cf_traits::ChainflipAccountStore<Self>;
	type ActiveToBackupValidatorRatio = ActiveToBackupValidatorRatio;
	type EmergencyRotation = Validator;
	type PercentageOfBackupValidatorsInEmergency = PercentageOfBackupValidatorsInEmergency;
}

// FIXME: These would be changed
parameter_types! {
	pub const MinEpoch: BlockNumber = 1;
	pub const EmergencyRotationPercentageRange: PercentageRange = PercentageRange {
		bottom: 67,
		top: 80,
	};
}

impl pallet_cf_validator::Config for Runtime {
	type Event = Event;
	type MinEpoch = MinEpoch;
	type EpochTransitionHandler = ChainflipEpochTransitions;
	type ValidatorWeightInfo = pallet_cf_validator::weights::PalletWeight<Runtime>;
	type Amount = FlipBalance;
	type Auctioneer = Auction;
	type EmergencyRotationPercentageRange = EmergencyRotationPercentageRange;
}

impl pallet_cf_environment::Config for Runtime {
	type Event = Event;
}

parameter_types! {
	pub const KeygenResponseGracePeriod: BlockNumber = constants::common::KEYGEN_RESPONSE_GRACE_PERIOD;
}

impl pallet_cf_vaults::Config for Runtime {
	type Event = Event;
	type RotationHandler = ChainflipVaultRotationHandler;
	type OfflineReporter = Reputation;
	type SigningContext = chainflip::EthereumSigningContext;
	type ThresholdSigner = EthereumThresholdSigner;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type KeygenResponseGracePeriod = KeygenResponseGracePeriod;
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
	type BaseCallFilter = frame_support::traits::Everything;
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
	type OnKilledAccount =
		(pallet_cf_flip::BurnFlipAccount<Self>, pallet_cf_validator::DeletePeerMapping<Self>);
	/// The data to be stored in an account.
	type AccountData = ChainflipAccountData;
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = ();
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	/// The set code logic, just the default since we're not a parachain.
	type OnSetCode = ();
}

impl pallet_randomness_collective_flip::Config for Runtime {}

impl frame_system::offchain::SigningTypes for Runtime {
	type Public = <Signature as Verify>::Signer;
	type Signature = Signature;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
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
	type StakeHandler = ChainflipStakeHandler;
	type WeightInfo = pallet_cf_flip::weights::PalletWeight<Runtime>;
	type WaivedFees = chainflip::WaivedFees;
}

impl pallet_cf_witnesser::Config for Runtime {
	type Event = Event;
	type Origin = Origin;
	type Call = Call;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type EpochInfo = pallet_cf_validator::Pallet<Self>;
	type Amount = FlipBalance;
	type WeightInfo = pallet_cf_witnesser::weights::PalletWeight<Runtime>;
}

parameter_types! {
	/// 6 days.
	pub const ClaimTTL: Duration = Duration::from_secs(3 * CLAIM_DELAY);
}

impl pallet_cf_staking::Config for Runtime {
	type Event = Event;
	type Balance = FlipBalance;
	type StakerId = AccountId;
	type Flip = Flip;
	type NonceProvider = Vaults;
	type SigningContext = chainflip::EthereumSigningContext;
	type ThresholdSigner = EthereumThresholdSigner;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, Instance1>;
	type TimeSource = Timestamp;
	type ClaimTTL = ClaimTTL;
	type WeightInfo = pallet_cf_staking::weights::PalletWeight<Runtime>;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
}

impl pallet_cf_governance::Config for Runtime {
	type Origin = Origin;
	type Call = Call;
	type Event = Event;
	type TimeSource = Timestamp;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type WeightInfo = pallet_cf_governance::weights::PalletWeight<Runtime>;
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
	type BlocksPerDay = BlocksPerDay;
	type MintInterval = MintInterval;
	type NonceProvider = Vaults;
	type SigningContext = chainflip::EthereumSigningContext;
	type ThresholdSigner = EthereumThresholdSigner;
	type WeightInfo = pallet_cf_emissions::weights::PalletWeight<Runtime>;
}

impl pallet_cf_rewards::Config for Runtime {
	type Event = Event;
	type WeightInfoRewards = pallet_cf_rewards::weights::PalletWeight<Runtime>;
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
	type WeightInfoWitnesser = pallet_cf_witnesser::weights::PalletWeight<Runtime>;
}

parameter_types! {
	pub const HeartbeatBlockInterval: BlockNumber = 150;
	pub const ReputationPointPenalty: ReputationPenalty<BlockNumber> = ReputationPenalty { points: 1, blocks: 10 };
	pub const ReputationPointFloorAndCeiling: (i32, i32) = (-2880, 2880);
}

impl pallet_cf_reputation::Config for Runtime {
	type Event = Event;
	type HeartbeatBlockInterval = HeartbeatBlockInterval;
	type ReputationPointPenalty = ReputationPointPenalty;
	type ReputationPointFloorAndCeiling = ReputationPointFloorAndCeiling;
	type Slasher = FlipSlasher<Self>;
	type Penalty = OfflinePenalty;
	type WeightInfo = pallet_cf_reputation::weights::PalletWeight<Runtime>;
	type Banned = pallet_cf_online::Pallet<Self>;
}

impl pallet_cf_online::Config for Runtime {
	type HeartbeatBlockInterval = HeartbeatBlockInterval;
	type Heartbeat = ChainflipHeartbeat;
	type WeightInfo = pallet_cf_online::weights::PalletWeight<Runtime>;
}

use frame_support::instances::Instance1;
use pallet_cf_validator::PercentageRange;

parameter_types! {
	pub const ThresholdFailureTimeout: BlockNumber = 15;
	pub const CeremonyRetryDelay: BlockNumber = 1;
}

impl pallet_cf_threshold_signature::Config<Instance1> for Runtime {
	type Event = Event;
	type SignerNomination = chainflip::RandomSignerNomination;
	type TargetChain = cf_chains::Ethereum;
	type SigningContext = chainflip::EthereumSigningContext;
	type KeyProvider = chainflip::EthereumKeyProvider;
	type OfflineReporter = Reputation;
	type ThresholdFailureTimeout = ThresholdFailureTimeout;
	type CeremonyRetryDelay = CeremonyRetryDelay;
}

parameter_types! {
	pub const EthereumSigningTimeout: BlockNumber = 5;
	pub const EthereumTransmissionTimeout: BlockNumber = 10 * MINUTES;
	pub const MaximumAttempts: AttemptCount = MAXIMUM_BROADCAST_ATTEMPTS;
}

impl pallet_cf_broadcast::Config<Instance1> for Runtime {
	type Event = Event;
	type TargetChain = cf_chains::Ethereum;
	type BroadcastConfig = chainflip::EthereumBroadcastConfig;
	type SignerNomination = chainflip::RandomSignerNomination;
	type OfflineReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, Instance1>;
	type SigningTimeout = EthereumSigningTimeout;
	type TransmissionTimeout = EthereumTransmissionTimeout;
	type MaximumAttempts = MaximumAttempts;
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
}

construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = opaque::Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		RandomnessCollectiveFlip: pallet_randomness_collective_flip::{Pallet, Storage},
		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
		Environment: pallet_cf_environment::{Pallet, Storage, Event<T>, Config},
		Flip: pallet_cf_flip::{Pallet, Event<T>, Storage, Config<T>},
		Emissions: pallet_cf_emissions::{Pallet, Event<T>, Storage, Config},
		Rewards: pallet_cf_rewards::{Pallet, Call, Event<T>},
		Staking: pallet_cf_staking::{Pallet, Call, Storage, Event<T>, Config<T>},
		TransactionPayment: pallet_transaction_payment::{Pallet, Storage},
		Session: pallet_session::{Pallet, Storage, Event, Config<T>},
		Historical: session_historical::{Pallet},
		Witnesser: pallet_cf_witnesser::{Pallet, Call, Storage, Event<T>, Origin},
		WitnesserApi: pallet_cf_witnesser_api::{Pallet, Call},
		Auction: pallet_cf_auction::{Pallet, Call, Storage, Event<T>, Config<T>},
		Validator: pallet_cf_validator::{Pallet, Call, Storage, Event<T>, Config<T>},
		Aura: pallet_aura::{Pallet, Config<T>},
		Authorship: pallet_authorship::{Pallet, Call, Storage, Inherent},
		Grandpa: pallet_grandpa::{Pallet, Call, Storage, Config, Event},
		Governance: pallet_cf_governance::{Pallet, Call, Storage, Event<T>, Config<T>, Origin},
		Vaults: pallet_cf_vaults::{Pallet, Call, Storage, Event<T>, Config},
		Online: pallet_cf_online::{Pallet, Call, Storage},
		Reputation: pallet_cf_reputation::{Pallet, Call, Storage, Event<T>, Config<T>},
		EthereumThresholdSigner: pallet_cf_threshold_signature::<Instance1>::{Pallet, Call, Storage, Event<T>, Origin<T>, ValidateUnsigned},
		EthereumBroadcaster: pallet_cf_broadcast::<Instance1>::{Pallet, Call, Storage, Event<T>},
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
	AllPallets,
	(),
>;

impl_runtime_apis! {

	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			Executive::execute_block(block);
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

		fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
			block_hash: <Block as BlockT>::Hash,
		) -> TransactionValidity {
			Executive::validate_transaction(source, tx, block_hash)
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
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

		fn current_set_id() -> fg_primitives::SetId {
			Grandpa::current_set_id()
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

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade() -> Result<(Weight, Weight), sp_runtime::RuntimeString> {
			let weight = Executive::try_runtime_upgrade()?;
			Ok((weight, BlockWeights::get().max_block))
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(extra: bool) -> (
			Vec<frame_benchmarking::BenchmarkList>,
			Vec<frame_support::traits::StorageInfo>,
		) {
			use frame_benchmarking::{list_benchmark, Benchmarking, BenchmarkList};
			use frame_support::traits::StorageInfoTrait;
			use frame_system_benchmarking::Pallet as SystemBench;

			let mut list = Vec::<BenchmarkList>::new();

			list_benchmark!(list, extra, frame_system, SystemBench::<Runtime>);
			list_benchmark!(list, extra, pallet_timestamp, Timestamp);
			list_benchmark!(list, extra, pallet_cf_validator, Validator);
			list_benchmark!(list, extra, pallet_cf_auction, Auction);
			list_benchmark!(list, extra, pallet_cf_staking, Staking);
			list_benchmark!(list, extra, pallet_cf_flip, Flip);
			list_benchmark!(list, extra, pallet_cf_governance, Governance);
			list_benchmark!(list, extra, pallet_cf_online, Online);
			list_benchmark!(list, extra, pallet_cf_emissions, Emissions);
			list_benchmark!(list, extra, pallet_cf_reputation, Reputation);
			list_benchmark!(list, extra, pallet_cf_rewards, Rewards);
			list_benchmark!(list, extra, pallet_cf_vaults, Vaults);
			list_benchmark!(list, extra, pallet_cf_witnesser, Witnesser);

			let storage_info = AllPalletsWithSystem::storage_info();

			return (list, storage_info)
		}

		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{Benchmarking, BenchmarkBatch, add_benchmark, TrackedStorageKey};

			use frame_system_benchmarking::Pallet as SystemBench;
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
			add_benchmark!(params, batches, pallet_cf_staking, Staking);
			add_benchmark!(params, batches, pallet_cf_flip, Flip);
			add_benchmark!(params, batches, pallet_cf_governance, Governance);
			add_benchmark!(params, batches, pallet_cf_vaults, Vaults);
			add_benchmark!(params, batches, pallet_cf_online, Online);
			add_benchmark!(params, batches, pallet_cf_witnesser, Witnesser);
			add_benchmark!(params, batches, pallet_cf_rewards, Rewards);
			add_benchmark!(params, batches, pallet_cf_reputation, Reputation);
			add_benchmark!(params, batches, pallet_cf_emissions, Emissions);
			// add_benchmark!(params, batches, pallet_cf_broadcast, EthereumBroadcaster);
			// add_benchmark!(params, batches, pallet_cf_threshold_signature, EthereumThresholdSigner);

			if batches.is_empty() { return Err("Benchmark not found for this pallet.".into()) }
			Ok(batches)
		}
	}
}
