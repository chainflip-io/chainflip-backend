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

use super::*;
use cf_chains::instances::{
	ArbitrumInstance, AssethubInstance, BitcoinCryptoInstance, BitcoinInstance, BscInstance,
	EthereumInstance, EvmInstance, PolkadotCryptoInstance, PolkadotInstance, SolanaCryptoInstance,
	SolanaInstance, TronInstance,
};

#[derive(
	Encode, Decode, TypeInfo, Clone, PartialEq, Eq, frame_support::pallet_prelude::RuntimeDebug,
)]
pub struct EmissionsSafeMode {
	pub emissions_sync_enabled: bool,
}

impl Default for EmissionsSafeMode {
	fn default() -> Self {
		Self { emissions_sync_enabled: true }
	}
}

// The runtime safe mode before the emissions pallet was removed. Also the on-chain storage format
// of `pallet_cf_environment::RuntimeSafeMode` prior to the v2.4 runtime.
#[derive(
	Encode,
	Decode,
	TypeInfo,
	Default,
	Clone,
	PartialEq,
	Eq,
	frame_support::pallet_prelude::RuntimeDebug,
)]
pub struct RuntimeSafeMode {
	pub emissions: EmissionsSafeMode,
	pub funding: pallet_cf_funding::PalletSafeMode,
	pub swapping: pallet_cf_swapping::PalletSafeMode,
	pub liquidity_provider: pallet_cf_lp::PalletSafeMode,
	pub validator: pallet_cf_validator::PalletSafeMode,
	pub pools: pallet_cf_pools::PalletSafeMode,
	pub trading_strategies: pallet_cf_trading_strategy::PalletSafeMode,
	pub lending_pools: pallet_cf_lending_pools::PalletSafeMode,
	pub reputation: pallet_cf_reputation::PalletSafeMode,
	pub asset_balances: pallet_cf_asset_balances::PalletSafeMode,
	pub threshold_signature_evm: pallet_cf_threshold_signature::PalletSafeMode<EvmInstance>,
	pub threshold_signature_bitcoin:
		pallet_cf_threshold_signature::PalletSafeMode<BitcoinCryptoInstance>,
	pub threshold_signature_polkadot:
		pallet_cf_threshold_signature::PalletSafeMode<PolkadotCryptoInstance>,
	pub threshold_signature_solana:
		pallet_cf_threshold_signature::PalletSafeMode<SolanaCryptoInstance>,
	pub broadcast_ethereum: pallet_cf_broadcast::PalletSafeMode<EthereumInstance>,
	pub broadcast_bitcoin: pallet_cf_broadcast::PalletSafeMode<BitcoinInstance>,
	pub broadcast_polkadot: pallet_cf_broadcast::PalletSafeMode<PolkadotInstance>,
	pub broadcast_arbitrum: pallet_cf_broadcast::PalletSafeMode<ArbitrumInstance>,
	pub broadcast_solana: pallet_cf_broadcast::PalletSafeMode<SolanaInstance>,
	pub broadcast_assethub: pallet_cf_broadcast::PalletSafeMode<AssethubInstance>,
	pub broadcast_tron: pallet_cf_broadcast::PalletSafeMode<TronInstance>,
	pub broadcast_bsc: pallet_cf_broadcast::PalletSafeMode<BscInstance>,
	pub witnesser: pallet_cf_witnesser::PalletSafeMode<crate::safe_mode::WitnesserCallPermission>,
	pub ingress_egress_ethereum: pallet_cf_ingress_egress::PalletSafeMode<EthereumInstance>,
	pub ingress_egress_bitcoin: pallet_cf_ingress_egress::PalletSafeMode<BitcoinInstance>,
	pub ingress_egress_polkadot: pallet_cf_ingress_egress::PalletSafeMode<PolkadotInstance>,
	pub ingress_egress_arbitrum: pallet_cf_ingress_egress::PalletSafeMode<ArbitrumInstance>,
	pub ingress_egress_solana: pallet_cf_ingress_egress::PalletSafeMode<SolanaInstance>,
	pub ingress_egress_assethub: pallet_cf_ingress_egress::PalletSafeMode<AssethubInstance>,
	pub ingress_egress_tron: pallet_cf_ingress_egress::PalletSafeMode<TronInstance>,
	pub ingress_egress_bsc: pallet_cf_ingress_egress::PalletSafeMode<BscInstance>,
	pub elections_generic:
		crate::chainflip::witnessing::generic_elections::GenericElectionsSafeMode,
	pub ethereum_elections:
		crate::chainflip::witnessing::ethereum_elections::EthereumElectionsSafeMode,
	pub arbitrum_elections:
		crate::chainflip::witnessing::arbitrum_elections::ArbitrumElectionsSafeMode,
	pub tron_elections: crate::chainflip::witnessing::tron_elections::TronElectionsSafeMode,
	pub bsc_elections: crate::chainflip::witnessing::bsc_elections::BscElectionsSafeMode,
}

