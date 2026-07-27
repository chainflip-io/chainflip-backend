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
	ArbitrumInstance, AssethubInstance, BitcoinCryptoInstance, BitcoinInstance, EthereumInstance,
	EvmInstance, PolkadotCryptoInstance, PolkadotInstance, SolanaCryptoInstance, SolanaInstance,
};
use codec::{DecodeWithMemTracking, MaxEncodedLen};
use pallet_cf_lending_pools::LendingPoolConfiguration;

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, PartialOrd, Ord,
)]
pub enum EncodedAddress {
	Eth([u8; 20]),
	Dot([u8; 32]),
	Btc(Vec<u8>),
	Arb([u8; 20]),
	Sol([u8; sol_prim::consts::SOLANA_ADDRESS_LEN]),
	Hub([u8; 32]),
	Tron([u8; 20]),
}
impl From<EncodedAddress> for super::EncodedAddress {
	fn from(value: EncodedAddress) -> Self {
		match value {
			EncodedAddress::Eth(address) => Self::Eth(address),
			EncodedAddress::Dot(address) => Self::Dot(address),
			EncodedAddress::Btc(address) => Self::Btc(address),
			EncodedAddress::Arb(address) => Self::Arb(address),
			EncodedAddress::Sol(address) => Self::Sol(address),
			EncodedAddress::Hub(address) => Self::Hub(address),
			EncodedAddress::Tron(address) => Self::Tron(address),
		}
	}
}

#[derive(
	Copy,
	Clone,
	Debug,
	PartialEq,
	Eq,
	Hash,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Default,
)]
pub struct AssetMap<T> {
	pub eth: EthAssetMap<T>,
	pub dot: cf_primitives::chains::assets::dot::AssetMap<T>,
	pub btc: cf_primitives::chains::assets::btc::AssetMap<T>,
	pub arb: cf_primitives::chains::assets::arb::AssetMap<T>,
	pub sol: cf_primitives::chains::assets::sol::AssetMap<T>,
	pub hub: cf_primitives::chains::assets::hub::AssetMap<T>,
	pub tron: cf_primitives::chains::assets::tron::AssetMap<T>,
}

impl<T: Default> From<AssetMap<T>> for cf_primitives::chains::assets::any::AssetMap<T> {
	fn from(value: AssetMap<T>) -> Self {
		Self {
			eth: value.eth.into(),
			dot: value.dot,
			btc: value.btc,
			arb: value.arb,
			sol: value.sol,
			hub: value.hub,
			tron: value.tron,
			bsc: Default::default(),
		}
	}
}

// cbBTC was added to the eth asset set in the 2.3 (v20300) cycle. It must not leak into this
// historical shape, so the eth map is frozen here to its pre-cbBTC fields.
#[derive(
	Copy,
	Clone,
	Debug,
	PartialEq,
	Eq,
	Hash,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	Default,
)]
pub struct EthAssetMap<T> {
	pub eth: T,
	pub flip: T,
	pub usdc: T,
	pub usdt: T,
	pub wbtc: T,
}

