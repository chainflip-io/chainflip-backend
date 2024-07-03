use crate::{safe_mode, Runtime};
use cf_chains::{
	instances::{
		ArbitrumInstance, BitcoinInstance, EthereumInstance, PolkadotInstance, SolanaInstance,
	},
	sol::{api::DurableNonceAndAccount, SolHash},
};
use cf_traits::SafeMode;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sol_prim::consts::{const_address, const_hash};
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
		use cf_chains::{assets::sol::Asset::SolUsdc, sol::SolAddress};
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

		let (vault_address, genesis_hash, usdc_address, durable_nonces_and_accounts): (
			SolAddress,
			Option<SolHash>,
			SolAddress,
			Vec<DurableNonceAndAccount>,
		) = match cf_runtime_upgrade_utilities::genesis_hashes::genesis_hash::<Runtime>() {
			cf_runtime_upgrade_utilities::genesis_hashes::BERGHAIN => {
				log::error!("Need to set up Solana integration for Berghain");
				(
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					Some(SolHash(hex_literal::hex![
						"45296998a6f8e2a784db5d9f95e18fc23f70441a1039446801089879b08c7ef0"
					])),
					SolAddress(hex_literal::hex!(
						"c6fa7af3bedbad3a3d65f36aabc97431b1bbe4c2d2f6e0e47ca60203452f5d61"
					)),
					vec![(
						SolAddress(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
						SolHash(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
					)],
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
					SolAddress(hex_literal::hex!(
						"3b442cb3912157f13a933d0134282d032b5ffecd01a2dbf1b7790608df002ea7"
					)),
					vec![(
						SolAddress(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
						SolHash(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
					)],
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
					SolAddress(hex_literal::hex!(
						"3b442cb3912157f13a933d0134282d032b5ffecd01a2dbf1b7790608df002ea7"
					)),
					vec![(
						SolAddress(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
						SolHash(hex_literal::hex!(
							"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
						)),
					)],
				)
			},
			_ => {
				// Assume testnet
				(
					const_address("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf"),
					None,
					const_address("24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p"),
					vec![
						(
							const_address("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw"),
							const_hash("8PUq9wFfALkRq4G4f2SNWERhN93pA2GfFzZpvMAFL43Y"),
						),
						(
							const_address("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo"),
							const_hash("5P3UrY376M2wVe7PuTSFpqnbHQSqpVsyqS2VogUqQZJ"),
						),
						(
							const_address("HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p"),
							const_hash("2jUwmSErAu7DR6Hd3MJgDWrTEvbbbcmqh8cNwu8qxe8X"),
						),
						(
							const_address("HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2"),
							const_hash("GhzGACyEghEb1Pb8JMhKmGn7fmXoH8BKC8156P13XiCt"),
						),
						(
							const_address("GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM"),
							const_hash("HTZzc4YWgD9vxj3a1xsBtC9xaLxrUYEH7qr6fygoqbbc"),
						),
						(
							const_address("EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn"),
							const_hash("4DNnxKKdUkVpaZiAB7bqFA2SPkaGcTE9bvgD3zYiHiu3"),
						),
						(
							const_address("9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa"),
							const_hash("GgjtavVDxo4t5DywJPENe5aNb8U9LjHDU2qKEd3FQBRv"),
						),
					],
				)
			},
		};

		pallet_cf_environment::SolanaVaultAddress::<Runtime>::put(vault_address);
		pallet_cf_environment::SolanaGenesisHash::<Runtime>::set(genesis_hash);
		pallet_cf_environment::SolanaSupportedAssets::<Runtime>::insert(SolUsdc, usdc_address);
		pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::set(
			durable_nonces_and_accounts,
		);

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
