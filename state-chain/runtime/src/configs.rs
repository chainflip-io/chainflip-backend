use crate::{
	chainflip::{
		epoch_transition::ChainflipEpochTransitions,
		multi_vault_activator::MultiVaultActivator,
		witnessing::solana_elections::{
			SolanaChainTrackingProvider, SolanaEgressWitnessingTrigger, SolanaIngress,
			SolanaNonceTrackingTrigger,
		},
		BroadcastReadyProvider, BtcEnvironment, CfCcmAdditionalDataHandler, ChainAddressConverter,
		ChainlinkOracle, DotEnvironment, EvmEnvironment, EvmLimit, HubEnvironment,
		MinimumDepositProvider, SolEnvironment, SolanaLimit, TokenholderGovernanceBroadcaster,
	},
	constants::common::*,
	*,
};
pub use cf_chains::instances::{
	ArbitrumInstance, AssethubInstance, BitcoinInstance, EthereumInstance, EvmInstance,
	PolkadotCryptoInstance, PolkadotInstance, SolanaInstance, TronInstance,
};
use cf_chains::{
	arb::api::ArbitrumApi,
	btc::{BitcoinCrypto, BitcoinRetryPolicy},
	dot::{self, PolkadotCrypto},
	eth::{self, api::EthereumApi, Ethereum},
	evm::EvmCrypto,
	hub,
	instances::ChainInstanceAlias,
	sol::SolanaCrypto,
	Arbitrum, Assethub, Bitcoin, DefaultRetryPolicy, Polkadot, Solana, Tron,
};
pub use cf_primitives::{
	chains::assets::any, AccountRole, Asset, AssetAmount, BlockNumber, FlipBalance, SemVer,
	SwapOutput,
};
pub use cf_traits::{
	AccountInfo, BoostBalancesApi, Chainflip, EpochInfo, OrderId, PoolApi, QualifyNode,
	SessionKeysRegistered, SwappingApi,
};
use cf_traits::{
	ChainflipWithTargetChain, DummyEgressSuccessWitnesser, DummyIngressSource, NoLimit,
};

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
use frame_support::{
	derive_impl,
	instances::*,
	sp_runtime::traits::{BlakeTwo256, ConvertInto, One, OpaqueKeys, Verify},
};
pub use frame_system::Call as SystemCall;
use frame_system::{offchain::SendTransactionTypes, pallet_prelude::BlockNumberFor};
use pallet_cf_flip::{Bonder, FlipIssuance, FlipSlasher};
use pallet_cf_reputation::{ExclusionList, HeartbeatQualification, ReputationPointsQualification};
use pallet_cf_trading_strategy::TradingStrategyDeregistrationCheck;
pub use pallet_cf_validator::SetSizeParameters;
use pallet_cf_validator::{DelegatedRewardsDistribution, DelegationSlasher};
pub use pallet_grandpa::AuthorityId as GrandpaId;
pub use pallet_timestamp::Call as TimestampCall;
pub use pallet_transaction_payment::ChargeTransactionPayment;
use pallet_transaction_payment::{ConstFeeMultiplier, Multiplier};
use safe_mode::{RuntimeSafeMode, WitnesserCallPermission};
pub use sp_consensus_aura::sr25519::AuthorityId as AuraId;
pub use sp_core::crypto::KeyTypeId;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
pub use sp_runtime::{Perbill, Permill};
use sp_version::RuntimeVersion;

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
		SolanaBroadcaster,
		AssethubBroadcaster
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
							(
								ReputationPointsQualification<Self>,
								pallet_cf_validator::QualifyByMinimumStake<Self>,
							),
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
	type MinimumFunding = Funding;
	type CfePeerRegistration = CfeInterface;
}

parameter_types! {
	pub CurrentReleaseVersion: SemVer = SemVer {
		major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().expect("Cargo version must be set"),
		minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().expect("Cargo version must be set"),
		patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().expect("Cargo version must be set"),
	};
}

/// A workaround for the lack of `Default` implementation on
/// `pallet_transaction_payment::ChargeTransactionPayment`.
pub struct GetTransactionPayments;