impl<T: Default> From<EthAssetMap<T>> for cf_primitives::chains::assets::eth::AssetMap<T> {
	fn from(value: EthAssetMap<T>) -> Self {
		Self {
			eth: value.eth,
			flip: value.flip,
			usdc: value.usdc,
			usdt: value.usdt,
			wbtc: value.wbtc,
			cbbtc: Default::default(),
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Clone)]
pub struct NetworkFeeDetails {
	pub standard_rate_and_minimum: FeeRateAndMinimum,
	pub rates: AssetMap<Permill>,
}

impl From<NetworkFeeDetails> for super::NetworkFeeDetails {
	fn from(value: NetworkFeeDetails) -> Self {
		Self {
			standard_rate_and_minimum: value.standard_rate_and_minimum,
			rates: value.rates.into(),
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Clone)]
pub struct NetworkFees {
	pub regular_network_fee: NetworkFeeDetails,
	pub internal_swap_network_fee: NetworkFeeDetails,
}

impl From<NetworkFees> for super::NetworkFees {
	fn from(value: NetworkFees) -> Self {
		Self {
			regular_network_fee: value.regular_network_fee.into(),
			internal_swap_network_fee: value.internal_swap_network_fee.into(),
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Clone)]
pub struct TradingStrategyLimits {
	pub minimum_deployment_amount: AssetMap<Option<AssetAmount>>,
	pub minimum_added_funds_amount: AssetMap<Option<AssetAmount>>,
}

impl From<TradingStrategyLimits> for super::TradingStrategyLimits {
	fn from(value: TradingStrategyLimits) -> Self {
		Self {
			minimum_deployment_amount: value.minimum_deployment_amount.into(),
			minimum_added_funds_amount: value.minimum_added_funds_amount.into(),
		}
	}
}

#[derive(Encode, Decode, Eq, PartialEq, TypeInfo, Default)]
pub struct LiquidityProviderInfo {
	pub refund_addresses: Vec<(ForeignChain, Option<ForeignChainAddress>)>,
	pub balances: Vec<(Asset, AssetAmount)>,
	pub earned_fees: AssetMap<AssetAmount>,
	pub boost_balances: AssetMap<Vec<LiquidityProviderBoostPoolInfo>>,
	pub lending_positions: Vec<LendingPosition<AssetAmount>>,
	pub collateral_balances: Vec<(Asset, AssetAmount)>,
}

impl From<LiquidityProviderInfo> for super::LiquidityProviderInfo {
	fn from(value: LiquidityProviderInfo) -> Self {
		Self {
			refund_addresses: value.refund_addresses,
			balances: value.balances,
			earned_fees: value.earned_fees.into(),
			boost_balances: value.boost_balances.into(),
			lending_positions: value.lending_positions,
			collateral_balances: value.collateral_balances,
		}
	}
}

// Decode-only intermediate (converted to the current type via `From`); no serde needed.
#[derive(Encode, Decode, TypeInfo, Clone, Default, Debug)]
pub struct RpcAccountInfoCommonItems<Balance> {
	pub account_id: Option<AccountId32>,
	pub vanity_name: VanityName,
	pub flip_balance: Balance,
	pub asset_balances: AssetMap<Balance>,
	pub bond: Balance,
	pub estimated_redeemable_balance: Balance,
	pub bound_redeem_address: Option<EvmAddress>,
	pub restricted_balances: BTreeMap<EvmAddress, Balance>,
	pub current_delegation_status: Option<DelegationInfo<Balance>>,
	pub upcoming_delegation_status: Option<DelegationInfo<Balance>>,
}

impl<B: Default> From<RpcAccountInfoCommonItems<B>> for super::RpcAccountInfoCommonItems<B> {
	fn from(value: RpcAccountInfoCommonItems<B>) -> Self {
		Self {
			account_id: value.account_id,
			vanity_name: value.vanity_name,
			flip_balance: value.flip_balance,
			asset_balances: value.asset_balances.into(),
			bond: value.bond,
			estimated_redeemable_balance: value.estimated_redeemable_balance,
			bound_redeem_address: value.bound_redeem_address,
			restricted_balances: value.restricted_balances,
			current_delegation_status: value.current_delegation_status,
			upcoming_delegation_status: value.upcoming_delegation_status,
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Clone)]
pub struct VaultAddresses {
	pub ethereum: EncodedAddress,
	pub arbitrum: EncodedAddress,
	pub bitcoin: Vec<(AccountId32, EncodedAddress)>,
	pub sol_vault_program: EncodedAddress,
	pub sol_swap_endpoint_program_data_account: EncodedAddress,
	pub usdc_token_mint_pubkey: EncodedAddress,
	pub usdt_token_mint_pubkey: EncodedAddress,

	pub bitcoin_vault: Option<EncodedAddress>,
	pub solana_sol_vault: Option<EncodedAddress>,
	pub solana_usdc_token_vault_ata: EncodedAddress,
	pub solana_usdt_token_vault_ata: EncodedAddress,
	pub solana_vault_swap_account: Option<EncodedAddress>,
	pub tron: EncodedAddress,

