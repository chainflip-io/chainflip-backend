use crate::{safe_mode, Runtime};
use cf_chains::{
	instances::{
		ArbitrumInstance, BitcoinInstance, EthereumInstance, PolkadotInstance, SolanaInstance,
	},
	sol::SolHash,
};
use cf_traits::SafeMode;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::{vec, vec::Vec};

pub mod old {
	use super::*;
	use cf_chains::instances::{BitcoinCryptoInstance, EvmInstance, PolkadotCryptoInstance};
	use frame_support::pallet_prelude::*;

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct RuntimeSafeMode {
		pub emissions: pallet_cf_emissions::PalletSafeMode,
		pub funding: pallet_cf_funding::PalletSafeMode,
		pub swapping: pallet_cf_swapping::PalletSafeMode,
		pub liquidity_provider: pallet_cf_lp::PalletSafeMode,
		pub validator: pallet_cf_validator::PalletSafeMode,
		pub pools: pallet_cf_pools::PalletSafeMode,
		pub reputation: pallet_cf_reputation::PalletSafeMode,
		pub threshold_signature_evm: pallet_cf_threshold_signature::PalletSafeMode<EvmInstance>,
		pub threshold_signature_bitcoin:
			pallet_cf_threshold_signature::PalletSafeMode<BitcoinCryptoInstance>,
		pub threshold_signature_polkadot:
			pallet_cf_threshold_signature::PalletSafeMode<PolkadotCryptoInstance>,
		pub broadcast_ethereum: pallet_cf_broadcast::PalletSafeMode<EthereumInstance>,
		pub broadcast_bitcoin: pallet_cf_broadcast::PalletSafeMode<BitcoinInstance>,
		pub broadcast_polkadot: pallet_cf_broadcast::PalletSafeMode<PolkadotInstance>,
		pub broadcast_arbitrum: pallet_cf_broadcast::PalletSafeMode<ArbitrumInstance>,
		pub ingress_egress_ethereum: pallet_cf_ingress_egress::PalletSafeMode<EthereumInstance>,
		pub ingress_egress_bitcoin: pallet_cf_ingress_egress::PalletSafeMode<BitcoinInstance>,
		pub ingress_egress_polkadot: pallet_cf_ingress_egress::PalletSafeMode<PolkadotInstance>,
		pub ingress_egress_arbitrum: pallet_cf_ingress_egress::PalletSafeMode<ArbitrumInstance>,
		pub witnesser: pallet_cf_witnesser::PalletSafeMode<safe_mode::WitnesserCallPermission>,
	}
}

pub struct SolanaIntegration;

impl OnRuntimeUpgrade for SolanaIntegration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		use cf_chains::sol::SolAddress;
		use frame_support::assert_ok;

		assert_ok!(pallet_cf_environment::RuntimeSafeMode::<Runtime>::translate(
			|maybe_old: Option<old::RuntimeSafeMode>| {
				maybe_old.map(|old| {
					safe_mode::RuntimeSafeMode {
							emissions: old.emissions,
							funding: old.funding,
							swapping: old.swapping,
							liquidity_provider: old.liquidity_provider,
							validator: old.validator,
							pools: old.pools,
							reputation: old.reputation,
							threshold_signature_evm: old.threshold_signature_evm,
							threshold_signature_bitcoin: old.threshold_signature_bitcoin,
							threshold_signature_polkadot: old.threshold_signature_polkadot,
							threshold_signature_solana: <pallet_cf_threshold_signature::PalletSafeMode<
								SolanaInstance,
							> as SafeMode>::CODE_GREEN,
							broadcast_ethereum: old.broadcast_ethereum,
							broadcast_bitcoin: old.broadcast_bitcoin,
							broadcast_polkadot: old.broadcast_polkadot,
							broadcast_arbitrum: old.broadcast_arbitrum,
							broadcast_solana: <pallet_cf_broadcast::PalletSafeMode<
								SolanaInstance,
							> as SafeMode>::CODE_GREEN,
							// Set safe mode on for ingress-egress to disable boost features.
							ingress_egress_ethereum: old.ingress_egress_ethereum,
							ingress_egress_bitcoin: old.ingress_egress_bitcoin,
							ingress_egress_polkadot: old.ingress_egress_polkadot,
							ingress_egress_arbitrum: old.ingress_egress_arbitrum,
							ingress_egress_solana: <pallet_cf_ingress_egress::PalletSafeMode<SolanaInstance> as SafeMode>::CODE_RED,
							witnesser: old.witnesser,
						}
				})
			},
		));

		let (vault_address, genesis_hash): (SolAddress, Option<SolHash>) =
			match cf_runtime_upgrade_utilities::genesis_hashes::genesis_hash::<Runtime>() {
				cf_runtime_upgrade_utilities::genesis_hashes::BERGHAIN => {
					log::warn!("Need to set up Solana integration for Berghain");
					(
						SolAddress(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)), /* put correct values here */
						Some(SolHash(hex_literal::hex![
							"45296998a6f8e2a784db5d9f95e18fc23f70441a1039446801089879b08c7ef0"
						])),
					)
				},

				cf_runtime_upgrade_utilities::genesis_hashes::PERSEVERANCE => {
					log::warn!("Need to set up Solana integration for Perseverance");
					(
						SolAddress(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)), /* put correct values here */
						Some(SolHash(hex_literal::hex![
							"ce59db5080fc2c6d3bcf7ca90712d3c2e5e6c28f27f0dfbb9953bdb0894c03ab"
						])),
					)
				},
				cf_runtime_upgrade_utilities::genesis_hashes::SISYPHOS => {
					log::warn!("Need to set up Solana integration for Sisyphos");
					(
						SolAddress(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)), /* put correct values here */
						Some(SolHash(hex_literal::hex![
							"ce59db5080fc2c6d3bcf7ca90712d3c2e5e6c28f27f0dfbb9953bdb0894c03ab"
						])),
					)
				},
				_ => {
					// Assume testnet
					(
						SolAddress(hex_literal::hex!(
							"72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c"
						)),
						None,
					)
				},
			};

		pallet_cf_environment::SolanaVaultAddress::<Runtime>::put(vault_address);
		pallet_cf_environment::SolanaGenesisHash::<Runtime>::put(genesis_hash);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
