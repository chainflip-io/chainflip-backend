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

use crate::{Environment, Runtime, RuntimeCall};
use cf_primitives::ForeignChain;
use cf_traits::{impl_runtime_safe_mode, CallDispatchFilter, EmergencyPreset, EmergencySafeMode};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::instances::*;
use scale_info::TypeInfo;
use sp_std::collections::btree_set::BTreeSet;

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
	broadcast_tron: pallet_cf_broadcast::PalletSafeMode<Instance7>,
	broadcast_bsc: pallet_cf_broadcast::PalletSafeMode<Instance8>,
	witnesser: pallet_cf_witnesser::PalletSafeMode<WitnesserCallPermission>,
	ingress_egress_ethereum: pallet_cf_ingress_egress::PalletSafeMode<Instance1>,
	ingress_egress_bitcoin: pallet_cf_ingress_egress::PalletSafeMode<Instance3>,
	ingress_egress_polkadot: pallet_cf_ingress_egress::PalletSafeMode<Instance2>,
	ingress_egress_arbitrum: pallet_cf_ingress_egress::PalletSafeMode<Instance4>,
	ingress_egress_solana: pallet_cf_ingress_egress::PalletSafeMode<Instance5>,
	ingress_egress_assethub: pallet_cf_ingress_egress::PalletSafeMode<Instance6>,
	ingress_egress_tron: pallet_cf_ingress_egress::PalletSafeMode<Instance7>,
	ingress_egress_bsc: pallet_cf_ingress_egress::PalletSafeMode<Instance8>,
	elections_generic: crate::chainflip::witnessing::generic_elections::GenericElectionsSafeMode,
	ethereum_elections: crate::chainflip::witnessing::ethereum_elections::EthereumElectionsSafeMode,
	arbitrum_elections: crate::chainflip::witnessing::arbitrum_elections::ArbitrumElectionsSafeMode,
	tron_elections: crate::chainflip::witnessing::tron_elections::TronElectionsSafeMode,
	bsc_elections: crate::chainflip::witnessing::bsc_elections::BscElectionsSafeMode,
}

/// Contains permissions for different Runtime calls.
/// This is done through the SafeMode::CodeAmber of the Witnesser pallet.
/// Only calls allowed here can be dispatched with Witnesser origin.
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

	// Tron pallets
	pub tron_broadcast: bool,
	pub tron_chain_tracking: bool,
	pub tron_ingress_egress: bool,
	pub tron_vault: bool,

	// BSC pallets
	pub bsc_broadcast: bool,
	pub bsc_chain_tracking: bool,
	pub bsc_ingress_egress: bool,
	pub bsc_vault: bool,
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
			tron_broadcast: true,
			tron_chain_tracking: true,
			tron_ingress_egress: true,
			tron_vault: true,
			bsc_broadcast: true,
			bsc_chain_tracking: true,
			bsc_ingress_egress: true,
			bsc_vault: true,
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

			RuntimeCall::TronBroadcaster(..) => self.tron_broadcast,
			RuntimeCall::TronChainTracking(..) => self.tron_chain_tracking,
			RuntimeCall::TronIngressEgress(..) => self.tron_ingress_egress,
			RuntimeCall::TronVault(..) => self.tron_vault,

			RuntimeCall::BscBroadcaster(..) => self.bsc_broadcast,
			RuntimeCall::BscChainTracking(..) => self.bsc_chain_tracking,
			RuntimeCall::BscIngressEgress(..) => self.bsc_ingress_egress,
			RuntimeCall::BscVault(..) => self.bsc_vault,

			_ => {
				cf_runtime_utilities::log_or_panic!(
					"All witnesser calls must be controllable through `WitnesserCallPermission`. Call: {:?}",
					call
				);
				false
			},
		}
	}
}

impl EmergencySafeMode for RuntimeSafeMode {
	fn apply(presets: &BTreeSet<EmergencyPreset>) {
		Environment::mutate_safe_mode(|safe_mode| apply_emergency_presets(safe_mode, presets));
	}
}

