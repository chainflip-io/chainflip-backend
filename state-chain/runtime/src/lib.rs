#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]
pub mod chainflip;
pub mod constants;
pub mod migrations;
pub mod monitoring_apis;
pub mod runtime_apis;
pub mod safe_mode;
#[cfg(feature = "std")]
pub mod test_runner;
mod weights;
use crate::{
	chainflip::{
		calculate_account_apy,
		solana_elections::{
			SolanaChainTrackingProvider, SolanaEgressWitnessingTrigger, SolanaIngress,
			SolanaNonceTrackingTrigger,
		},
		Offence,
	},
	migrations::solana_transaction_data_migration::NoopUpgrade,
	monitoring_apis::{
		ActivateKeysBroadcastIds, AuthoritiesInfo, BtcUtxos, EpochState, ExternalChainsBlockHeight,
		FeeImbalance, FlipSupply, LastRuntimeUpgradeInfo, MonitoringDataV2, OpenDepositChannels,
		PendingBroadcasts, PendingTssCeremonies, RedemptionsInfo, SolanaNonces,
	},
	runtime_apis::{
		runtime_decl_for_custom_runtime_api::CustomRuntimeApi, AuctionState, BoostPoolDepth,
		BoostPoolDetails, BrokerInfo, DispatchErrorWithMessage, FailingWitnessValidators,
		LiquidityProviderBoostPoolInfo, LiquidityProviderInfo, RuntimeApiPenalty,
		SimulateSwapAdditionalOrder, SimulatedSwapInformation, TransactionScreeningEvents,
		ValidatorInfo, VaultSwapDetails,
	},
};
use cf_amm::{
	common::{PoolPairsMap, Side},
	math::{Amount, Tick},
	range_orders::Liquidity,
};
pub use cf_chains::instances::{
	ArbitrumInstance, BitcoinInstance, EthereumInstance, EvmInstance, PolkadotInstance,
	SolanaInstance,
};
use cf_chains::{
	address::{AddressConverter, EncodedAddress},
	arb::api::ArbitrumApi,
	assets::any::{AssetMap, ForeignChainAndAsset},
	btc::{
		api::BitcoinApi,
		vault_swap_encoding::{
			encode_swap_params_in_nulldata_payload, SharedCfParameters, UtxoEncodedData,
		},
		BitcoinCrypto, BitcoinRetryPolicy, ScriptPubkey,
	},
	dot::{self, PolkadotAccountId, PolkadotCrypto},
	eth::{self, api::EthereumApi, Address as EthereumAddress, Ethereum},
	evm::EvmCrypto,
	sol::{SolAddress, SolanaCrypto},
	Arbitrum, Bitcoin, DefaultRetryPolicy, ForeignChain, Polkadot, Solana, TransactionBuilder,
};
use cf_primitives::{
	AffiliateAndFee, AffiliateShortId, Affiliates, BasisPoints, Beneficiary, BroadcastId,
	DcaParameters, EpochIndex, NetworkEnvironment, STABLE_ASSET, SWAP_DELAY_BLOCKS,
};
use cf_traits::{
	AdjustedFeeEstimationApi, AffiliateRegistry, AssetConverter, BalanceApi,
	DummyEgressSuccessWitnesser, DummyIngressSource, EpochKey, GetBlockHeight, KeyProvider,
	NoLimit, SwapLimits, SwapLimitsProvider,
};
use codec::{alloc::string::ToString, Decode, Encode};
use core::ops::Range;
use frame_support::{derive_impl, instances::*, migrations::VersionedMigration};
pub use frame_system::Call as SystemCall;
use pallet_cf_governance::GovCallHash;
use pallet_cf_ingress_egress::{
	ChannelAction, DepositWitness, IngressOrEgress, OwedAmount, TargetChainAsset,
};
use pallet_cf_pools::{
	AskBidMap, AssetPair, HistoricalEarnedFees, OrderId, PoolLiquidity, PoolOrderbook, PoolPriceV1,
	PoolPriceV2, UnidirectionalPoolDepth,
};
use pallet_cf_swapping::{BatchExecutionError, FeeType, Swap};
use runtime_apis::ChainAccounts;

use crate::{chainflip::EvmLimit, runtime_apis::TransactionScreeningEvent};

use pallet_cf_reputation::{ExclusionList, HeartbeatQualification, ReputationPointsQualification};
use pallet_cf_swapping::SwapLegInfo;
use pallet_cf_validator::SetSizeMaximisingAuctionResolver;
use pallet_transaction_payment::{ConstFeeMultiplier, Multiplier};
use scale_info::prelude::string::String;
use sp_std::collections::btree_map::BTreeMap;

pub use frame_support::{
	debug, parameter_types,
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
use sp_runtime::{
	traits::{
		BlakeTwo256, Block as BlockT, ConvertInto, IdentifyAccount, NumberFor, One, OpaqueKeys,
		Saturating, UniqueSaturatedInto, Verify,
	},
	BoundedVec,
};

use frame_support::genesis_builder_helper::build_state;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, DispatchError, MultiSignature,
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
	AccountInfo, BoostApi, Chainflip, EpochInfo, PoolApi, QualifyNode, SessionKeysRegistered,
	SwappingApi,
};
// Required for genesis config.
pub use pallet_cf_validator::SetSizeParameters;

use chainflip::{
	boost_api::IngressEgressBoostApi, epoch_transition::ChainflipEpochTransitions,
	evm_vault_activator::EvmVaultActivator, BroadcastReadyProvider, BtcEnvironment,
	ChainAddressConverter, ChainflipHeartbeat, DotEnvironment, EvmEnvironment, SolEnvironment,
	SolanaLimit, TokenholderGovernanceBroadcaster,
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
	spec_version: 180,
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
	type KeyRotator = cons_key_rotator!(
		EvmThresholdSigner,
		PolkadotThresholdSigner,
		BitcoinThresholdSigner,
		SolanaThresholdSigner
	);
	type RotationBroadcastsPending = cons_rotation_broadcasts_pending!(
		EthereumBroadcaster,
		PolkadotBroadcaster,
		BitcoinBroadcaster,
		ArbitrumBroadcaster,
		SolanaBroadcaster
	);
	type MissedAuthorshipSlots = chainflip::MissedAuraSlots;
	type KeygenQualification = (
		HeartbeatQualification<Self>,
		(
			ExclusionList<Self, chainflip::KeygenExclusionOffences>,
			(
				pallet_cf_validator::PeerMapping<Self>,
				(
					SessionKeysRegistered<Self, pallet_session::Pallet<Self>>,
					(
						chainflip::ValidatorRoleQualification,
						(
							pallet_cf_validator::QualifyByCfeVersion<Self>,
							ReputationPointsQualification<Self>,
						),
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
	type ArbitrumVaultKeyWitnessedHandler = ArbitrumVault;
	type SolanaVaultKeyWitnessedHandler = SolanaVault;
	type SolanaNonceWatch = SolanaNonceTrackingTrigger;
	type BitcoinFeeInfo = chainflip::BitcoinFeeGetter;
	type BitcoinKeyProvider = BitcoinThresholdSigner;
	type RuntimeSafeMode = RuntimeSafeMode;
	type CurrentReleaseVersion = CurrentReleaseVersion;
	type WeightInfo = pallet_cf_environment::weights::PalletWeight<Runtime>;
}

parameter_types! {
	pub const NetworkFee: Permill = Permill::from_perthousand(1);
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
	type IngressEgressFeeHandler = chainflip::IngressEgressFeeHandler;
	type CcmValidityChecker = cf_chains::ccm_checker::CcmValidityChecker;
	type NetworkFee = NetworkFee;
	type BalanceApi = AssetBalances;
	type ChannelIdAllocator = BitcoinIngressEgress;
	type Bonder = Bonder<Runtime>;
}

impl pallet_cf_vaults::Config<Instance1> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Chain = Ethereum;
	type SetAggKeyWithAggKey = eth::api::EthereumApi<EvmEnvironment>;
	type Broadcaster = EthereumBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = EthereumChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<Instance2> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Chain = Polkadot;
	type SetAggKeyWithAggKey = dot::api::PolkadotApi<DotEnvironment>;
	type Broadcaster = PolkadotBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = PolkadotChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<Instance3> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Chain = Bitcoin;
	type SetAggKeyWithAggKey = cf_chains::btc::api::BitcoinApi<BtcEnvironment>;
	type Broadcaster = BitcoinBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = BitcoinChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<Instance4> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Chain = Arbitrum;
	type SetAggKeyWithAggKey = cf_chains::arb::api::ArbitrumApi<EvmEnvironment>;
	type Broadcaster = ArbitrumBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = ArbitrumChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<Instance5> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Chain = Solana;
	type SetAggKeyWithAggKey = cf_chains::sol::api::SolanaApi<SolEnvironment>;
	type Broadcaster = SolanaBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = SolanaChainTrackingProvider;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

use chainflip::address_derivation::AddressDerivation;

impl pallet_cf_ingress_egress::Config<Instance1> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	type IngressSource = DummyIngressSource<Ethereum>;
	type TargetChain = Ethereum;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type PoolApi = LiquidityPools;
	type ChainApiCall = eth::api::EthereumApi<EvmEnvironment>;
	type Broadcaster = EthereumBroadcaster;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = EthereumChainTracking;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = EvmLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapLimitsProvider = Swapping;
	type CcmValidityChecker = cf_chains::ccm_checker::CcmValidityChecker;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<false>;
}

impl pallet_cf_ingress_egress::Config<Instance2> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	type IngressSource = DummyIngressSource<Polkadot>;
	type TargetChain = Polkadot;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type PoolApi = LiquidityPools;
	type ChainApiCall = dot::api::PolkadotApi<chainflip::DotEnvironment>;
	type Broadcaster = PolkadotBroadcaster;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = PolkadotChainTracking;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = NoLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapLimitsProvider = Swapping;
	type CcmValidityChecker = cf_chains::ccm_checker::CcmValidityChecker;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<false>;
}

impl pallet_cf_ingress_egress::Config<Instance3> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	type IngressSource = DummyIngressSource<Bitcoin>;
	type TargetChain = Bitcoin;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type PoolApi = LiquidityPools;
	type ChainApiCall = cf_chains::btc::api::BitcoinApi<chainflip::BtcEnvironment>;
	type Broadcaster = BitcoinBroadcaster;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = BitcoinChainTracking;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = NoLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapLimitsProvider = Swapping;
	type CcmValidityChecker = cf_chains::ccm_checker::CcmValidityChecker;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<true>;
}

impl pallet_cf_ingress_egress::Config<Instance4> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	type IngressSource = DummyIngressSource<Arbitrum>;
	type TargetChain = Arbitrum;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type PoolApi = LiquidityPools;
	type ChainApiCall = ArbitrumApi<EvmEnvironment>;
	type Broadcaster = ArbitrumBroadcaster;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = ArbitrumChainTracking;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = EvmLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapLimitsProvider = Swapping;
	type CcmValidityChecker = cf_chains::ccm_checker::CcmValidityChecker;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<false>;
}

impl pallet_cf_ingress_egress::Config<Instance5> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = false;
	type IngressSource = SolanaIngress;
	type TargetChain = Solana;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type PoolApi = LiquidityPools;
	type ChainApiCall = cf_chains::sol::api::SolanaApi<SolEnvironment>;
	type Broadcaster = SolanaBroadcaster;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = SolanaChainTrackingProvider;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = SolanaLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapLimitsProvider = Swapping;
	type CcmValidityChecker = cf_chains::ccm_checker::CcmValidityChecker;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<false>;
}

