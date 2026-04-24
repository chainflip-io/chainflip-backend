// Copyright 2026 Chainflip Labs GmbH
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

use cf_traits::SafeMode;
use frame_support::{instances::Instance7, traits::OnRuntimeUpgrade, weights::Weight};

use crate::Runtime;

pub struct SafeModeMigration;

mod old {
	use crate::chainflip::witnessing::{
		arbitrum_elections::ArbitrumElectionsSafeMode,
		ethereum_elections::EthereumElectionsSafeMode, generic_elections::GenericElectionsSafeMode,
	};
	use cf_chains::instances::{
		ArbitrumInstance, AssethubInstance, BitcoinCryptoInstance, BitcoinInstance,
		EthereumInstance, EvmInstance, PolkadotCryptoInstance, PolkadotInstance,
		SolanaCryptoInstance, SolanaInstance,
	};
	use cf_primitives::Asset;
	use cf_traits::SafeModeSet;
	use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
	use scale_info::TypeInfo;
	use sp_core::RuntimeDebug;

	/// The old lending pools safe mode with add_collateral and remove_collateral fields.
	#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
	pub struct LendingPoolsSafeMode {
		pub add_boost_funds_enabled: bool,
		pub stop_boosting_enabled: bool,
		pub borrowing: SafeModeSet<Asset>,
		pub add_lender_funds: SafeModeSet<Asset>,
		pub withdraw_lender_funds: SafeModeSet<Asset>,
		pub add_collateral: SafeModeSet<Asset>,
		pub remove_collateral: SafeModeSet<Asset>,
		pub liquidations_enabled: bool,
	}

	/// The pre-Tron WitnesserCallPermission (no tron_* fields).
	#[derive(
		serde::Serialize,
		serde::Deserialize,
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
		RuntimeDebug,
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
	}

	#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
	pub struct RuntimeSafeMode {
		pub emissions: pallet_cf_emissions::PalletSafeMode,
		pub funding: pallet_cf_funding::PalletSafeMode,
		pub swapping: pallet_cf_swapping::PalletSafeMode,
		pub liquidity_provider: pallet_cf_lp::PalletSafeMode,
		pub validator: pallet_cf_validator::PalletSafeMode,
		pub pools: pallet_cf_pools::PalletSafeMode,
		pub trading_strategies: pallet_cf_trading_strategy::PalletSafeMode,
		pub lending_pools: LendingPoolsSafeMode,
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
		pub witnesser: pallet_cf_witnesser::PalletSafeMode<WitnesserCallPermission>,
		pub ingress_egress_ethereum: pallet_cf_ingress_egress::PalletSafeMode<EthereumInstance>,
		pub ingress_egress_bitcoin: pallet_cf_ingress_egress::PalletSafeMode<BitcoinInstance>,
		pub ingress_egress_polkadot: pallet_cf_ingress_egress::PalletSafeMode<PolkadotInstance>,
		pub ingress_egress_arbitrum: pallet_cf_ingress_egress::PalletSafeMode<ArbitrumInstance>,
		pub ingress_egress_solana: pallet_cf_ingress_egress::PalletSafeMode<SolanaInstance>,
		pub ingress_egress_assethub: pallet_cf_ingress_egress::PalletSafeMode<AssethubInstance>,
		pub elections_generic: GenericElectionsSafeMode,
		pub ethereum_elections: EthereumElectionsSafeMode,
		pub arbitrum_elections: ArbitrumElectionsSafeMode,
	}
}

