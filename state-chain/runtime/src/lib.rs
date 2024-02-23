#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]
pub mod chainflip;
pub mod constants;
pub mod migrations;
pub mod runtime_apis;
pub mod safe_mode;
#[cfg(feature = "std")]
pub mod test_runner;
mod weights;
use crate::{
	chainflip::{calculate_account_apy, Offence},
	runtime_apis::{
		AuctionState, DispatchErrorWithMessage, FailingWitnessValidators, LiquidityProviderInfo,
		RuntimeApiAccountInfoV2, RuntimeApiPenalty,
	},
};
use cf_amm::{
	common::{Amount, PoolPairsMap, Side, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	assets::any::ForeignChainAndAsset,
	btc::{BitcoinCrypto, BitcoinRetryPolicy},
	dot::{self, PolkadotCrypto},
	eth::{self, api::EthereumApi, Address as EthereumAddress, Ethereum},
	evm::EvmCrypto,
	sol::SolanaCrypto,
	Bitcoin, CcmChannelMetadata, DefaultRetryPolicy, FeeEstimationApi, ForeignChain, Polkadot,
	Solana, TransactionBuilder,
};
use cf_primitives::{BroadcastId, NetworkEnvironment};
use cf_traits::{AssetConverter, GetTrackedData, LpBalanceApi};
use core::ops::Range;
pub use frame_system::Call as SystemCall;
use pallet_cf_governance::GovCallHash;
use pallet_cf_ingress_egress::{ChannelAction, DepositWitness};
use pallet_cf_pools::{
	AskBidMap, AssetPair, PoolLiquidity, PoolOrderbook, PoolPriceV1, PoolPriceV2,
	UnidirectionalPoolDepth,
};
use pallet_cf_reputation::ExclusionList;
use pallet_cf_swapping::CcmSwapAmounts;
use pallet_cf_validator::SetSizeMaximisingAuctionResolver;
use pallet_transaction_payment::{ConstFeeMultiplier, Multiplier};
use scale_info::prelude::string::String;
use sp_std::collections::btree_map::BTreeMap;

pub use frame_support::{
	construct_runtime, debug,
	instances::{Instance1, Instance2, Instance3},
	parameter_types,
	traits::{
		ConstBool, ConstU128, ConstU16, ConstU32, ConstU64, ConstU8, Get, KeyOwnerProofSystem,
		Randomness, StorageInfo,
	},
	weights::{
		constants::{
			BlockExecutionWeight, ExtrinsicBaseWeight, ParityDbWeight as DbWeight,
			WEIGHT_REF_TIME_PER_SECOND,
		},
		ConstantMultiplier, IdentityFee, Weight,
	},
	StorageValue,
};
use frame_system::offchain::SendTransactionTypes;
use pallet_cf_funding::MinimumFunding;
use pallet_cf_pools::{PoolInfo, PoolOrders};
use pallet_grandpa::AuthorityId as GrandpaId;
use pallet_session::historical as session_historical;
pub use pallet_timestamp::Call as TimestampCall;
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::traits::{
	AccountIdLookup, BlakeTwo256, Block as BlockT, ConvertInto, IdentifyAccount, NumberFor, One,
	OpaqueKeys, UniqueSaturatedInto, Verify,
};

use frame_support::genesis_builder_helper::{build_config, create_default_config};
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

pub use cf_primitives::{
	chains::assets::any, AccountRole, Asset, AssetAmount, BlockNumber, FlipBalance, SemVer,
	SwapOutput,
};
pub use cf_traits::{
	AccountInfo, BidderProvider, CcmHandler, Chainflip, EpochInfo, PoolApi, QualifyNode,
	SessionKeysRegistered, SwappingApi,
};
// Required for genesis config.
pub use pallet_cf_validator::SetSizeParameters;

pub use chainflip::chain_instances::*;
use chainflip::{
	epoch_transition::ChainflipEpochTransitions, BroadcastReadyProvider, BtcEnvironment,
	ChainAddressConverter, ChainflipHeartbeat, DotEnvironment, EthEnvironment, SolanaEnvironment,
	TokenholderGovernanceBroadcaster,
};
use safe_mode::{RuntimeSafeMode, WitnesserCallPermission};

use constants::common::*;
use pallet_cf_flip::{Bonder, FlipSlasher};
pub use pallet_transaction_payment::ChargeTransactionPayment;

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Nonce of a transaction in the chain.
pub type Nonce = u32;

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
	spec_version: 130,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 12,
	state_version: 1,
};

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

impl pallet_cf_validator::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Offence = chainflip::Offence;
	type EpochTransitionHandler = ChainflipEpochTransitions;
	type ValidatorWeightInfo = pallet_cf_validator::weights::PalletWeight<Runtime>;
	type KeyRotator =
		cons_key_rotator!(EthereumThresholdSigner, PolkadotThresholdSigner, BitcoinThresholdSigner);
	type MissedAuthorshipSlots = chainflip::MissedAuraSlots;
	type BidderProvider = pallet_cf_funding::Pallet<Self>;
	type KeygenQualification = (
		Reputation,
		(
			ExclusionList<Self, chainflip::KeygenExclusionOffences>,
			(
				pallet_cf_validator::PeerMapping<Self>,
				(
					SessionKeysRegistered<Self, pallet_session::Pallet<Self>>,
					(
						chainflip::ValidatorRoleQualification,
						pallet_cf_validator::QualifyByCfeVersion<Self>,
					),
				),
			),
		),
	);
	type OffenceReporter = Reputation;
	type Bonder = Bonder<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type ReputationResetter = Reputation;
	type CfePeerRegistration = CfeInterface;
}

parameter_types! {
	pub CurrentReleaseVersion: SemVer = SemVer {
		major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().expect("Cargo version must be set"),
		minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().expect("Cargo version must be set"),
		patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().expect("Cargo version must be set"),
	};
}

impl pallet_cf_environment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type PolkadotVaultKeyWitnessedHandler = PolkadotVault;
	type BitcoinVaultKeyWitnessedHandler = BitcoinVault;
	type BitcoinFeeInfo = chainflip::BitcoinFeeGetter;
	type RuntimeSafeMode = RuntimeSafeMode;
	type CurrentReleaseVersion = CurrentReleaseVersion;
	type WeightInfo = pallet_cf_environment::weights::PalletWeight<Runtime>;
}

impl pallet_cf_swapping::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type DepositHandler = chainflip::AnyChainIngressEgressHandler;
	type EgressHandler = chainflip::AnyChainIngressEgressHandler;
	type SwappingApi = LiquidityPools;
	type AddressConverter = ChainAddressConverter;
	type SafeMode = RuntimeSafeMode;
	type WeightInfo = pallet_cf_swapping::weights::PalletWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type FeePayment = Flip;
}