impl pallet_cf_pools::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type LpBalance = AssetBalances;
	type LpRegistrationApi = LiquidityProvider;
	type SwapRequestHandler = Swapping;
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
	type BalanceApi = AssetBalances;
	type BoostApi = IngressEgressBoostApi;
	type WeightInfo = pallet_cf_lp::weights::PalletWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type FeePayment = Flip;
	type MigrationHelper = LiquidityPools;
}

impl pallet_cf_account_roles::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type WeightInfo = ();
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

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(50);

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
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
	/// The block type for the runtime.
	type Block = Block;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = BlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = BlockLength;
	/// The hashing algorithm used.
	type Hashing = BlakeTwo256;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = DbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// What to do if a new account is created.
	type OnNewAccount = AccountRoles;
	/// What to do if an account is fully reaped from the system.
	type OnKilledAccount = (
		pallet_cf_flip::BurnFlipAccount<Self>,
		GrandpaOffenceReporter<Self>,
		Funding,
		AccountRoles,
		Reputation,
		pallet_cf_pools::DeleteHistoricalEarnedFees<Self>,
		pallet_cf_asset_balances::DeleteAccount<Self>,
	);
	/// The data to be stored in an account.
	type AccountData = ();
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = weights::frame_system::SubstrateWeight<Runtime>;
	/// This is used as an identifier of the chain.
	type SS58Prefix = ConstU16<CHAINFLIP_SS58_PREFIX>;

	/// We don't use RuntimeTask.
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
	type SlotDuration = ConstU64<SLOT_DURATION>;
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
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, EvmInstance>;
	type RegisterRedemption = EthereumApi<EvmEnvironment>;
	type TimeSource = Timestamp;
	type RedemptionChecker = Validator;
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
	type UpgradeCondition = (
		pallet_cf_validator::NotDuringRotation<Runtime>,
		(pallet_cf_swapping::NoPendingSwaps<Runtime>, pallet_cf_environment::NoUsedNonce<Runtime>),
	);
	type RuntimeUpgrade = chainflip::RuntimeUpgradeManager;
	type CompatibleCfeVersions = Environment;
	type AuthoritiesCfeVersions = Validator;
}

impl pallet_cf_emissions::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type HostChain = Ethereum;
	type FlipBalance = FlipBalance;
	type ApiCall = eth::api::EthereumApi<EvmEnvironment>;
	type Broadcaster = EthereumBroadcaster;
	type Surplus = pallet_cf_flip::Surplus<Runtime>;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;
	type RewardsDistribution = chainflip::BlockAuthorRewardDistribution;
	type CompoundingInterval = ConstU32<COMPOUNDING_INTERVAL>;
	type EthEnvironment = EvmEnvironment;
	type FlipToBurn = Swapping;
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

impl pallet_cf_threshold_signature::Config<Instance16> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Offence = chainflip::Offence;
	type RuntimeOrigin = RuntimeOrigin;
	type ThresholdCallable = RuntimeCall;
	type ThresholdSignerNomination = chainflip::RandomSignerNomination;
	type TargetChainCrypto = EvmCrypto;
	type VaultActivator = EvmVaultActivator<EthereumVault, ArbitrumVault>;
	type OffenceReporter = Reputation;
	type CeremonyRetryDelay = ConstU32<1>;
	type SafeMode = RuntimeSafeMode;
	type Slasher = FlipSlasher<Self>;
	type CfeMultisigRequest = CfeInterface;
	type Weights = pallet_cf_threshold_signature::weights::PalletWeight<Self>;
}

impl pallet_cf_threshold_signature::Config<Instance2> for Runtime {
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

impl pallet_cf_threshold_signature::Config<Instance3> for Runtime {
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

impl pallet_cf_threshold_signature::Config<Instance5> for Runtime {
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

impl pallet_cf_broadcast::Config<Instance1> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type TargetChain = Ethereum;
	type ApiCall = eth::api::EthereumApi<EvmEnvironment>;
	type ThresholdSigner = EvmThresholdSigner;
	type TransactionBuilder = chainflip::EthTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, EvmInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type SafeModeBlockMargin = ConstU32<10>;
	type SafeModeChainBlockMargin = ConstU64<BLOCKS_PER_MINUTE_ETHEREUM>;
	type ChainTracking = EthereumChainTracking;
	type RetryPolicy = DefaultRetryPolicy;
	type LiabilityTracker = AssetBalances;
	type CfeBroadcastRequest = CfeInterface;
	type ElectionEgressWitnesser = DummyEgressSuccessWitnesser<EvmCrypto>;
}

impl pallet_cf_broadcast::Config<Instance2> for Runtime {
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
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type SafeModeBlockMargin = ConstU32<10>;
	type SafeModeChainBlockMargin = ConstU32<BLOCKS_PER_MINUTE_POLKADOT>;
	type ChainTracking = PolkadotChainTracking;
	type RetryPolicy = DefaultRetryPolicy;
	type LiabilityTracker = AssetBalances;
	type CfeBroadcastRequest = CfeInterface;
	type ElectionEgressWitnesser = DummyEgressSuccessWitnesser<PolkadotCrypto>;
}

impl pallet_cf_broadcast::Config<Instance3> for Runtime {
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
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type SafeModeBlockMargin = ConstU32<10>;
	type SafeModeChainBlockMargin = ConstU64<1>; // 10 minutes
	type ChainTracking = BitcoinChainTracking;
	type RetryPolicy = BitcoinRetryPolicy;
	type LiabilityTracker = AssetBalances;
	type CfeBroadcastRequest = CfeInterface;
	type ElectionEgressWitnesser = DummyEgressSuccessWitnesser<BitcoinCrypto>;
}

impl pallet_cf_broadcast::Config<Instance4> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type TargetChain = Arbitrum;
	type ApiCall = cf_chains::arb::api::ArbitrumApi<EvmEnvironment>;
	type ThresholdSigner = EvmThresholdSigner;
	type TransactionBuilder = chainflip::ArbTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, EvmInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type SafeModeBlockMargin = ConstU32<10>;
	type SafeModeChainBlockMargin = ConstU64<BLOCKS_PER_MINUTE_ARBITRUM>;
	type ChainTracking = ArbitrumChainTracking;
	type RetryPolicy = DefaultRetryPolicy;
	type LiabilityTracker = AssetBalances;
	type CfeBroadcastRequest = CfeInterface;
	type ElectionEgressWitnesser = DummyEgressSuccessWitnesser<EvmCrypto>;
}

impl pallet_cf_asset_balances::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type EgressHandler = chainflip::AnyChainIngressEgressHandler;
	type PolkadotKeyProvider = PolkadotThresholdSigner;
	type SafeMode = RuntimeSafeMode;
}