impl Get<pallet_transaction_payment::ChargeTransactionPayment<Runtime>> for GetTransactionPayments {
	fn get() -> pallet_transaction_payment::ChargeTransactionPayment<Runtime> {
		pallet_transaction_payment::ChargeTransactionPayment::from(Default::default())
	}
}

impl ChainflipWithTargetChain<Instance1> for Runtime {
	type TargetChain = Ethereum;
}
impl ChainflipWithTargetChain<Instance2> for Runtime {
	type TargetChain = Polkadot;
}
impl ChainflipWithTargetChain<Instance3> for Runtime {
	type TargetChain = Bitcoin;
}
impl ChainflipWithTargetChain<Instance4> for Runtime {
	type TargetChain = Arbitrum;
}
impl ChainflipWithTargetChain<Instance5> for Runtime {
	type TargetChain = Solana;
}
impl ChainflipWithTargetChain<Instance6> for Runtime {
	type TargetChain = Assethub;
}
impl ChainflipWithTargetChain<Instance7> for Runtime {
	type TargetChain = Tron;
}

impl pallet_cf_environment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type PolkadotVaultKeyWitnessedHandler = PolkadotVault;
	type BitcoinVaultKeyWitnessedHandler = BitcoinVault;
	type ArbitrumVaultKeyWitnessedHandler = ArbitrumVault;
	type SolanaVaultKeyWitnessedHandler = SolanaVault;
	type AssethubVaultKeyWitnessedHandler = AssethubVault;
	type TronVaultKeyWitnessedHandler = TronVault;
	type SolanaNonceWatch = SolanaNonceTrackingTrigger;
	type BitcoinFeeInfo = chainflip::BitcoinFeeGetter;
	type BitcoinKeyProvider = BitcoinThresholdSigner;
	type RuntimeSafeMode = RuntimeSafeMode;
	type CurrentReleaseVersion = CurrentReleaseVersion;
	type SolEnvironment = SolEnvironment;
	type SolanaBroadcaster = SolanaBroadcaster;
	type TransactionPayments = pallet_transaction_payment::ChargeTransactionPayment<Self>;
	type GetTransactionPayments = GetTransactionPayments;
	type WeightInfo = pallet_cf_environment::weights::PalletWeight<Runtime>;
}

parameter_types! {
	pub const ScreeningBrokerId: AccountId = AccountId::new(
		// Screening Account: `cFHvfaLQ8prf25JCxY2tzGR8WuNiCLjkALzy5J3H8jbo3Brok`
		hex_literal::hex!("026de9d675fae14536ce79a478f4d16215571984b8bad180463fa27ea78d9c4f")
	);
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
	type BalanceApi = AssetBalances;
	type PoolPriceApi = LiquidityPools;
	type ChannelIdAllocator = BitcoinIngressEgress;
	type Bonder = Bonder<Runtime>;
	type PriceFeedApi = ChainlinkOracle;
	type LendingSystemApi = LendingPools;
	type FundAccount = Funding;
	type MinimumFunding = Funding;
	type RuntimeCall = RuntimeCall;
	type ChainflipNetwork = chainflip::ChainflipNetworkProvider;
}

impl pallet_cf_vaults::Config<Instance1> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SetAggKeyWithAggKey = eth::api::EthereumApi<EvmEnvironment>;
	type Broadcaster = EthereumBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = EthereumChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<Instance2> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SetAggKeyWithAggKey = dot::api::PolkadotApi<DotEnvironment>;
	type Broadcaster = PolkadotBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = PolkadotChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<Instance3> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SetAggKeyWithAggKey = cf_chains::btc::api::BitcoinApi<BtcEnvironment>;
	type Broadcaster = BitcoinBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = BitcoinChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<Instance4> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SetAggKeyWithAggKey = cf_chains::arb::api::ArbitrumApi<EvmEnvironment>;
	type Broadcaster = ArbitrumBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = ArbitrumChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<Instance5> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SetAggKeyWithAggKey = cf_chains::sol::api::SolanaApi<SolEnvironment>;
	type Broadcaster = SolanaBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = SolanaChainTrackingProvider;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<Instance6> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SetAggKeyWithAggKey = hub::api::AssethubApi<HubEnvironment>;
	type Broadcaster = AssethubBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = AssethubChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