impl pallet_cf_vaults::Config<EthereumInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Chain = Ethereum;
	type SetAggKeyWithAggKey = eth::api::EthereumApi<EthEnvironment>;
	type Broadcaster = EthereumBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = EthereumChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<PolkadotInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Chain = Polkadot;
	type SetAggKeyWithAggKey = dot::api::PolkadotApi<DotEnvironment>;
	type Broadcaster = PolkadotBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = PolkadotChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<BitcoinInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Chain = Bitcoin;
	type SetAggKeyWithAggKey = cf_chains::btc::api::BitcoinApi<BtcEnvironment>;
	type Broadcaster = BitcoinBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = BitcoinChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<SolanaInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Chain = Solana;
	type SetAggKeyWithAggKey = cf_chains::sol::api::SolanaApi<SolanaEnvironment>;
	type Broadcaster = SolanaBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = SolanaChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

use chainflip::address_derivation::AddressDerivation;

impl pallet_cf_ingress_egress::Config<EthereumInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type TargetChain = Ethereum;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type LpBalance = LiquidityProvider;
	type SwapDepositHandler = Swapping;
	type ChainApiCall = eth::api::EthereumApi<EthEnvironment>;
	type Broadcaster = EthereumBroadcaster;
	type DepositHandler = chainflip::EthDepositHandler;
	type CcmHandler = Swapping;
	type ChainTracking = EthereumChainTracking;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type NetworkEnvironment = Environment;
	type AssetConverter = LiquidityPools;
	type FeePayment = Flip;
}

impl pallet_cf_ingress_egress::Config<PolkadotInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type TargetChain = Polkadot;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type LpBalance = LiquidityProvider;
	type SwapDepositHandler = Swapping;
	type ChainApiCall = dot::api::PolkadotApi<chainflip::DotEnvironment>;
	type Broadcaster = PolkadotBroadcaster;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type DepositHandler = chainflip::DotDepositHandler;
	type ChainTracking = PolkadotChainTracking;
	type CcmHandler = Swapping;
	type NetworkEnvironment = Environment;
	type AssetConverter = LiquidityPools;
	type FeePayment = Flip;
}

impl pallet_cf_ingress_egress::Config<BitcoinInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type TargetChain = Bitcoin;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type LpBalance = LiquidityProvider;
	type SwapDepositHandler = Swapping;
	type ChainApiCall = cf_chains::btc::api::BitcoinApi<chainflip::BtcEnvironment>;
	type Broadcaster = BitcoinBroadcaster;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type DepositHandler = chainflip::BtcDepositHandler;
	type ChainTracking = BitcoinChainTracking;
	type CcmHandler = Swapping;
	type NetworkEnvironment = Environment;
	type AssetConverter = LiquidityPools;
	type FeePayment = Flip;
}

impl pallet_cf_ingress_egress::Config<SolanaInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type TargetChain = Solana;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type LpBalance = LiquidityProvider;
	type SwapDepositHandler = Swapping;
	type ChainApiCall = cf_chains::sol::api::SolanaApi<chainflip::SolanaEnvironment>;
	type Broadcaster = SolanaBroadcaster;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type DepositHandler = chainflip::SolDepositHandler;
	type ChainTracking = SolanaChainTracking;
	type CcmHandler = Swapping;
	type NetworkEnvironment = Environment;
	type AssetConverter = LiquidityPools;
	type FeePayment = Flip;
}

parameter_types! {
	pub const NetworkFee: Permill = Permill::from_perthousand(1);
}

impl pallet_cf_pools::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type LpBalance = LiquidityProvider;
	type NetworkFee = NetworkFee;
	type SafeMode = RuntimeSafeMode;
	type WeightInfo = ();
}

impl pallet_cf_lp::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type DepositHandler = chainflip::AnyChainIngressEgressHandler;
	type EgressHandler = chainflip::AnyChainIngressEgressHandler;
	type AddressConverter = ChainAddressConverter;
	type SafeMode = RuntimeSafeMode;
	type PoolApi = LiquidityPools;
	type WeightInfo = pallet_cf_lp::weights::PalletWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type FeePayment = Flip;
}

impl pallet_cf_account_roles::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type WeightInfo = pallet_cf_account_roles::weights::PalletWeight<Runtime>;
}

impl<LocalCall> SendTransactionTypes<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	type Extrinsic = UncheckedExtrinsic;
	type OverarchingCall = RuntimeCall;
}

impl pallet_session::Config for Runtime {
	type SessionHandler = <opaque::SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type ShouldEndSession = Validator;
	type SessionManager = Validator;
	type RuntimeEvent = RuntimeEvent;
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
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::with_sensible_defaults(
			Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
			NORMAL_DISPATCH_RATIO,
		);
	pub BlockLength: frame_system::limits::BlockLength = frame_system::limits::BlockLength
		::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
}

// Configure FRAME pallets to include in runtime.
impl frame_system::Config for Runtime {
	/// The basic call filter to use in dispatchable.
	type BaseCallFilter = frame_support::traits::Everything;
	/// The block type for the runtime.
	type Block = Block;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = BlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = BlockLength;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The aggregated dispatch type that is available for extrinsics.
	type RuntimeCall = RuntimeCall;
	/// The type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The lookup mechanism to get account ID from whatever is passed in dispatchers.
	type Lookup = AccountIdLookup<AccountId, ()>;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The hashing algorithm used.
	type Hashing = BlakeTwo256;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	/// The ubiquitous origin type.
	type RuntimeOrigin = RuntimeOrigin;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = DbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// Converts a module to the index of the module in `construct_runtime!`.
	///
	/// This type is being generated by `construct_runtime!`.
	type PalletInfo = PalletInfo;
	/// What to do if a new account is created.
	type OnNewAccount = AccountRoles;
	/// What to do if an account is fully reaped from the system.
	type OnKilledAccount = (
		pallet_cf_flip::BurnFlipAccount<Self>,
		pallet_cf_validator::DeletePeerMapping<Self>,
		pallet_cf_validator::DeleteVanityName<Self>,
		GrandpaOffenceReporter<Self>,
		Funding,
		AccountRoles,
		Reputation,
	);
	/// The data to be stored in an account.
	type AccountData = ();
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = weights::frame_system::SubstrateWeight<Runtime>;
	/// This is used as an identifier of the chain.
	type SS58Prefix = ConstU16<CHAINFLIP_SS58_PREFIX>;
	/// The set code logic, just the default since we're not a parachain.
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
	type RuntimeTask = ();
}

impl frame_system::offchain::SigningTypes for Runtime {
	type Public = <Signature as Verify>::Signer;
	type Signature = Signature;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<MAX_AUTHORITIES>;
	type AllowMultipleBlocksPerSlot = ConstBool<false>;
}

parameter_types! {
	pub storage BlocksPerEpoch: u64 = Validator::blocks_per_epoch().into();
}

type KeyOwnerIdentification<T, Id> =
	<T as KeyOwnerProofSystem<(KeyTypeId, Id)>>::IdentificationTuple;
type GrandpaOffenceReporter<T> = pallet_cf_reputation::ChainflipOffenceReportingAdapter<
	T,
	pallet_grandpa::EquivocationOffence<KeyOwnerIdentification<Historical, GrandpaId>>,
	<T as pallet_session::historical::Config>::FullIdentification,
>;

impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type MaxAuthorities = ConstU32<MAX_AUTHORITIES>;
	// Note: We don't use nomination.
	type MaxNominators = ConstU32<0>;

	type MaxSetIdSessionEntries = ConstU64<8>;
	type KeyOwnerProof = sp_session::MembershipProof;
	type EquivocationReportSystem = pallet_grandpa::EquivocationReportSystem<
		Self,
		GrandpaOffenceReporter<Self>,
		Historical,
		ConstU64<14400>,
	>;
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
	type EventHandler = ();
}

impl pallet_cf_flip::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = FlipBalance;
	type BlocksPerDay = ConstU32<DAYS>;
	type OnAccountFunded = pallet_cf_validator::UpdateBackupMapping<Self>;
	type WeightInfo = pallet_cf_flip::weights::PalletWeight<Runtime>;
	type WaivedFees = chainflip::WaivedFees;
}

impl pallet_cf_witnesser::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type SafeMode = RuntimeSafeMode;
	type CallDispatchPermission = WitnesserCallPermission;
	type Offence = chainflip::Offence;
	type OffenceReporter = Reputation;
	type LateWitnessGracePeriod = ConstU32<LATE_WITNESS_GRACE_PERIOD>;
	type WeightInfo = pallet_cf_witnesser::weights::PalletWeight<Runtime>;
}

impl pallet_cf_funding::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ThresholdCallable = RuntimeCall;
	type FunderId = AccountId;
	type Flip = Flip;
	type Broadcaster = EthereumBroadcaster;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, Instance1>;
	type RegisterRedemption = EthereumApi<EthEnvironment>;
	type TimeSource = Timestamp;
	type SafeMode = RuntimeSafeMode;
	type WeightInfo = pallet_cf_funding::weights::PalletWeight<Runtime>;
}

impl pallet_cf_tokenholder_governance::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type FeePayment = Flip;
	type WeightInfo = pallet_cf_tokenholder_governance::weights::PalletWeight<Runtime>;
	type VotingPeriod = ConstU32<{ 14 * DAYS }>;
	type AnyChainGovKeyBroadcaster = TokenholderGovernanceBroadcaster;
	type CommKeyBroadcaster = TokenholderGovernanceBroadcaster;
	type ProposalFee = ConstU128<{ 1_000 * FLIPPERINOS_PER_FLIP }>;
	type EnactmentDelay = ConstU32<{ 7 * DAYS }>;
}

impl pallet_cf_governance::Config for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type TimeSource = Timestamp;
	type WeightInfo = pallet_cf_governance::weights::PalletWeight<Runtime>;
	type UpgradeCondition = pallet_cf_validator::NotDuringRotation<Runtime>;
	type RuntimeUpgrade = chainflip::RuntimeUpgradeManager;
	type CompatibleCfeVersions = Environment;
	type AuthoritiesCfeVersions = Validator;
}

impl pallet_cf_emissions::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type HostChain = Ethereum;
	type FlipBalance = FlipBalance;
	type ApiCall = eth::api::EthereumApi<EthEnvironment>;
	type Broadcaster = EthereumBroadcaster;
	type Surplus = pallet_cf_flip::Surplus<Runtime>;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;
	type RewardsDistribution = chainflip::BlockAuthorRewardDistribution;
	type CompoundingInterval = ConstU32<COMPOUNDING_INTERVAL>;
	type EthEnvironment = EthEnvironment;
	type FlipToBurn = LiquidityPools;
	type EgressHandler = pallet_cf_ingress_egress::Pallet<Runtime, EthereumInstance>;
	type SafeMode = RuntimeSafeMode;
	type WeightInfo = pallet_cf_emissions::weights::PalletWeight<Runtime>;
}

parameter_types! {
	pub FeeMultiplier: Multiplier = Multiplier::one();
}

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = pallet_cf_flip::FlipTransactionPayment<Self>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = ConstantMultiplier<FlipBalance, ConstU128<{ TX_FEE_MULTIPLIER }>>;
	type LengthToFee = ConstantMultiplier<FlipBalance, ConstU128<1_000_000>>;
	type FeeMultiplierUpdate = ConstFeeMultiplier<FeeMultiplier>;
}

parameter_types! {
	pub const ReputationPointFloorAndCeiling: (i32, i32) = (-2880, 2880);
	pub const MaximumAccruableReputation: pallet_cf_reputation::ReputationPoints = 15;
}

impl pallet_cf_cfe_interface::Config for Runtime {
	type WeightInfo = pallet_cf_cfe_interface::PalletWeight<Runtime>;
}

impl pallet_cf_reputation::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Offence = chainflip::Offence;
	type Heartbeat = ChainflipHeartbeat;
	type HeartbeatBlockInterval = ConstU32<HEARTBEAT_BLOCK_INTERVAL>;
	type ReputationPointFloorAndCeiling = ReputationPointFloorAndCeiling;
	type Slasher = FlipSlasher<Self>;
	type WeightInfo = pallet_cf_reputation::weights::PalletWeight<Runtime>;
	type MaximumAccruableReputation = MaximumAccruableReputation;
	type SafeMode = RuntimeSafeMode;
}

impl pallet_cf_threshold_signature::Config<EthereumInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Offence = chainflip::Offence;
	type RuntimeOrigin = RuntimeOrigin;
	type ThresholdCallable = RuntimeCall;
	type ThresholdSignerNomination = chainflip::RandomSignerNomination;
	type TargetChainCrypto = EvmCrypto;
	type VaultActivator = EthereumVault;
	type OffenceReporter = Reputation;
	type CeremonyRetryDelay = ConstU32<1>;
	type SafeMode = RuntimeSafeMode;
	type Slasher = FlipSlasher<Self>;
	type CfeMultisigRequest = CfeInterface;
	type Weights = pallet_cf_threshold_signature::weights::PalletWeight<Self>;
}

impl pallet_cf_threshold_signature::Config<PolkadotInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Offence = chainflip::Offence;
	type RuntimeOrigin = RuntimeOrigin;
	type ThresholdCallable = RuntimeCall;
	type ThresholdSignerNomination = chainflip::RandomSignerNomination;
	type TargetChainCrypto = PolkadotCrypto;
	type VaultActivator = PolkadotVault;
	type OffenceReporter = Reputation;
	type CeremonyRetryDelay = ConstU32<1>;
	type SafeMode = RuntimeSafeMode;
	type Slasher = FlipSlasher<Self>;
	type CfeMultisigRequest = CfeInterface;
	type Weights = pallet_cf_threshold_signature::weights::PalletWeight<Self>;
}

impl pallet_cf_threshold_signature::Config<BitcoinInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Offence = chainflip::Offence;
	type RuntimeOrigin = RuntimeOrigin;
	type ThresholdCallable = RuntimeCall;
	type ThresholdSignerNomination = chainflip::RandomSignerNomination;
	type TargetChainCrypto = BitcoinCrypto;
	type VaultActivator = BitcoinVault;
	type OffenceReporter = Reputation;
	type CeremonyRetryDelay = ConstU32<1>;
	type SafeMode = RuntimeSafeMode;
	type Slasher = FlipSlasher<Self>;
	type CfeMultisigRequest = CfeInterface;
	type Weights = pallet_cf_threshold_signature::weights::PalletWeight<Self>;
}