impl pallet_cf_broadcast::Config<Instance5> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type TargetChain = Solana;
	type ApiCall = cf_chains::sol::api::SolanaApi<SolEnvironment>;
	type ThresholdSigner = SolanaThresholdSigner;
	type TransactionBuilder = chainflip::SolanaTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, SolanaInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type SafeModeBlockMargin = ConstU32<10>;
	type SafeModeChainBlockMargin = ConstU64<BLOCKS_PER_MINUTE_SOLANA>;
	type ChainTracking = SolanaChainTrackingProvider;
	type RetryPolicy = DefaultRetryPolicy;
	type LiabilityTracker = AssetBalances;
	type CfeBroadcastRequest = CfeInterface;
	type ElectionEgressWitnesser = SolanaEgressWitnessingTrigger;
}

impl pallet_cf_chain_tracking::Config<Instance1> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Ethereum;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
}

impl pallet_cf_chain_tracking::Config<Instance2> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Polkadot;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
}

impl pallet_cf_chain_tracking::Config<Instance3> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Bitcoin;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
}

impl pallet_cf_chain_tracking::Config<Instance4> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Arbitrum;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
}

impl pallet_cf_chain_tracking::Config<Instance5> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Solana;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
}

impl pallet_cf_elections::Config<Instance5> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ElectoralSystemRunner = chainflip::solana_elections::SolanaElectoralSystemRunner;
	type WeightInfo = pallet_cf_elections::weights::PalletWeight<Runtime>;
}

#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(RuntimeCall, RuntimeEvent, RuntimeError, RuntimeOrigin)]
	pub struct Runtime;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;
	#[runtime::pallet_index(1)]
	pub type Timestamp = pallet_timestamp;
	#[runtime::pallet_index(2)]
	pub type Environment = pallet_cf_environment;
	#[runtime::pallet_index(3)]
	pub type Flip = pallet_cf_flip;
	#[runtime::pallet_index(4)]
	pub type Emissions = pallet_cf_emissions;

	// AccountRoles after funding; since account creation comes first.
	#[runtime::pallet_index(5)]
	pub type Funding = pallet_cf_funding;
	#[runtime::pallet_index(6)]
	pub type AccountRoles = pallet_cf_account_roles;
	#[runtime::pallet_index(7)]
	pub type TransactionPayment = pallet_transaction_payment;
	#[runtime::pallet_index(8)]
	pub type Witnesser = pallet_cf_witnesser;
	#[runtime::pallet_index(9)]
	pub type Validator = pallet_cf_validator;
	#[runtime::pallet_index(10)]
	pub type Session = pallet_session;
	#[runtime::pallet_index(11)]
	pub type Historical = session_historical;
	#[runtime::pallet_index(12)]
	pub type Aura = pallet_aura;
	#[runtime::pallet_index(13)]
	pub type Authorship = pallet_authorship;
	#[runtime::pallet_index(14)]
	pub type Grandpa = pallet_grandpa;
	#[runtime::pallet_index(15)]
	pub type Governance = pallet_cf_governance;
	#[runtime::pallet_index(16)]
	pub type TokenholderGovernance = pallet_cf_tokenholder_governance;
	#[runtime::pallet_index(17)]
	pub type Reputation = pallet_cf_reputation;

	#[runtime::pallet_index(18)]
	pub type EthereumChainTracking = pallet_cf_chain_tracking<Instance1>;
	#[runtime::pallet_index(19)]
	pub type PolkadotChainTracking = pallet_cf_chain_tracking<Instance2>;
	#[runtime::pallet_index(20)]
	pub type BitcoinChainTracking = pallet_cf_chain_tracking<Instance3>;

	#[runtime::pallet_index(21)]
	pub type EthereumVault = pallet_cf_vaults<Instance1>;
	#[runtime::pallet_index(22)]
	pub type PolkadotVault = pallet_cf_vaults<Instance2>;
	#[runtime::pallet_index(23)]
	pub type BitcoinVault = pallet_cf_vaults<Instance3>;

	#[runtime::pallet_index(24)]
	pub type EvmThresholdSigner = pallet_cf_threshold_signature<Instance16>;
	#[runtime::pallet_index(25)]
	pub type PolkadotThresholdSigner = pallet_cf_threshold_signature<Instance2>;
	#[runtime::pallet_index(26)]
	pub type BitcoinThresholdSigner = pallet_cf_threshold_signature<Instance3>;

	#[runtime::pallet_index(27)]
	pub type EthereumBroadcaster = pallet_cf_broadcast<Instance1>;
	#[runtime::pallet_index(28)]
	pub type PolkadotBroadcaster = pallet_cf_broadcast<Instance2>;
	#[runtime::pallet_index(29)]
	pub type BitcoinBroadcaster = pallet_cf_broadcast<Instance3>;

	#[runtime::pallet_index(30)]
	pub type Swapping = pallet_cf_swapping;
	#[runtime::pallet_index(31)]
	pub type LiquidityProvider = pallet_cf_lp;

	#[runtime::pallet_index(32)]
	pub type EthereumIngressEgress = pallet_cf_ingress_egress<Instance1>;
	#[runtime::pallet_index(33)]
	pub type PolkadotIngressEgress = pallet_cf_ingress_egress<Instance2>;
	#[runtime::pallet_index(34)]
	pub type BitcoinIngressEgress = pallet_cf_ingress_egress<Instance3>;

	#[runtime::pallet_index(35)]
	pub type LiquidityPools = pallet_cf_pools;

	#[runtime::pallet_index(36)]
	pub type CfeInterface = pallet_cf_cfe_interface;

	#[runtime::pallet_index(37)]
	pub type ArbitrumChainTracking = pallet_cf_chain_tracking<Instance4>;
	#[runtime::pallet_index(38)]
	pub type ArbitrumVault = pallet_cf_vaults<Instance4>;
	#[runtime::pallet_index(39)]
	pub type ArbitrumBroadcaster = pallet_cf_broadcast<Instance4>;
	#[runtime::pallet_index(40)]
	pub type ArbitrumIngressEgress = pallet_cf_ingress_egress<Instance4>;

	#[runtime::pallet_index(41)]
	pub type SolanaVault = pallet_cf_vaults<Instance5>;
	#[runtime::pallet_index(42)]
	pub type SolanaThresholdSigner = pallet_cf_threshold_signature<Instance5>;
	#[runtime::pallet_index(43)]
	pub type SolanaBroadcaster = pallet_cf_broadcast<Instance5>;
	#[runtime::pallet_index(44)]
	pub type SolanaIngressEgress = pallet_cf_ingress_egress<Instance5>;
	#[runtime::pallet_index(45)]
	pub type SolanaElections = pallet_cf_elections<Instance5>;
	#[runtime::pallet_index(46)]
	pub type SolanaChainTracking = pallet_cf_chain_tracking<Instance5>;

	#[runtime::pallet_index(47)]
	pub type AssetBalances = pallet_cf_asset_balances;
}

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
	AllMigrations,
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
	AssetBalances,
	// Chain Tracking
	EthereumChainTracking,
	PolkadotChainTracking,
	BitcoinChainTracking,
	ArbitrumChainTracking,
	SolanaChainTracking,
	// Elections
	SolanaElections,
	// Vaults
	EthereumVault,
	PolkadotVault,
	BitcoinVault,
	ArbitrumVault,
	SolanaVault,
	// Threshold Signers
	EvmThresholdSigner,
	PolkadotThresholdSigner,
	BitcoinThresholdSigner,
	SolanaThresholdSigner,
	// Broadcasters
	EthereumBroadcaster,
	PolkadotBroadcaster,
	BitcoinBroadcaster,
	ArbitrumBroadcaster,
	SolanaBroadcaster,
	// Swapping and Liquidity Provision
	Swapping,
	LiquidityProvider,
	// Ingress Egress
	EthereumIngressEgress,
	PolkadotIngressEgress,
	BitcoinIngressEgress,
	ArbitrumIngressEgress,
	SolanaIngressEgress,
	// Liquidity Pools
	LiquidityPools,
);

/// Contains:
/// - ClearEvents in CfeInterface migration. Don't remove this.
/// - The VersionUpdate migration. Don't remove this.
/// - Individual pallet migrations. Don't remove these unless there's a good reason. Prefer to
///   disable these at the pallet level (ie. set it to () or PhantomData).
/// - Release-specific migrations: remove these if they are no longer needed.
type AllMigrations = (
	// This ClearEvents should only be run at the start of all migrations. This is in case another
	// migration needs to trigger an event like a Broadcast for example.
	pallet_cf_cfe_interface::migrations::ClearEvents<Runtime>,
	// DO NOT REMOVE `VersionUpdate`. THIS IS REQUIRED TO UPDATE THE VERSION FOR THE CFES EVERY
	// UPGRADE
	pallet_cf_environment::migrations::VersionUpdate<Runtime>,
	PalletMigrations,
	MigrationsForV1_8,
	migrations::housekeeping::Migration,
	migrations::reap_old_accounts::Migration,
);