	pub predicted_seconds_until_next_vault_rotation: u64,
}

impl From<VaultAddresses> for super::VaultAddresses {
	fn from(old: VaultAddresses) -> Self {
		Self {
			ethereum: old.ethereum.into(),
			arbitrum: old.arbitrum.into(),
			bitcoin: old.bitcoin.into_iter().map(|(a, b)| (a, b.into())).collect(),
			sol_vault_program: old.sol_vault_program.into(),
			sol_swap_endpoint_program_data_account: old
				.sol_swap_endpoint_program_data_account
				.into(),
			usdc_token_mint_pubkey: old.usdc_token_mint_pubkey.into(),
			usdt_token_mint_pubkey: old.usdt_token_mint_pubkey.into(),
			bitcoin_vault: old.bitcoin_vault.map(Into::into),
			solana_sol_vault: old.solana_sol_vault.map(Into::into),
			solana_usdc_token_vault_ata: old.solana_usdc_token_vault_ata.into(),
			solana_usdt_token_vault_ata: old.solana_usdt_token_vault_ata.into(),
			solana_vault_swap_account: old.solana_vault_swap_account.map(Into::into),
			tron: old.tron.into(),
			bsc: super::EncodedAddress::Bsc([0u8; 20]),
			predicted_seconds_until_next_vault_rotation: old
				.predicted_seconds_until_next_vault_rotation,
		}
	}
}

#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Default,
	Copy,
	Clone,
	PartialEq,
	Eq,
	frame_support::pallet_prelude::RuntimeDebug,
)]
pub struct WitnesserCallPermission {
	pub governance: bool,
	pub funding: bool,
	pub swapping: bool,
	pub ethereum_broadcast: bool,
	pub ethereum_chain_tracking: bool,
	pub ethereum_ingress_egress: bool,
	pub ethereum_vault: bool,
	pub polkadot_broadcast: bool,
	pub polkadot_chain_tracking: bool,
	pub polkadot_ingress_egress: bool,
	pub polkadot_vault: bool,
	pub bitcoin_broadcast: bool,
	pub bitcoin_chain_tracking: bool,
	pub bitcoin_ingress_egress: bool,
	pub bitcoin_vault: bool,
	pub arbitrum_broadcast: bool,
	pub arbitrum_chain_tracking: bool,
	pub arbitrum_ingress_egress: bool,
	pub arbitrum_vault: bool,
	pub solana_broadcast: bool,
	pub solana_vault: bool,
	pub assethub_broadcast: bool,
	pub assethub_chain_tracking: bool,
	pub assethub_ingress_egress: bool,
	pub assethub_vault: bool,
	pub tron_broadcast: bool,
	pub tron_chain_tracking: bool,
	pub tron_ingress_egress: bool,
	pub tron_vault: bool,
}

impl From<WitnesserCallPermission> for crate::safe_mode::WitnesserCallPermission {
	fn from(old: WitnesserCallPermission) -> Self {
		Self {
			governance: old.governance,
			funding: old.funding,
			swapping: old.swapping,
			ethereum_broadcast: old.ethereum_broadcast,
			ethereum_chain_tracking: old.ethereum_chain_tracking,
			ethereum_ingress_egress: old.ethereum_ingress_egress,
			ethereum_vault: old.ethereum_vault,
			polkadot_broadcast: old.polkadot_broadcast,
			polkadot_chain_tracking: old.polkadot_chain_tracking,
			polkadot_ingress_egress: old.polkadot_ingress_egress,
			polkadot_vault: old.polkadot_vault,
			bitcoin_broadcast: old.bitcoin_broadcast,
			bitcoin_chain_tracking: old.bitcoin_chain_tracking,
			bitcoin_ingress_egress: old.bitcoin_ingress_egress,
			bitcoin_vault: old.bitcoin_vault,
			arbitrum_broadcast: old.arbitrum_broadcast,
			arbitrum_chain_tracking: old.arbitrum_chain_tracking,
			arbitrum_ingress_egress: old.arbitrum_ingress_egress,
			arbitrum_vault: old.arbitrum_vault,
			solana_broadcast: old.solana_broadcast,
			solana_vault: old.solana_vault,
			assethub_broadcast: old.assethub_broadcast,
			assethub_chain_tracking: old.assethub_chain_tracking,
			assethub_ingress_egress: old.assethub_ingress_egress,
			assethub_vault: old.assethub_vault,
			tron_broadcast: old.tron_broadcast,
			tron_chain_tracking: old.tron_chain_tracking,
			tron_ingress_egress: old.tron_ingress_egress,
			tron_vault: old.tron_vault,
			bsc_broadcast: true,
			bsc_chain_tracking: true,
			bsc_ingress_egress: true,
			bsc_vault: true,
		}
	}
}

#[derive(
	Encode, Decode, TypeInfo, Clone, PartialEq, Eq, frame_support::pallet_prelude::RuntimeDebug,
)]
pub struct RuntimeSafeMode {
	pub emissions: pallet_cf_emissions::PalletSafeMode,
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
	pub witnesser: pallet_cf_witnesser::PalletSafeMode<WitnesserCallPermission>,
	pub ingress_egress_ethereum: pallet_cf_ingress_egress::PalletSafeMode<EthereumInstance>,
	pub ingress_egress_bitcoin: pallet_cf_ingress_egress::PalletSafeMode<BitcoinInstance>,
	pub ingress_egress_polkadot: pallet_cf_ingress_egress::PalletSafeMode<PolkadotInstance>,
	pub ingress_egress_arbitrum: pallet_cf_ingress_egress::PalletSafeMode<ArbitrumInstance>,
	pub ingress_egress_solana: pallet_cf_ingress_egress::PalletSafeMode<SolanaInstance>,
	pub ingress_egress_assethub: pallet_cf_ingress_egress::PalletSafeMode<AssethubInstance>,
	pub ingress_egress_tron: pallet_cf_ingress_egress::PalletSafeMode<TronInstance>,
	pub elections_generic:
		crate::chainflip::witnessing::generic_elections::GenericElectionsSafeMode,
	pub ethereum_elections:
		crate::chainflip::witnessing::ethereum_elections::EthereumElectionsSafeMode,
	pub arbitrum_elections:
		crate::chainflip::witnessing::arbitrum_elections::ArbitrumElectionsSafeMode,
	pub tron_elections: crate::chainflip::witnessing::tron_elections::TronElectionsSafeMode,
}