impl pallet_cf_threshold_signature::Config<SolanaInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Offence = chainflip::Offence;
	type RuntimeOrigin = RuntimeOrigin;
	type ThresholdCallable = RuntimeCall;
	type ThresholdSignerNomination = chainflip::RandomSignerNomination;
	type TargetChainCrypto = SolanaCrypto;
	type VaultActivator = SolanaVault;
	type OffenceReporter = Reputation;
	type CeremonyRetryDelay = ConstU32<1>;
	type SafeMode = RuntimeSafeMode;
	type Slasher = FlipSlasher<Self>;
	type CfeMultisigRequest = CfeInterface;
	type Weights = pallet_cf_threshold_signature::weights::PalletWeight<Self>;
}

impl pallet_cf_broadcast::Config<EthereumInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type TargetChain = Ethereum;
	type ApiCall = eth::api::EthereumApi<EthEnvironment>;
	type ThresholdSigner = EthereumThresholdSigner;
	type TransactionBuilder = chainflip::EthTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, EthereumInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type BroadcastTimeout = ConstU32<{ 10 * MINUTES }>;
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type SafeModeBlockMargin = ConstU32<10>;
	type ChainTracking = EthereumChainTracking;
	type RetryPolicy = DefaultRetryPolicy;
	type CfeBroadcastRequest = CfeInterface;
}

impl pallet_cf_broadcast::Config<PolkadotInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type TargetChain = Polkadot;
	type ApiCall = dot::api::PolkadotApi<DotEnvironment>;
	type ThresholdSigner = PolkadotThresholdSigner;
	type TransactionBuilder = chainflip::DotTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, PolkadotInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type BroadcastTimeout = ConstU32<{ 10 * MINUTES }>;
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type SafeModeBlockMargin = ConstU32<10>;
	type ChainTracking = PolkadotChainTracking;
	type RetryPolicy = DefaultRetryPolicy;
	type CfeBroadcastRequest = CfeInterface;
}

impl pallet_cf_broadcast::Config<BitcoinInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type TargetChain = Bitcoin;
	type ApiCall = cf_chains::btc::api::BitcoinApi<BtcEnvironment>;
	type ThresholdSigner = BitcoinThresholdSigner;
	type TransactionBuilder = chainflip::BtcTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, BitcoinInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type BroadcastTimeout = ConstU32<{ 90 * MINUTES }>;
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type SafeModeBlockMargin = ConstU32<10>;
	type ChainTracking = BitcoinChainTracking;
	type RetryPolicy = BitcoinRetryPolicy;
	type CfeBroadcastRequest = CfeInterface;
}

impl pallet_cf_broadcast::Config<SolanaInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type TargetChain = Solana;
	type ApiCall = cf_chains::sol::api::SolanaApi<SolanaEnvironment>;
	type ThresholdSigner = SolanaThresholdSigner;
	type TransactionBuilder = chainflip::SolanaTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, SolanaInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type BroadcastTimeout = ConstU32<{ 90 * MINUTES }>;
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type SafeModeBlockMargin = ConstU32<10>;
	type ChainTracking = SolanaChainTracking;
	type RetryPolicy = DefaultRetryPolicy;
	type CfeBroadcastRequest = CfeInterface;
}

impl pallet_cf_chain_tracking::Config<EthereumInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Ethereum;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
}

impl pallet_cf_chain_tracking::Config<PolkadotInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Polkadot;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
}

impl pallet_cf_chain_tracking::Config<BitcoinInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Bitcoin;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
}

impl pallet_cf_chain_tracking::Config<SolanaInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Solana;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
}