/// All the pallet-specific migrations and migrations that depend on pallet migration order. Do not
/// comment out or remove pallet migrations. Prefer to delete the migration at the pallet level and
/// replace with a dummy migration.
type PalletMigrations = (
	pallet_cf_environment::migrations::PalletMigration<Runtime>,
	pallet_cf_funding::migrations::PalletMigration<Runtime>,
	pallet_cf_account_roles::migrations::PalletMigration<Runtime>,
	pallet_cf_validator::migrations::PalletMigration<Runtime>,
	pallet_cf_governance::migrations::PalletMigration<Runtime>,
	pallet_cf_tokenholder_governance::migrations::PalletMigration<Runtime>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, EthereumInstance>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, PolkadotInstance>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, ArbitrumInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, EthereumInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, PolkadotInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, ArbitrumInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, SolanaInstance>,
	pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, EvmInstance>,
	pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, PolkadotInstance>,
	pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, SolanaInstance>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, EthereumInstance>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, PolkadotInstance>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, ArbitrumInstance>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, SolanaInstance>,
	pallet_cf_swapping::migrations::PalletMigration<Runtime>,
	pallet_cf_lp::migrations::PalletMigration<Runtime>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, EthereumInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, PolkadotInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, ArbitrumInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, SolanaInstance>,
	pallet_cf_pools::migrations::PalletMigration<Runtime>,
	pallet_cf_cfe_interface::migrations::PalletMigration<Runtime>,
);

type MigrationsForV1_8 = (
	VersionedMigration<
		2,
		3,
		migrations::solana_vault_swaps_migration::SolanaVaultSwapsMigration,
		pallet_cf_elections::Pallet<Runtime, SolanaInstance>,
		DbWeight,
	>,
	// Only the Solana Transaction type has changed
	VersionedMigration<
		10,
		11,
		migrations::solana_transaction_data_migration::SolanaTransactionDataMigration,
		pallet_cf_broadcast::Pallet<Runtime, SolanaInstance>,
		DbWeight,
	>,
	VersionedMigration<
		10,
		11,
		NoopUpgrade,
		pallet_cf_broadcast::Pallet<Runtime, SolanaInstance>,
		DbWeight,
	>,
	VersionedMigration<
		10,
		11,
		NoopUpgrade,
		pallet_cf_broadcast::Pallet<Runtime, EthereumInstance>,
		DbWeight,
	>,
	VersionedMigration<
		10,
		11,
		NoopUpgrade,
		pallet_cf_broadcast::Pallet<Runtime, PolkadotInstance>,
		DbWeight,
	>,
	VersionedMigration<
		10,
		11,
		NoopUpgrade,
		pallet_cf_broadcast::Pallet<Runtime, BitcoinInstance>,
		DbWeight,
	>,
	VersionedMigration<
		10,
		11,
		NoopUpgrade,
		pallet_cf_broadcast::Pallet<Runtime, ArbitrumInstance>,
		DbWeight,
	>,
);

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
		[pallet_cf_threshold_signature, EvmThresholdSigner]
		[pallet_cf_broadcast, EthereumBroadcaster]
		[pallet_cf_chain_tracking, EthereumChainTracking]
		[pallet_cf_swapping, Swapping]
		[pallet_cf_account_roles, AccountRoles]
		[pallet_cf_ingress_egress, EthereumIngressEgress]
		[pallet_cf_lp, LiquidityProvider]
		[pallet_cf_pools, LiquidityPools]
		[pallet_cf_cfe_interface, CfeInterface]
		[pallet_cf_asset_balances, AssetBalances]
		[pallet_cf_elections, SolanaElections]
	);
}

