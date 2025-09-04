// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![feature(btree_extract_if)]
#![feature(step_trait)]
#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "512"]
pub mod chainflip;
pub mod constants;
pub mod migrations;
pub mod monitoring_apis;
pub mod runtime_apis;
pub mod safe_mode;
#[cfg(all(feature = "std", feature = "mocks"))]
pub mod test_runner;
mod weights;
use crate::{
	chainflip::{
		address_derivation::btc::{
			derive_btc_vault_deposit_addresses, BitcoinPrivateBrokerDepositAddresses,
		},
		calculate_account_apy,
		ethereum_sc_calls::EthereumAccount,
		solana_elections::{
			SolanaChainTrackingProvider, SolanaEgressWitnessingTrigger, SolanaIngress,
			SolanaNonceTrackingTrigger,
		},
		EvmLimit, Offence,
	},
	monitoring_apis::{
		ActivateKeysBroadcastIds, AuthoritiesInfo, BtcUtxos, EpochState, ExternalChainsBlockHeight,
		FeeImbalance, FlipSupply, LastRuntimeUpgradeInfo, OpenDepositChannels, PendingBroadcasts,
		PendingTssCeremonies, RedemptionsInfo, SolanaNonces,
	},
	runtime_apis::{
		runtime_decl_for_custom_runtime_api::CustomRuntimeApi, AuctionState, BoostPoolDepth,
		BoostPoolDetails, BrokerInfo, CcmData, ChannelActionType, DispatchErrorWithMessage,
		FailingWitnessValidators, FeeTypes, LiquidityProviderBoostPoolInfo, LiquidityProviderInfo,
		NetworkFeeDetails, NetworkFees, OpenedDepositChannels, OperatorInfo, RuntimeApiPenalty,
		SimulateSwapAdditionalOrder, SimulatedSwapInformation, TradingStrategyInfo,
		TradingStrategyLimits, TransactionScreeningEvent, TransactionScreeningEvents,
		ValidatorInfo, VaultAddresses, VaultSwapDetails,
	},
};
use cf_amm::{
	common::PoolPairsMap,
	math::{Amount, Tick},
	range_orders::Liquidity,
};
pub use cf_chains::instances::{
	ArbitrumInstance, AssethubInstance, BitcoinInstance, EthereumInstance, EvmInstance,
	PolkadotCryptoInstance, PolkadotInstance, SolanaInstance,
};
use cf_chains::{
	address::{AddressConverter, EncodedAddress, IntoForeignChainAddress},
	arb::api::ArbitrumApi,
	assets::any::{AssetMap, ForeignChainAndAsset},
	btc::{api::BitcoinApi, BitcoinCrypto, BitcoinRetryPolicy, ScriptPubkey},
	cf_parameters::build_and_encode_cf_parameters,
	dot::{self, PolkadotAccountId, PolkadotCrypto},
	eth::{self, api::EthereumApi, Address as EthereumAddress, Ethereum},
	evm::{api::EvmCall, EvmCrypto},
	hub,
	instances::ChainInstanceAlias,
	sol::{SolAddress, SolanaCrypto},
	Arbitrum, Assethub, Bitcoin, CcmChannelMetadataUnchecked,
	ChannelRefundParametersUncheckedEncoded, DefaultRetryPolicy, EvmVaultSwapExtraParameters,
	ForeignChain, Polkadot, Solana, TransactionBuilder, VaultSwapExtraParameters,
	VaultSwapExtraParametersEncoded, VaultSwapInputEncoded,
};
use cf_primitives::{
	Affiliates, BasisPoints, Beneficiary, BroadcastId, ChannelId, DcaParameters, EpochIndex,
	NetworkEnvironment, STABLE_ASSET,
};
use cf_traits::{
	AdjustedFeeEstimationApi, AssetConverter, BalanceApi, DummyEgressSuccessWitnesser,
	DummyIngressSource, EpochKey, GetBlockHeight, KeyProvider, MinimumDeposit, NoLimit, SwapLimits,
	SwapParameterValidation,
};
use codec::{alloc::string::ToString, Decode, Encode};
use core::ops::Range;
use frame_support::{derive_impl, instances::*, migrations::VersionedMigration};
pub use frame_system::Call as SystemCall;
use monitoring_apis::MonitoringDataV2;
use pallet_cf_elections::electoral_systems::oracle_price::{
	chainlink::{get_latest_oracle_prices, OraclePrice},
	price::PriceAsset,
};
use pallet_cf_governance::GovCallHash;
use pallet_cf_ingress_egress::IngressOrEgress;
use pallet_cf_pools::{
	AskBidMap, HistoricalEarnedFees, PoolLiquidity, PoolOrderbook, PoolPriceV1, PoolPriceV2,
	UnidirectionalPoolDepth,
};
use pallet_cf_reputation::{ExclusionList, HeartbeatQualification, ReputationPointsQualification};
use pallet_cf_swapping::{
	AffiliateDetails, BatchExecutionError, BrokerPrivateBtcChannels, FeeType, NetworkFeeTracker,
	Swap, SwapLegInfo,
};
use pallet_cf_trading_strategy::TradingStrategyDeregistrationCheck;
use pallet_cf_validator::{
	AssociationToOperator, DelegatedRewardsDistribution, DelegationAcceptance, DelegationAmount,
	DelegationSlasher, SetSizeMaximisingAuctionResolver,
};
use pallet_transaction_payment::{ConstFeeMultiplier, Multiplier};
use runtime_apis::{ChainAccounts, EvmCallDetails};
use scale_info::prelude::string::String;
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

use crate::chainflip::ethereum_sc_calls::EthereumSCApi;
use cf_chains::evm::{
	api::sc_utils::{
		deposit_flip_to_sc_gateway_and_call::DepositToSCGatewayAndCall, sc_call::SCCall,
	},
	U256,
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
use frame_system::{offchain::SendTransactionTypes, pallet_prelude::BlockNumberFor};
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
	AccountInfo, BoostBalancesApi, Chainflip, EpochInfo, OrderId, PoolApi, QualifyNode,
	SessionKeysRegistered, SwappingApi,
};
// Required for genesis config.
pub use pallet_cf_validator::SetSizeParameters;

use chainflip::{
	epoch_transition::ChainflipEpochTransitions, multi_vault_activator::MultiVaultActivator,
	BroadcastReadyProvider, BtcEnvironment, CfCcmAdditionalDataHandler, ChainAddressConverter,
	ChainlinkOracle, DotEnvironment, EvmEnvironment, HubEnvironment, MinimumDepositProvider,
	SolEnvironment, SolanaLimit, TokenholderGovernanceBroadcaster,
};
use safe_mode::{RuntimeSafeMode, WitnesserCallPermission};

use constants::common::*;
use pallet_cf_flip::{Bonder, FlipIssuance, FlipSlasher};
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
	spec_version: 1_11_00,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 13,
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
								pallet_cf_validator::QualifyByMinimumBid<Self>,
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
	type AssethubVaultKeyWitnessedHandler = AssethubVault;
	type SolanaNonceWatch = SolanaNonceTrackingTrigger;
	type BitcoinFeeInfo = chainflip::BitcoinFeeGetter;
	type BitcoinKeyProvider = BitcoinThresholdSigner;
	type RuntimeSafeMode = RuntimeSafeMode;
	type CurrentReleaseVersion = CurrentReleaseVersion;
	type SolEnvironment = SolEnvironment;
	type SolanaBroadcaster = SolanaBroadcaster;
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