impl From<RuntimeSafeMode> for crate::safe_mode::RuntimeSafeMode {
	fn from(old: RuntimeSafeMode) -> Self {
		Self {
			funding: old.funding,
			swapping: old.swapping,
			liquidity_provider: old.liquidity_provider,
			validator: old.validator,
			pools: old.pools,
			trading_strategies: old.trading_strategies,
			lending_pools: old.lending_pools,
			reputation: old.reputation,
			asset_balances: old.asset_balances,
			threshold_signature_evm: old.threshold_signature_evm,
			threshold_signature_bitcoin: old.threshold_signature_bitcoin,
			threshold_signature_polkadot: old.threshold_signature_polkadot,
			threshold_signature_solana: old.threshold_signature_solana,
			broadcast_ethereum: old.broadcast_ethereum,
			broadcast_bitcoin: old.broadcast_bitcoin,
			broadcast_polkadot: old.broadcast_polkadot,
			broadcast_arbitrum: old.broadcast_arbitrum,
			broadcast_solana: old.broadcast_solana,
			broadcast_assethub: old.broadcast_assethub,
			broadcast_tron: old.broadcast_tron,
			broadcast_bsc: old.broadcast_bsc,
			witnesser: old.witnesser,
			ingress_egress_ethereum: old.ingress_egress_ethereum,
			ingress_egress_bitcoin: old.ingress_egress_bitcoin,
			ingress_egress_polkadot: old.ingress_egress_polkadot,
			ingress_egress_arbitrum: old.ingress_egress_arbitrum,
			ingress_egress_solana: old.ingress_egress_solana,
			ingress_egress_assethub: old.ingress_egress_assethub,
			ingress_egress_tron: old.ingress_egress_tron,
			ingress_egress_bsc: old.ingress_egress_bsc,
			elections_generic: old.elections_generic,
			ethereum_elections: old.ethereum_elections,
			arbitrum_elections: old.arbitrum_elections,
			tron_elections: old.tron_elections,
			bsc_elections: old.bsc_elections,
		}
	}
}

#[derive(Encode, Decode, TypeInfo)]
pub enum RuntimeApiAccountInfo {
	Unregistered,
	Broker(Box<super::BrokerInfo<<Bitcoin as Chain>::ChainAccount>>),
	LiquidityProvider(Box<super::LiquidityProviderInfo>),
	Validator(Box<ValidatorInfo>),
	Operator(Box<super::OperatorInfo<FlipBalance>>),
}

impl From<RuntimeApiAccountInfo> for super::RuntimeApiAccountInfo {
	fn from(old: RuntimeApiAccountInfo) -> Self {
		match old {
			RuntimeApiAccountInfo::Unregistered => Self::Unregistered,
			RuntimeApiAccountInfo::Broker(info) => Self::Broker(info),
			RuntimeApiAccountInfo::LiquidityProvider(info) => Self::LiquidityProvider(info),
			RuntimeApiAccountInfo::Validator(info) => Self::Validator(Box::new((*info).into())),
			RuntimeApiAccountInfo::Operator(info) => Self::Operator(info),
		}
	}
}

#[derive(Encode, Decode, TypeInfo)]
pub struct RuntimeApiAccountInfoWrapper {
	pub common_items: RpcAccountInfoCommonItems<FlipBalance>,
	pub role: RuntimeApiAccountInfo,
}

impl From<RuntimeApiAccountInfoWrapper> for super::RuntimeApiAccountInfoWrapper {
	fn from(value: RuntimeApiAccountInfoWrapper) -> Self {
		Self { common_items: value.common_items, role: value.role.into() }
	}
}

// ValidatorInfo before `apy_bp` was removed along with emissions.
#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Serialize, Deserialize)]
pub struct ValidatorInfo {
	pub balance: AssetAmount,
	pub bond: AssetAmount,
	pub last_heartbeat: u32,
	pub reputation_points: i32,
	pub keyholder_epochs: Vec<EpochIndex>,
	pub is_current_authority: bool,
	#[deprecated]
	pub is_current_backup: bool,
	pub is_qualified: bool,
	pub is_online: bool,
	pub is_bidding: bool,
	pub bound_redeem_address: Option<EvmAddress>,
	pub apy_bp: Option<u32>,
	pub restricted_balances: BTreeMap<EvmAddress, AssetAmount>,
	pub estimated_redeemable_balance: AssetAmount,
	pub operator: Option<AccountId32>,
	pub bid: AssetAmount,
	pub max_bid: Option<AssetAmount>,
}

impl From<ValidatorInfo> for super::ValidatorInfo {
	fn from(old: ValidatorInfo) -> Self {
		Self {
			balance: old.balance,
			bond: old.bond,
			last_heartbeat: old.last_heartbeat,
			reputation_points: old.reputation_points,
			keyholder_epochs: old.keyholder_epochs,
			is_current_authority: old.is_current_authority,
			#[expect(deprecated)]
			is_current_backup: old.is_current_backup,
			is_qualified: old.is_qualified,
			is_online: old.is_online,
			is_bidding: old.is_bidding,
			bound_redeem_address: old.bound_redeem_address,
			restricted_balances: old.restricted_balances,
			estimated_redeemable_balance: old.estimated_redeemable_balance,
			operator: old.operator,
			bid: old.bid,
			max_bid: old.max_bid,
		}
	}
}
