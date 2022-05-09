#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]
pub mod chainflip;
pub mod constants;
mod migrations;
pub mod runtime_apis;
pub use frame_system::Call as SystemCall;
#[cfg(test)]
mod tests;
use cf_chains::{eth, Ethereum};
pub use frame_support::{
	construct_runtime, debug,
	instances::Instance1,
	parameter_types,
	traits::{
		ConstU128, ConstU32, ConstU64, ConstU8, KeyOwnerProofSystem, Randomness, StorageInfo,
	},
	weights::{
		constants::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight, WEIGHT_PER_SECOND},
		ConstantMultiplier, IdentityFee, Weight,
	},
	StorageValue,
};
use frame_system::offchain::SendTransactionTypes;
pub use pallet_cf_environment::cfe::CfeSettings;
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

use cf_traits::EpochInfo;

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

use cf_traits::ChainflipAccountData;
pub use cf_traits::{BlockNumber, FlipBalance, SessionKeysRegistered};
pub use chainflip::chain_instances::*;
use chainflip::{
	epoch_transition::ChainflipEpochTransitions, ChainflipHeartbeat, ChainflipStakeHandler,
	KeygenOffences,
};
use constants::common::*;
use pallet_cf_flip::{Bonder, FlipSlasher};
use pallet_cf_validator::PercentageRange;
pub use pallet_transaction_payment::ChargeTransactionPayment;

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
//   https://docs.substrate.io/v3/runtime/upgrades#runtime-versioning
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("chainflip-node"),
	impl_name: create_runtime_str!("chainflip-node"),
	authoring_version: 1,
	spec_version: 112,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 2,
	state_version: 1,
};

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

parameter_types! {
	pub const AuthorityToBackupRatio: u32 = 3;
	pub const PercentageOfBackupNodesInEmergency: u32 = 30;
}

impl pallet_cf_auction::Config for Runtime {
	type Event = Event;
	type BidderProvider = pallet_cf_staking::Pallet<Self>;
	type WeightInfo = pallet_cf_auction::weights::PalletWeight<Runtime>;
	type ChainflipAccount = cf_traits::ChainflipAccountStore<Self>;
	type AuctionQualification = (
		Online,
		pallet_cf_validator::PeerMapping<Self>,
		SessionKeysRegistered<
			<Self as frame_system::Config>::AccountId,
			pallet_session::Pallet<Self>,
		>,
	);
	type AuthorityToBackupRatio = AuthorityToBackupRatio;
	type EmergencyRotation = Validator;
	type PercentageOfBackupNodesInEmergency = PercentageOfBackupNodesInEmergency;
	type KeygenExclusionSet = chainflip::ExclusionSetFor<KeygenOffences>;
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
	type Offence = chainflip::Offence;
	type MinEpoch = MinEpoch;
	type EpochTransitionHandler = ChainflipEpochTransitions;
	type ValidatorWeightInfo = pallet_cf_validator::weights::PalletWeight<Runtime>;
	type Auctioneer = Auction;
	type VaultRotator = EthereumVault;
	type EmergencyRotationPercentageRange = EmergencyRotationPercentageRange;
	type ChainflipAccount = cf_traits::ChainflipAccountStore<Self>;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type Bonder = Bonder<Runtime>;
	type MissedAuthorshipSlots = chainflip::MissedAuraSlots;
	type OffenceReporter = Reputation;
	type ReputationResetter = Reputation;
}

impl pallet_cf_environment::Config for Runtime {
	type Event = Event;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type WeightInfo = pallet_cf_environment::weights::PalletWeight<Runtime>;
	type EthEnvironmentProvider = Environment;
}

parameter_types! {
	pub const KeygenResponseGracePeriod: BlockNumber =
		constants::common::KEYGEN_CEREMONY_TIMEOUT_BLOCKS +
		constants::common::THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS;
}

impl pallet_cf_vaults::Config<EthereumInstance> for Runtime {
	type Event = Event;
	type Offence = chainflip::Offence;
	type Chain = Ethereum;
	type ApiCall = eth::api::EthereumApi;
	type Broadcaster = EthereumBroadcaster;
	type OffenceReporter = Reputation;
	type CeremonyIdProvider = pallet_cf_validator::CeremonyIdProvider<Self>;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ReplayProtectionProvider = chainflip::EthReplayProtectionProvider;
	type KeygenResponseGracePeriod = KeygenResponseGracePeriod;
	type EthEnvironmentProvider = Environment;
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
	type MaxConsumers = ConstU32<16>;
}

