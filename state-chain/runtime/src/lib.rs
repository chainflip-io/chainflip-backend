#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]
#![feature(iter_zip)]
#![feature(int_abs_diff)]
pub mod chainflip;
pub mod constants;
mod migrations;
pub mod runtime_apis;
mod weights;
pub use frame_system::Call as SystemCall;
#[cfg(test)]
mod tests;
use crate::{
	chainflip::Offence,
	runtime_apis::{RuntimeApiAccountInfo, RuntimeApiPenalty, RuntimeApiPendingClaim},
};
use cf_chains::{eth, eth::api::register_claim::RegisterClaim, Ethereum};
pub use frame_support::{
	construct_runtime, debug,
	instances::Instance1,
	parameter_types,
	traits::{
		ConstU128, ConstU16, ConstU32, ConstU64, ConstU8, KeyOwnerProofSystem, Randomness,
		StorageInfo,
	},
	weights::{
		constants::{BlockExecutionWeight, ExtrinsicBaseWeight, WEIGHT_PER_SECOND},
		ConstantMultiplier, IdentityFee, Weight,
	},
	StorageValue,
};
use frame_system::offchain::SendTransactionTypes;
pub use pallet_cf_environment::cfe::CfeSettings;
use pallet_cf_staking::MinimumStake;
use pallet_grandpa::{
	fg_primitives, AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList,
};
use pallet_session::historical as session_historical;
pub use pallet_timestamp::Call as TimestampCall;
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::traits::{
	AccountIdLookup, BlakeTwo256, Block as BlockT, ConvertInto, IdentifyAccount, NumberFor,
	OpaqueKeys, UniqueSaturatedInto, Verify,
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

pub use cf_traits::{
	BlockNumber, ChainflipAccount, ChainflipAccountData, ChainflipAccountState,
	ChainflipAccountStore, EpochInfo, FlipBalance, SessionKeysRegistered,
};
pub use chainflip::chain_instances::*;
use chainflip::{epoch_transition::ChainflipEpochTransitions, ChainflipHeartbeat};
use constants::common::*;
use pallet_cf_flip::{Bonder, FlipSlasher};
pub use pallet_cf_staking::WithdrawalAddresses;
use pallet_cf_validator::PercentageRange;
use pallet_cf_vaults::Vault;
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

impl pallet_cf_auction::Config for Runtime {
	type Event = Event;
	type BidderProvider = pallet_cf_staking::Pallet<Self>;
	type WeightInfo = pallet_cf_auction::weights::PalletWeight<Runtime>;
	type AuctionQualification = (
		Reputation,
		pallet_cf_validator::PeerMapping<Self>,
		SessionKeysRegistered<
			<Self as frame_system::Config>::AccountId,
			pallet_session::Pallet<Self>,
		>,
	);
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
}

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
	type EpochTransitionHandler = ChainflipEpochTransitions;
	type MinEpoch = MinEpoch;
	type ValidatorWeightInfo = pallet_cf_validator::weights::PalletWeight<Runtime>;
	type Auctioneer = Auction;
	type VaultRotator = EthereumVault;
	type ChainflipAccount = cf_traits::ChainflipAccountStore<Self>;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type MissedAuthorshipSlots = chainflip::MissedAuraSlots;
	type BidderProvider = pallet_cf_staking::Pallet<Self>;
	type ValidatorQualification = <Self as pallet_cf_auction::Config>::AuctionQualification;
	type OffenceReporter = Reputation;
	type EmergencyRotationPercentageRange = EmergencyRotationPercentageRange;
	type Bonder = Bonder<Runtime>;
	type ReputationResetter = Reputation;
}

impl pallet_cf_environment::Config for Runtime {
	type Event = Event;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type WeightInfo = pallet_cf_environment::weights::PalletWeight<Runtime>;
	type EthEnvironmentProvider = Environment;
}

