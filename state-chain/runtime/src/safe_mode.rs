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

//! For filtering runtime calls and other related utilities.

use crate::{Runtime, RuntimeCall};
use cf_traits::{impl_runtime_safe_mode, CallDispatchFilter};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::instances::*;
use scale_info::TypeInfo;

impl_runtime_safe_mode! {
	RuntimeSafeMode,
	pallet_cf_environment::RuntimeSafeMode<Runtime>,
	emissions: pallet_cf_emissions::PalletSafeMode,
	funding: pallet_cf_funding::PalletSafeMode,
	swapping: pallet_cf_swapping::PalletSafeMode,
	liquidity_provider: pallet_cf_lp::PalletSafeMode,
	validator: pallet_cf_validator::PalletSafeMode,
	pools: pallet_cf_pools::PalletSafeMode,
	trading_strategies: pallet_cf_trading_strategy::PalletSafeMode,
	lending_pools: pallet_cf_lending_pools::PalletSafeMode,
	reputation: pallet_cf_reputation::PalletSafeMode,
	asset_balances: pallet_cf_asset_balances::PalletSafeMode,
	threshold_signature_evm: pallet_cf_threshold_signature::PalletSafeMode<Instance16>,
	threshold_signature_bitcoin: pallet_cf_threshold_signature::PalletSafeMode<Instance3>,
	threshold_signature_polkadot: pallet_cf_threshold_signature::PalletSafeMode<Instance15>,
	threshold_signature_solana: pallet_cf_threshold_signature::PalletSafeMode<Instance5>,
	broadcast_ethereum: pallet_cf_broadcast::PalletSafeMode<Instance1>,
	broadcast_bitcoin: pallet_cf_broadcast::PalletSafeMode<Instance3>,
	broadcast_polkadot: pallet_cf_broadcast::PalletSafeMode<Instance2>,
	broadcast_arbitrum: pallet_cf_broadcast::PalletSafeMode<Instance4>,
	broadcast_solana: pallet_cf_broadcast::PalletSafeMode<Instance5>,
	broadcast_assethub: pallet_cf_broadcast::PalletSafeMode<Instance6>,
	witnesser: pallet_cf_witnesser::PalletSafeMode<WitnesserCallPermission>,
	ingress_egress_ethereum: pallet_cf_ingress_egress::PalletSafeMode<Instance1>,
	ingress_egress_bitcoin: pallet_cf_ingress_egress::PalletSafeMode<Instance3>,
	ingress_egress_polkadot: pallet_cf_ingress_egress::PalletSafeMode<Instance2>,
	ingress_egress_arbitrum: pallet_cf_ingress_egress::PalletSafeMode<Instance4>,
	ingress_egress_solana: pallet_cf_ingress_egress::PalletSafeMode<Instance5>,
	ingress_egress_assethub: pallet_cf_ingress_egress::PalletSafeMode<Instance6>,
	elections_generic: crate::chainflip::generic_elections::GenericElectionsSafeMode,
}

/// Contains permissions for different Runtime calls.
/// This is done through the SafeMode::CodeAmber of the Witnesser pallet.
/// Only calls allowed here can be dispatched with Witnesser origin.
#[derive(
	serde::Serialize,
	serde::Deserialize,
	Encode,
	Decode,
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
	// Non-instantiable pallets
	pub governance: bool,
	pub funding: bool,
	pub swapping: bool,

	// Ethereum pallets
	pub ethereum_broadcast: bool,
	pub ethereum_chain_tracking: bool,
	pub ethereum_ingress_egress: bool,
	pub ethereum_vault: bool,

	// Polkadot pallets
	pub polkadot_broadcast: bool,
	pub polkadot_chain_tracking: bool,
	pub polkadot_ingress_egress: bool,
	pub polkadot_vault: bool,

	// Bitcoin pallets
	pub bitcoin_broadcast: bool,
	pub bitcoin_chain_tracking: bool,
	pub bitcoin_ingress_egress: bool,
	pub bitcoin_vault: bool,

	// Arbitrum pallets
	pub arbitrum_broadcast: bool,
	pub arbitrum_chain_tracking: bool,
	pub arbitrum_ingress_egress: bool,
	pub arbitrum_vault: bool,

	// Solana pallets
	pub solana_broadcast: bool,
	pub solana_vault: bool,

	// Assethub pallets
	pub assethub_broadcast: bool,
	pub assethub_chain_tracking: bool,
	pub assethub_ingress_egress: bool,
	pub assethub_vault: bool,
}