impl pallet_cf_vaults::Config<Instance6> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Chain = Assethub;
	type SetAggKeyWithAggKey = hub::api::AssethubApi<HubEnvironment>;
	type Broadcaster = AssethubBroadcaster;
	type WeightInfo = pallet_cf_vaults::weights::PalletWeight<Runtime>;
	type ChainTracking = AssethubChainTracking;
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
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = EvmLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<true>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
}

impl pallet_cf_ingress_egress::Config<Instance2> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = false;
	type IngressSource = DummyIngressSource<Polkadot, BlockNumberFor<Runtime>>;
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
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = NoLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<false>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
}

impl pallet_cf_ingress_egress::Config<Instance3> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = false;
	type IngressSource = DummyIngressSource<Bitcoin, BlockNumberFor<Runtime>>;
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
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = NoLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<true>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
}

impl pallet_cf_ingress_egress::Config<Instance4> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = true;
	type IngressSource = DummyIngressSource<Arbitrum, BlockNumberFor<Runtime>>;
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
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = EvmLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<true>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
}

impl pallet_cf_ingress_egress::Config<Instance5> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = false;
	const ONLY_PREALLOCATE_FROM_POOL: bool = false;
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
	type CcmAdditionalDataHandler = CfCcmAdditionalDataHandler;
	type AssetWithholding = AssetBalances;
	type FetchesTransfersLimitProvider = SolanaLimit;
	type SafeMode = RuntimeSafeMode;
	type SwapParameterValidation = Swapping;
	type AffiliateRegistry = Swapping;
	type AllowTransactionReports = ConstBool<false>;
	type ScreeningBrokerId = ScreeningBrokerId;
	type BoostApi = LendingPools;
}

impl pallet_cf_ingress_egress::Config<Instance6> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	const MANAGE_CHANNEL_LIFETIME: bool = true;
	const ONLY_PREALLOCATE_FROM_POOL: bool = false;
	type IngressSource = DummyIngressSource<Assethub, BlockNumberFor<Runtime>>;
	type TargetChain = Assethub;
	type AddressDerivation = AddressDerivation;
	type AddressConverter = ChainAddressConverter;
	type Balance = AssetBalances;
	type PoolApi = LiquidityPools;
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
	type ApiCall = eth::api::EthereumApi<EvmEnvironment>;
	type Broadcaster = EthereumBroadcaster;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;
	type RewardsDistribution = DelegatedRewardsDistribution<Runtime, FlipIssuance<Runtime>>;
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
	type VaultActivator = MultiVaultActivator<EthereumVault, ArbitrumVault>;
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
	type VaultActivator = MultiVaultActivator<PolkadotVault, AssethubVault>;
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
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, PolkadotCryptoInstance>;
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

impl pallet_cf_broadcast::Config<Instance6> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;
	type BroadcastCallable = RuntimeCall;
	type Offence = chainflip::Offence;
	type TargetChain = Assethub;
	type ApiCall = hub::api::AssethubApi<HubEnvironment>;
	type ThresholdSigner = PolkadotThresholdSigner;
	type TransactionBuilder = chainflip::DotTransactionBuilder;
	type BroadcastSignerNomination = chainflip::RandomSignerNomination;
	type OffenceReporter = Reputation;
	type EnsureThresholdSigned =
		pallet_cf_threshold_signature::EnsureThresholdSigned<Self, PolkadotCryptoInstance>;
	type BroadcastReadyProvider = BroadcastReadyProvider;
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

impl pallet_cf_asset_balances::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type EgressHandler = chainflip::AnyChainIngressEgressHandler;
	type PolkadotKeyProvider = PolkadotThresholdSigner;
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

impl pallet_cf_elections::Config<Instance5> for Runtime {
	const TYPE_INFO_SUFFIX: &'static str = <Solana as ChainInstanceAlias>::TYPE_INFO_SUFFIX;
	type RuntimeEvent = RuntimeEvent;
	type ElectoralSystemRunner = chainflip::solana_elections::SolanaElectoralSystemRunner;
	type WeightInfo = pallet_cf_elections::weights::PalletWeight<Runtime>;
	type ElectoralSystemConfiguration =
		chainflip::solana_elections::SolanaElectoralSystemConfiguration;
	type SafeMode = RuntimeSafeMode;
}

impl pallet_cf_elections::Config<Instance3> for Runtime {
	const TYPE_INFO_SUFFIX: &'static str = <Bitcoin as ChainInstanceAlias>::TYPE_INFO_SUFFIX;
	type RuntimeEvent = RuntimeEvent;
	type ElectoralSystemRunner = chainflip::bitcoin_elections::BitcoinElectoralSystemRunner;
	type WeightInfo = pallet_cf_elections::weights::PalletWeight<Runtime>;
	type ElectoralSystemConfiguration =
		chainflip::bitcoin_elections::BitcoinElectoralSystemConfiguration;
	type SafeMode = RuntimeSafeMode;
}

impl pallet_cf_elections::Config for Runtime {
	const TYPE_INFO_SUFFIX: &'static str = "GenericElections";
	type RuntimeEvent = RuntimeEvent;
	type ElectoralSystemRunner = chainflip::generic_elections::GenericElectoralSystemRunner;
	type WeightInfo = pallet_cf_elections::weights::PalletWeight<Runtime>;
	type ElectoralSystemConfiguration = chainflip::generic_elections::GenericElectionHooks;
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
	pub type PolkadotThresholdSigner = pallet_cf_threshold_signature<Instance15>;
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

	#[runtime::pallet_index(48)]
	pub type AssethubChainTracking = pallet_cf_chain_tracking<Instance6>;
	#[runtime::pallet_index(49)]
	pub type AssethubVault = pallet_cf_vaults<Instance6>;
	#[runtime::pallet_index(50)]
	pub type AssethubBroadcaster = pallet_cf_broadcast<Instance6>;
	#[runtime::pallet_index(51)]
	pub type AssethubIngressEgress = pallet_cf_ingress_egress<Instance6>;

	#[runtime::pallet_index(52)]
	pub type TradingStrategy = pallet_cf_trading_strategy;

	#[runtime::pallet_index(53)]
	pub type LendingPools = pallet_cf_lending_pools;

	#[runtime::pallet_index(54)]
	pub type BitcoinElections = pallet_cf_elections<Instance3>;

	#[runtime::pallet_index(55)]
	pub type GenericElections = pallet_cf_elections;
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
	frame_metadata_hash_extension::CheckMetadataHash<Runtime>,
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
	AssethubChainTracking,
	// Elections
	SolanaElections,
	BitcoinElections,
	GenericElections,
	// Vaults
	EthereumVault,
	PolkadotVault,
	BitcoinVault,
	ArbitrumVault,
	SolanaVault,
	AssethubVault,
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
	AssethubBroadcaster,
	// Swapping and Liquidity Provision
	Swapping,
	LiquidityProvider,
	// Ingress Egress
	EthereumIngressEgress,
	PolkadotIngressEgress,
	BitcoinIngressEgress,
	ArbitrumIngressEgress,
	SolanaIngressEgress,
	AssethubIngressEgress,
	// Liquidity Pools
	LiquidityPools,
	// Miscellaneous
	TradingStrategy,
	LendingPools,
);