impl From<RuntimeSafeMode> for crate::safe_mode::RuntimeSafeMode {
	fn from(old: RuntimeSafeMode) -> Self {
		use cf_traits::SafeMode;
		let witnesser = match old.witnesser {
			pallet_cf_witnesser::PalletSafeMode::CodeGreen =>
				pallet_cf_witnesser::PalletSafeMode::CodeGreen,
			pallet_cf_witnesser::PalletSafeMode::CodeRed =>
				pallet_cf_witnesser::PalletSafeMode::CodeRed,
			pallet_cf_witnesser::PalletSafeMode::CodeAmber(old_perms) =>
				pallet_cf_witnesser::PalletSafeMode::CodeAmber(old_perms.into()),
		};
		Self {
			emissions: old.emissions,
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
			broadcast_tron:old.broadcast_tron,
			broadcast_bsc:
			<pallet_cf_broadcast::PalletSafeMode<_> as SafeMode>::code_green(),
			witnesser,
			ingress_egress_ethereum: old.ingress_egress_ethereum,
			ingress_egress_bitcoin: old.ingress_egress_bitcoin,
			ingress_egress_polkadot: old.ingress_egress_polkadot,
			ingress_egress_arbitrum: old.ingress_egress_arbitrum,
			ingress_egress_solana: old.ingress_egress_solana,
			ingress_egress_assethub: old.ingress_egress_assethub,
			ingress_egress_tron:old.ingress_egress_tron,
			ingress_egress_bsc:
			<pallet_cf_ingress_egress::PalletSafeMode<_> as SafeMode>::code_green(),
			elections_generic: old.elections_generic,
			ethereum_elections: old.ethereum_elections,
			arbitrum_elections: old.arbitrum_elections,
			tron_elections:old.tron_elections,
			bsc_elections:
			<crate::chainflip::witnessing::bsc_elections::BscElectionsSafeMode as SafeMode>::code_green(),
		}
	}
}