construct_runtime!(
	pub struct Runtime
	{
		System: frame_system,
		Timestamp: pallet_timestamp,
		Environment: pallet_cf_environment,
		Flip: pallet_cf_flip,
		Emissions: pallet_cf_emissions,
		// AccountRoles after funding, since account creation comes first.
		Funding: pallet_cf_funding,
		AccountRoles: pallet_cf_account_roles,
		TransactionPayment: pallet_transaction_payment,
		Witnesser: pallet_cf_witnesser,
		Validator: pallet_cf_validator,
		Session: pallet_session,
		Historical: session_historical::{Pallet},
		Aura: pallet_aura,
		Authorship: pallet_authorship,
		Grandpa: pallet_grandpa,
		Governance: pallet_cf_governance,
		TokenholderGovernance: pallet_cf_tokenholder_governance,
		Reputation: pallet_cf_reputation,

		EthereumChainTracking: pallet_cf_chain_tracking::<Instance1>,
		PolkadotChainTracking: pallet_cf_chain_tracking::<Instance2>,
		BitcoinChainTracking: pallet_cf_chain_tracking::<Instance3>,
		SolanaChainTracking: pallet_cf_chain_tracking::<Instance4>,

		EthereumVault: pallet_cf_vaults::<Instance1>,
		PolkadotVault: pallet_cf_vaults::<Instance2>,
		BitcoinVault: pallet_cf_vaults::<Instance3>,
		SolanaVault: pallet_cf_vaults::<Instance4>,

		EthereumThresholdSigner: pallet_cf_threshold_signature::<Instance1>,
		PolkadotThresholdSigner: pallet_cf_threshold_signature::<Instance2>,
		BitcoinThresholdSigner: pallet_cf_threshold_signature::<Instance3>,
		SolanaThresholdSigner: pallet_cf_threshold_signature::<Instance4>,

		EthereumBroadcaster: pallet_cf_broadcast::<Instance1>,
		PolkadotBroadcaster: pallet_cf_broadcast::<Instance2>,
		BitcoinBroadcaster: pallet_cf_broadcast::<Instance3>,
		SolanaBroadcaster: pallet_cf_broadcast::<Instance4>,

		Swapping: pallet_cf_swapping,
		LiquidityProvider: pallet_cf_lp,

		EthereumIngressEgress: pallet_cf_ingress_egress::<Instance1>,
		PolkadotIngressEgress: pallet_cf_ingress_egress::<Instance2>,
		BitcoinIngressEgress: pallet_cf_ingress_egress::<Instance3>,
		SolanaIngressEgress: pallet_cf_ingress_egress::<Instance4>,

		LiquidityPools: pallet_cf_pools,

		CfeInterface: pallet_cf_cfe_interface,
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
pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<RuntimeCall, SignedExtra>;
/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, RuntimeCall, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	PalletExecutionOrder,
	PalletMigrations,
>;

pub type PalletExecutionOrder = (
	System,
	Timestamp,
	CfeInterface,
	Environment,
	Flip,
	Emissions,
	Funding,
	AccountRoles,
	TransactionPayment,
	Witnesser,
	Validator,
	Session,
	Historical,
	Aura,
	Authorship,
	Grandpa,
	Governance,
	TokenholderGovernance,
	Reputation,
	EthereumChainTracking,
	PolkadotChainTracking,
	BitcoinChainTracking,
	SolanaChainTracking,
	EthereumVault,
	PolkadotVault,
	BitcoinVault,
	SolanaVault,
	EthereumThresholdSigner,
	PolkadotThresholdSigner,
	BitcoinThresholdSigner,
	SolanaThresholdSigner,
	EthereumBroadcaster,
	PolkadotBroadcaster,
	BitcoinBroadcaster,
	SolanaBroadcaster,
	Swapping,
	LiquidityProvider,
	EthereumIngressEgress,
	PolkadotIngressEgress,
	BitcoinIngressEgress,
	SolanaIngressEgress,
	LiquidityPools,
);

// Pallet Migrations for each pallet.
// We use the executive pallet because the `pre_upgrade` and `post_upgrade` hooks are noops
// for tuple migrations (like these).
type PalletMigrations = (
	// DO NOT REMOVE `VersionUpdate`. THIS IS REQUIRED TO UPDATE THE VERSION FOR THE CFES EVERY
	// UPGRADE
	pallet_cf_environment::migrations::VersionUpdate<Runtime>,
	pallet_cf_environment::migrations::PalletMigration<Runtime>,
	pallet_cf_funding::migrations::PalletMigration<Runtime>,
	// pallet_cf_validator::migrations::PalletMigration<Runtime>,
	pallet_grandpa::migrations::MigrateV4ToV5<Runtime>,
	pallet_cf_governance::migrations::PalletMigration<Runtime>,
	// pallet_cf_tokenholder_governance::migrations::PalletMigration<Runtime>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, Instance1>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, Instance2>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, Instance3>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, Instance1>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, Instance2>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, Instance3>,
	// TODO: Remove this after version 1.3 release.
	ThresholdSignatureRefactorMigration,
	pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, Instance1>,
	pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, Instance2>,
	pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, Instance3>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, Instance1>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, Instance2>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, Instance3>,
	pallet_cf_swapping::migrations::PalletMigration<Runtime>,
	// pallet_cf_lp::migrations::PalletMigration<Runtime>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, Instance1>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, Instance2>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, Instance3>,
	// pallet_cf_pools::migrations::PalletMigration<Runtime>,
);

pub struct ThresholdSignatureRefactorMigration;

mod threshold_signature_refactor_migration {
	use super::Runtime;
	use cf_runtime_upgrade_utilities::move_pallet_storage;
	use frame_support::traits::GetStorageVersion;

	pub fn migrate_instance<I: 'static>()
	where
		Runtime: pallet_cf_threshold_signature::Config<I>,
		Runtime: pallet_cf_vaults::Config<I>,
	{
		// The migration needs to be run *after* the vaults pallet migration (3 -> 4) and *before*
		// the threshold signer pallet migration (4 -> 5).
		if <pallet_cf_threshold_signature::Pallet::<Runtime, I> as GetStorageVersion>::on_chain_storage_version() == 4 &&
			<pallet_cf_vaults::Pallet::<Runtime, I> as GetStorageVersion>::on_chain_storage_version() == 4 {

				log::info!("✅ Applying threshold signature refactor storage migration.");
				for storage_name in [
					"CeremonyIdCounter",
					"KeygenSlashAmount",
					"Vaults",
				] {
					move_pallet_storage::<
						pallet_cf_vaults::Pallet<Runtime, I>,
						pallet_cf_threshold_signature::Pallet<Runtime, I>,
					>(storage_name.as_bytes());
				}
			} else {
				log::info!("⏭ Skipping threshold signature refactor migration.");
			}
	}
}

impl frame_support::traits::OnRuntimeUpgrade for ThresholdSignatureRefactorMigration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("⏫ Applying threshold signature refactor storage migration.");
		threshold_signature_refactor_migration::migrate_instance::<EthereumInstance>();
		threshold_signature_refactor_migration::migrate_instance::<BitcoinInstance>();
		threshold_signature_refactor_migration::migrate_instance::<PolkadotInstance>();

		Default::default()
	}
}

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
		[pallet_cf_funding, Funding]
		[pallet_session, SessionBench::<Runtime>]
		[pallet_cf_witnesser, Witnesser]
		[pallet_cf_validator, Validator]
		[pallet_cf_governance, Governance]
		[pallet_cf_tokenholder_governance, TokenholderGovernance]
		[pallet_cf_vaults, EthereumVault]
		[pallet_cf_reputation, Reputation]
		[pallet_cf_threshold_signature, EthereumThresholdSigner]
		[pallet_cf_broadcast, EthereumBroadcaster]
		[pallet_cf_chain_tracking, EthereumChainTracking]
		[pallet_cf_swapping, Swapping]
		[pallet_cf_account_roles, AccountRoles]
		[pallet_cf_ingress_egress, EthereumIngressEgress]
		[pallet_cf_lp, LiquidityProvider]
		[pallet_cf_pools, LiquidityPools]
		[pallet_cf_cfe_interface, CfeInterface]
	);
}