/// Contains:
/// - ClearEvents in CfeInterface migration. Don't remove this.
/// - The VersionUpdate migration. Don't remove this.
/// - Individual pallet migrations. Don't remove these unless there's a good reason. Prefer to
///   disable these at the pallet level (ie. set it to () or PhantomData).
/// - Housekeeping migrations. Don't remove this - check individual housekeeping migrations and
///   remove them when they are no longer needed.
/// - Release-specific migrations: remove these if they are no longer needed.
type AllMigrations = (
	// This ClearEvents should only be run at the start of all migrations. This is in case another
	// migration needs to trigger an event like a Broadcast for example.
	pallet_cf_cfe_interface::migrations::ClearEvents<Runtime>,
	// DO NOT REMOVE `VersionUpdate`. THIS IS REQUIRED TO UPDATE THE VERSION FOR THE CFEs EVERY
	// UPGRADE
	pallet_cf_environment::migrations::VersionUpdate<Runtime>,
	PalletMigrations,
	migrations::housekeeping::Migration,
	migrations::bitcoin_elections::Migration,
	migrations::generic_elections::Migration,
	VersionedMigration<
		6,
		7,
		NoopMigration,
		pallet_cf_elections::Pallet<Runtime, SolanaInstance>,
		<Runtime as frame_system::Config>::DbWeight,
	>,
	MigrationsForV1_11,
);

/// All the pallet-specific migrations and migrations that depend on pallet migration order. Do not
/// comment out or remove pallet migrations. Prefer to delete the migration at the pallet level and
/// replace with a dummy migration.
type PalletMigrations = (
	pallet_cf_environment::migrations::PalletMigration<Runtime>,
	pallet_cf_funding::migrations::PalletMigration<Runtime>,
	pallet_cf_account_roles::migrations::PalletMigration<Runtime>,
	pallet_cf_validator::migrations::PalletMigration<Runtime>,
	pallet_cf_emissions::migrations::PalletMigration<Runtime>,
	pallet_cf_governance::migrations::PalletMigration<Runtime>,
	pallet_cf_tokenholder_governance::migrations::PalletMigration<Runtime>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, EthereumInstance>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, PolkadotInstance>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, ArbitrumInstance>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, SolanaInstance>,
	pallet_cf_chain_tracking::migrations::PalletMigration<Runtime, AssethubInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, EthereumInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, PolkadotInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, ArbitrumInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, SolanaInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, AssethubInstance>,
	pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, EvmInstance>,
	pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, PolkadotCryptoInstance>,
	pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_threshold_signature::migrations::PalletMigration<Runtime, SolanaInstance>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, EthereumInstance>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, PolkadotInstance>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, ArbitrumInstance>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, SolanaInstance>,
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, AssethubInstance>,
	pallet_cf_swapping::migrations::PalletMigration<Runtime>,
	pallet_cf_lp::migrations::PalletMigration<Runtime>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, EthereumInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, PolkadotInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, ArbitrumInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, SolanaInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, AssethubInstance>,
	pallet_cf_pools::migrations::PalletMigration<Runtime>,
	pallet_cf_cfe_interface::migrations::PalletMigration<Runtime>,
	pallet_cf_trading_strategy::migrations::PalletMigration<Runtime>,
	pallet_cf_lending_pools::migrations::PalletMigration<Runtime>,
	pallet_cf_elections::migrations::PalletMigration<Runtime, SolanaInstance>,
);

pub struct NoopMigration;
impl frame_support::traits::UncheckedOnRuntimeUpgrade for NoopMigration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("🤷 Noop migration");
		Default::default()
	}
}

#[allow(unused_macros)]
macro_rules! instanced_migrations {
	(
		module: $module:ident,
		migration: $migration:ty,
		from: $from:literal,
		to: $to:literal,
		include_instances: [$( $include:ident ),+ $(,)?],
		exclude_instances: [$( $exclude:ident ),* $(,)?] $(,)?
	) => {
		(
			$(
				VersionedMigration<
					$from,
					$to,
					$migration,
					$module::Pallet<Runtime, $include>,
					DbWeight,
				>,
			)+
			$(
				VersionedMigration<
					$from,
					$to,
					NoopMigration,
					$module::Pallet<Runtime, $exclude>,
					DbWeight,
				>,
			)*
		)
	}
}

type MigrationsForV1_11 = (
	VersionedMigration<
		18,
		19,
		migrations::safe_mode::SafeModeMigration,
		pallet_cf_environment::Pallet<Runtime>,
		<Runtime as frame_system::Config>::DbWeight,
	>,
);

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
extern crate frame_benchmarking;
extern crate core;

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
		[pallet_cf_trading_strategy, TradingStrategy]
		[pallet_cf_lending_pools, LendingPools]
	);
}