impl pallet_randomness_collective_flip::Config for Runtime {}

impl frame_system::offchain::SigningTypes for Runtime {
	type Public = <Signature as Verify>::Signer;
	type Signature = Signature;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<MAX_AUTHORITIES>;
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
	type MaxAuthorities = ConstU32<MAX_AUTHORITIES>;
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

impl pallet_authorship::Config for Runtime {
	type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
	type UncleGenerations = ConstU32<5>;
	type FilterUncle = ();
	type EventHandler = ();
}

impl pallet_cf_flip::Config for Runtime {
	type Event = Event;
	type Balance = FlipBalance;
	type ExistentialDeposit = ConstU128<500>;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type BlocksPerDay = ConstU32<DAYS>;
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

impl pallet_cf_staking::Config for Runtime {
	type Event = Event;
	type ThresholdCallable = Call;
	type StakerId = AccountId;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type Balance = FlipBalance;
	type Flip = Flip;
	type ReplayProtectionProvider = chainflip::EthReplayProtectionProvider;
	type EthEnvironmentProvider = Environment;
	type ThresholdSigner = EthereumThresholdSigner;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, Instance1>;
	type RegisterClaim = eth::api::EthereumApi;
	type TimeSource = Timestamp;
	type WeightInfo = pallet_cf_staking::weights::PalletWeight<Runtime>;
}

impl pallet_cf_governance::Config for Runtime {
	type Origin = Origin;
	type Call = Call;
	type Event = Event;
	type TimeSource = Timestamp;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type WeightInfo = pallet_cf_governance::weights::PalletWeight<Runtime>;
	type UpgradeCondition = pallet_cf_validator::NotDuringRotation<Runtime>;
	type RuntimeUpgrade = chainflip::RuntimeUpgradeManager;
}

impl pallet_cf_emissions::Config for Runtime {
	type Event = Event;
	type HostChain = Ethereum;
	type FlipBalance = FlipBalance;
	type ApiCall = eth::api::EthereumApi;
	type Broadcaster = EthereumBroadcaster;
	type Surplus = pallet_cf_flip::Surplus<Runtime>;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;
	type RewardsDistribution = chainflip::BlockAuthorRewardDistribution;
	type BlocksPerDay = ConstU32<DAYS>;
	type ReplayProtectionProvider = chainflip::EthReplayProtectionProvider;
	type EthEnvironmentProvider = Environment;
	type WeightInfo = pallet_cf_emissions::weights::PalletWeight<Runtime>;
}

impl pallet_transaction_payment::Config for Runtime {
	type OnChargeTransaction = pallet_cf_flip::FlipTransactionPayment<Self>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = IdentityFee<FlipBalance>;
	type LengthToFee = ConstantMultiplier<FlipBalance, ConstU128<1_000_000>>;
	type FeeMultiplierUpdate = ();
}

parameter_types! {
	pub const ReputationPointFloorAndCeiling: (i32, i32) = (-2880, 2880);
	pub const MaximumReputationPointAccrued: pallet_cf_reputation::ReputationPoints = 15;
}

impl pallet_cf_reputation::Config for Runtime {
	type Event = Event;
	type Offence = chainflip::Offence;
	type HeartbeatBlockInterval = ConstU32<HEARTBEAT_BLOCK_INTERVAL>;
	type ReputationPointFloorAndCeiling = ReputationPointFloorAndCeiling;
	type Slasher = FlipSlasher<Self>;
	type WeightInfo = pallet_cf_reputation::weights::PalletWeight<Runtime>;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type MaximumReputationPointAccrued = MaximumReputationPointAccrued;
}

impl pallet_cf_online::Config for Runtime {
	type HeartbeatBlockInterval = ConstU32<HEARTBEAT_BLOCK_INTERVAL>;
	type Heartbeat = ChainflipHeartbeat;
	type WeightInfo = pallet_cf_online::weights::PalletWeight<Runtime>;
}

impl pallet_cf_threshold_signature::Config<EthereumInstance> for Runtime {
	type Event = Event;
	type Offence = chainflip::Offence;
	type RuntimeOrigin = Origin;
	type ThresholdCallable = Call;
	type SignerNomination = chainflip::RandomSignerNomination;
	type TargetChain = cf_chains::Ethereum;
	type KeyProvider = EthereumVault;
	type OffenceReporter = Reputation;
	type CeremonyIdProvider = pallet_cf_validator::CeremonyIdProvider<Self>;
	type ThresholdFailureTimeout = ConstU32<THRESHOLD_SIGNATURE_CEREMONY_TIMEOUT_BLOCKS>;
	type CeremonyRetryDelay = ConstU32<1>;
	type Weights = pallet_cf_threshold_signature::weights::PalletWeight<Self>;
}

impl pallet_cf_broadcast::Config<EthereumInstance> for Runtime {
	type Event = Event;
	type Call = Call;
	type Offence = chainflip::Offence;
	type TargetChain = cf_chains::Ethereum;
	type ApiCall = eth::api::EthereumApi;
	type ThresholdSigner = EthereumThresholdSigner;
	type TransactionBuilder = chainflip::EthTransactionBuilder;
	type SignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, Instance1>;
	type SigningTimeout = ConstU32<5>;
	type TransmissionTimeout = ConstU32<{ 10 * MINUTES }>;
	type MaximumAttempts = ConstU32<MAXIMUM_BROADCAST_ATTEMPTS>;
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
}

construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = opaque::Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		System: frame_system,
		RandomnessCollectiveFlip: pallet_randomness_collective_flip,
		Timestamp: pallet_timestamp,
		Environment: pallet_cf_environment,
		Flip: pallet_cf_flip,
		Emissions: pallet_cf_emissions,
		Staking: pallet_cf_staking,
		TransactionPayment: pallet_transaction_payment,
		Session: pallet_session,
		Historical: session_historical::{Pallet},
		Witnesser: pallet_cf_witnesser,
		Auction: pallet_cf_auction,
		Validator: pallet_cf_validator,
		Aura: pallet_aura,
		Authorship: pallet_authorship,
		Grandpa: pallet_grandpa,
		Governance: pallet_cf_governance,
		EthereumVault: pallet_cf_vaults::<Instance1>,
		Online: pallet_cf_online,
		Reputation: pallet_cf_reputation,
		EthereumThresholdSigner: pallet_cf_threshold_signature::<Instance1>,
		EthereumBroadcaster: pallet_cf_broadcast::<Instance1>,
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
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<Address, Call, Signature, SignedExtra>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<Call, SignedExtra>;
/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, Call, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
	// Note: the following run *before* all pallet migrations.
	migrations::VersionedMigration<
		(
			migrations::DeleteRewardsPallet,
			migrations::UnifyCeremonyIds,
			migrations::refactor_offences::Migration,
			migrations::migrate_contract_addresses::Migration,
			migrations::add_flip_contract_address::Migration,
			migrations::migrate_claims::Migration,
		),
		112,
	>,
>;

impl_runtime_apis! {
	// START custom runtime APIs
	impl runtime_apis::CustomRuntimeApi<Block> for Runtime {
		fn is_auction_phase() -> bool {
			Validator::is_auction_phase()
		}
	}
	// END custom runtime APIs

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
			OpaqueMetadata::new(Runtime::metadata().into())
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
			Aura::authorities().into_inner()
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

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
		fn query_info(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_info(uxt, len)
		}
		fn query_fee_details(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			TransactionPayment::query_fee_details(uxt, len)
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade() -> Result<(Weight, Weight), sp_runtime::RuntimeString> {
			// Use unwrap here otherwise the error is swallowed silently.
			let weight = Executive::try_runtime_upgrade().map_err(|e| {
				log::error!("{}", e);
				e
			})?;
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
			list_benchmark!(list, extra, pallet_cf_vaults, EthereumVault);
			list_benchmark!(list, extra, pallet_cf_witnesser, Witnesser);
			list_benchmark!(list, extra, pallet_cf_threshold_signature, EthereumThresholdSigner);
			list_benchmark!(list, extra, pallet_cf_broadcast, EthereumBroadcaster);
			list_benchmark!(list, extra, pallet_cf_environment, Environment);

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
			add_benchmark!(params, batches, pallet_cf_vaults, EthereumVault);
			add_benchmark!(params, batches, pallet_cf_online, Online);
			add_benchmark!(params, batches, pallet_cf_witnesser, Witnesser);
			add_benchmark!(params, batches, pallet_cf_reputation, Reputation);
			add_benchmark!(params, batches, pallet_cf_emissions, Emissions);
			add_benchmark!(params, batches, pallet_cf_threshold_signature, EthereumThresholdSigner);
			add_benchmark!(params, batches, pallet_cf_broadcast, EthereumBroadcaster);
			add_benchmark!(params, batches, pallet_cf_environment, Environment);

			if batches.is_empty() { return Err("Benchmark not found for this pallet.".into()) }
			Ok(batches)
		}
	}
}