impl_runtime_apis! {
	impl runtime_apis::ElectoralRuntimeApi<Block, SolanaInstance> for Runtime {
		fn cf_electoral_data(account_id: AccountId) -> Vec<u8> {
			SolanaElections::electoral_data(&account_id).encode()
		}

		fn cf_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			SolanaElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}
	}

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
			(EvmThresholdSigner::keys(epoch_index).unwrap_or_default().to_pubkey_compressed(), EthereumVault::vault_start_block_numbers(epoch_index).unwrap().unique_saturated_into())
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
			let mut vanity_names = AccountRoles::vanity_names();
			frame_system::Account::<Runtime>::iter_keys()
				.map(|account_id| {
					let vanity_name = vanity_names.remove(&account_id).unwrap_or_default().into();
					(account_id, vanity_name)
				})
				.collect()
		}
		fn cf_free_balances(account_id: AccountId) -> AssetMap<AssetAmount> {
			LiquidityPools::sweep(&account_id).unwrap();
			AssetBalances::free_balances(&account_id)
		}
		fn cf_lp_total_balances(account_id: AccountId) -> AssetMap<AssetAmount> {
			let free_balances = AssetBalances::free_balances(&account_id);
			let open_order_balances = LiquidityPools::open_order_balances(&account_id);
			let boost_pools_balances = IngressEgressBoostApi::boost_pool_account_balances(&account_id);
			free_balances.saturating_add(open_order_balances).saturating_add(boost_pools_balances)
		}
		fn cf_account_flip_balance(account_id: &AccountId) -> u128 {
			pallet_cf_flip::Account::<Runtime>::get(account_id).total()
		}
		fn cf_validator_info(account_id: &AccountId) -> ValidatorInfo {
			let is_current_backup = pallet_cf_validator::Backups::<Runtime>::get().contains_key(account_id);
			let key_holder_epochs = pallet_cf_validator::HistoricalActiveEpochs::<Runtime>::get(account_id);
			let is_qualified = <<Runtime as pallet_cf_validator::Config>::KeygenQualification as QualifyNode<_>>::is_qualified(account_id);
			let is_current_authority = pallet_cf_validator::CurrentAuthorities::<Runtime>::get().contains(account_id);
			let is_bidding = Validator::is_bidding(account_id);
			let bound_redeem_address = pallet_cf_funding::BoundRedeemAddress::<Runtime>::get(account_id);
			let apy_bp = calculate_account_apy(account_id);
			let reputation_info = pallet_cf_reputation::Reputations::<Runtime>::get(account_id);
			let account_info = pallet_cf_flip::Account::<Runtime>::get(account_id);
			let restricted_balances = pallet_cf_funding::RestrictedBalances::<Runtime>::get(account_id);
			ValidatorInfo {
				balance: account_info.total(),
				bond: account_info.bond(),
				last_heartbeat: pallet_cf_reputation::LastHeartbeat::<Runtime>::get(account_id).unwrap_or(0),
				reputation_points: reputation_info.reputation_points,
				keyholder_epochs: key_holder_epochs,
				is_current_authority,
				is_current_backup,
				is_qualified: is_bidding && is_qualified,
				is_online: HeartbeatQualification::<Runtime>::is_qualified(account_id),
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
					Validator::get_qualified_bidders::<<Runtime as pallet_cf_validator::Config>::KeygenQualification>(),
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
			Ok(
				LiquidityPools::pool_price(base_asset, quote_asset)?
					.map_sell_and_buy_prices(|price| price.sqrt_price)
			)
		}

		/// Simulates a swap and return the intermediate (if any) and final output.
		///
		/// If no swap rate can be calculated, returns None. This can happen if the pools are not
		/// provisioned, or if the input amount amount is too high or too low to give a meaningful
		/// output.
		///
		/// Note: This function must only be called through RPC, because RPC has its own storage buffer
		/// layer and would not affect on-chain storage.
		fn cf_pool_simulate_swap(
			input_asset: Asset,
			output_asset: Asset,
			input_amount: AssetAmount,
			broker_commission: BasisPoints,
			dca_parameters: Option<DcaParameters>,
			additional_orders: Option<Vec<SimulateSwapAdditionalOrder>>,
		) -> Result<SimulatedSwapInformation, DispatchErrorWithMessage> {
			if let Some(additional_orders) = additional_orders {
				for (index, additional_order) in additional_orders.into_iter().enumerate() {
					match additional_order {
						SimulateSwapAdditionalOrder::LimitOrder {
							base_asset,
							quote_asset,
							side,
							tick,
							sell_amount,
						} => {
							LiquidityPools::try_add_limit_order(
								&AccountId::new([0; 32]),
								base_asset,
								quote_asset,
								side,
								index as OrderId,
								tick,
								sell_amount.into(),
							)?;
						}
					}
				}
			}

			fn remove_fees(ingress_or_egress: IngressOrEgress, asset: Asset, amount: AssetAmount) -> (AssetAmount, AssetAmount) {
				use pallet_cf_ingress_egress::AmountAndFeesWithheld;

				match asset.into() {
					ForeignChainAndAsset::Ethereum(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees, fees_withheld)
					},
					ForeignChainAndAsset::Polkadot(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, PolkadotInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees, fees_withheld)
					},
					ForeignChainAndAsset::Bitcoin(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees.into(), fees_withheld.into())
					},
					ForeignChainAndAsset::Arbitrum(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, ArbitrumInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees, fees_withheld)
					},
					ForeignChainAndAsset::Solana(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees.into(), fees_withheld.into())
					},
				}
			}

			let (amount_to_swap, ingress_fee) = remove_fees(IngressOrEgress::Ingress, input_asset, input_amount);

			// Estimate swap result for a chunk, then extrapolate the result.
			// If no DCA parameter is given, swap the entire amount with 1 chunk.
			let number_of_chunks: u128 = dca_parameters.map(|dca|dca.number_of_chunks).unwrap_or(1u32).into();
			let amount_per_chunk = amount_to_swap / number_of_chunks;

			let swap_output_per_chunk = Swapping::try_execute_without_violations(
				vec![
					Swap::new(
						Default::default(),
						Default::default(),
						input_asset,
						output_asset,
						amount_per_chunk,
						None,
						vec![
							FeeType::NetworkFee,
							FeeType::BrokerFee(
								vec![Beneficiary {
									account: AccountId::new([0xbb; 32]),
									bps: broker_commission,
								}]
								.try_into()
								.expect("Beneficiary with a length of 1 must be within length bound.")
							)
						],
					)
				],
			).map_err(|e| DispatchErrorWithMessage::Other(match e {
				BatchExecutionError::SwapLegFailed { .. } => DispatchError::Other("Swap leg failed."),
				BatchExecutionError::PriceViolation { .. } => DispatchError::Other("Price Violation: Some swaps failed due to Price Impact Limitations."),
				BatchExecutionError::DispatchError { error } => error,
			}))?;

			let (
				network_fee,
				broker_fee,
				intermediary,
				output,
			) = {
				(
					swap_output_per_chunk[0].network_fee_taken.unwrap_or_default() * number_of_chunks,
					swap_output_per_chunk[0].broker_fee_taken.unwrap_or_default() * number_of_chunks,
					swap_output_per_chunk[0].stable_amount.map(|amount| amount * number_of_chunks)
						.filter(|_| ![input_asset, output_asset].contains(&STABLE_ASSET)),
					swap_output_per_chunk[0].final_output.unwrap_or_default() * number_of_chunks,
				)
			};

			let (output, egress_fee) = remove_fees(IngressOrEgress::Egress, output_asset, output);

			Ok(SimulatedSwapInformation {
				intermediary,
				output,
				network_fee,
				ingress_fee,
				egress_fee,
				broker_fee,
			})
		}

		fn cf_pool_info(base_asset: Asset, quote_asset: Asset) -> Result<PoolInfo, DispatchErrorWithMessage> {
			LiquidityPools::pool_info(base_asset, quote_asset).map_err(Into::into)
		}

		fn cf_lp_events() -> Vec<pallet_cf_pools::Event<Runtime>> {
			System::read_events_no_consensus().filter_map(|event_record| {
				if let RuntimeEvent::LiquidityPools(pools_event) = event_record.event {
					Some(pools_event)
				} else {
					None
				}
			}).collect()

		}

		fn cf_pool_depth(base_asset: Asset, quote_asset: Asset, tick_range: Range<cf_amm::math::Tick>) -> Result<AskBidMap<UnidirectionalPoolDepth>, DispatchErrorWithMessage> {
			LiquidityPools::pool_depth(base_asset, quote_asset, tick_range).map_err(Into::into)
		}

		fn cf_pool_liquidity(base_asset: Asset, quote_asset: Asset) -> Result<PoolLiquidity, DispatchErrorWithMessage> {
			LiquidityPools::pool_liquidity(base_asset, quote_asset).map_err(Into::into)
		}

		fn cf_required_asset_ratio_for_range_order(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<cf_amm::math::Tick>,
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
			filled_orders: bool,
		) -> Result<PoolOrders<Runtime>, DispatchErrorWithMessage> {
			LiquidityPools::pool_orders(base_asset, quote_asset, lp, filled_orders).map_err(Into::into)
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
				ForeignChainAndAsset::Arbitrum(asset) => MinimumDeposit::<Runtime, ArbitrumInstance>::get(asset),
				ForeignChainAndAsset::Solana(asset) => MinimumDeposit::<Runtime, SolanaInstance>::get(asset).into(),
			}
		}

		fn cf_egress_dust_limit(generic_asset: Asset) -> AssetAmount {
			use pallet_cf_ingress_egress::EgressDustLimit;

			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => EgressDustLimit::<Runtime, EthereumInstance>::get(asset),
				ForeignChainAndAsset::Polkadot(asset) => EgressDustLimit::<Runtime, PolkadotInstance>::get(asset),
				ForeignChainAndAsset::Bitcoin(asset) => EgressDustLimit::<Runtime, BitcoinInstance>::get(asset),
				ForeignChainAndAsset::Arbitrum(asset) => EgressDustLimit::<Runtime, ArbitrumInstance>::get(asset),
				ForeignChainAndAsset::Solana(asset) => EgressDustLimit::<Runtime, SolanaInstance>::get(asset),
			}
		}

		fn cf_ingress_fee(generic_asset: Asset) -> Option<AssetAmount> {
			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => {
					pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Ethereum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::estimate_ingress_fee(asset)
					)
				},
				ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::estimate_ingress_fee(asset)),
				ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::estimate_ingress_fee(asset).into()),
				ForeignChainAndAsset::Arbitrum(asset) => {
					pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Arbitrum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, ArbitrumInstance>::estimate_ingress_fee(asset)
					)
				},
				ForeignChainAndAsset::Solana(asset) => Some(SolanaChainTrackingProvider::
				estimate_ingress_fee(asset).into()),
			}
		}

		fn cf_egress_fee(generic_asset: Asset) -> Option<AssetAmount> {
			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => {
					pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Ethereum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::estimate_egress_fee(asset)
					)
				},
				ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::estimate_egress_fee(asset)),
				ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::estimate_egress_fee(asset).into()),
				ForeignChainAndAsset::Arbitrum(asset) => {
					pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Arbitrum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, ArbitrumInstance>::estimate_egress_fee(asset)
					)
				},
				ForeignChainAndAsset::Solana(asset) => Some(SolanaChainTrackingProvider::
				estimate_egress_fee(asset).into()),
			}
		}

		fn cf_witness_safety_margin(chain: ForeignChain) -> Option<u64> {
			match chain {
				ForeignChain::Bitcoin => pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::witness_safety_margin(),
				ForeignChain::Ethereum => pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::witness_safety_margin(),
				ForeignChain::Polkadot => pallet_cf_ingress_egress::Pallet::<Runtime, PolkadotInstance>::witness_safety_margin().map(Into::into),
				ForeignChain::Arbitrum => pallet_cf_ingress_egress::Pallet::<Runtime, ArbitrumInstance>::witness_safety_margin(),
				ForeignChain::Solana => pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::witness_safety_margin(),
			}
		}

		fn cf_liquidity_provider_info(
			account_id: AccountId,
		) -> LiquidityProviderInfo {
			let refund_addresses = ForeignChain::iter().map(|chain| {
				(chain, pallet_cf_lp::LiquidityRefundAddress::<Runtime>::get(&account_id, chain))
			}).collect();

			LiquidityPools::sweep(&account_id).unwrap();

			LiquidityProviderInfo {
				refund_addresses,
				balances: Asset::all().map(|asset|
					(asset, pallet_cf_asset_balances::FreeBalances::<Runtime>::get(&account_id, asset))
				).collect(),
				earned_fees: AssetMap::from_iter(HistoricalEarnedFees::<Runtime>::iter_prefix(&account_id)),
				boost_balances: AssetMap::from_fn(|asset| {
					let pool_details = Self::cf_boost_pool_details(asset);

					pool_details.into_iter().filter_map(|(fee_tier, details)| {
						let available_balance = details.available_amounts.into_iter().find_map(|(id, amount)| {
							if id == account_id {
								Some(amount)
							} else {
								None
							}
						}).unwrap_or(0);

						let owed_amount = details.pending_boosts.into_iter().flat_map(|(_, pending_deposits)| {
							pending_deposits.into_iter().filter_map(|(id, amount)| {
								if id == account_id {
									Some(amount.total)
								} else {
									None
								}
							})
						}).sum();

						let total_balance = available_balance + owed_amount;

						if total_balance == 0 {
							return None
						}

						Some(LiquidityProviderBoostPoolInfo {
							fee_tier,
							total_balance,
							available_balance,
							in_use_balance: owed_amount,
							is_withdrawing: details.pending_withdrawals.keys().any(|id| *id == account_id),
						})
					}).collect()
				}),
			}
		}

		fn cf_broker_info(
			account_id: AccountId,
		) -> BrokerInfo {
			let earned_fees = Asset::all().map(|asset|
				(asset, AssetBalances::get_balance(&account_id, asset))
			).collect();

			BrokerInfo { earned_fees }
		}

		fn cf_account_role(account_id: AccountId) -> Option<AccountRole> {
			pallet_cf_account_roles::AccountRoles::<Runtime>::get(account_id)
		}

		fn cf_redemption_tax() -> AssetAmount {
			pallet_cf_funding::RedemptionTax::<Runtime>::get()
		}

		fn cf_swap_retry_delay_blocks() -> u32 {
			pallet_cf_swapping::SwapRetryDelay::<Runtime>::get()
		}

		fn cf_swap_limits() -> SwapLimits {
			pallet_cf_swapping::Pallet::<Runtime>::get_swap_limits()
		}

		fn cf_minimum_chunk_size(asset: Asset) -> AssetAmount {
			Swapping::minimum_chunk_size(asset)
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
						ChannelAction::CcmTransfer { .. } => {
							// Ingoring: ccm swaps aren't supported for BTC (which is the only chain where pre-witnessing is enabled)
						}
						_ => {
							// ignore other deposit actions
						}
					}
				}
				filtered_swaps
			}

			let mut all_prewitnessed_swaps = Vec::new();
			let current_block_events = System::read_events_no_consensus();

			for event in current_block_events {
				#[allow(clippy::collapsible_match)]
				match *event {
					frame_system::EventRecord::<RuntimeEvent, sp_core::H256> { event: RuntimeEvent::Witnesser(pallet_cf_witnesser::Event::Prewitnessed { call }), ..} => {
						match call {
							RuntimeCall::BitcoinIngressEgress(pallet_cf_ingress_egress::Call::process_deposits {
								deposit_witnesses, ..
							}) => {
								all_prewitnessed_swaps.extend(filter_deposit_swaps::<Bitcoin, BitcoinInstance>(from, to, deposit_witnesses));
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

		fn cf_scheduled_swaps(base_asset: Asset, quote_asset: Asset) -> Vec<(SwapLegInfo, BlockNumber)> {
			assert_eq!(quote_asset, STABLE_ASSET, "Only USDC is supported as quote asset");

			let current_block = System::block_number();

			pallet_cf_swapping::SwapQueue::<Runtime>::iter().flat_map(|(block, swaps_for_block)| {
				// In case `block` has already passed, the swaps will be re-tried at the next block:
				let execute_at = core::cmp::max(block, current_block.saturating_add(1));

				let swaps: Vec<_> = swaps_for_block
					.iter()
					.filter(|swap| swap.from == base_asset || swap.to == base_asset)
					.cloned()
					.collect();

				let pool_sell_price = LiquidityPools::pool_price(base_asset, quote_asset).
					expect("Pool should exist")
					.sell
					.map(|price| price.sqrt_price);

				Swapping::get_scheduled_swap_legs(swaps, base_asset, pool_sell_price)
					.into_iter()
					.map(move |swap| (swap, execute_at))
			}).collect()
		}

		fn cf_failed_call_ethereum(broadcast_id: BroadcastId) -> Option<<cf_chains::Ethereum as cf_chains::Chain>::Transaction> {
			if EthereumIngressEgress::get_failed_call(broadcast_id).is_some() {
				EthereumBroadcaster::threshold_signature_data(broadcast_id).map(|api_call|{
					chainflip::EthTransactionBuilder::build_transaction(&api_call)
				})
			} else {
				None
			}
		}

		fn cf_failed_call_arbitrum(broadcast_id: BroadcastId) -> Option<<cf_chains::Arbitrum as cf_chains::Chain>::Transaction> {
			if ArbitrumIngressEgress::get_failed_call(broadcast_id).is_some() {
				ArbitrumBroadcaster::threshold_signature_data(broadcast_id).map(|api_call|{
					chainflip::ArbTransactionBuilder::build_transaction(&api_call)
				})
			} else {
				None
			}
		}

		fn cf_witness_count(hash: pallet_cf_witnesser::CallHash, epoch_index: Option<EpochIndex>) -> Option<FailingWitnessValidators> {
			let mut result: FailingWitnessValidators = FailingWitnessValidators {
				failing_count: 0,
				validators: vec![],
			};
			let voting_validators = Witnesser::count_votes(epoch_index.unwrap_or(<Runtime as Chainflip>::EpochInfo::current_epoch()), hash);
			let vanity_names: BTreeMap<AccountId, BoundedVec<u8, _>> = pallet_cf_account_roles::VanityNames::<Runtime>::get();
			voting_validators?.iter().for_each(|(val, voted)| {
				let vanity = vanity_names.get(val).cloned().unwrap_or_default();
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
				ForeignChain::Arbitrum => pallet_cf_ingress_egress::Pallet::<Runtime, ArbitrumInstance>::channel_opening_fee(),
				ForeignChain::Solana => pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::channel_opening_fee(),
			}
		}

		fn cf_boost_pools_depth() -> Vec<BoostPoolDepth> {

			fn boost_pools_depth<I: 'static>() -> Vec<BoostPoolDepth>
				where Runtime: pallet_cf_ingress_egress::Config<I> {

				pallet_cf_ingress_egress::BoostPools::<Runtime, I>::iter().map(|(asset, tier, pool)|

					BoostPoolDepth {
						asset: asset.into(),
						tier,
						available_amount: pool.get_available_amount().into()
					}

				).collect()
			}

			ForeignChain::iter().flat_map(|chain| {
				match chain {
					ForeignChain::Ethereum => boost_pools_depth::<EthereumInstance>(),
					ForeignChain::Polkadot => boost_pools_depth::<PolkadotInstance>(),
					ForeignChain::Bitcoin => boost_pools_depth::<BitcoinInstance>(),
					ForeignChain::Arbitrum => boost_pools_depth::<ArbitrumInstance>(),
					ForeignChain::Solana => boost_pools_depth::<SolanaInstance>(),
				}
			}).collect()

		}

		fn cf_boost_pool_details(asset: Asset) -> BTreeMap<u16, BoostPoolDetails> {

			fn boost_pools_details<I: 'static>(asset: TargetChainAsset::<Runtime, I>) -> BTreeMap<u16, BoostPoolDetails>
				where Runtime: pallet_cf_ingress_egress::Config<I> {

				pallet_cf_ingress_egress::BoostPools::<Runtime, I>::iter_prefix(asset).map(|(tier, pool)| {
					(
						tier,
						BoostPoolDetails {
							available_amounts: pool.get_amounts().into_iter().map(|(id, amount)| (id, amount.into())).collect(),
							pending_boosts: pool.get_pending_boosts().into_iter().map(|(deposit_id, owed_amounts)| {
								(
									deposit_id,
									owed_amounts.into_iter().map(|(id, amount)| (id, OwedAmount {total: amount.total.into(), fee: amount.fee.into()})).collect()
								)
							}).collect(),
							pending_withdrawals: pool.get_pending_withdrawals().clone(),
						}
					)
				}).collect()

			}

			let chain: ForeignChain = asset.into();

			match chain {
				ForeignChain::Ethereum => boost_pools_details::<EthereumInstance>(asset.try_into().unwrap()),
				ForeignChain::Polkadot => boost_pools_details::<PolkadotInstance>(asset.try_into().unwrap()),
				ForeignChain::Bitcoin => boost_pools_details::<BitcoinInstance>(asset.try_into().unwrap()),
				ForeignChain::Arbitrum => boost_pools_details::<ArbitrumInstance>(asset.try_into().unwrap()),
				ForeignChain::Solana => boost_pools_details::<SolanaInstance>(asset.try_into().unwrap()),
			}

		}

		fn cf_safe_mode_statuses() -> RuntimeSafeMode {
			pallet_cf_environment::RuntimeSafeMode::<Runtime>::get()
		}

		fn cf_pools() -> Vec<PoolPairsMap<Asset>> {
			LiquidityPools::pools()
		}

		fn cf_validate_dca_params(number_of_chunks: u32, chunk_interval: u32) -> Result<(), DispatchErrorWithMessage> {
			pallet_cf_swapping::Pallet::<Runtime>::validate_dca_params(&DcaParameters{number_of_chunks, chunk_interval}).map_err(Into::into)
		}

		fn cf_validate_refund_params(retry_duration: BlockNumber) -> Result<(), DispatchErrorWithMessage> {
			pallet_cf_swapping::Pallet::<Runtime>::validate_refund_params(retry_duration).map_err(Into::into)
		}

		fn cf_get_vault_swap_details(
			broker_id: AccountId,
			source_asset: Asset,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			broker_commission: BasisPoints,
			min_output_amount: AssetAmount,
			retry_duration: BlockNumber,
			boost_fee: BasisPoints,
			affiliate_fees: Affiliates<AccountId>,
			dca_parameters: Option<DcaParameters>,
		) -> Result<VaultSwapDetails<String>, DispatchErrorWithMessage> {
			// Validate params
			pallet_cf_swapping::Pallet::<Runtime>::validate_refund_params(retry_duration)?;
			if let Some(params) = dca_parameters.as_ref() {
				pallet_cf_swapping::Pallet::<Runtime>::validate_dca_params(params)?;
			}
			ChainAddressConverter::try_from_encoded_address(destination_address.clone())
				.and_then(|address| {
					if ForeignChain::from(destination_asset) != address.chain() {
						Err(())
					} else {
						Ok(())
					}
				})
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::InvalidDestinationAddress)?;

			// Encode swap
			match ForeignChain::from(source_asset) {
				ForeignChain::Bitcoin => {
					use cf_chains::btc::deposit_address::DepositAddress;

					let private_channel_id =
						pallet_cf_swapping::BrokerPrivateBtcChannels::<Runtime>::get(&broker_id)
							.ok_or(
								pallet_cf_swapping::Error::<Runtime>::NoPrivateChannelExistsForBroker,
							)?;
					let params = UtxoEncodedData {
							output_asset: destination_asset,
							output_address: destination_address,
							parameters: SharedCfParameters {
								retry_duration: retry_duration.try_into()
									.map_err(|_| pallet_cf_swapping::Error::<Runtime>::SwapRequestDurationTooLong)?,
								min_output_amount,
								number_of_chunks: dca_parameters
									.as_ref()
									.map(|params| params.number_of_chunks)
									.unwrap_or(1)
									.try_into()
									.map_err(|_| pallet_cf_swapping::Error::<Runtime>::InvalidDcaParameters)?,
								chunk_interval: dca_parameters
									.as_ref()
									.map(|params| params.chunk_interval)
									.unwrap_or(SWAP_DELAY_BLOCKS)
									.try_into()
									.map_err(|_| pallet_cf_swapping::Error::<Runtime>::InvalidDcaParameters)?,
								boost_fee: boost_fee.try_into().map_err(|_| pallet_cf_swapping::Error::<Runtime>::BoostFeeTooHigh)?,
								broker_fee: broker_commission.try_into().map_err(|_| pallet_cf_swapping::Error::<Runtime>::BrokerFeeTooHigh)?,
								affiliates: affiliate_fees.into_iter().map(|beneficiary|
										Result::<AffiliateAndFee, DispatchErrorWithMessage>::Ok(
											AffiliateAndFee {
												affiliate: Swapping::get_short_id(&broker_id, &beneficiary.account)
													.ok_or(pallet_cf_swapping::Error::<Runtime>::AffiliateNotRegistered)?,
												fee: beneficiary.bps.try_into()
													.map_err(|_| pallet_cf_swapping::Error::<Runtime>::AffiliateFeeTooHigh)?
											}
										)
									)
									.collect::<Result<Vec<AffiliateAndFee>,_>>()?
									.try_into()
									.map_err(|_| pallet_cf_swapping::Error::<Runtime>::TooManyAffiliates)?,
							},
						};

					let EpochKey { key, .. } = BitcoinThresholdSigner::active_epoch_key()
						.expect("We should always have a key for the current epoch.");
					let deposit_address = DepositAddress::new(
						key.current,
						private_channel_id.try_into().map_err(
							// TODO: Ensure this can't happen.
							|_| {
								DispatchErrorWithMessage::Other(
									"Private channel id out of bounds.".into(),
								)
							},
						)?,
					)
					.script_pubkey()
					.to_address(&Environment::network_environment().into());

					Ok(VaultSwapDetails::Bitcoin {
						nulldata_payload: encode_swap_params_in_nulldata_payload(params),
						deposit_address,
					})
				},
				_ => Err(pallet_cf_swapping::Error::<Runtime>::UnsupportedSourceAsset.into()),
			}
		}

		fn cf_get_open_deposit_channels(account_id: Option<AccountId>) -> ChainAccounts {
			let btc_chain_accounts = pallet_cf_ingress_egress::DepositChannelLookup::<Runtime,BitcoinInstance>::iter_values()
				.filter(|channel_details| account_id.is_none() || Some(&channel_details.owner) == account_id.as_ref())
				.map(|channel_details| channel_details.deposit_channel.address)
				.collect::<Vec<_>>();

			ChainAccounts {
				btc_chain_accounts
			}
		}

		fn cf_transaction_screening_events() -> crate::runtime_apis::TransactionScreeningEvents {
			let btc_events = System::read_events_no_consensus().filter_map(|event_record| {
				if let RuntimeEvent::BitcoinIngressEgress(btc_ie_event) = event_record.event {
					match btc_ie_event {
						pallet_cf_ingress_egress::Event::TransactionRejectionRequestExpired{ account_id, tx_id } =>
							Some(TransactionScreeningEvent::TransactionRejectionRequestExpired{ account_id, tx_id }),
						pallet_cf_ingress_egress::Event::TransactionRejectionRequestReceived{ account_id, tx_id, expires_at: _ } =>
							Some(TransactionScreeningEvent::TransactionRejectionRequestReceived{account_id, tx_id }),
						pallet_cf_ingress_egress::Event::TransactionRejectedByBroker{ broadcast_id, tx_id } =>
							Some(TransactionScreeningEvent::TransactionRejectedByBroker{ refund_broadcast_id: broadcast_id, tx_id: tx_id.id.tx_id }),
						_ => None,
					}
				} else {
					None
				}
			}).collect();

			TransactionScreeningEvents {
				btc_events
			}
		}

		fn cf_get_affiliates(
			broker: AccountId,
		) -> Vec<(AffiliateShortId, AccountId)>{
			pallet_cf_swapping::AffiliateIdMapping::<Runtime>::iter_prefix(&broker).collect()
		}
	}


	impl monitoring_apis::MonitoringRuntimeApi<Block> for Runtime {

		fn cf_authorities() -> AuthoritiesInfo {
			let mut authorities = pallet_cf_validator::CurrentAuthorities::<Runtime>::get();
			let mut backups = pallet_cf_validator::Backups::<Runtime>::get();
			let mut result = AuthoritiesInfo {
				authorities: authorities.len() as u32,
				online_authorities: 0,
				backups: backups.len() as u32,
				online_backups: 0,
			};
			authorities.retain(HeartbeatQualification::<Runtime>::is_qualified);
			backups.retain(|id, _| HeartbeatQualification::<Runtime>::is_qualified(id));
			result.online_authorities = authorities.len() as u32;
			result.online_backups = backups.len() as u32;
			result
		}

		fn cf_external_chains_block_height() -> ExternalChainsBlockHeight {
			// safe to unwrap these value as stated on the storage item doc
			let btc = pallet_cf_chain_tracking::CurrentChainState::<Runtime, BitcoinInstance>::get().unwrap();
			let eth = pallet_cf_chain_tracking::CurrentChainState::<Runtime, EthereumInstance>::get().unwrap();
			let dot = pallet_cf_chain_tracking::CurrentChainState::<Runtime, PolkadotInstance>::get().unwrap();
			let arb = pallet_cf_chain_tracking::CurrentChainState::<Runtime, ArbitrumInstance>::get().unwrap();
			let sol = SolanaChainTrackingProvider::get_block_height();

			ExternalChainsBlockHeight {
				bitcoin: btc.block_height,
				ethereum: eth.block_height,
				polkadot: dot.block_height.into(),
				solana: sol,
				arbitrum: arb.block_height,
			}
		}

		fn cf_btc_utxos() -> BtcUtxos {
			let utxos = pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get();
			let mut btc_balance = utxos.iter().fold(0, |acc, elem| acc + elem.amount);
			//Sum the btc balance contained in the change utxos to the btc "free_balance"
			let btc_ceremonies = pallet_cf_threshold_signature::PendingCeremonies::<Runtime,BitcoinInstance>::iter_values().map(|ceremony|{
				ceremony.request_context.request_id
			}).collect::<Vec<_>>();
			let EpochKey { key, .. } = pallet_cf_threshold_signature::Pallet::<Runtime, BitcoinInstance>::active_epoch_key()
				.expect("We should always have a key for the current epoch");
			for ceremony in btc_ceremonies {
				if let RuntimeCall::BitcoinBroadcaster(pallet_cf_broadcast::pallet::Call::on_signature_ready{ api_call, ..}) = pallet_cf_threshold_signature::RequestCallback::<Runtime, BitcoinInstance>::get(ceremony).unwrap() {
					if let BitcoinApi::BatchTransfer(batch_transfer) = *api_call {
						for output in batch_transfer.bitcoin_transaction.outputs {
							if [
								ScriptPubkey::Taproot(key.previous.unwrap_or_default()),
								ScriptPubkey::Taproot(key.current),
							]
							.contains(&output.script_pubkey)
							{
								btc_balance += output.amount;
							}
						}
					}
				}
			}
			BtcUtxos {
				total_balance: btc_balance,
				count: utxos.len() as u32,
			}
		}

		fn cf_dot_aggkey() -> PolkadotAccountId {
			let epoch = PolkadotThresholdSigner::current_key_epoch().unwrap_or_default();
			PolkadotThresholdSigner::keys(epoch).unwrap_or_default()
		}

		fn cf_suspended_validators() -> Vec<(Offence, u32)> {
			let suspended_for_keygen = match pallet_cf_validator::Pallet::<Runtime>::current_rotation_phase() {
				pallet_cf_validator::RotationPhase::KeygensInProgress(rotation_state) |
				pallet_cf_validator::RotationPhase::KeyHandoversInProgress(rotation_state) |
				pallet_cf_validator::RotationPhase::ActivatingKeys(rotation_state) |
				pallet_cf_validator::RotationPhase::NewKeysActivated(rotation_state) => { rotation_state.banned.len() as u32 },
				_ => {0u32}
			};
			pallet_cf_reputation::Suspensions::<Runtime>::iter().map(|(key, _)| {
				if key == pallet_cf_threshold_signature::PalletOffence::FailedKeygen.into() {
					return (key, suspended_for_keygen);
				}
				(key, pallet_cf_reputation::Pallet::<Runtime>::validators_suspended_for(&[key]).len() as u32)
			}).collect()
		}
		fn cf_epoch_state() -> EpochState {
			let auction_params = Validator::auction_parameters();
			let min_active_bid = SetSizeMaximisingAuctionResolver::try_new(
				<Runtime as Chainflip>::EpochInfo::current_authority_count(),
				auction_params,
			)
			.and_then(|resolver| {
				resolver.resolve_auction(
					Validator::get_qualified_bidders::<<Runtime as pallet_cf_validator::Config>::KeygenQualification>(),
					Validator::auction_bid_cutoff_percentage(),
				)
			})
			.ok()
			.map(|auction_outcome| auction_outcome.bond);
			EpochState {
				blocks_per_epoch: Validator::blocks_per_epoch(),
				current_epoch_started_at: Validator::current_epoch_started_at(),
				current_epoch_index: Validator::current_epoch(),
				min_active_bid,
				rotation_phase: Validator::current_rotation_phase().to_str().to_string(),
			}
		}
		fn cf_redemptions() -> RedemptionsInfo {
			let redemptions: Vec<_> = pallet_cf_funding::PendingRedemptions::<Runtime>::iter().collect();
			RedemptionsInfo {
				total_balance: redemptions.iter().fold(0, |acc, elem| acc + elem.1.total),
				count: redemptions.len() as u32,
			}
		}
		fn cf_pending_broadcasts_count() -> PendingBroadcasts {
			PendingBroadcasts {
				ethereum: pallet_cf_broadcast::PendingBroadcasts::<Runtime, EthereumInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				bitcoin: pallet_cf_broadcast::PendingBroadcasts::<Runtime, BitcoinInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				polkadot: pallet_cf_broadcast::PendingBroadcasts::<Runtime, PolkadotInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				arbitrum: pallet_cf_broadcast::PendingBroadcasts::<Runtime, ArbitrumInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				solana: pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
			}
		}
		fn cf_pending_tss_ceremonies_count() -> PendingTssCeremonies {
			PendingTssCeremonies {
				evm: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, EvmInstance>::iter().collect::<Vec<_>>().len() as u32,
				bitcoin: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, BitcoinInstance>::iter().collect::<Vec<_>>().len() as u32,
				polkadot: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, PolkadotInstance>::iter().collect::<Vec<_>>().len() as u32,
				solana: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, SolanaInstance>::iter().collect::<Vec<_>>().len() as u32,
			}
		}
		fn cf_pending_swaps_count() -> u32 {
			let swaps: Vec<_> = pallet_cf_swapping::SwapQueue::<Runtime>::iter().collect();
			swaps.iter().fold(0u32, |acc, elem| acc + elem.1.len() as u32)
		}
		fn cf_open_deposit_channels_count() -> OpenDepositChannels {
			fn open_channels<BlockHeight, I: 'static>() -> u32
				where BlockHeight: GetBlockHeight<<Runtime as pallet_cf_ingress_egress::Config<I>>::TargetChain>, Runtime: pallet_cf_ingress_egress::Config<I>
			{
				pallet_cf_ingress_egress::DepositChannelLookup::<Runtime, I>::iter().filter(|(_key, elem)| elem.expires_at > BlockHeight::get_block_height()).collect::<Vec<_>>().len() as u32
			}

			OpenDepositChannels{
				ethereum: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, EthereumInstance>, EthereumInstance>(),
				bitcoin: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, BitcoinInstance>, BitcoinInstance>(),
				polkadot: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, PolkadotInstance>, PolkadotInstance>(),
				arbitrum: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, ArbitrumInstance>, ArbitrumInstance>(),
				solana: open_channels::<SolanaChainTrackingProvider, SolanaInstance>(),
			}
		}
		fn cf_fee_imbalance() -> FeeImbalance<AssetAmount> {
			FeeImbalance {
				ethereum: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Ethereum.gas_asset()),
				polkadot: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Polkadot.gas_asset()),
				arbitrum: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Arbitrum.gas_asset()),
				bitcoin: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Bitcoin.gas_asset()),
				solana: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Solana.gas_asset()),
			}
		}
		fn cf_build_version() -> LastRuntimeUpgradeInfo {
			let info = frame_system::LastRuntimeUpgrade::<Runtime>::get().expect("this has to be set");
			LastRuntimeUpgradeInfo {
				spec_version: info.spec_version.into(),
				spec_name: info.spec_name,
			}
		}
		fn cf_rotation_broadcast_ids() -> ActivateKeysBroadcastIds{
			ActivateKeysBroadcastIds{
				ethereum: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, EthereumInstance>::get().map(|val| val.1),
				bitcoin: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, BitcoinInstance>::get().map(|val| val.1),
				polkadot: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, PolkadotInstance>::get().map(|val| val.1),
				arbitrum: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, ArbitrumInstance>::get().map(|val| val.1),
				solana: {
					let broadcast_id = pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, SolanaInstance>::get().map(|val| val.1);
					(broadcast_id, pallet_cf_broadcast::AwaitingBroadcast::<Runtime, SolanaInstance>::get(broadcast_id.unwrap_or_default()).map(|broadcast_data| broadcast_data.transaction_out_id))
				}
			}
		}
		fn cf_sol_nonces() -> SolanaNonces{
			SolanaNonces {
				available: pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::get(),
				unavailable: pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::iter_keys().collect()
			}
		}
		fn cf_sol_aggkey() -> SolAddress{
			let epoch = SolanaThresholdSigner::current_key_epoch().unwrap_or_default();
			SolanaThresholdSigner::keys(epoch).unwrap_or_default()
		}
		fn cf_sol_onchain_key() -> SolAddress{
			SolanaBroadcaster::current_on_chain_key().unwrap_or_default()
		}
		fn cf_monitoring_data() -> MonitoringDataV2 {
			MonitoringDataV2{
				external_chains_height: Self::cf_external_chains_block_height(),
				btc_utxos: Self::cf_btc_utxos(),
				epoch: Self::cf_epoch_state(),
				pending_redemptions: Self::cf_redemptions(),
				pending_broadcasts: Self::cf_pending_broadcasts_count(),
				pending_tss: Self::cf_pending_tss_ceremonies_count(),
				open_deposit_channels: Self::cf_open_deposit_channels_count(),
				fee_imbalance: Self::cf_fee_imbalance(),
				authorities: Self::cf_authorities(),
				build_version: Self::cf_build_version(),
				suspended_validators: Self::cf_suspended_validators(),
				pending_swaps: Self::cf_pending_swaps_count(),
				dot_aggkey: Self::cf_dot_aggkey(),
				flip_supply: {
					let flip = Self::cf_flip_supply();
					FlipSupply { total_supply: flip.0, offchain_supply: flip.1}
				},
				sol_aggkey: Self::cf_sol_aggkey(),
				sol_onchain_key: Self::cf_sol_onchain_key(),
				sol_nonces: Self::cf_sol_nonces(),
				activating_key_broadcast_ids: Self::cf_rotation_broadcast_ids(),
			}
		}
		fn cf_accounts_info(accounts: BoundedVec<AccountId, ConstU32<10>>) -> Vec<ValidatorInfo> {
			accounts.iter().map(|account_id| {
				Self::cf_validator_info(account_id)
			}).collect()
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

		fn initialize_block(header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
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
			pallet_aura::Authorities::<Runtime>::get().into_inner()
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
			let weight = Executive::try_runtime_upgrade(checks)
				.inspect_err(|e| log::error!("try_runtime_upgrade failed with: {:?}", e)).unwrap();
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
		fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
			build_state::<RuntimeGenesisConfig>(config)
		}

		fn get_preset(_id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
			None
		}

		fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
			Default::default()
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

		#[allow(non_local_definitions)]
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