impl_runtime_apis! {
	impl runtime_apis::ElectoralRuntimeApi<Block> for Runtime {
		fn cf_solana_electoral_data(account_id: AccountId) -> Vec<u8> {
			SolanaElections::electoral_data(&account_id).encode()
		}

		fn cf_solana_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			SolanaElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}

		fn cf_bitcoin_electoral_data(account_id: AccountId) -> Vec<u8> {
			BitcoinElections::electoral_data(&account_id).encode()
		}

		fn cf_bitcoin_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			BitcoinElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}

		fn cf_generic_electoral_data(account_id: AccountId) -> Vec<u8> {
			GenericElections::electoral_data(&account_id).encode()
		}

		fn cf_generic_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			GenericElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
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
			Validator::epoch_duration()
		}
		fn cf_current_epoch_started_at() -> u32 {
			Validator::current_epoch_started_at()
		}
		fn cf_authority_emission_per_block() -> u128 {
			Emissions::current_authority_emission_per_block()
		}
		fn cf_backup_emission_per_block() -> u128 {
			0 // Backups don't exist any more.
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
			LiquidityPools::sweep(&account_id).unwrap();
			let free_balances = AssetBalances::free_balances(&account_id);
			let open_order_balances = LiquidityPools::open_order_balances(&account_id);

			let boost_pools_balances = AssetMap::from_fn(|asset| {
				LendingPools::boost_pool_account_balance(&account_id, asset)
			});

			free_balances.saturating_add(open_order_balances).saturating_add(boost_pools_balances)
		}
		fn cf_account_flip_balance(account_id: &AccountId) -> u128 {
			pallet_cf_flip::Account::<Runtime>::get(account_id).total()
		}
		fn cf_validator_info(account_id: &AccountId) -> ValidatorInfo {
			let key_holder_epochs = pallet_cf_validator::HistoricalActiveEpochs::<Runtime>::get(account_id);
			let is_qualified = <<Runtime as pallet_cf_validator::Config>::KeygenQualification as QualifyNode<_>>::is_qualified(account_id);
			let is_current_authority = pallet_cf_validator::CurrentAuthorities::<Runtime>::get().contains(account_id);
			let is_bidding = Validator::is_bidding(account_id);
			let bound_redeem_address = pallet_cf_funding::BoundRedeemAddress::<Runtime>::get(account_id);
			let apy_bp = calculate_account_apy(account_id);
			let reputation_info = pallet_cf_reputation::Reputations::<Runtime>::get(account_id);
			let account_info = pallet_cf_flip::Account::<Runtime>::get(account_id);
			let restricted_balances = pallet_cf_funding::RestrictedBalances::<Runtime>::get(account_id);
			let estimated_redeemable_balance = pallet_cf_funding::Redemption::<Runtime>::for_rpc(
				account_id,
			).map(|redemption| redemption.redeem_amount).unwrap_or_default();
			ValidatorInfo {
				balance: account_info.total(),
				bond: account_info.bond(),
				last_heartbeat: pallet_cf_reputation::LastHeartbeat::<Runtime>::get(account_id).unwrap_or(0),
				reputation_points: reputation_info.reputation_points,
				keyholder_epochs: key_holder_epochs,
				is_current_authority,
				is_current_backup: false,
				is_qualified: is_bidding && is_qualified,
				is_online: HeartbeatQualification::<Runtime>::is_qualified(account_id),
				is_bidding,
				bound_redeem_address,
				apy_bp,
				restricted_balances,
				estimated_redeemable_balance,
				operator: pallet_cf_validator::ManagedValidators::<Runtime>::get(account_id),
			}
		}

		fn cf_operator_info(account_id: &AccountId) -> OperatorInfo<FlipBalance> {
			let settings= pallet_cf_validator::OperatorSettingsLookup::<Runtime>::get(account_id).unwrap_or_default();
			let exceptions = pallet_cf_validator::Exceptions::<Runtime>::get(account_id).into_iter().collect();
			let (allowed, blocked) = match &settings.delegation_acceptance {
				DelegationAcceptance::Allow => (Default::default(), exceptions),
				DelegationAcceptance::Deny => (exceptions, Default::default()),
			};
			OperatorInfo {
				managed_validators: pallet_cf_validator::Pallet::<Runtime>::get_all_associations_by_operator(
					account_id,
					AssociationToOperator::Validator,
					pallet_cf_flip::Pallet::<Runtime>::balance
				),
				settings,
				allowed,
				blocked,
				delegators: pallet_cf_validator::Pallet::<Runtime>::get_all_associations_by_operator(
					account_id,
					AssociationToOperator::Delegator,
					pallet_cf_flip::Pallet::<Runtime>::balance
				),
				flip_balance: pallet_cf_flip::Account::<Runtime>::get(account_id).total(),
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
				epoch_duration: Validator::epoch_duration(),
				current_epoch_started_at: Validator::current_epoch_started_at(),
				redemption_period_as_percentage: Validator::redemption_period_as_percentage().deconstruct(),
				min_funding: MinimumFunding::<Runtime>::get().unique_saturated_into(),
				min_bid: pallet_cf_validator::MinimumAuctionBid::<Runtime>::get().unique_saturated_into(),
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
			ccm_data: Option<CcmData>,
			exclude_fees: BTreeSet<FeeTypes>,
			additional_orders: Option<Vec<SimulateSwapAdditionalOrder>>,
			is_internal: Option<bool>,
		) -> Result<SimulatedSwapInformation, DispatchErrorWithMessage> {
			let is_internal = is_internal.unwrap_or_default();
			let mut exclude_fees = exclude_fees;
			if is_internal {
				exclude_fees.insert(FeeTypes::IngressDepositChannel);
				exclude_fees.insert(FeeTypes::Egress);
				exclude_fees.insert(FeeTypes::IngressVaultSwap);
			}

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
					ForeignChainAndAsset::Assethub(asset) => {
						let AmountAndFeesWithheld {
							amount_after_fees,
							fees_withheld,
						} = pallet_cf_ingress_egress::Pallet::<Runtime, AssethubInstance>::withhold_ingress_or_egress_fee(ingress_or_egress, asset, amount.unique_saturated_into());

						(amount_after_fees, fees_withheld)
					},
				}
			}

			let include_fee = |fee_type: FeeTypes| !exclude_fees.contains(&fee_type);

			// Default to using the DepositChannel fee unless specified.
			let (amount_to_swap, ingress_fee) = if include_fee(FeeTypes::IngressDepositChannel) {
				remove_fees(IngressOrEgress::IngressDepositChannel, input_asset, input_amount)
			} else if include_fee(FeeTypes::IngressVaultSwap) {
				remove_fees(IngressOrEgress::IngressVaultSwap, input_asset, input_amount)
			}else {
				(input_amount, 0u128)
			};

			// Estimate swap result for a chunk, then extrapolate the result.
			// If no DCA parameter is given, swap the entire amount with 1 chunk.
			let number_of_chunks: u128 = dca_parameters.map(|dca|dca.number_of_chunks).unwrap_or(1u32).into();
			let amount_per_chunk = amount_to_swap / number_of_chunks;

			let mut fees_vec = vec![];

			if include_fee(FeeTypes::Network) {
				fees_vec.push(FeeType::NetworkFee(NetworkFeeTracker::new(
					pallet_cf_swapping::Pallet::<Runtime>::get_network_fee_for_swap(
						input_asset,
						output_asset,
						is_internal,
					),
				)));
			}

			if broker_commission > 0 {
				fees_vec.push(FeeType::BrokerFee(
					vec![Beneficiary {
						account: AccountId::new([0xbb; 32]),
						bps: broker_commission,
					}]
					.try_into()
					.expect("Beneficiary with a length of 1 must be within length bound.")
				));
			}

			// Simulate the swap
			let swap_output_per_chunk = Swapping::try_execute_without_violations(
				vec![
					Swap::new(
						Default::default(), // Swap id
						Default::default(), // Swap request id
						input_asset,
						output_asset,
						amount_per_chunk,
						None,
						fees_vec,
						Default::default(), // Execution block
					)
				],
			).map_err(|e| match e {
				BatchExecutionError::SwapLegFailed { .. } => DispatchError::Other("Swap leg failed."),
				BatchExecutionError::PriceViolation { .. } => DispatchError::Other("Price Violation: Some swaps failed due to Price Impact Limitations."),
				BatchExecutionError::DispatchError { error } => error,
			})?;

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

			let (output, egress_fee) = if include_fee(FeeTypes::Egress) {
				let egress = match ccm_data {
					Some(CcmData { gas_budget, message_length}) => {
						IngressOrEgress::EgressCcm {
							gas_budget,
							message_length: message_length as usize,
						}
					},
					None => IngressOrEgress::Egress,
				};
				remove_fees(egress, output_asset, output)
			} else {
				(output, 0u128)
			};


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
			chainflip::MinimumDepositProvider::get(asset)
		}

		fn cf_egress_dust_limit(generic_asset: Asset) -> AssetAmount {
			use pallet_cf_ingress_egress::EgressDustLimit;

			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => EgressDustLimit::<Runtime, EthereumInstance>::get(asset),
				ForeignChainAndAsset::Polkadot(asset) => EgressDustLimit::<Runtime, PolkadotInstance>::get(asset),
				ForeignChainAndAsset::Bitcoin(asset) => EgressDustLimit::<Runtime, BitcoinInstance>::get(asset),
				ForeignChainAndAsset::Arbitrum(asset) => EgressDustLimit::<Runtime, ArbitrumInstance>::get(asset),
				ForeignChainAndAsset::Solana(asset) => EgressDustLimit::<Runtime, SolanaInstance>::get(asset),
				ForeignChainAndAsset::Assethub(asset) => EgressDustLimit::<Runtime, AssethubInstance>::get(asset),
			}
		}

		fn cf_ingress_fee(generic_asset: Asset) -> Option<AssetAmount> {
			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Ethereum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::estimate_ingress_fee(asset)
					))
				},
				ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::estimate_ingress_fee(asset)),
				ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::estimate_ingress_fee(asset).into()),
				ForeignChainAndAsset::Arbitrum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Arbitrum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, ArbitrumInstance>::estimate_ingress_fee(asset)
					))
				},
				ForeignChainAndAsset::Solana(asset) => Some(SolanaChainTrackingProvider::estimate_ingress_fee(asset).into()),
				ForeignChainAndAsset::Assethub(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Assethub>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, AssethubInstance>::estimate_ingress_fee(asset)
					))
				},
			}
		}

		fn cf_egress_fee(generic_asset: Asset) -> Option<AssetAmount> {
			match generic_asset.into() {
				ForeignChainAndAsset::Ethereum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Ethereum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::estimate_egress_fee(asset)
					))
				},
				ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::estimate_egress_fee(asset)),
				ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::estimate_egress_fee(asset).into()),
				ForeignChainAndAsset::Arbitrum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Arbitrum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, ArbitrumInstance>::estimate_egress_fee(asset)
					))
				},
				ForeignChainAndAsset::Solana(asset) => Some(SolanaChainTrackingProvider::estimate_egress_fee(asset).into()),
				ForeignChainAndAsset::Assethub(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Assethub>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, AssethubInstance>::estimate_egress_fee(asset)
					))
				},
			}
		}

		fn cf_witness_safety_margin(chain: ForeignChain) -> Option<u64> {
			match chain {
				ForeignChain::Bitcoin => pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::witness_safety_margin(),
				ForeignChain::Ethereum => pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::witness_safety_margin(),
				ForeignChain::Polkadot => pallet_cf_ingress_egress::Pallet::<Runtime, PolkadotInstance>::witness_safety_margin().map(Into::into),
				ForeignChain::Arbitrum => pallet_cf_ingress_egress::Pallet::<Runtime, ArbitrumInstance>::witness_safety_margin(),
				ForeignChain::Solana => pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::witness_safety_margin(),
				ForeignChain::Assethub => pallet_cf_ingress_egress::Pallet::<Runtime, AssethubInstance>::witness_safety_margin().map(Into::into),
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
			let account_info = pallet_cf_flip::Account::<Runtime>::get(&account_id);
			BrokerInfo {
				earned_fees: Asset::all().map(|asset|
					(asset, AssetBalances::get_balance(&account_id, asset))
				).collect(),
				btc_vault_deposit_address: BrokerPrivateBtcChannels::<Runtime>::get(&account_id)
					.map(|channel| derive_btc_vault_deposit_addresses(channel).current_address()),
				affiliates: pallet_cf_swapping::AffiliateAccountDetails::<Runtime>::iter_prefix(&account_id).collect(),
				bond: account_info.bond()
			}
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

		fn cf_scheduled_swaps(base_asset: Asset, quote_asset: Asset) -> Vec<(SwapLegInfo, BlockNumber)> {
			assert_eq!(quote_asset, STABLE_ASSET, "Only USDC is supported as quote asset");
			Swapping::get_scheduled_swap_legs(base_asset)
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
				ForeignChain::Assethub => pallet_cf_ingress_egress::Pallet::<Runtime, AssethubInstance>::channel_opening_fee(),
			}
		}

		fn cf_boost_pools_depth() -> Vec<BoostPoolDepth> {

			pallet_cf_lending_pools::boost_pools_iter::<Runtime>().map(|(asset, tier, core_pool)| {

				BoostPoolDepth {
					asset,
					tier,
					available_amount: core_pool.get_available_amount()
				}

			}).collect()

		}

		fn cf_boost_pool_details(asset: Asset) -> BTreeMap<u16, BoostPoolDetails<AccountId>> {
			pallet_cf_lending_pools::get_boost_pool_details::<Runtime>(asset)
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

		fn cf_validate_refund_params(
			input_asset: Asset,
			output_asset: Asset,
			retry_duration: BlockNumber,
			max_oracle_price_slippage: Option<BasisPoints>,
		) -> Result<(), DispatchErrorWithMessage> {
			pallet_cf_swapping::Pallet::<Runtime>::validate_refund_params(
				input_asset,
				output_asset,
				retry_duration,
				max_oracle_price_slippage,
			)
			.map_err(Into::into)
		}

		fn cf_request_swap_parameter_encoding(
			broker: AccountId,
			source_asset: Asset,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			broker_commission: BasisPoints,
			extra_parameters: VaultSwapExtraParametersEncoded,
			channel_metadata: Option<CcmChannelMetadataUnchecked>,
			boost_fee: BasisPoints,
			affiliate_fees: Affiliates<AccountId>,
			dca_parameters: Option<DcaParameters>,
		) -> Result<VaultSwapDetails<String>, DispatchErrorWithMessage> {
			let source_chain = ForeignChain::from(source_asset);
			let destination_chain = ForeignChain::from(destination_asset);


			// Validate refund params
			let (retry_duration, max_oracle_price_slippage) = match &extra_parameters {
				VaultSwapExtraParametersEncoded::Bitcoin { retry_duration, max_oracle_price_slippage, .. } => {
					let max_oracle_price_slippage = match max_oracle_price_slippage {
						Some(slippage) if *slippage == u8::MAX => None,
						Some(slippage) => Some((*slippage).into()),
						None => None,
					};
					(*retry_duration, max_oracle_price_slippage)
				},
				VaultSwapExtraParametersEncoded::Ethereum(EvmVaultSwapExtraParameters { refund_parameters, .. }) => {
					refund_parameters.clone().try_map_refund_address_to_foreign_chain_address::<ChainAddressConverter>()?.into_checked(None, source_asset)?;
					(refund_parameters.retry_duration, refund_parameters.max_oracle_price_slippage)
				},
				VaultSwapExtraParametersEncoded::Arbitrum(EvmVaultSwapExtraParameters { refund_parameters, .. }) => {
					refund_parameters.clone().try_map_refund_address_to_foreign_chain_address::<ChainAddressConverter>()?.into_checked(None, source_asset)?;
					(refund_parameters.retry_duration, refund_parameters.max_oracle_price_slippage)
				},
				VaultSwapExtraParametersEncoded::Solana { refund_parameters, .. } => {
					refund_parameters.clone().try_map_refund_address_to_foreign_chain_address::<ChainAddressConverter>()?.into_checked(None, source_asset)?;
					(refund_parameters.retry_duration, refund_parameters.max_oracle_price_slippage)
				},
			};

			let checked_ccm = crate::chainflip::vault_swaps::validate_parameters(
				&broker,
				source_asset,
				&destination_address,
				destination_asset,
				&dca_parameters,
				boost_fee,
				broker_commission,
				&affiliate_fees,
				retry_duration,
				&channel_metadata,
				max_oracle_price_slippage,
			)?;

			// Conversion implicitly verifies address validity.
			frame_support::ensure!(
				ChainAddressConverter::try_from_encoded_address(destination_address.clone())
					.map_err(|_| pallet_cf_swapping::Error::<Runtime>::InvalidDestinationAddress)?
					.chain() == destination_chain
				,
				"Destination address and asset are on different chains."
			);

			// Convert boost fee.
			let boost_fee: u8 = boost_fee
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::BoostFeeTooHigh)?;

			// Validate broker fee
			if broker_commission < pallet_cf_swapping::Pallet::<Runtime>::get_minimum_vault_swap_fee_for_broker(&broker) {
				return Err(DispatchErrorWithMessage::from("Broker commission is too low"));
			}
			let _beneficiaries = pallet_cf_swapping::Pallet::<Runtime>::assemble_and_validate_broker_fees(
				broker.clone(),
				broker_commission,
				affiliate_fees.clone(),
			)?;

			// Encode swap
			match (source_chain, extra_parameters) {
				(
					ForeignChain::Bitcoin,
					VaultSwapExtraParameters::Bitcoin {
						min_output_amount,
						retry_duration,
						max_oracle_price_slippage,
					}
				) => {
					crate::chainflip::vault_swaps::bitcoin_vault_swap(
						broker,
						destination_asset,
						destination_address,
						broker_commission,
						min_output_amount,
						retry_duration,
						boost_fee,
						affiliate_fees,
						dca_parameters,
						max_oracle_price_slippage,
					)
				},
				(
					ForeignChain::Ethereum,
					VaultSwapExtraParametersEncoded::Ethereum(extra_params)
				)|
				(
					ForeignChain::Arbitrum,
					VaultSwapExtraParametersEncoded::Arbitrum(extra_params)
				) => {
					crate::chainflip::vault_swaps::evm_vault_swap(
						broker,
						source_asset,
						extra_params.input_amount,
						destination_asset,
						destination_address,
						broker_commission,
						extra_params.refund_parameters,
						boost_fee,
						affiliate_fees,
						dca_parameters,
						checked_ccm,
					)
				},
				(
					ForeignChain::Solana,
					VaultSwapExtraParameters::Solana {
						from,
						seed,
						input_amount,
						refund_parameters,
						from_token_account,
					}
				) => crate::chainflip::vault_swaps::solana_vault_swap(
					broker,
					input_amount,
					source_asset,
					destination_asset,
					destination_address,
					broker_commission,
					refund_parameters,
					checked_ccm,
					boost_fee,
					affiliate_fees,
					dca_parameters,
					from,
					seed,
					from_token_account,
				),
				_ => Err(DispatchErrorWithMessage::from(
					"Incompatible or unsupported source_asset and extra_parameters"
				)),
			}
		}

		fn cf_decode_vault_swap_parameter(
			broker: AccountId,
			vault_swap: VaultSwapDetails<String>,
		) -> Result<VaultSwapInputEncoded, DispatchErrorWithMessage> {
			match vault_swap {
				VaultSwapDetails::Bitcoin {
					nulldata_payload,
					deposit_address: _,
				} => {
					crate::chainflip::vault_swaps::decode_bitcoin_vault_swap(
						broker,
						nulldata_payload,
					)
				},
				VaultSwapDetails::Solana {
					instruction,
				} => {
					crate::chainflip::vault_swaps::decode_solana_vault_swap(
						instruction.into(),
					)
				},
				_ => Err(DispatchErrorWithMessage::from(
					"Decoding Vault Swap only supports Bitcoin and Solana"
				)),
			}
		}

		fn cf_encode_cf_parameters(
			broker: AccountId,
			source_asset: Asset,
			destination_address: EncodedAddress,
			destination_asset: Asset,
			refund_parameters: ChannelRefundParametersUncheckedEncoded,
			dca_parameters: Option<DcaParameters>,
			boost_fee: BasisPoints,
			broker_commission: BasisPoints,
			affiliate_fees: Affiliates<AccountId>,
			channel_metadata: Option<CcmChannelMetadataUnchecked>,
		) -> Result<Vec<u8>, DispatchErrorWithMessage> {
			// Validate the parameters
			let checked_ccm = crate::chainflip::vault_swaps::validate_parameters(
				&broker,
				source_asset,
				&destination_address,
				destination_asset,
				&dca_parameters,
				boost_fee,
				broker_commission,
				&affiliate_fees,
				refund_parameters.retry_duration,
				&channel_metadata,
				refund_parameters.max_oracle_price_slippage,
			)?;

			let boost_fee: u8 = boost_fee
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::BoostFeeTooHigh)?;

			let affiliate_and_fees = crate::chainflip::vault_swaps::to_affiliate_and_fees(&broker, affiliate_fees)?
				.try_into()
				.map_err(|_| "Too many affiliates.")?;

			macro_rules! build_and_encode_cf_parameters_for_chain {
				($chain:ty) => {
					build_and_encode_cf_parameters::<<$chain as cf_chains::Chain>::ChainAccount>(
						refund_parameters.try_map_address(|addr| {
							Ok::<_, DispatchErrorWithMessage>(
								ChainAddressConverter::try_from_encoded_address(addr)
									.and_then(|addr| addr.try_into().map_err(|_| ()))
									.map_err(|_| "Invalid refund address")?,
							)
						})?,
						dca_parameters,
						boost_fee,
						broker,
						broker_commission,
						affiliate_and_fees,
						checked_ccm.as_ref(),
					)
				}
			}

			Ok(match ForeignChain::from(source_asset) {
				ForeignChain::Ethereum => build_and_encode_cf_parameters_for_chain!(Ethereum),
				ForeignChain::Arbitrum => build_and_encode_cf_parameters_for_chain!(Arbitrum),
				ForeignChain::Solana => build_and_encode_cf_parameters_for_chain!(Solana),
				_ => Err(DispatchErrorWithMessage::from("Unsupported source chain for encoding cf_parameters"))?,
			})
		}

		fn cf_get_preallocated_deposit_channels(account_id: <Runtime as frame_system::Config>::AccountId, chain: ForeignChain) -> Vec<ChannelId> {

			fn preallocated_deposit_channels_for_chain<T: pallet_cf_ingress_egress::Config<I>, I: 'static>(
				account_id: &<T as frame_system::Config>::AccountId,
			) -> Vec<ChannelId>
			{
				pallet_cf_ingress_egress::PreallocatedChannels::<T, I>::get(account_id).iter()
					.map(|channel| channel.channel_id)
					.collect()
			}

			match chain {
				ForeignChain::Bitcoin => preallocated_deposit_channels_for_chain::<Runtime, BitcoinInstance>(&account_id),
				ForeignChain::Ethereum => preallocated_deposit_channels_for_chain::<Runtime, EthereumInstance>(&account_id),
				ForeignChain::Polkadot => preallocated_deposit_channels_for_chain::<Runtime, PolkadotInstance>(&account_id),
				ForeignChain::Arbitrum => preallocated_deposit_channels_for_chain::<Runtime, ArbitrumInstance>(&account_id),
				ForeignChain::Solana => preallocated_deposit_channels_for_chain::<Runtime, SolanaInstance>(&account_id),
				ForeignChain::Assethub => preallocated_deposit_channels_for_chain::<Runtime, AssethubInstance>(&account_id),
			}
		}

		fn cf_get_open_deposit_channels(account_id: Option<<Runtime as frame_system::Config>::AccountId>) -> ChainAccounts {
			fn open_deposit_channels_for_account<T: pallet_cf_ingress_egress::Config<I>, I: 'static>(
				account_id: Option<&<T as frame_system::Config>::AccountId>
			) -> Vec<EncodedAddress>
			{
				let network_environment = Environment::network_environment();
				pallet_cf_ingress_egress::DepositChannelLookup::<T, I>::iter_values()
					.filter(|channel_details| account_id.is_none() || Some(&channel_details.owner) == account_id)
					.map(|channel_details|
						channel_details.deposit_channel.address
							.into_foreign_chain_address()
							.to_encoded_address(network_environment)
					)
					.collect::<Vec<_>>()
			}

			ChainAccounts {
				chain_accounts: [
					open_deposit_channels_for_account::<Runtime, BitcoinInstance>(account_id.as_ref()),
					open_deposit_channels_for_account::<Runtime, EthereumInstance>(account_id.as_ref()),
					open_deposit_channels_for_account::<Runtime, ArbitrumInstance>(account_id.as_ref()),
				].into_iter().flatten().collect()
			}
		}

		fn cf_all_open_deposit_channels() -> Vec<OpenedDepositChannels> {
			use sp_std::collections::btree_set::BTreeSet;

			#[allow(clippy::type_complexity)]
			fn open_deposit_channels_for_chain_instance<T: pallet_cf_ingress_egress::Config<I>, I: 'static>()
				-> BTreeMap<(<T as frame_system::Config>::AccountId, ChannelActionType), Vec<EncodedAddress>>
			{
				let network_environment = Environment::network_environment();
				pallet_cf_ingress_egress::DepositChannelLookup::<T, I>::iter_values()
					.fold(BTreeMap::new(), |mut acc, channel_details| {
						acc.entry((channel_details.owner.clone(), channel_details.action.into()))
							.or_default()
							.push(
								channel_details.deposit_channel.address
								.into_foreign_chain_address()
								.to_encoded_address(network_environment)
							);
						acc
					})
			}

			let btc_chain_accounts = open_deposit_channels_for_chain_instance::<Runtime, BitcoinInstance>();
			let eth_chain_accounts = open_deposit_channels_for_chain_instance::<Runtime, EthereumInstance>();
			let arb_chain_accounts = open_deposit_channels_for_chain_instance::<Runtime, ArbitrumInstance>();
			let accounts = btc_chain_accounts.keys()
				.chain(eth_chain_accounts.keys())
				.chain(arb_chain_accounts.keys())
				.cloned().collect::<BTreeSet<_>>();

			accounts.into_iter().map(|key| {
				let (account_id, channel_action_type) = key.clone();
				(account_id, channel_action_type, ChainAccounts {
					chain_accounts: [
						btc_chain_accounts.get(&key).cloned().unwrap_or_default(),
						eth_chain_accounts.get(&key).cloned().unwrap_or_default(),
						arb_chain_accounts.get(&key).cloned().unwrap_or_default(),
					].into_iter().flatten().collect()
				})
			}).collect()
		}

		fn cf_transaction_screening_events() -> crate::runtime_apis::TransactionScreeningEvents {
			use crate::runtime_apis::BrokerRejectionEventFor;
			fn extract_screening_events<
				T: pallet_cf_ingress_egress::Config<I, AccountId = <Runtime as frame_system::Config>::AccountId>,
				I: 'static
			>(
				event: pallet_cf_ingress_egress::Event::<T, I>,
			) -> Vec<BrokerRejectionEventFor<T::TargetChain>> {
				use cf_chains::DepositDetailsToTransactionInId;
				match event {
					pallet_cf_ingress_egress::Event::TransactionRejectionRequestExpired { account_id, tx_id } =>
						vec![TransactionScreeningEvent::TransactionRejectionRequestExpired { account_id, tx_id }],
					pallet_cf_ingress_egress::Event::TransactionRejectionRequestReceived { account_id, tx_id, expires_at: _ } =>
						vec![TransactionScreeningEvent::TransactionRejectionRequestReceived { account_id, tx_id }],
					pallet_cf_ingress_egress::Event::TransactionRejectedByBroker { broadcast_id, tx_id } => tx_id
						.deposit_ids()
						.into_iter()
						.flat_map(IntoIterator::into_iter)
						.map(|tx_id|
							TransactionScreeningEvent::TransactionRejectedByBroker { refund_broadcast_id: broadcast_id, tx_id }
						)
						.collect(),
					_ => Default::default(),
				}
			}

			let mut btc_events: Vec<BrokerRejectionEventFor<cf_chains::Bitcoin>> = Default::default();
			let mut eth_events: Vec<BrokerRejectionEventFor<cf_chains::Ethereum>> = Default::default();
			let mut arb_events: Vec<BrokerRejectionEventFor<cf_chains::Arbitrum>> = Default::default();
			for event_record in System::read_events_no_consensus() {
				match event_record.event {
					RuntimeEvent::BitcoinIngressEgress(event) => btc_events.extend(extract_screening_events::<Runtime, BitcoinInstance>(event)),
					RuntimeEvent::EthereumIngressEgress(event) => eth_events.extend(extract_screening_events::<Runtime, EthereumInstance>(event)),
					RuntimeEvent::ArbitrumIngressEgress(event) => arb_events.extend(extract_screening_events::<Runtime, ArbitrumInstance>(event)),
					_ => {},
				}
			}

			TransactionScreeningEvents {
				btc_events,
				eth_events,
				arb_events,
			}
		}

		fn cf_affiliate_details(
			broker: AccountId,
			affiliate: Option<AccountId>,
		) -> Vec<(AccountId, AffiliateDetails)>{
			if let Some(affiliate) = affiliate {
				pallet_cf_swapping::AffiliateAccountDetails::<Runtime>::get(&broker, &affiliate)
					.map(|details| (affiliate, details))
					.into_iter()
					.collect()
			} else {
				pallet_cf_swapping::AffiliateAccountDetails::<Runtime>::iter_prefix(&broker).collect()
			}
		}

		fn cf_vault_addresses() -> VaultAddresses {
			VaultAddresses {
				ethereum: EncodedAddress::Eth(Environment::eth_vault_address().into()),
				arbitrum: EncodedAddress::Arb(Environment::arb_vault_address().into()),
				bitcoin: BrokerPrivateBtcChannels::<Runtime>::iter()
					.flat_map(|(account_id, channel_id)| {
						let BitcoinPrivateBrokerDepositAddresses { previous, current } = derive_btc_vault_deposit_addresses(channel_id)
							.with_encoded_addresses();
						previous.into_iter().chain(core::iter::once(current))
							.map(move |address| (account_id.clone(), address))
					})
					.collect(),
			}
		}

		fn cf_get_trading_strategies(lp_id: Option<AccountId>,) -> Vec<TradingStrategyInfo<AssetAmount>> {

			type Strategies = pallet_cf_trading_strategy::Strategies::<Runtime>;
			type Strategy = pallet_cf_trading_strategy::TradingStrategy;

			fn to_strategy_info(lp_id: AccountId, strategy_id: AccountId, strategy: Strategy) -> TradingStrategyInfo<AssetAmount> {

				LiquidityPools::sweep(&strategy_id).unwrap();

				let free_balances = AssetBalances::free_balances(&strategy_id);
				let open_order_balances = LiquidityPools::open_order_balances(&strategy_id);

				let total_balances = free_balances.saturating_add(open_order_balances);

				let supported_assets = strategy.supported_assets();
				let supported_asset_balances = total_balances.iter()
					.filter(|(asset, _amount)| supported_assets.contains(asset))
					.map(|(asset, amount)| (asset, *amount));

				TradingStrategyInfo {
					lp_id,
					strategy_id,
					strategy,
					balance: supported_asset_balances.collect(),
				}

			}

			if let Some(lp_id) = &lp_id {
				Strategies::iter_prefix(lp_id).map(|(strategy_id, strategy)| to_strategy_info(lp_id.clone(), strategy_id, strategy)).collect()
			} else {
				Strategies::iter().map(|(lp_id, strategy_id, strategy)| to_strategy_info(lp_id, strategy_id, strategy)).collect()
			}

		}

		fn cf_trading_strategy_limits() -> TradingStrategyLimits{
			TradingStrategyLimits{
				minimum_deployment_amount: AssetMap::from_iter(pallet_cf_trading_strategy::MinimumDeploymentAmountForStrategy::<Runtime>::get().into_iter()
					.map(|(asset, balance)| (asset, Some(balance)))),
				minimum_added_funds_amount: AssetMap::from_iter(pallet_cf_trading_strategy::MinimumAddedFundsToStrategy::<Runtime>::get().into_iter()
					.map(|(asset, balance)| (asset, Some(balance)))),
			}
		}

		fn cf_network_fees() -> NetworkFees{
			let regular_network_fee = pallet_cf_swapping::NetworkFee::<Runtime>::get();
			let internal_swap_network_fee = pallet_cf_swapping::InternalSwapNetworkFee::<Runtime>::get();
			NetworkFees {
				regular_network_fee: NetworkFeeDetails{
					rates: AssetMap::from_fn(|asset|{
						pallet_cf_swapping::NetworkFeeForAsset::<Runtime>::get(asset).unwrap_or(regular_network_fee.rate)
					}),
					standard_rate_and_minimum: regular_network_fee,
				},
				internal_swap_network_fee: NetworkFeeDetails{
					rates: AssetMap::from_fn(|asset|{
						pallet_cf_swapping::InternalSwapNetworkFeeForAsset::<Runtime>::get(asset).unwrap_or(internal_swap_network_fee.rate)
					}),
					standard_rate_and_minimum: internal_swap_network_fee,
				},
			}
		}

		fn cf_oracle_prices(base_and_quote_asset: Option<(PriceAsset, PriceAsset)>,) -> Vec<OraclePrice> {
			if let Some(state) = pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime, ()>::get() {
				get_latest_oracle_prices(&state.0, base_and_quote_asset)
			} else {
				vec![]
			}
		}

		fn cf_evm_calldata(
			caller: EthereumAddress,
			call: EthereumSCApi<u128>,
		) -> Result<EvmCallDetails, DispatchErrorWithMessage> {
			use chainflip::ethereum_sc_calls::DelegationApi;
			let caller_id = EthereumAccount(caller).into_account_id();
			let required_deposit = match call {
				EthereumSCApi::Delegation { call: DelegationApi::Delegate { increase: DelegationAmount::Some(ref increase), .. } } => {
					pallet_cf_validator::MaxDelegationBid::<Runtime>::get(&caller_id).unwrap_or_default()
						.saturating_add(*increase)
						.saturating_sub(pallet_cf_flip::Pallet::<Runtime>::balance(&caller_id))
				},
				_ => 0,
			};
			Ok(EvmCallDetails {
				calldata: if required_deposit > 0 {
					DepositToSCGatewayAndCall::new(required_deposit, call.encode()).abi_encoded_payload()
				} else {
					SCCall::new(call.encode()).abi_encoded_payload()
				},
				value: U256::zero(),
				to: Environment::eth_sc_utils_address(),
				source_token_address: if required_deposit > 0 {
					Some(
						Environment::supported_eth_assets(cf_primitives::chains::assets::eth::Asset::Flip)
							.ok_or(DispatchErrorWithMessage::from(
								"flip token address not found on the state chain: {e}",
							))?
					)
				} else {
					None
				},
			})
		}
	}


	impl monitoring_apis::MonitoringRuntimeApi<Block> for Runtime {

		fn cf_authorities() -> AuthoritiesInfo {
			let mut authorities = pallet_cf_validator::CurrentAuthorities::<Runtime>::get();
			let mut result = AuthoritiesInfo {
				authorities: authorities.len() as u32,
				online_authorities: 0,
				backups: 0,
				online_backups: 0,
			};
			authorities.retain(HeartbeatQualification::<Runtime>::is_qualified);
			result.online_authorities = authorities.len() as u32;
			result
		}

		fn cf_external_chains_block_height() -> ExternalChainsBlockHeight {
			// safe to unwrap these value as stated on the storage item doc
			let btc = pallet_cf_chain_tracking::CurrentChainState::<Runtime, BitcoinInstance>::get().unwrap();
			let eth = pallet_cf_chain_tracking::CurrentChainState::<Runtime, EthereumInstance>::get().unwrap();
			let dot = pallet_cf_chain_tracking::CurrentChainState::<Runtime, PolkadotInstance>::get().unwrap();
			let arb = pallet_cf_chain_tracking::CurrentChainState::<Runtime, ArbitrumInstance>::get().unwrap();
			let sol = SolanaChainTrackingProvider::get_block_height();
			let hub = pallet_cf_chain_tracking::CurrentChainState::<Runtime, AssethubInstance>::get().unwrap();

			ExternalChainsBlockHeight {
				bitcoin: btc.block_height,
				ethereum: eth.block_height,
				polkadot: dot.block_height.into(),
				solana: sol,
				arbitrum: arb.block_height,
				assethub: hub.block_height.into(),
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
				epoch_duration: Validator::epoch_duration(),
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
				assethub: pallet_cf_broadcast::PendingBroadcasts::<Runtime, AssethubInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
			}
		}
		fn cf_pending_tss_ceremonies_count() -> PendingTssCeremonies {
			PendingTssCeremonies {
				evm: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, EvmInstance>::iter().collect::<Vec<_>>().len() as u32,
				bitcoin: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, BitcoinInstance>::iter().collect::<Vec<_>>().len() as u32,
				polkadot: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, PolkadotCryptoInstance>::iter().collect::<Vec<_>>().len() as u32,
				solana: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, SolanaInstance>::iter().collect::<Vec<_>>().len() as u32,
			}
		}
		fn cf_pending_swaps_count() -> u32 {
			pallet_cf_swapping::ScheduledSwaps::<Runtime>::get().len() as u32
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
				assethub: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, AssethubInstance>, AssethubInstance>(),
			}
		}
		fn cf_fee_imbalance() -> FeeImbalance<AssetAmount> {
			FeeImbalance {
				ethereum: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Ethereum.gas_asset()),
				polkadot: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Polkadot.gas_asset()),
				arbitrum: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Arbitrum.gas_asset()),
				bitcoin: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Bitcoin.gas_asset()),
				solana: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Solana.gas_asset()),
				assethub: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Assethub.gas_asset()),
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
				},
				assethub: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, AssethubInstance>::get().map(|val| val.1),
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
			MonitoringDataV2 {
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

	#[test]
	fn account_id_size() {
		assert!(
			core::mem::size_of::<AccountId>() <= 32,
			r"Our use of blake2_256 to derive account ids requires that account ids are no larger than 32 bytes"
		);
	}

	// Introduced from polkadot
	#[test]
	fn call_size() {
		let call_size = core::mem::size_of::<RuntimeCall>();
		assert!(
			call_size <= CALL_ENUM_MAX_SIZE,
			r"
			Polkadot suggests a 230 byte limit for the size of the Call type. We use {CALL_ENUM_MAX_SIZE} but this runtime's call size
			is {call_size}. If this test fails then you have just added a call variant that exceed the limit.

			Congratulations!

			Maybe consider boxing some calls to reduce their size. Otherwise, increasing the CALL_ENUM_MAX_SIZE is
			acceptable (within reason). The issue is that the enum always uses max(enum_size) of memory, even if your
			are using a smaller variant. Note this is irrelevant from a SCALE-encoding POV, it only affects the size of
			the enum on the stack.
			Context:
			  - https://github.com/paritytech/substrate/pull/9418
			  - https://rust-lang.github.io/rust-clippy/master/#large_enum_variant
			  - https://fasterthanli.me/articles/peeking-inside-a-rust-enum
			"
		);
	}
}