impl pallet_cf_vaults::Config<EthereumInstance> for Runtime {
	type Event = Event;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type Offence = chainflip::Offence;
	type Chain = Ethereum;
	type ApiCall = eth::api::EthereumApi;
	type Broadcaster = EthereumBroadcaster;
	type OffenceReporter = Reputation;
	type CeremonyIdProvider = pallet_cf_validator::CeremonyIdProvider<Self>;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ReplayProtectionProvider = chainflip::EthReplayProtectionProvider;
	type EthEnvironmentProvider = Environment;
	type SystemStateManager = pallet_cf_environment::SystemStateProvider<Runtime>;
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
	type ValidatorIdOf = ConvertInto;
	type WeightInfo = weights::pallet_session::SubstrateWeight<Runtime>;
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
	type DbWeight = weights::rocksdb_weights::constants::RocksDbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// Converts a module to the index of the module in `construct_runtime!`.
	///
	/// This type is being generated by `construct_runtime!`.
	type PalletInfo = PalletInfo;
	/// What to do if a new account is created.
	type OnNewAccount = ();
	/// What to do if an account is fully reaped from the system.
	type OnKilledAccount = (
		pallet_cf_flip::BurnFlipAccount<Self>,
		pallet_cf_validator::DeletePeerMapping<Self>,
		GrandpaOffenceReporter<Self>,
	);
	/// The data to be stored in an account.
	type AccountData = ChainflipAccountData;
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = weights::frame_system::SubstrateWeight<Runtime>;
	/// This is used as an identifier of the chain.
	type SS58Prefix = ConstU16<CHAINFLIP_SS58_PREFIX>;
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

parameter_types! {
	pub storage BlocksPerEpoch: u64 = Validator::epoch_number_of_blocks().into();
}

type KeyOwnerIdentification<T, Id> =
	<T as KeyOwnerProofSystem<(KeyTypeId, Id)>>::IdentificationTuple;
type KeyOwnerProof<T, Id> = <T as KeyOwnerProofSystem<(KeyTypeId, Id)>>::Proof;
type GrandpaOffenceReporter<T> = pallet_cf_reputation::ChainflipOffenceReportingAdapter<
	T,
	pallet_grandpa::GrandpaEquivocationOffence<
		<T as pallet_grandpa::Config>::KeyOwnerIdentification,
	>,
	<T as pallet_session::historical::Config>::FullIdentification,
>;

impl pallet_grandpa::Config for Runtime {
	type Event = Event;
	type Call = Call;
	type KeyOwnerProofSystem = Historical;
	type KeyOwnerProof = KeyOwnerProof<Historical, GrandpaId>;
	type KeyOwnerIdentification = KeyOwnerIdentification<Historical, GrandpaId>;
	type HandleEquivocation = pallet_grandpa::EquivocationHandler<
		Self::KeyOwnerIdentification,
		GrandpaOffenceReporter<Self>,
		BlocksPerEpoch,
	>;
	type WeightInfo = ();
	type MaxAuthorities = ConstU32<MAX_AUTHORITIES>;
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = weights::pallet_timestamp::SubstrateWeight<Runtime>;
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
	type StakeHandler = pallet_cf_validator::UpdateBackupAndPassiveAccounts<Self>;
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
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
}

impl pallet_transaction_payment::Config for Runtime {
	type OnChargeTransaction = pallet_cf_flip::FlipTransactionPayment<Self>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee =
		ConstantMultiplier<FlipBalance, ConstU128<{ constants::common::TX_FEE_MULTIPLIER }>>;
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
	type Heartbeat = ChainflipHeartbeat;
	type HeartbeatBlockInterval = ConstU32<HEARTBEAT_BLOCK_INTERVAL>;
	type ReputationPointFloorAndCeiling = ReputationPointFloorAndCeiling;
	type Slasher = FlipSlasher<Self>;
	type WeightInfo = pallet_cf_reputation::weights::PalletWeight<Runtime>;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type MaximumAccruableReputation = MaximumReputationPointAccrued;
}

impl pallet_cf_threshold_signature::Config<EthereumInstance> for Runtime {
	type Event = Event;
	type Offence = chainflip::Offence;
	type RuntimeOrigin = Origin;
	type ThresholdCallable = Call;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type SignerNomination = chainflip::RandomSignerNomination;
	type TargetChain = cf_chains::Ethereum;
	type KeyProvider = EthereumVault;
	type OffenceReporter = Reputation;
	type CeremonyIdProvider = pallet_cf_validator::CeremonyIdProvider<Self>;
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
	type KeyProvider = EthereumVault;
}

impl pallet_cf_chain_tracking::Config<EthereumInstance> for Runtime {
	type Event = Event;
	type TargetChain = Ethereum;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
	type AgeLimit = ConstU64<{ constants::common::eth::BLOCK_SAFETY_MARGIN }>;
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
		Witnesser: pallet_cf_witnesser,
		Auction: pallet_cf_auction,
		Validator: pallet_cf_validator,
		Session: pallet_session,
		Historical: session_historical::{Pallet},
		Aura: pallet_aura,
		Authorship: pallet_authorship,
		Grandpa: pallet_grandpa,
		Governance: pallet_cf_governance,
		EthereumVault: pallet_cf_vaults::<Instance1>,
		Reputation: pallet_cf_reputation,
		EthereumThresholdSigner: pallet_cf_threshold_signature::<Instance1>,
		EthereumBroadcaster: pallet_cf_broadcast::<Instance1>,
		EthereumChainTracking: pallet_cf_chain_tracking::<Instance1>,
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
	(
		migrations::VersionedMigration<
			(
				migrations::DeleteRewardsPallet,
				migrations::UnifyCeremonyIds,
				migrations::migrate_contract_addresses::Migration,
				migrations::add_flip_contract_address::Migration,
				migrations::migrate_claims::Migration,
			),
			112,
		>,
		migrations::VersionedMigration<(migrations::migrate_backup_triage::Migration,), 113>,
	),
>;

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
extern crate frame_benchmarking;

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	define_benchmarks!(
		[frame_benchmarking, BaselineBench::<Runtime>]
		[frame_system, SystemBench::<Runtime>]
		[pallet_timestamp, Timestamp]
		[pallet_cf_environment, Environment]
		[pallet_cf_flip, Flip]
		[pallet_cf_emissions, Emissions]
		[pallet_cf_staking, Staking]
		[pallet_session, SessionBench::<Runtime>]
		[pallet_cf_witnesser, Witnesser]
		[pallet_cf_auction, Auction]
		[pallet_cf_validator, Validator]
		[pallet_cf_governance, Governance]
		[pallet_cf_vaults, EthereumVault]
		[pallet_cf_reputation, Reputation]
		[pallet_cf_threshold_signature, EthereumThresholdSigner]
		[pallet_cf_broadcast, EthereumBroadcaster]
		[pallet_cf_chain_tracking, EthereumChainTracking]
	);
}

impl_runtime_apis! {
	// START custom runtime APIs
	impl runtime_apis::CustomRuntimeApi<Block> for Runtime {
		fn cf_is_auction_phase() -> bool {
			Validator::is_auction_phase()
		}
		fn cf_eth_flip_token_address() -> [u8; 20] {
			Environment::flip_token_address()
		}
		fn cf_eth_stake_manager_address() -> [u8; 20] {
			Environment::stake_manager_address()
		}
		fn cf_eth_key_manager_address() -> [u8; 20] {
			Environment::key_manager_address()
		}
		fn cf_eth_chain_id() -> u64 {
			Environment::ethereum_chain_id()
		}
		fn cf_eth_vault() -> ([u8; 33], BlockNumber) {
			let epoch_index = Self::cf_current_epoch();
			// We should always have a Vault for the current epoch, but in case we do
			// not, just return an empty Vault.
			let vault: Vault<Ethereum> = EthereumVault::vaults(&epoch_index).unwrap_or_default();
			(vault.public_key.to_pubkey_compressed(), vault.active_from_block.unique_saturated_into())
		}
		fn cf_auction_parameters() -> (u32, u32) {
			let auction_params = Auction::auction_parameters();
			(auction_params.min_size, auction_params.max_size)
		}
		fn cf_min_stake() -> u128 {
			MinimumStake::<Runtime>::get().unique_saturated_into()
		}
		fn cf_current_epoch() -> u32 {
			Validator::current_epoch()
		}
		fn cf_epoch_duration() -> u32 {
			Validator::epoch_number_of_blocks()
		}
		fn cf_current_epoch_started_at() -> u32 {
			Validator::current_epoch_started_at()
		}
		fn cf_authority_emission_per_block() -> u128 {
			Emissions::current_authority_emission_per_block()
		}
		fn cf_backup_emission_per_block() -> u128 {
			Emissions::backup_node_emission_per_block()
		}
		fn cf_flip_supply() -> (u128, u128) {
			(Flip::total_issuance(), Flip::offchain_funds())
		}
		fn cf_accounts() -> Vec<(AccountId, Vec<u8>)> {
			let mut vanity_names = Validator::vanity_names();
			pallet_cf_flip::Account::<Runtime>::iter_keys()
				.map(|account_id| {
					let vanity_name = vanity_names.remove(&account_id).unwrap_or_default();
					(account_id, vanity_name)
				})
				.collect()
		}
		fn cf_account_info(account_id: AccountId) -> RuntimeApiAccountInfo {
			let account_info = pallet_cf_flip::Account::<Runtime>::get(&account_id);
			let last_heartbeat = pallet_cf_reputation::LastHeartbeat::<Runtime>::get(&account_id);
			let reputation_info = pallet_cf_reputation::Reputations::<Runtime>::get(&account_id);
			let withdrawal_address = pallet_cf_staking::WithdrawalAddresses::<Runtime>::get(&account_id).unwrap_or([0; 20]);
			let account_data = ChainflipAccountStore::<Runtime>::get(&account_id);

			RuntimeApiAccountInfo {
				stake: account_info.total(),
				bond: account_info.bond(),
				last_heartbeat: last_heartbeat.unwrap_or(0),
				online_credits: reputation_info.online_credits,
				reputation_points: reputation_info.reputation_points,
				withdrawal_address,
				state: account_data.state
			}
		}
		fn cf_pending_claim(account_id: AccountId) -> Option<RuntimeApiPendingClaim> {
			let api_call = pallet_cf_staking::PendingClaims::<Runtime>::get(&account_id)?;
			let pending_claim: RegisterClaim = match api_call {
				eth::api::EthereumApi::RegisterClaim(tx) => tx,
				_ => unreachable!(),
			};
			Some(RuntimeApiPendingClaim {
				amount: pending_claim.amount,
				address: pending_claim.address.into(),
				expiry: pending_claim.expiry,
				sig_data: pending_claim.sig_data,
			})
		}
		fn cf_penalties() -> Vec<(Offence, RuntimeApiPenalty)> {
			pallet_cf_reputation::Penalties::<Runtime>::iter_keys()
				.map(|offence| {
					let penalty = pallet_cf_reputation::Penalties::<Runtime>::get(offence).unwrap_or_default();
					(offence, RuntimeApiPenalty {
						reputation_points: penalty.reputation,
						suspension_duration_blocks: penalty.suspension
					})
				})
				.collect()
		}
		fn cf_suspensions() -> Vec<(Offence, Vec<(u32, AccountId)>)> {
			pallet_cf_reputation::Suspensions::<Runtime>::iter_keys()
				.map(|offence| {
					let suspension = pallet_cf_reputation::Suspensions::<Runtime>::get(offence);
					(offence, suspension.into())
				})
				.collect()
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
		fn on_runtime_upgrade() -> (Weight, Weight) {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here. If any of the pre/post migration checks fail, we shall stop
			// right here and right now.
			let weight = Executive::try_runtime_upgrade().unwrap();
			(weight, BlockWeights::get().max_block)
		}

		fn execute_block_no_check(block: Block) -> Weight {
			Executive::execute_block_no_check(block)
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(extra: bool) -> (
			Vec<frame_benchmarking::BenchmarkList>,
			Vec<frame_support::traits::StorageInfo>,
		) {
			use frame_benchmarking::{baseline, Benchmarking, BenchmarkList};
			use frame_support::traits::StorageInfoTrait;
			use frame_system_benchmarking::Pallet as SystemBench;
			use cf_session_benchmarking::Pallet as SessionBench;
			use baseline::Pallet as BaselineBench;

			let mut list = Vec::<BenchmarkList>::new();

			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();

			(list, storage_info)
		}

		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{baseline, Benchmarking, BenchmarkBatch, TrackedStorageKey};

			use frame_system_benchmarking::Pallet as SystemBench;
			use baseline::Pallet as BaselineBench;
			use cf_session_benchmarking::Pallet as SessionBench;

			impl cf_session_benchmarking::Config for Runtime {}
			impl frame_system_benchmarking::Config for Runtime {}
			impl baseline::Config for Runtime {}

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
			add_benchmarks!(params, batches);

			Ok(batches)
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

	const CALL_ENUM_MAX_SIZE: usize = 320;

	// Introduced from polkdadot
	#[test]
	fn call_size() {
		assert!(
			core::mem::size_of::<Call>() <= CALL_ENUM_MAX_SIZE,
			r"
			Polkadot suggests a 230 byte limit for the size of the Call type. We use {} but this runtime's call size
			is {}. If this test fails then you have just added a call variant that exceed the limit.

			Congratulations!

			Maybe consider boxing some calls to reduce their size. Otherwise, increasing the CALL_ENUM_MAX_SIZE is
			acceptable (within reason). The issue is that the enum always uses max(enum_size) of memory, even if your
			are using a smaller variant. Note this is irrelevant from a SCALE-encoding POV, it only affects the size of
			the enum on the stack.
			Context:
			  - https://github.com/paritytech/substrate/pull/9418
			  - https://rust-lang.github.io/rust-clippy/master/#large_enum_variant
			  - https://fasterthanli.me/articles/peeking-inside-a-rust-enum
			",
			CALL_ENUM_MAX_SIZE,
			core::mem::size_of::<Call>(),
		);
	}
}