impl WitnesserCallPermission {
	pub fn allow_all() -> Self {
		WitnesserCallPermission {
			governance: true,
			funding: true,
			swapping: true,
			ethereum_broadcast: true,
			ethereum_chain_tracking: true,
			ethereum_ingress_egress: true,
			ethereum_vault: true,
			polkadot_broadcast: true,
			polkadot_chain_tracking: true,
			polkadot_ingress_egress: true,
			polkadot_vault: true,
			bitcoin_broadcast: true,
			bitcoin_chain_tracking: true,
			bitcoin_ingress_egress: true,
			bitcoin_vault: true,
			arbitrum_broadcast: true,
			arbitrum_chain_tracking: true,
			arbitrum_ingress_egress: true,
			arbitrum_vault: true,
			solana_broadcast: true,
			solana_vault: true,
			assethub_broadcast: true,
			assethub_chain_tracking: true,
			assethub_ingress_egress: true,
			assethub_vault: true,
		}
	}
}

impl CallDispatchFilter<RuntimeCall> for WitnesserCallPermission {
	fn should_dispatch(&self, call: &RuntimeCall) -> bool {
		match call {
			RuntimeCall::Governance(..) => self.governance,
			RuntimeCall::Funding(..) => self.funding,
			RuntimeCall::Swapping(..) => self.swapping,

			RuntimeCall::EthereumBroadcaster(..) => self.ethereum_broadcast,
			RuntimeCall::EthereumChainTracking(..) => self.ethereum_chain_tracking,
			RuntimeCall::EthereumIngressEgress(..) => self.ethereum_ingress_egress,
			RuntimeCall::EthereumVault(..) => self.ethereum_vault,

			RuntimeCall::PolkadotBroadcaster(..) => self.polkadot_broadcast,
			RuntimeCall::PolkadotChainTracking(..) => self.polkadot_chain_tracking,
			RuntimeCall::PolkadotIngressEgress(..) => self.polkadot_ingress_egress,
			RuntimeCall::PolkadotVault(..) => self.polkadot_vault,

			RuntimeCall::BitcoinBroadcaster(..) => self.bitcoin_broadcast,
			RuntimeCall::BitcoinChainTracking(..) => self.bitcoin_chain_tracking,
			RuntimeCall::BitcoinIngressEgress(..) => self.bitcoin_ingress_egress,
			RuntimeCall::BitcoinVault(..) => self.bitcoin_vault,

			RuntimeCall::ArbitrumBroadcaster(..) => self.arbitrum_broadcast,
			RuntimeCall::ArbitrumChainTracking(..) => self.arbitrum_chain_tracking,
			RuntimeCall::ArbitrumIngressEgress(..) => self.arbitrum_ingress_egress,
			RuntimeCall::ArbitrumVault(..) => self.arbitrum_vault,

			RuntimeCall::SolanaBroadcaster(..) => self.solana_broadcast,
			RuntimeCall::SolanaVault(..) => self.solana_vault,

			RuntimeCall::AssethubBroadcaster(..) => self.assethub_broadcast,
			RuntimeCall::AssethubChainTracking(..) => self.assethub_chain_tracking,
			RuntimeCall::AssethubIngressEgress(..) => self.assethub_ingress_egress,
			RuntimeCall::AssethubVault(..) => self.assethub_vault,

			_ => {
				cf_runtime_utilities::log_or_panic!(
					"All witnesser calls must be controllable through `WitnesserCallPermission`. Call: {:?}",
					call
				);
				#[allow(unreachable_code)]
				false
			},
		}
	}
}