fn apply_emergency_presets(safe_mode: &mut RuntimeSafeMode, presets: &BTreeSet<EmergencyPreset>) {
	for preset in presets {
		match *preset {
			EmergencyPreset::DisableDepositChannelCreation(chain) =>
				*deposit_channel_creation_enabled(safe_mode, chain) = false,
			EmergencyPreset::DisableNewLoans(asset) =>
				safe_mode.lending_pools.borrowing.disable(asset),
			EmergencyPreset::DisableLpWithdrawals =>
				safe_mode.liquidity_provider.withdrawal_enabled = false,
			EmergencyPreset::DisableInternalSwaps =>
				safe_mode.liquidity_provider.internal_swaps_enabled = false,
			EmergencyPreset::DisableBoost(chain) =>
				*boost_deposits_enabled(safe_mode, chain) = false,
			EmergencyPreset::DisableBrokerWithdrawals =>
				safe_mode.swapping.withdrawals_enabled = false,
			EmergencyPreset::DisableRedemptions => safe_mode.funding.redeem_enabled = false,
			EmergencyPreset::DisableIngressWitnessing(chain) => {
				*deposit_channel_witnessing_enabled(safe_mode, chain) = false;
				*vault_deposit_witnessing_enabled(safe_mode, chain) = false;
			},
		}
	}
}

/// A chain's ingress-egress flag, keeping the chain-to-instance mapping in one place.
macro_rules! ingress_egress_flag {
	($safe_mode:expr, $chain:expr, $flag:ident) => {
		match $chain {
			ForeignChain::Ethereum => &mut $safe_mode.ingress_egress_ethereum.$flag,
			ForeignChain::Polkadot => &mut $safe_mode.ingress_egress_polkadot.$flag,
			ForeignChain::Bitcoin => &mut $safe_mode.ingress_egress_bitcoin.$flag,
			ForeignChain::Arbitrum => &mut $safe_mode.ingress_egress_arbitrum.$flag,
			ForeignChain::Solana => &mut $safe_mode.ingress_egress_solana.$flag,
			ForeignChain::Assethub => &mut $safe_mode.ingress_egress_assethub.$flag,
			ForeignChain::Tron => &mut $safe_mode.ingress_egress_tron.$flag,
			ForeignChain::Bsc => &mut $safe_mode.ingress_egress_bsc.$flag,
		}
	};
}

fn deposit_channel_creation_enabled(
	safe_mode: &mut RuntimeSafeMode,
	chain: ForeignChain,
) -> &mut bool {
	ingress_egress_flag!(safe_mode, chain, deposit_channel_creation_enabled)
}

fn boost_deposits_enabled(safe_mode: &mut RuntimeSafeMode, chain: ForeignChain) -> &mut bool {
	ingress_egress_flag!(safe_mode, chain, boost_deposits_enabled)
}

fn deposit_channel_witnessing_enabled(
	safe_mode: &mut RuntimeSafeMode,
	chain: ForeignChain,
) -> &mut bool {
	ingress_egress_flag!(safe_mode, chain, deposit_channel_witnessing_enabled)
}

fn vault_deposit_witnessing_enabled(
	safe_mode: &mut RuntimeSafeMode,
	chain: ForeignChain,
) -> &mut bool {
	ingress_egress_flag!(safe_mode, chain, vault_deposit_witnessing_enabled)
}

#[cfg(test)]
mod emergency_preset_tests {
	use super::*;
	use cf_primitives::Asset;
	use cf_traits::{SafeMode, SafeModeSet};
	use proptest::{collection::vec, prelude::*};

	fn applied(
		safe_mode: &RuntimeSafeMode,
		presets: &BTreeSet<EmergencyPreset>,
	) -> RuntimeSafeMode {
		let mut safe_mode = safe_mode.clone();
		apply_emergency_presets(&mut safe_mode, presets);
		safe_mode
	}