impl pallet_cf_vaults::Config<Instance7> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SetAggKeyWithAggKey = cf_chains::tron::api::TronApi<EvmEnvironment>;
	type Broadcaster = TronBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = TronChainTracking;
	type SafeMode = RuntimeSafeMode;
	type CfeMultisigRequest = CfeInterface;
}

use chainflip::address_derivation::AddressDerivation;

impl pallet_cf_ingress_egress::Config<Instance1> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = true;
	type IngressSource = DummyIngressSource<Ethereum, BlockNumberFor<Runtime>>;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type ChainApiCall = eth::api::EthereumApi<EvmEnvironment>;
	type Broadcaster = EthereumBroadcaster;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = EthereumChainTracking;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = EvmLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<true>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
	type FundAccount = Funding;
	type LpRegistrationApi = LiquidityProvider;
}

impl pallet_cf_ingress_egress::Config<Instance2> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = false;
	type IngressSource = DummyIngressSource<Polkadot, BlockNumberFor<Runtime>>;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type ChainApiCall = dot::api::PolkadotApi<chainflip::DotEnvironment>;
	type Broadcaster = PolkadotBroadcaster;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = PolkadotChainTracking;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = NoLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<false>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
	type FundAccount = Funding;
	type LpRegistrationApi = LiquidityProvider;
}

impl pallet_cf_ingress_egress::Config<Instance3> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = false;
	type IngressSource = DummyIngressSource<Bitcoin, BlockNumberFor<Runtime>>;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type ChainApiCall = cf_chains::btc::api::BitcoinApi<chainflip::BtcEnvironment>;
	type Broadcaster = BitcoinBroadcaster;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = BitcoinChainTracking;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = NoLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<true>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
	type FundAccount = Funding;
	type LpRegistrationApi = LiquidityProvider;
}

impl pallet_cf_ingress_egress::Config<Instance4> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = true;
	type IngressSource = DummyIngressSource<Arbitrum, BlockNumberFor<Runtime>>;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type ChainApiCall = ArbitrumApi<EvmEnvironment>;
	type Broadcaster = ArbitrumBroadcaster;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = ArbitrumChainTracking;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = EvmLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<true>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
	type FundAccount = Funding;
	type LpRegistrationApi = LiquidityProvider;
}

impl pallet_cf_ingress_egress::Config<Instance5> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = false;
	const ONLY_PREALLOCATE_FROM_POOL: bool = false;
	type IngressSource = SolanaIngress;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type ChainApiCall = cf_chains::sol::api::SolanaApi<SolEnvironment>;
	type Broadcaster = SolanaBroadcaster;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = SolanaChainTrackingProvider;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = SolanaLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<true>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
	type FundAccount = Funding;
	type LpRegistrationApi = LiquidityProvider;
}

impl pallet_cf_ingress_egress::Config<Instance6> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = false;
	type IngressSource = DummyIngressSource<Assethub, BlockNumberFor<Runtime>>;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type ChainApiCall = hub::api::AssethubApi<chainflip::HubEnvironment>;
	type Broadcaster = AssethubBroadcaster;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = AssethubChainTracking;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = NoLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<false>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
	type FundAccount = Funding;
	type LpRegistrationApi = LiquidityProvider;
}

impl pallet_cf_ingress_egress::Config<Instance7> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = false;
	type IngressSource = DummyIngressSource<Tron, BlockNumberFor<Runtime>>;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type ChainApiCall = cf_chains::tron::api::TronApi<EvmEnvironment>;
	type Broadcaster = TronBroadcaster;
	type WeightInfo = pallet_cf_ingress_egress::weights::PalletWeight<Runtime>;
	type DepositHandler = chainflip::DepositHandler;
	type ChainTracking = TronChainTracking;
	type NetworkEnvironment = Environment;
	type AssetConverter = Swapping;
	type FeePayment = Flip;
	type SwapRequestHandler = Swapping;
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = EvmLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<true>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
	type FundAccount = Funding;
	type LpRegistrationApi = LiquidityProvider;
}

