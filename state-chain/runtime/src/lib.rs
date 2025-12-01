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

#![feature(step_trait)]
#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "512"]
pub mod chainflip;
pub mod constants;
pub mod migrations;
pub mod runtime_apis;
pub mod safe_mode;
#[cfg(all(feature = "std", feature = "mocks"))]
pub mod test_runner;
mod weights;

use crate::{
	chainflip::{
		solana_elections::{
			SolanaChainTrackingProvider, SolanaEgressWitnessingTrigger, SolanaIngress,
			SolanaNonceTrackingTrigger,
		},
		EvmLimit,
	},
	runtime_apis::impl_api::RUNTIME_API_VERSIONS,
};
pub use cf_chains::instances::{
	ArbitrumInstance, AssethubInstance, BitcoinInstance, EthereumInstance, EvmInstance,
	PolkadotCryptoInstance, PolkadotInstance, SolanaInstance,
};
use cf_chains::{
	arb::api::ArbitrumApi,
	btc::{BitcoinCrypto, BitcoinRetryPolicy},
	dot::{self, PolkadotCrypto},
	eth::{self, api::EthereumApi, Address as EthereumAddress, Ethereum},
	evm::EvmCrypto,
	hub,
	instances::ChainInstanceAlias,
	sol::SolanaCrypto,
	Arbitrum, Assethub, Bitcoin, DefaultRetryPolicy, Polkadot, Solana,
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
	DummyEgressSuccessWitnesser, DummyIngressSource, EpochKey, MinimumDeposit, NoLimit,
};
use chainflip::{
	epoch_transition::ChainflipEpochTransitions, multi_vault_activator::MultiVaultActivator,
	BroadcastReadyProvider, BtcEnvironment, CfCcmAdditionalDataHandler, ChainAddressConverter,
	ChainlinkOracle, DotEnvironment, EvmEnvironment, HubEnvironment, MinimumDepositProvider,
	SolEnvironment, SolanaLimit, TokenholderGovernanceBroadcaster,
};
use codec::{Decode, Encode};
use constants::common::*;
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
	migrations::VersionedMigration,
	sp_runtime::{
		create_runtime_str, generic, impl_opaque_keys,
		traits::{BlakeTwo256, ConvertInto, IdentifyAccount, One, OpaqueKeys, Verify},
		MultiSignature,
	},
};
pub use frame_system::Call as SystemCall;
use frame_system::{offchain::SendTransactionTypes, pallet_prelude::BlockNumberFor};
use pallet_cf_flip::{Bonder, FlipIssuance, FlipSlasher};
use pallet_cf_reputation::{ExclusionList, HeartbeatQualification, ReputationPointsQualification};
use pallet_cf_trading_strategy::TradingStrategyDeregistrationCheck;
pub use pallet_cf_validator::SetSizeParameters;
use pallet_cf_validator::{DelegatedRewardsDistribution, DelegationSlasher};
use pallet_grandpa::AuthorityId as GrandpaId;
use pallet_session::historical as session_historical;
pub use pallet_timestamp::Call as TimestampCall;
pub use pallet_transaction_payment::ChargeTransactionPayment;
use pallet_transaction_payment::{ConstFeeMultiplier, Multiplier};
use safe_mode::{RuntimeSafeMode, WitnesserCallPermission};
use scale_info::prelude::string::String;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::crypto::KeyTypeId;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
pub use sp_runtime::{Perbill, Permill};
use sp_std::prelude::*;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

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
	spec_version: 2_00_03,
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

impl pallet_cf_environment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
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
	type TargetChain = Polkadot;
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
	type TargetChain = Bitcoin;
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
	type TargetChain = Arbitrum;
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
	type TargetChain = Solana;
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
	type TargetChain = Assethub;
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
	type PriceApi = ChainlinkOracle;
	type LpRegistrationApi = LiquidityProvider;
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
	MigrationsForV2_0,
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
	pallet_cf_elections::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_elections::migrations::PalletMigration<Runtime, ()>,
);

pub struct NoopMigration;
impl frame_support::traits::UncheckedOnRuntimeUpgrade for NoopMigration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("🤷 Noop migration");
		Default::default()
	}
}

#[allow(clippy::allow_attributes)]
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

type MigrationsForV2_0 = (
	VersionedMigration<
		20,
		21,
		migrations::safe_mode::SafeModeMigration,
		pallet_cf_environment::Pallet<Runtime>,
		<Runtime as frame_system::Config>::DbWeight,
	>,
	instanced_migrations!(
		module: pallet_cf_ingress_egress,
		migration: migrations::ingress_delay::IngressEgressDelay,
		from: 28,
		to: 29,
		include_instances: [
			SolanaInstance,
		],
		exclude_instances: [
			EthereumInstance,
			PolkadotInstance,
			BitcoinInstance,
			ArbitrumInstance,
			AssethubInstance
		]
	),
);

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