	/// Whether the setting `preset` covers is switched off.
	fn is_off(safe_mode: &RuntimeSafeMode, preset: EmergencyPreset) -> bool {
		let mut safe_mode = safe_mode.clone();
		match preset {
			EmergencyPreset::DisableDepositChannelCreation(chain) =>
				!*deposit_channel_creation_enabled(&mut safe_mode, chain),
			EmergencyPreset::DisableNewLoans(asset) =>
				safe_mode.lending_pools.borrowing.disabled(&asset),
			EmergencyPreset::DisableLpWithdrawals =>
				!safe_mode.liquidity_provider.withdrawal_enabled,
			EmergencyPreset::DisableInternalSwaps =>
				!safe_mode.liquidity_provider.internal_swaps_enabled,
			EmergencyPreset::DisableBoost(chain) => !*boost_deposits_enabled(&mut safe_mode, chain),
			EmergencyPreset::DisableBrokerWithdrawals => !safe_mode.swapping.withdrawals_enabled,
			EmergencyPreset::DisableRedemptions => !safe_mode.funding.redeem_enabled,
			EmergencyPreset::DisableIngressWitnessing(chain) =>
				!*deposit_channel_witnessing_enabled(&mut safe_mode, chain) &&
					!*vault_deposit_witnessing_enabled(&mut safe_mode, chain),
		}
	}

	/// Resets every setting a preset can cover, so two safe modes can be compared on everything
	/// else.
	///
	/// A preset that covers a new setting must be added here: the property test fails until it is.
	/// This list is deliberately separate from the code under test, so it can't share its mistakes.
	fn with_preset_settings_reset(safe_mode: &RuntimeSafeMode) -> RuntimeSafeMode {
		let mut safe_mode = safe_mode.clone();
		for chain in ForeignChain::iter() {
			*deposit_channel_creation_enabled(&mut safe_mode, chain) = true;
			*boost_deposits_enabled(&mut safe_mode, chain) = true;
			*deposit_channel_witnessing_enabled(&mut safe_mode, chain) = true;
			*vault_deposit_witnessing_enabled(&mut safe_mode, chain) = true;
		}
		safe_mode.lending_pools.borrowing = SafeModeSet::Green;
		safe_mode.liquidity_provider.withdrawal_enabled = true;
		safe_mode.liquidity_provider.internal_swaps_enabled = true;
		safe_mode.swapping.withdrawals_enabled = true;
		safe_mode.funding.redeem_enabled = true;
		safe_mode
	}

	#[test]
	fn chain_presets_reach_only_that_chains_pallet() {
		use cf_chains::{
			instances::ChainInstanceFor, Arbitrum, Assethub, Bitcoin, Bsc, Chain, Ethereum,
			Polkadot, Solana, Tron,
		};
		use frame_support::traits::Get;
		use std::collections::BTreeMap;

		// Reads the flags the way the chain's own ingress-egress pallet does, independently of
		// `ingress_egress_flag!`.
		fn flags<C: Chain + Get<ForeignChain>>() -> (ForeignChain, [bool; 4])
		where
			Runtime: pallet_cf_ingress_egress::Config<ChainInstanceFor<C>>,
		{
			let safe_mode =
				<Runtime as pallet_cf_ingress_egress::Config<ChainInstanceFor<C>>>::SafeMode::get();
			(
				C::get(),
				[
					safe_mode.deposit_channel_creation_enabled,
					safe_mode.boost_deposits_enabled,
					safe_mode.deposit_channel_witnessing_enabled,
					safe_mode.vault_deposit_witnessing_enabled,
				],
			)
		}
		let flags_per_chain = || {
			BTreeMap::from([
				flags::<Ethereum>(),
				flags::<Polkadot>(),
				flags::<Bitcoin>(),
				flags::<Arbitrum>(),
				flags::<Solana>(),
				flags::<Assethub>(),
				flags::<Tron>(),
				flags::<Bsc>(),
			])
		};

		sp_io::TestExternalities::default().execute_with(|| {
			assert_eq!(flags_per_chain().len(), ForeignChain::iter().count());
			for chain in ForeignChain::iter() {
				for (preset, expected_flags) in [
					(
						EmergencyPreset::DisableDepositChannelCreation(chain),
						[false, true, true, true],
					),
					(EmergencyPreset::DisableBoost(chain), [true, false, true, true]),
					(EmergencyPreset::DisableIngressWitnessing(chain), [true, true, false, false]),
				] {
					pallet_cf_environment::RuntimeSafeMode::<Runtime>::put(applied(
						&RuntimeSafeMode::code_green(),
						&BTreeSet::from([preset]),
					));
					for (pallet_chain, flags) in flags_per_chain() {
						let expected =
							if pallet_chain == chain { expected_flags } else { [true; 4] };
						assert_eq!(flags, expected, "{preset:?}, {pallet_chain:?}");
					}
				}
			}
		});
	}

