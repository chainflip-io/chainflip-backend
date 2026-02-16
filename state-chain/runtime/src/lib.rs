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

#![feature(trait_alias)]
#![feature(step_trait)]
#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "512"]
pub mod chainflip;
pub mod configs;
pub mod constants;
pub mod migrations;
pub mod runtime_apis;
pub mod safe_mode;
#[cfg(all(feature = "std", feature = "mocks"))]
pub mod test_runner;
mod weights;

pub use configs::*;

use crate::runtime_apis::impl_api::RUNTIME_API_VERSIONS;
use codec::{Decode, Encode};
// use constants::common::*;
use frame_support::sp_runtime::{
	create_runtime_str,
	traits::{BlakeTwo256, IdentifyAccount, Verify},
	MultiSignature,
};
use pallet_session::historical as session_historical;
use sp_runtime::generic;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
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

	sp_runtime::impl_opaque_keys! {
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
	spec_version: 2_01_00,
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
	#[runtime::pallet_index(56)]
	pub type EthereumElections = pallet_cf_elections<Instance1>;
	#[runtime::pallet_index(57)]
	pub type ArbitrumElections = pallet_cf_elections<Instance4>;

	//  TODO: Not adding chaintracking nor any elections for now
	#[runtime::pallet_index(58)]
	pub type TronVault = pallet_cf_vaults<Instance7>;
	#[runtime::pallet_index(59)]
	pub type TronBroadcaster = pallet_cf_broadcast<Instance7>;
	#[runtime::pallet_index(60)]
	pub type TronIngressEgress = pallet_cf_ingress_egress<Instance7>;
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
	GenericElections,
	SolanaElections,
	BitcoinElections,
	EthereumElections,
	ArbitrumElections,
	// Vaults
	EthereumVault,
	PolkadotVault,
	BitcoinVault,
	ArbitrumVault,
	SolanaVault,
	AssethubVault,
	TronVault,
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
	TronBroadcaster,
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
	TronIngressEgress,
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
	migrations::sol_election_settings::Migration,
	PalletMigrations,
	migrations::housekeeping::Migration,
	MigrationsForV2_1,
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
	// No TronChainTracking pallet for now
	pallet_cf_vaults::migrations::PalletMigration<Runtime, EthereumInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, PolkadotInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, ArbitrumInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, SolanaInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, AssethubInstance>,
	pallet_cf_vaults::migrations::PalletMigration<Runtime, TronInstance>,
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
	pallet_cf_broadcast::migrations::PalletMigration<Runtime, TronInstance>,
	pallet_cf_swapping::migrations::PalletMigration<Runtime>,
	pallet_cf_lp::migrations::PalletMigration<Runtime>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, EthereumInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, PolkadotInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, BitcoinInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, ArbitrumInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, SolanaInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, AssethubInstance>,
	pallet_cf_ingress_egress::migrations::PalletMigration<Runtime, TronInstance>,
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

use frame_support::migrations::VersionedMigration;
#[allow(clippy::allow_attributes)]
#[allow(unused_macros)]
macro_rules! instanced_migrations {
	(
		module: $module:ident,
		migration: $migration:ident,
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
					$migration<Runtime, $include>,
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

// Add version-specific migrations here.
use pallet_cf_ingress_egress::migrations::channel_status_migration::Migration as ChannelStatusMigration;
type MigrationsForV2_1 = (
	migrations::ethereum_elections::Migration,
	migrations::arbitrum_elections::Migration,
	migrations::safe_mode::SafeModeMigration,
	instanced_migrations! {
		module: pallet_cf_ingress_egress,
		migration: ChannelStatusMigration,
		from: 29,
		to: 30,
		include_instances: [EthereumInstance, ArbitrumInstance],
		exclude_instances: [PolkadotInstance, BitcoinInstance, SolanaInstance, AssethubInstance],
	},
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