#[derive(Serialize, Deserialize, Encode, Decode, Eq, PartialEq, TypeInfo, Debug, Clone)]
pub struct TransactionScreeningEvents {
	pub btc_events: Vec<BrokerRejectionEventFor<cf_chains::Bitcoin>>,
	pub eth_events: Vec<BrokerRejectionEventFor<cf_chains::Ethereum>>,
	pub arb_events: Vec<BrokerRejectionEventFor<cf_chains::Arbitrum>>,
	pub sol_events: Vec<BrokerRejectionEventFor<cf_chains::Solana>>,
	pub tron_events: Vec<BrokerRejectionEventFor<cf_chains::Tron>>,
}

impl From<TransactionScreeningEvents> for super::TransactionScreeningEvents {
	fn from(old: TransactionScreeningEvents) -> Self {
		Self {
			btc_events: old.btc_events,
			eth_events: old.eth_events,
			arb_events: old.arb_events,
			sol_events: old.sol_events,
			tron_events: old.tron_events,
			bsc_events: Default::default(),
		}
	}
}

#[derive(Encode, Decode, TypeInfo)]
pub enum RuntimeApiAccountInfo {
	Unregistered,
	Broker(Box<super::BrokerInfo<<Bitcoin as Chain>::ChainAccount>>),
	LiquidityProvider(Box<LiquidityProviderInfo>),
	Validator(Box<super::ValidatorInfo>),
	Operator(Box<super::OperatorInfo<FlipBalance>>),
}