	fn safe_mode_strategy() -> impl Strategy<Value = RuntimeSafeMode> {
		(
			any::<bool>(),
			vec(any::<bool>(), ForeignChain::iter().count()),
			vec(any::<bool>(), ForeignChain::iter().count()),
			vec(any::<bool>(), ForeignChain::iter().count()),
			vec(any::<bool>(), ForeignChain::iter().count()),
			any::<bool>(),
			any::<bool>(),
			any::<bool>(),
			any::<bool>(),
			0..3u8,
			vec(any::<bool>(), Asset::all().count()),
		)
			.prop_map(
				|(
					red,
					channels,
					boost,
					channel_witnessing,
					vault_witnessing,
					withdrawals,
					internal_swaps,
					broker_withdrawals,
					redemptions,
					borrowing,
					assets_off,
				)| {
					let mut safe_mode = if red {
						RuntimeSafeMode::code_red()
					} else {
						RuntimeSafeMode::code_green()
					};
					for (chain, enabled) in ForeignChain::iter().zip(channels) {
						*deposit_channel_creation_enabled(&mut safe_mode, chain) = enabled;
					}
					for (chain, enabled) in ForeignChain::iter().zip(boost) {
						*boost_deposits_enabled(&mut safe_mode, chain) = enabled;
					}
					for (chain, enabled) in ForeignChain::iter().zip(channel_witnessing) {
						*deposit_channel_witnessing_enabled(&mut safe_mode, chain) = enabled;
					}
					for (chain, enabled) in ForeignChain::iter().zip(vault_witnessing) {
						*vault_deposit_witnessing_enabled(&mut safe_mode, chain) = enabled;
					}
					safe_mode.liquidity_provider.withdrawal_enabled = withdrawals;
					safe_mode.liquidity_provider.internal_swaps_enabled = internal_swaps;
					safe_mode.swapping.withdrawals_enabled = broker_withdrawals;
					safe_mode.funding.redeem_enabled = redemptions;
					safe_mode.lending_pools.borrowing = match borrowing {
						0 => SafeModeSet::Green,
						1 => SafeModeSet::Red,
						_ => SafeModeSet::Amber(
							Asset::all()
								.zip(assets_off)
								.filter_map(|(a, off)| off.then_some(a))
								.collect(),
						),
					};
					safe_mode
				},
			)
	}

	fn presets_strategy() -> impl Strategy<Value = BTreeSet<EmergencyPreset>> {
		vec(any::<bool>(), EmergencyPreset::all().count()).prop_map(|picks| {
			EmergencyPreset::all()
				.zip(picks)
				.filter_map(|(p, pick)| pick.then_some(p))
				.collect()
		})
	}

	proptest! {
		#[test]
		fn presets_only_switch_their_own_settings_off(
			before in safe_mode_strategy(),
			presets in presets_strategy(),
		) {
			let after = applied(&before, &presets);

			for preset in EmergencyPreset::all() {
				let expected_off = is_off(&before, preset) || presets.contains(&preset);
				prop_assert_eq!(is_off(&after, preset), expected_off, "{:?}", preset);
			}
			// Settings that no preset covers are untouched.
			prop_assert_eq!(
				with_preset_settings_reset(&after),
				with_preset_settings_reset(&before)
			);
			// Applying the same presets again changes nothing.
			prop_assert_eq!(&applied(&after, &presets), &after);
		}
	}
}
