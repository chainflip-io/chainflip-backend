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

use frame_support::{traits::UncheckedOnRuntimeUpgrade, weights::Weight};

use crate::{
	chainflip::{
		arbitrum_elections::ArbitrumElectionsSafeMode,
		ethereum_elections::EthereumElectionsSafeMode,
	},
	Runtime,
};
use cf_traits::SafeMode;
pub struct SafeModeMigration;

mod old {
	use cf_chains::instances::{
		ArbitrumInstance, AssethubInstance, BitcoinCryptoInstance, BitcoinInstance,
		EthereumInstance, EvmInstance, PolkadotCryptoInstance, PolkadotInstance,
		SolanaCryptoInstance, SolanaInstance,
	};
	use codec::{Decode, Encode};
	use sp_core::RuntimeDebug;

	use crate::safe_mode;

	#[derive(Clone, Encode, Decode, PartialEq, Eq, RuntimeDebug)]
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
		pub witnesser: pallet_cf_witnesser::PalletSafeMode<safe_mode::WitnesserCallPermission>,
		pub ingress_egress_ethereum: pallet_cf_ingress_egress::PalletSafeMode<EthereumInstance>,
		pub ingress_egress_bitcoin: pallet_cf_ingress_egress::PalletSafeMode<BitcoinInstance>,
		pub ingress_egress_polkadot: pallet_cf_ingress_egress::PalletSafeMode<PolkadotInstance>,
		pub ingress_egress_arbitrum: pallet_cf_ingress_egress::PalletSafeMode<ArbitrumInstance>,
		pub ingress_egress_solana: pallet_cf_ingress_egress::PalletSafeMode<SolanaInstance>,
		pub ingress_egress_assethub: pallet_cf_ingress_egress::PalletSafeMode<AssethubInstance>,
		pub elections_generic: crate::chainflip::generic_elections::GenericElectionsSafeMode,
	}
}

impl UncheckedOnRuntimeUpgrade for SafeModeMigration {
	fn on_runtime_upgrade() -> Weight {
		let _ = pallet_cf_environment::RuntimeSafeMode::<Runtime>::translate(
			|maybe_old: Option<old::RuntimeSafeMode>| {
				maybe_old.map(|old| crate::safe_mode::RuntimeSafeMode {
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
                        add_collateral: old.lending_pools.add_collateral,
                        remove_collateral: old.lending_pools.remove_collateral,
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
                    witnesser: old.witnesser,
                    ingress_egress_ethereum: old.ingress_egress_ethereum,
                    ingress_egress_bitcoin: old.ingress_egress_bitcoin,
                    ingress_egress_polkadot: old.ingress_egress_polkadot,
                    ingress_egress_arbitrum: old.ingress_egress_arbitrum,
                    ingress_egress_solana: old.ingress_egress_solana,
                    ingress_egress_assethub: old.ingress_egress_assethub,
					elections_generic: old.elections_generic,
                    ethereum_elections: EthereumElectionsSafeMode::code_green(),
					arbitrum_elections: ArbitrumElectionsSafeMode::code_green(),
				})
			},
		).map_err(|_| {
			log::warn!("Migration for Runtime Safe mode was not able to interpret the existing storage in the old format!")
		});

		Weight::zero()
	}
}