impl From<RuntimeApiAccountInfo> for super::RuntimeApiAccountInfo {
	fn from(old: RuntimeApiAccountInfo) -> Self {
		match old {
			RuntimeApiAccountInfo::Unregistered => Self::Unregistered,
			RuntimeApiAccountInfo::Broker(info) => Self::Broker(info),
			RuntimeApiAccountInfo::LiquidityProvider(info) =>
				Self::LiquidityProvider(Box::new((*info).into())),
			RuntimeApiAccountInfo::Validator(info) => Self::Validator(info),
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
		Self { common_items: value.common_items.into(), role: value.role.into() }
	}
}

// Ingress events as returned by pre-v19 runtimes: identical to the current types except the
// `Bsc` variant (appended at the end at v19) is absent from `TransactionInId`, `DepositDetails`
// and `EncodedAddress`. These types are only decoded from old-wasm responses and then converted
// to the current (super) types, so they do not need serde.

#[derive(Clone, Debug, PartialEq, Eq, TypeInfo, Encode, Decode)]
pub enum TransactionInId {
	Bitcoin(cf_chains::TransactionInIdFor<Bitcoin>),
	Ethereum(cf_chains::TransactionInIdFor<Ethereum>),
	Arbitrum(cf_chains::TransactionInIdFor<Arbitrum>),
	Tron(cf_chains::TransactionInIdFor<Tron>),
	Solana(cf_chains::TransactionInIdFor<cf_chains::Solana>),
	SolanaDepositChannel(SolAddress),
}

impl From<TransactionInId> for super::TransactionInId {
	fn from(old: TransactionInId) -> Self {
		match old {
			TransactionInId::Bitcoin(id) => Self::Bitcoin(id),
			TransactionInId::Ethereum(id) => Self::Ethereum(id),
			TransactionInId::Arbitrum(id) => Self::Arbitrum(id),
			TransactionInId::Tron(id) => Self::Tron(id),
			TransactionInId::Solana(id) => Self::Solana(id),
			TransactionInId::SolanaDepositChannel(address) => Self::SolanaDepositChannel(address),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, TypeInfo, Encode, Decode)]
pub enum DepositDetails {
	Bitcoin(<Bitcoin as Chain>::DepositDetails),
	Ethereum(<Ethereum as Chain>::DepositDetails),
	Arbitrum(<Arbitrum as Chain>::DepositDetails),
	Tron(<Tron as Chain>::DepositDetails),
}

impl From<DepositDetails> for super::DepositDetails {
	fn from(old: DepositDetails) -> Self {
		match old {
			DepositDetails::Bitcoin(details) => Self::Bitcoin(details),
			DepositDetails::Ethereum(details) => Self::Ethereum(details),
			DepositDetails::Arbitrum(details) => Self::Arbitrum(details),
			DepositDetails::Tron(details) => Self::Tron(details),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, TypeInfo, Encode, Decode)]
pub struct DepositWitnessInfo {
	pub deposit_chain_block_height: u64,
	pub deposit_address: EncodedAddress,
	pub amount: AssetAmount,
	pub asset: Asset,
	pub deposit_details: DepositDetails,
}

impl From<DepositWitnessInfo> for super::DepositWitnessInfo {
	fn from(old: DepositWitnessInfo) -> Self {
		Self {
			deposit_chain_block_height: old.deposit_chain_block_height,
			deposit_address: old.deposit_address.into(),
			amount: old.amount,
			asset: old.asset,
			deposit_details: old.deposit_details.into(),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, TypeInfo, Encode, Decode)]
pub struct VaultDepositWitnessInfo {
	pub tx_id: TransactionInId,
	pub deposit_chain_block_height: u64,
	pub input_asset: Asset,
	pub output_asset: Asset,
	pub amount: AssetAmount,
	pub destination_address: EncodedAddress,
	pub deposit_details: DepositDetails,
}

impl From<VaultDepositWitnessInfo> for super::VaultDepositWitnessInfo {
	fn from(old: VaultDepositWitnessInfo) -> Self {
		Self {
			tx_id: old.tx_id.into(),
			deposit_chain_block_height: old.deposit_chain_block_height,
			input_asset: old.input_asset,
			output_asset: old.output_asset,
			amount: old.amount,
			destination_address: old.destination_address.into(),
			deposit_details: old.deposit_details.into(),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, TypeInfo, Encode, Decode)]
pub struct IngressEvents {
	pub deposits: Vec<DepositWitnessInfo>,
	pub vault_deposits: Vec<VaultDepositWitnessInfo>,
}

impl From<IngressEvents> for super::IngressEvents {
	fn from(old: IngressEvents) -> Self {
		Self {
			deposits: old.deposits.into_iter().map(Into::into).collect(),
			vault_deposits: old.vault_deposits.into_iter().map(Into::into).collect(),
		}
	}
}

/// The v17-v18 wire shape of `RpcLendingPool`. The `owed_to_network` field was dropped at v19
/// when the IOU mechanism was replaced by accruing uncollected network fees back to
/// `pending_interest` (see PRO-2850).
#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Debug)]
pub struct RpcLendingPool<Amount> {
	pub asset: Asset,
	pub total_amount: Amount,
	pub available_amount: Amount,
	pub owed_to_network: Amount,
	pub utilisation_rate: Permill,
	pub utilisation_cap: Permill,
	pub current_interest_rate: Permill,
	pub config: LendingPoolConfiguration,
}

impl<Amount> From<RpcLendingPool<Amount>> for pallet_cf_lending_pools::RpcLendingPool<Amount> {
	fn from(value: RpcLendingPool<Amount>) -> Self {
		Self {
			asset: value.asset,
			total_amount: value.total_amount,
			available_amount: value.available_amount,
			utilisation_rate: value.utilisation_rate,
			utilisation_cap: value.utilisation_cap,
			current_interest_rate: value.current_interest_rate,
			config: value.config,
		}
	}
}