impl OnRuntimeUpgrade for SafeModeMigration {
	fn on_runtime_upgrade() -> Weight {
		let _ = pallet_cf_environment::RuntimeSafeMode::<Runtime>::translate(
			|maybe_old: Option<old::RuntimeSafeMode>| {
				maybe_old.map(|old| {
					let witnesser = match old.witnesser {
						pallet_cf_witnesser::PalletSafeMode::CodeGreen =>
							pallet_cf_witnesser::PalletSafeMode::CodeGreen,
						pallet_cf_witnesser::PalletSafeMode::CodeRed =>
							pallet_cf_witnesser::PalletSafeMode::CodeRed,
						pallet_cf_witnesser::PalletSafeMode::CodeAmber(old_perms) =>
							pallet_cf_witnesser::PalletSafeMode::CodeAmber(
								crate::safe_mode::WitnesserCallPermission {
									governance: old_perms.governance,
									funding: old_perms.funding,
									swapping: old_perms.swapping,
									ethereum_broadcast: old_perms.ethereum_broadcast,
									ethereum_chain_tracking: old_perms.ethereum_chain_tracking,
									ethereum_ingress_egress: old_perms.ethereum_ingress_egress,
									ethereum_vault: old_perms.ethereum_vault,
									polkadot_broadcast: old_perms.polkadot_broadcast,
									polkadot_chain_tracking: old_perms.polkadot_chain_tracking,
									polkadot_ingress_egress: old_perms.polkadot_ingress_egress,
									polkadot_vault: old_perms.polkadot_vault,
									bitcoin_broadcast: old_perms.bitcoin_broadcast,
									bitcoin_chain_tracking: old_perms.bitcoin_chain_tracking,
									bitcoin_ingress_egress: old_perms.bitcoin_ingress_egress,
									bitcoin_vault: old_perms.bitcoin_vault,
									arbitrum_broadcast: old_perms.arbitrum_broadcast,
									arbitrum_chain_tracking: old_perms.arbitrum_chain_tracking,
									arbitrum_ingress_egress: old_perms.arbitrum_ingress_egress,
									arbitrum_vault: old_perms.arbitrum_vault,
									solana_broadcast: old_perms.solana_broadcast,
									solana_vault: old_perms.solana_vault,
									assethub_broadcast: old_perms.assethub_broadcast,
									assethub_chain_tracking: old_perms.assethub_chain_tracking,
									assethub_ingress_egress: old_perms.assethub_ingress_egress,
									assethub_vault: old_perms.assethub_vault,
									tron_broadcast: true,
									tron_chain_tracking: true,
									tron_ingress_egress: true,
									tron_vault: true,
								},
							),
					};

					crate::safe_mode::RuntimeSafeMode {
						emissions: old.emissions,
						funding: old.funding,
						swapping: old.swapping,
						liquidity_provider: old.liquidity_provider,
						validator: old.validator,
						pools: old.pools,
						trading_strategies: old.trading_strategies,
						lending_pools: pallet_cf_lending_pools::PalletSafeMode {
							add_boost_funds_enabled: old.lending_pools.add_boost_funds_enabled,
							stop_boosting_enabled: old.lending_pools.stop_boosting_enabled,
							borrowing: old.lending_pools.borrowing,
							add_lender_funds: old.lending_pools.add_lender_funds,
							withdraw_lender_funds: old.lending_pools.withdraw_lender_funds,
							// add_collateral and remove_collateral are dropped
							liquidations_enabled: old.lending_pools.liquidations_enabled,
						},
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
						broadcast_tron:
							<pallet_cf_broadcast::PalletSafeMode<Instance7> as SafeMode>::code_green(),
						witnesser,
						ingress_egress_ethereum: old.ingress_egress_ethereum,
						ingress_egress_bitcoin: old.ingress_egress_bitcoin,
						ingress_egress_polkadot: old.ingress_egress_polkadot,
						ingress_egress_arbitrum: old.ingress_egress_arbitrum,
						ingress_egress_solana: old.ingress_egress_solana,
						ingress_egress_assethub: old.ingress_egress_assethub,
						ingress_egress_tron:
							<pallet_cf_ingress_egress::PalletSafeMode<Instance7> as SafeMode>::code_green(),
						elections_generic: old.elections_generic,
						ethereum_elections: old.ethereum_elections,
						arbitrum_elections: old.arbitrum_elections,
						tron_elections:
							<crate::chainflip::witnessing::tron_elections::TronElectionsSafeMode as SafeMode>::code_green(),
					}
				})
			},
		)
		.map_err(|_| {
			log::warn!(
				"Safe mode migration was not able to interpret the existing storage in the old format!"
			);
		});

		Weight::zero()
	}
}