impl pallet_cf_pools::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type LpBalance = AssetBalances;
	type LpStats = LiquidityProvider;
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
	type BoostBalancesApi = LendingPools;
	type SwapRequestHandler = Swapping;
	type WeightInfo = pallet_cf_lp::weights::PalletWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type FeePayment = Flip;
	type MinimumDeposit = MinimumDepositProvider;
}

impl pallet_cf_account_roles::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type DeregistrationCheck = (Bonder<Self>, TradingStrategyDeregistrationCheck<Self>);
	type RuntimeCall = RuntimeCall;
	type SpawnAccount = Funding;
	#[cfg(feature = "runtime-benchmarks")]
	type FeePayment = Flip;
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

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(50);
const BLOCK_LENGTH_RATIO: Perbill = Perbill::from_percent(40);
pub const MAX_BLOCK_LENGTH: u32 = 1024 * 1024 * 625 / 100; // 6.25 MB

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
		::max_with_normal_ratio(MAX_BLOCK_LENGTH, BLOCK_LENGTH_RATIO);
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
		pallet_cf_validator::DelegatedAccountCleanup<Self>,
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
	type EventHandler = Emissions;
}

impl pallet_cf_flip::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = FlipBalance;
	type BlocksPerDay = ConstU32<DAYS>;
	type WeightInfo = pallet_cf_flip::weights::PalletWeight<Runtime>;
	type WaivedFees = chainflip::WaivedFees;
	type CallIndexer = chainflip::LpOrderCallIndexer;
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
	type EthereumSCApi = crate::chainflip::ethereum_sc_calls::EthereumSCApi<FlipBalance>;
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
	type ProposalFee = ConstU128<{ 1_000 * cf_primitives::FLIPPERINOS_PER_FLIP }>;
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
	type ApiCall = eth::api::EthereumApi<EvmEnvironment>;
	type Broadcaster = EthereumBroadcaster;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;
	type RewardsDistribution = DelegatedRewardsDistribution<Runtime, FlipIssuance<Runtime>>;
	type CompoundingInterval = ConstU32<COMPOUNDING_INTERVAL>;
	type EthEnvironment = EvmEnvironment;
	type FlipToBurnOrMove = Swapping;
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
	type HeartbeatBlockInterval = ConstU32<HEARTBEAT_BLOCK_INTERVAL>;
	type ReputationPointFloorAndCeiling = ReputationPointFloorAndCeiling;
	type Slasher = DelegationSlasher<Runtime, FlipSlasher<Runtime>>;
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
	type VaultActivator = MultiVaultActivator<EthereumVault, ArbitrumVault, TronVault>;
	type OffenceReporter = Reputation;
	type CeremonyRetryDelay = ConstU32<1>;
	type SafeMode = RuntimeSafeMode;
	type Slasher = DelegationSlasher<Runtime, FlipSlasher<Runtime>>;
	type CfeMultisigRequest = CfeInterface;
	type Weights = pallet_cf_threshold_signature::weights::PalletWeight<Self>;
}

impl pallet_cf_threshold_signature::Config<Instance15> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Offence = chainflip::Offence;
	type RuntimeOrigin = RuntimeOrigin;
	type ThresholdCallable = RuntimeCall;
	type ThresholdSignerNomination = chainflip::RandomSignerNomination;
	type TargetChainCrypto = PolkadotCrypto;
	type VaultActivator = AssethubVault;
	type OffenceReporter = Reputation;
	type CeremonyRetryDelay = ConstU32<1>;
	type SafeMode = RuntimeSafeMode;
	type Slasher = DelegationSlasher<Runtime, FlipSlasher<Runtime>>;
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
	type Slasher = DelegationSlasher<Runtime, FlipSlasher<Runtime>>;
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
	type Slasher = DelegationSlasher<Runtime, FlipSlasher<Runtime>>;
	type CfeMultisigRequest = CfeInterface;
	type Weights = pallet_cf_threshold_signature::weights::PalletWeight<Self>;
}