impl_runtime_apis! {
	// START custom runtime APIs
	impl runtime_apis::CustomRuntimeApi<Block> for Runtime {
		fn cf_is_auction_phase() -> bool {
			Validator::is_auction_phase()
		}
		fn cf_eth_flip_token_address() -> EthereumAddress {
			Environment::supported_eth_assets(cf_primitives::chains::assets::eth::Asset::Flip).expect("FLIP token address should exist")
		}
		fn cf_eth_state_chain_gateway_address() -> EthereumAddress {
			Environment::state_chain_gateway_address()
		}
		fn cf_eth_key_manager_address() -> EthereumAddress {
			Environment::key_manager_address()
		}
		fn cf_eth_chain_id() -> u64 {
			Environment::ethereum_chain_id()
		}
		fn cf_eth_vault() -> ([u8; 33], BlockNumber) {
			let epoch_index = Self::cf_current_epoch();
			// We should always have a Vault for the current epoch, but in case we do
			// not, just return an empty Vault.
			(EthereumThresholdSigner::keys(epoch_index).unwrap_or_default().to_pubkey_compressed(), EthereumVault::vault_start_block_numbers(epoch_index).unwrap().unique_saturated_into())
		}
		fn cf_auction_parameters() -> (u32, u32) {
			let auction_params = Validator::auction_parameters();
			(auction_params.min_size, auction_params.max_size)
		}
		fn cf_min_funding() -> u128 {
			MinimumFunding::<Runtime>::get().unique_saturated_into()
		}
		fn cf_current_epoch() -> u32 {
			Validator::current_epoch()
		}
		fn cf_current_compatibility_version() -> SemVer {
			Environment::current_release_version()
		}
		fn cf_epoch_duration() -> u32 {
			Validator::blocks_per_epoch()
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
			frame_system::Account::<Runtime>::iter_keys()
				.map(|account_id| {
					let vanity_name = vanity_names.remove(&account_id).unwrap_or_default();
					(account_id, vanity_name)
				})
				.collect()
		}
		fn cf_asset_balances(account_id: AccountId) -> Vec<(Asset, u128)> {
			LiquidityProvider::asset_balances(&account_id)
		}
		fn cf_account_flip_balance(account_id: &AccountId) -> u128 {
			pallet_cf_flip::Account::<Runtime>::get(account_id).total()
		}
		fn cf_account_info_v2(account_id: &AccountId) -> RuntimeApiAccountInfoV2 {
			let is_current_backup = pallet_cf_validator::Backups::<Runtime>::get().contains_key(account_id);
			let key_holder_epochs = pallet_cf_validator::HistoricalActiveEpochs::<Runtime>::get(account_id);
			let is_qualified = <<Runtime as pallet_cf_validator::Config>::KeygenQualification as QualifyNode<_>>::is_qualified(account_id);
			let is_current_authority = pallet_cf_validator::CurrentAuthorities::<Runtime>::get().contains(account_id);
			let is_bidding = pallet_cf_funding::ActiveBidder::<Runtime>::get(account_id);
			let bound_redeem_address = pallet_cf_funding::BoundRedeemAddress::<Runtime>::get(account_id);
			let apy_bp = calculate_account_apy(account_id);
			let reputation_info = pallet_cf_reputation::Reputations::<Runtime>::get(account_id);
			let account_info = pallet_cf_flip::Account::<Runtime>::get(account_id);
			let restricted_balances = pallet_cf_funding::RestrictedBalances::<Runtime>::get(account_id);
			RuntimeApiAccountInfoV2 {
				balance: account_info.total(),
				bond: account_info.bond(),
				last_heartbeat: pallet_cf_reputation::LastHeartbeat::<Runtime>::get(account_id).unwrap_or(0),
				reputation_points: reputation_info.reputation_points,
				keyholder_epochs: key_holder_epochs,
				is_current_authority,
				is_current_backup,
				is_qualified: is_bidding && is_qualified,
				is_online: Reputation::is_qualified(account_id),
				is_bidding,
				bound_redeem_address,
				apy_bp,
				restricted_balances,
			}
		}

		fn cf_penalties() -> Vec<(Offence, RuntimeApiPenalty)> {
			pallet_cf_reputation::Penalties::<Runtime>::iter_keys()
				.map(|offence| {
					let penalty = pallet_cf_reputation::Penalties::<Runtime>::get(offence);
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
		fn cf_generate_gov_key_call_hash(
			call: Vec<u8>,
		) -> GovCallHash {
			Governance::compute_gov_key_call_hash::<_>(call).0
		}

		fn cf_auction_state() -> AuctionState {
			let auction_params = Validator::auction_parameters();
			let min_active_bid = SetSizeMaximisingAuctionResolver::try_new(
				<Runtime as Chainflip>::EpochInfo::current_authority_count(),
				auction_params,
			)
			.and_then(|resolver| {
				resolver.resolve_auction(
					<Runtime as pallet_cf_validator::Config>::BidderProvider::get_qualified_bidders::<<Runtime as pallet_cf_validator::Config>::KeygenQualification>(),
					Validator::auction_bid_cutoff_percentage(),
				)
			})
			.ok()
			.map(|auction_outcome| auction_outcome.bond);
			AuctionState {
				blocks_per_epoch: Validator::blocks_per_epoch(),
				current_epoch_started_at: Validator::current_epoch_started_at(),
				redemption_period_as_percentage: Validator::redemption_period_as_percentage().deconstruct(),
				min_funding: MinimumFunding::<Runtime>::get().unique_saturated_into(),
				auction_size_range: (auction_params.min_size, auction_params.max_size),
				min_active_bid,
			}
		}

		fn cf_pool_price(
			from: Asset,
			to: Asset,
		) -> Option<PoolPriceV1> {
			LiquidityPools::current_price(from, to)
		}

		fn cf_pool_price_v2(base_asset: Asset, quote_asset: Asset) -> Result<PoolPriceV2, DispatchErrorWithMessage> {
			LiquidityPools::pool_price(base_asset, quote_asset).map_err(Into::into)
		}

		/// Simulates a swap and return the intermediate (if any) and final output.
		///
		/// If no swap rate can be calculated, returns None. This can happen if the pools are not
		/// provisioned, or if the input amount amount is too high or too low to give a meaningful
		/// output.
		///
		/// Note: This function must only be called through RPC, because RPC has its own storage buffer
		/// layer and would not affect on-chain storage.
		fn cf_pool_simulate_swap(from: Asset, to:Asset, amount: AssetAmount) -> Result<SwapOutput, DispatchErrorWithMessage> {
			LiquidityPools::swap_with_network_fee(from, to, amount).map_err(Into::into)
		}

		fn cf_pool_info(base_asset: Asset, quote_asset: Asset) -> Result<PoolInfo, DispatchErrorWithMessage> {
			LiquidityPools::pool_info(base_asset, quote_asset).map_err(Into::into)
		}

		fn cf_pool_depth(base_asset: Asset, quote_asset: Asset, tick_range: Range<cf_amm::common::Tick>) -> Result<AskBidMap<UnidirectionalPoolDepth>, DispatchErrorWithMessage> {
			LiquidityPools::pool_depth(base_asset, quote_asset, tick_range).map_err(Into::into)
		}

		fn cf_pool_liquidity(base_asset: Asset, quote_asset: Asset) -> Result<PoolLiquidity, DispatchErrorWithMessage> {
			LiquidityPools::pool_liquidity(base_asset, quote_asset).map_err(Into::into)
		}

		fn cf_required_asset_ratio_for_range_order(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<cf_amm::common::Tick>,
		) -> Result<PoolPairsMap<Amount>, DispatchErrorWithMessage> {
			LiquidityPools::required_asset_ratio_for_range_order(base_asset, quote_asset, tick_range).map_err(Into::into)
		}

		fn cf_pool_orderbook(
			base_asset: Asset,
			quote_asset: Asset,
			orders: u32,
		) -> Result<PoolOrderbook, DispatchErrorWithMessage> {
			LiquidityPools::pool_orderbook(base_asset, quote_asset, orders).map_err(Into::into)
		}

		fn cf_pool_orders(
			base_asset: Asset,
			quote_asset: Asset,
			lp: Option<AccountId>,
		) -> Result<PoolOrders<Runtime>, DispatchErrorWithMessage> {
			LiquidityPools::pool_orders(base_asset, quote_asset, lp).map_err(Into::into)
		}

		fn cf_pool_range_order_liquidity_value(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<Tick>,
			liquidity: Liquidity,
		) -> Result<PoolPairsMap<Amount>, DispatchErrorWithMessage> {
			LiquidityPools::pool_range_order_liquidity_value(base_asset, quote_asset, tick_range, liquidity).map_err(Into::into)
		}

		fn cf_network_environment() -> NetworkEnvironment {
			Environment::network_environment()
		}

		fn cf_max_swap_amount(asset: Asset) -> Option<AssetAmount> {
			Swapping::maximum_swap_amount(asset)
		}

		fn cf_min_deposit_amount(asset: Asset) -> AssetAmount {
			use pallet_cf_ingress_egress::MinimumDeposit;
			match asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => MinimumDeposit::<Runtime, EthereumInstance>::get(asset),
				ForeignChainAndAsset::Polkadot(asset) => MinimumDeposit::<Runtime, PolkadotInstance>::get(asset),
				ForeignChainAndAsset::Bitcoin(asset) => MinimumDeposit::<Runtime, BitcoinInstance>::get(asset).into(),
				ForeignChainAndAsset::Solana(asset) => MinimumDeposit::<Runtime, SolanaInstance>::get(asset),
			}
		}

		fn cf_egress_dust_limit(generic_asset: Asset) -> AssetAmount {
			use pallet_cf_ingress_egress::EgressDustLimit;

			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => EgressDustLimit::<Runtime, EthereumInstance>::get(asset),
				ForeignChainAndAsset::Polkadot(asset) => EgressDustLimit::<Runtime, PolkadotInstance>::get(asset),
				ForeignChainAndAsset::Bitcoin(asset) => EgressDustLimit::<Runtime, BitcoinInstance>::get(asset),
				ForeignChainAndAsset::Solana(asset) => EgressDustLimit::<Runtime, SolanaInstance>::get(asset),
			}
		}

		fn cf_ingress_fee(generic_asset: Asset) -> Option<AssetAmount> {
			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => {
					pallet_cf_pools::Pallet::<Runtime>::estimate_swap_input_for_desired_output(
						generic_asset,
						Asset::Eth,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::get_tracked_data()
							.estimate_ingress_fee(asset)
					)
				},
				ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::get_tracked_data()
					.estimate_ingress_fee(asset)),
				ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::get_tracked_data()
					.estimate_ingress_fee(asset).into()),
				ForeignChainAndAsset::Solana(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, SolanaInstance>::get_tracked_data()
					.estimate_ingress_fee(asset)),
			}
		}

		fn cf_egress_fee(generic_asset: Asset) -> Option<AssetAmount> {
			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => {
					pallet_cf_pools::Pallet::<Runtime>::estimate_swap_input_for_desired_output(
						generic_asset,
						Asset::Eth,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::get_tracked_data()
							.estimate_egress_fee(asset)
					)
				},
				ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::get_tracked_data()
					.estimate_egress_fee(asset)),
				ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::get_tracked_data()
					.estimate_egress_fee(asset).into()),
				ForeignChainAndAsset::Solana(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, SolanaInstance>::get_tracked_data()
					.estimate_egress_fee(asset)),
			}
		}

		fn cf_witness_safety_margin(chain: ForeignChain) -> Option<u64> {
			match chain {
				ForeignChain::Bitcoin => pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::witness_safety_margin(),
				ForeignChain::Ethereum => pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::witness_safety_margin(),
				ForeignChain::Polkadot => pallet_cf_ingress_egress::Pallet::<Runtime, PolkadotInstance>::witness_safety_margin().map(Into::into),
				ForeignChain::Solana => pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::witness_safety_margin(),
			}
		}


		fn cf_liquidity_provider_info(
			account_id: AccountId,
		) -> Option<LiquidityProviderInfo> {
			let role = Self::cf_account_role(account_id.clone())?;
			if role != AccountRole::LiquidityProvider {
				return None;
			}

			let refund_addresses = ForeignChain::iter().map(|chain| {
				(chain, pallet_cf_lp::LiquidityRefundAddress::<Runtime>::get(&account_id, chain))
			}).collect();

			LiquidityPools::sweep(&account_id).unwrap();

			Some(LiquidityProviderInfo {
				refund_addresses,
				balances: Asset::all().map(|asset|
					(asset, pallet_cf_lp::FreeBalances::<Runtime>::get(&account_id, asset).unwrap_or(0))
				).collect(),
				earned_fees: pallet_cf_lp::HistoricalEarnedFees::<Runtime>::get(&account_id),
			})
		}

		fn cf_account_role(account_id: AccountId) -> Option<AccountRole> {
			pallet_cf_account_roles::AccountRoles::<Runtime>::get(account_id)
		}

		fn cf_redemption_tax() -> AssetAmount {
			pallet_cf_funding::RedemptionTax::<Runtime>::get()
		}

		/// This should *not* be fully trusted as if the deposits that are pre-witnessed will definitely go through.
		/// This returns a list of swaps in the requested direction that are pre-witnessed in the current block.
		fn cf_prewitness_swaps(base_asset: Asset, quote_asset: Asset, side: Side) -> Vec<AssetAmount> {
			let (from, to) = AssetPair::to_swap(base_asset, quote_asset, side);

			fn filter_deposit_swaps<C, I: 'static>(from: Asset, to: Asset, deposit_witnesses: Vec<DepositWitness<C>>) -> Vec<AssetAmount>
				where Runtime: pallet_cf_ingress_egress::Config<I>,
				C: cf_chains::Chain<ChainAccount = <<Runtime as pallet_cf_ingress_egress::Config<I>>::TargetChain as cf_chains::Chain>::ChainAccount>
			{
				let mut filtered_swaps = Vec::new();
				for deposit in deposit_witnesses {
					let Some(details) = pallet_cf_ingress_egress::DepositChannelLookup::<Runtime, I>::get(
						deposit.deposit_address,
					) else {
						continue
					};
					let channel_asset: Asset = details.deposit_channel.asset.into();

					match details.action {
						ChannelAction::Swap { destination_asset, .. }
							if destination_asset == to && channel_asset == from =>
						{
							filtered_swaps.push(deposit.amount.into());
						},
						ChannelAction::CcmTransfer { destination_asset, channel_metadata, .. } => {
							filtered_swaps.extend(ccm_swaps(from, to, channel_asset, destination_asset, deposit.amount.into(), channel_metadata));
						}
						_ => {
							// ignore other deposit actions
						}
					}
				}
				filtered_swaps
			}

			fn ccm_swaps(from: Asset, to: Asset, source_asset: Asset, destination_asset: Asset, deposit_amount: AssetAmount, channel_metadata: CcmChannelMetadata) -> Vec<AssetAmount> {
				if source_asset != from {
					return Vec::new();
				}

				// There are two swaps for CCM, the principal swap, and the gas amount swap.
				let Ok(CcmSwapAmounts { principal_swap_amount, gas_budget, other_gas_asset }) = Swapping::principal_and_gas_amounts(deposit_amount, &channel_metadata, source_asset, destination_asset) else {
					// not a valid CCM
					return Vec::new();
				};

				let mut ccm_swaps = Vec::new();
				if destination_asset == to {
					// the principal swap is in the requested direction.
					ccm_swaps.push(principal_swap_amount);
				}

				if let Some(gas_asset) = other_gas_asset {
					if gas_asset == to {
						// the gas swap is in the requested direction
						ccm_swaps.push(gas_budget);
					}
				}

				ccm_swaps
			}

			let mut all_prewitnessed_swaps = Vec::new();
			let current_block_events = System::read_events_no_consensus();

			for event in current_block_events {
				match *event {
					frame_system::EventRecord::<RuntimeEvent, sp_core::H256> { event: RuntimeEvent::Witnesser(pallet_cf_witnesser::Event::Prewitnessed { call }), ..} => {
						match call {
							RuntimeCall::Swapping(pallet_cf_swapping::Call::schedule_swap_from_contract {
								from: swap_from, to: swap_to, deposit_amount, ..
							}) if from == swap_from && to == swap_to => {
								all_prewitnessed_swaps.push(deposit_amount);
							},
							RuntimeCall::EthereumIngressEgress(pallet_cf_ingress_egress::Call::process_deposits {
								deposit_witnesses, ..
							}) => {
								all_prewitnessed_swaps.extend(filter_deposit_swaps::<Ethereum, EthereumInstance>(from, to, deposit_witnesses));
							},
							RuntimeCall::BitcoinIngressEgress(pallet_cf_ingress_egress::Call::process_deposits {
								deposit_witnesses, ..
							}) => {
								all_prewitnessed_swaps.extend(filter_deposit_swaps::<Bitcoin, BitcoinInstance>(from, to, deposit_witnesses));
							},
							RuntimeCall::PolkadotIngressEgress(pallet_cf_ingress_egress::Call::process_deposits {
								deposit_witnesses, ..
							}) => {
								all_prewitnessed_swaps.extend(filter_deposit_swaps::<Polkadot, PolkadotInstance>(from, to, deposit_witnesses));
							},
							RuntimeCall::Swapping(pallet_cf_swapping::Call::ccm_deposit {
								source_asset, deposit_amount, destination_asset, deposit_metadata, ..
							}) => {
								// There are two swaps for CCM, the principal swap, and the gas amount swap.
								all_prewitnessed_swaps.extend(ccm_swaps(from, to, source_asset, destination_asset, deposit_amount, deposit_metadata.channel_metadata));
							},
							_ => {
								// ignore, we only care about calls that trigger swaps.
							},
						}
					}
					_ => {
						// ignore, we only care about Prewitnessed calls
					}
				}
			}

			all_prewitnessed_swaps
		}

		fn cf_failed_call(broadcast_id: BroadcastId) -> Option<<cf_chains::Ethereum as cf_chains::Chain>::Transaction> {
			if EthereumIngressEgress::get_failed_call(broadcast_id).is_some() {
				EthereumBroadcaster::threshold_signature_data(broadcast_id).map(|(api_call, _)|{
					chainflip::EthTransactionBuilder::build_transaction(&api_call)
				})
			} else {
				None
			}
		}

		fn cf_witness_count(hash: pallet_cf_witnesser::CallHash) -> Option<FailingWitnessValidators> {
			let mut result: FailingWitnessValidators = FailingWitnessValidators {
				failing_count: 0,
				validators: vec![],
			};
			let voting_validators = Witnesser::count_votes(<Runtime as Chainflip>::EpochInfo::current_epoch(), hash);
			let vanity_names: BTreeMap<AccountId, Vec<u8>> = pallet_cf_validator::VanityNames::<Runtime>::get();
			voting_validators?.iter().for_each(|(val, voted)| {
				let vanity = match vanity_names.get(val) {
					Some(vanity_name) => { vanity_name.clone() },
					None => { vec![] }
				};
				if !voted {
					result.failing_count += 1;
				}
				result.validators.push((val.clone(), String::from_utf8_lossy(&vanity).into(), *voted));
			});

			Some(result)
		}

		fn cf_channel_opening_fee(chain: ForeignChain) -> FlipBalance {
			match chain {
				ForeignChain::Ethereum => pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::channel_opening_fee(),
				ForeignChain::Polkadot => pallet_cf_ingress_egress::Pallet::<Runtime, PolkadotInstance>::channel_opening_fee(),
				ForeignChain::Bitcoin => pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::channel_opening_fee(),
				ForeignChain::Solana => pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::channel_opening_fee(),
			}
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

		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
			Runtime::metadata_at_version(version)
		}

		fn metadata_versions() -> sp_std::vec::Vec<u32> {
			Runtime::metadata_versions()
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


	impl sp_consensus_grandpa::GrandpaApi<Block> for Runtime {
		fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
			Grandpa::grandpa_authorities()
		}

		fn current_set_id() -> sp_consensus_grandpa::SetId {
			Grandpa::current_set_id()
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			equivocation_proof: sp_consensus_grandpa::EquivocationProof<
				<Block as BlockT>::Hash,
				NumberFor<Block>,
			>,
			key_owner_proof: sp_consensus_grandpa::OpaqueKeyOwnershipProof,
		) -> Option<()> {
			let key_owner_proof = key_owner_proof.decode()?;

			Grandpa::submit_unsigned_equivocation_report(
				equivocation_proof,
				key_owner_proof,
			)
		}

		fn generate_key_ownership_proof(
			_set_id: sp_consensus_grandpa::SetId,
			authority_id: GrandpaId,
		) -> Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof> {
			use codec::Encode;

			Historical::prove((sp_consensus_grandpa::KEY_TYPE, authority_id))
				.map(|p| p.encode())
				.map(sp_consensus_grandpa::OpaqueKeyOwnershipProof::new)
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
		fn account_nonce(account: AccountId) -> Nonce {
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
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentCallApi<Block, Balance, RuntimeCall>
		for Runtime
	{
		fn query_call_info(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment::RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_call_info(call, len)
		}
		fn query_call_fee_details(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			TransactionPayment::query_call_fee_details(call, len)
		}
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade(checks: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here. If any of the pre/post migration checks fail, we shall stop
			// right here and right now.
			let weight = Executive::try_runtime_upgrade(checks).unwrap();
			(weight, BlockWeights::get().max_block)
		}

		fn execute_block(
			block: Block,
			state_root_check: bool,
			signature_check: bool,
			select: frame_try_runtime::TryStateSelect
		) -> Weight {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here.
			Executive::try_execute_block(block, state_root_check, signature_check, select).expect("execute-block failed")
		}
	}

	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
		fn create_default_config() -> Vec<u8> {
			create_default_config::<RuntimeGenesisConfig>()
		}

		fn build_config(config: Vec<u8>) -> sp_genesis_builder::Result {
			build_config::<RuntimeGenesisConfig>(config)
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
			use frame_benchmarking::{baseline, Benchmarking, BenchmarkBatch};
			use frame_support::traits::TrackedStorageKey;

			use frame_system_benchmarking::Pallet as SystemBench;
			use baseline::Pallet as BaselineBench;
			use cf_session_benchmarking::Pallet as SessionBench;

			impl cf_session_benchmarking::Config for Runtime {}
			impl frame_system_benchmarking::Config for Runtime {}
			impl baseline::Config for Runtime {}

			use frame_support::traits::WhitelistedStorageKeys;
			let whitelist: Vec<TrackedStorageKey> = AllPalletsWithSystem::whitelisted_storage_keys();

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

	// Introduced from polkadot
	#[test]
	fn call_size() {
		assert!(
			core::mem::size_of::<RuntimeCall>() <= CALL_ENUM_MAX_SIZE,
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
			core::mem::size_of::<RuntimeCall>(),
		);
	}
}