impl pallet_cf_broadcast::Config<Instance1> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type ApiCall = eth::api::EthereumApi<EvmEnvironment>;
	type ThresholdSigner = EvmThresholdSigner;
	type TransactionBuilder = chainflip::EthTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, EvmInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type OnBroadcastSuccess = pallet_cf_ingress_egress::Pallet<Runtime, EthereumInstance>;
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
	type ApiCall = dot::api::PolkadotApi<DotEnvironment>;
	type ThresholdSigner = PolkadotThresholdSigner;
	type TransactionBuilder = chainflip::DotTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, PolkadotCryptoInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type OnBroadcastSuccess = ();
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
	type ApiCall = cf_chains::btc::api::BitcoinApi<BtcEnvironment>;
	type ThresholdSigner = BitcoinThresholdSigner;
	type TransactionBuilder = chainflip::BtcTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, BitcoinInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type OnBroadcastSuccess = ();
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
	type ApiCall = cf_chains::arb::api::ArbitrumApi<EvmEnvironment>;
	type ThresholdSigner = EvmThresholdSigner;
	type TransactionBuilder = chainflip::ArbTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, EvmInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type OnBroadcastSuccess = pallet_cf_ingress_egress::Pallet<Runtime, ArbitrumInstance>;
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

impl pallet_cf_broadcast::Config<Instance5> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type ApiCall = cf_chains::sol::api::SolanaApi<SolEnvironment>;
	type ThresholdSigner = SolanaThresholdSigner;
	type TransactionBuilder = chainflip::SolanaTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, SolanaInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type OnBroadcastSuccess = ();
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

impl pallet_cf_broadcast::Config<Instance6> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type ApiCall = hub::api::AssethubApi<HubEnvironment>;
	type ThresholdSigner = PolkadotThresholdSigner;
	type TransactionBuilder = chainflip::DotTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, PolkadotCryptoInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type OnBroadcastSuccess = ();
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type SafeModeBlockMargin = ConstU32<10>;
	type SafeModeChainBlockMargin = ConstU32<BLOCKS_PER_MINUTE_POLKADOT>;
	type ChainTracking = AssethubChainTracking;
	type RetryPolicy = DefaultRetryPolicy;
	type LiabilityTracker = AssetBalances;
	type CfeBroadcastRequest = CfeInterface;
	type ElectionEgressWitnesser = DummyEgressSuccessWitnesser<PolkadotCrypto>;
}

impl pallet_cf_broadcast::Config<Instance7> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type ApiCall = cf_chains::tron::api::TronApi<EvmEnvironment>;
	type ThresholdSigner = EvmThresholdSigner;
	type TransactionBuilder = chainflip::TronTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, EvmInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
	type OnBroadcastSuccess = ();
	type WeightInfo = pallet_cf_broadcast::weights::PalletWeight<Runtime>;
	type SafeMode = RuntimeSafeMode;
	type SafeModeBlockMargin = ConstU32<10>;
	type SafeModeChainBlockMargin = ConstU64<10>;
	type ChainTracking = TronChainTracking;
	type RetryPolicy = DefaultRetryPolicy;
	type LiabilityTracker = AssetBalances;
	type CfeBroadcastRequest = CfeInterface;
	type ElectionEgressWitnesser = DummyEgressSuccessWitnesser<EvmCrypto>;
}

impl pallet_cf_asset_balances::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type EgressHandler = chainflip::AnyChainIngressEgressHandler;
	type PolkadotKeyProvider = PolkadotThresholdSigner;
	type PoolApi = LiquidityPools;
	type SafeMode = RuntimeSafeMode;
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

impl pallet_cf_chain_tracking::Config<Instance6> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Assethub;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
}

impl pallet_cf_chain_tracking::Config<Instance7> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type TargetChain = Tron;
	type WeightInfo = pallet_cf_chain_tracking::weights::PalletWeight<Runtime>;
}

impl pallet_cf_elections::Config<Instance5> for Runtime {
	const TYPE_INFO_SUFFIX: &'static str = <Solana as ChainInstanceAlias>::TYPE_INFO_SUFFIX;
	type RuntimeEvent = RuntimeEvent;
	type ElectoralSystemRunner =
		chainflip::witnessing::solana_elections::SolanaElectoralSystemRunner;
	type WeightInfo = pallet_cf_elections::weights::PalletWeight<Runtime>;
	type ElectoralSystemConfiguration =
		chainflip::witnessing::solana_elections::SolanaElectoralSystemConfiguration;
	type SafeMode = RuntimeSafeMode;
}

impl pallet_cf_elections::Config<Instance3> for Runtime {
	const TYPE_INFO_SUFFIX: &'static str = <Bitcoin as ChainInstanceAlias>::TYPE_INFO_SUFFIX;
	type RuntimeEvent = RuntimeEvent;
	type ElectoralSystemRunner =
		chainflip::witnessing::bitcoin_elections::BitcoinElectoralSystemRunner;
	type WeightInfo = pallet_cf_elections::weights::PalletWeight<Runtime>;
	type ElectoralSystemConfiguration =
		chainflip::witnessing::bitcoin_elections::BitcoinElectoralSystemConfiguration;
	type SafeMode = RuntimeSafeMode;
}

impl pallet_cf_elections::Config for Runtime {
	const TYPE_INFO_SUFFIX: &'static str = "GenericElections";
	type RuntimeEvent = RuntimeEvent;
	type ElectoralSystemRunner =
		chainflip::witnessing::generic_elections::GenericElectoralSystemRunner;
	type WeightInfo = pallet_cf_elections::weights::PalletWeight<Runtime>;
	type ElectoralSystemConfiguration =
		chainflip::witnessing::generic_elections::GenericElectionHooks;
	type SafeMode = RuntimeSafeMode;
}

impl pallet_cf_elections::Config<Instance1> for Runtime {
	const TYPE_INFO_SUFFIX: &'static str = <Ethereum as ChainInstanceAlias>::TYPE_INFO_SUFFIX;
	type RuntimeEvent = RuntimeEvent;
	type ElectoralSystemRunner =
		chainflip::witnessing::ethereum_elections::EthereumElectoralSystemRunner;
	type WeightInfo = pallet_cf_elections::weights::PalletWeight<Runtime>;
	type ElectoralSystemConfiguration =
		chainflip::witnessing::ethereum_elections::ElectoralSystemConfiguration;
	type SafeMode = RuntimeSafeMode;
}

impl pallet_cf_elections::Config<Instance4> for Runtime {
	const TYPE_INFO_SUFFIX: &'static str = <Arbitrum as ChainInstanceAlias>::TYPE_INFO_SUFFIX;
	type RuntimeEvent = RuntimeEvent;
	type ElectoralSystemRunner =
		chainflip::witnessing::arbitrum_elections::ArbitrumElectoralSystemRunner;
	type WeightInfo = pallet_cf_elections::weights::PalletWeight<Runtime>;
	type ElectoralSystemConfiguration =
		chainflip::witnessing::arbitrum_elections::ElectoralSystemConfiguration;
	type SafeMode = RuntimeSafeMode;
}

impl pallet_cf_trading_strategy::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_cf_trading_strategy::weights::PalletWeight<Runtime>;
	type LpOrdersWeights = LiquidityPools;
	type BalanceApi = AssetBalances;
	type SafeMode = RuntimeSafeMode;
	type PoolApi = LiquidityPools;
	type LpRegistrationApi = LiquidityProvider;
}

impl pallet_cf_lending_pools::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_cf_lending_pools::weights::PalletWeight<Runtime>;
	type Balance = AssetBalances;
	type SwapRequestHandler = Swapping;
	type SafeMode = RuntimeSafeMode;
	type PoolApi = LiquidityPools;
	type PriceApi = ChainlinkOracle;
	type LpRegistrationApi = LiquidityProvider;
}
