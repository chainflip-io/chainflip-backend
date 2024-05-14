use crate::{safe_mode, Runtime};
use cf_chains::{
	instances::{
		ArbitrumInstance, BitcoinInstance, EthereumInstance, PolkadotInstance, SolanaInstance,
	},
	sol::SolTrackedData,
	ChainState,
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
		use cf_chains::sol::{SolAddress, SolHash};
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

		let (
			vault_address,
			vault_data_account_address,
			token_vault_address,
			token_vault_usdc_address,
			upgrade_manager_address,
			upgrade_manager_signer_seed,
			upgrade_manager_program_data_address,
			vault_program_data_address,
			nonce_accounts,
			usdc_address,
			vault_emit_event_address,
			genesis_hash,
			start_block_number,
			deposit_channel_lifetime,
		): (
			SolAddress,
			SolAddress,
			SolAddress,
			SolAddress,
			SolAddress,
			[u8; 6],
			SolAddress,
			SolAddress,
			Vec<(SolAddress, SolHash, bool)>,
			SolAddress,
			SolAddress,
			SolHash,
			u64,
			u64,
		) = match cf_runtime_upgrade_utilities::genesis_hashes::genesis_hash::<Runtime>() {
			// TODO: Continue here to finish the migration
			cf_runtime_upgrade_utilities::genesis_hashes::BERGHAIN => {
				(
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					[115, 105, 103, 110, 101, 114],
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					vec![], /* put correct values here */
					SolAddress(hex_literal::hex!(
						"c6fa7af3bedbad3a3d65f36aabc97431b1bbe4c2d2f6e0e47ca60203452f5d61"
					)),
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolHash(hex_literal::hex![
						"45296998a6f8e2a784db5d9f95e18fc23f70441a1039446801089879b08c7ef0"
					]),
					0, /* put correct values here */
					// // state-chain/node/src/chain_spec/berghain.rs
					24 * 3600 * 10 / 4,
				)
			},

			cf_runtime_upgrade_utilities::genesis_hashes::PERSEVERANCE => {
				(
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					[115, 105, 103, 110, 101, 114],
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					vec![], /* put correct values here */
					SolAddress(hex_literal::hex!(
						"3b442cb3912157f13a933d0134282d032b5ffecd01a2dbf1b7790608df002ea7"
					)),
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolHash(hex_literal::hex![
						"ce59db5080fc2c6d3bcf7ca90712d3c2e5e6c28f27f0dfbb9953bdb0894c03ab"
					]),
					0, /* put correct values here */
					// // state-chain/node/src/chain_spec/perseverance.rs
					2 * 60 * 60 * 10 / 4,
				)
			},
			cf_runtime_upgrade_utilities::genesis_hashes::SISYPHOS => {
				(
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					[115, 105, 103, 110, 101, 114],
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					vec![], /* put correct values here */
					SolAddress(hex_literal::hex!(
						"3b442cb3912157f13a933d0134282d032b5ffecd01a2dbf1b7790608df002ea7"
					)),
					SolAddress(hex_literal::hex!(
						"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
					)), /* put correct values here */
					SolHash(hex_literal::hex![
						"ce59db5080fc2c6d3bcf7ca90712d3c2e5e6c28f27f0dfbb9953bdb0894c03ab"
					]),
					0, /* put correct values here */
					// // state-chain/node/src/chain_spec/sisyphos.rs
					2 * 60 * 60 * 10 / 4,
				)
			},
			_ => {
				// Assume testnet
				(
					SolAddress(hex_literal::hex!(
						"72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c"
					)),
					SolAddress(hex_literal::hex!(
						"4a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b"
					)),
					SolAddress(hex_literal::hex!(
						"81a0052237ad76cb6e88fe505dc3d96bba6d8889f098b1eaa342ec8445880521"
					)),
					SolAddress(hex_literal::hex!(
						"b966a2b36557938f49cc5d00f8f12d86f16f48e03b63c8422967dba621ab60bf"
					)),
					SolAddress(hex_literal::hex!(
						"1068c72f83398081684c491910b66f8d8cca0edc00cbcf11c89f86c5c39d80f7"
					)),
					[115, 105, 103, 110, 101, 114],
					SolAddress(hex_literal::hex!(
						"a5cfec75730f8780ded36a7c8ae1dcc60d84e1a830765fc6108e7b40402e4951"
					)),
					SolAddress(hex_literal::hex!(
						"298f27f13ce155954657f0238e63932beb510964abd44e20e9603e6b6f2b424a"
					)),
					vec![], /* put correct values here */
					SolAddress(hex_literal::hex!(
						"0fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee87"
					)),
					SolAddress(hex_literal::hex!(
						"ae26080da692562cc5907d3f401b6c686f6d64f927065f9c1b32a1dc49d384b9"
					)),
					SolHash([0; 32]), /* put correct values here */
					0,                /* put correct values here */
					// // state-chain/node/src/chain_spec/testnet.rs
					2 * 60 * 60 * 10 / 4,
				)
			},
		};

		pallet_cf_environment::SolanaVaultAddress::<Runtime>::put(vault_address);
		pallet_cf_environment::SolanaVaultDataAccountAddress::<Runtime>::put(
			vault_data_account_address,
		);
		pallet_cf_environment::SolanaTokenVaultAddress::<Runtime>::put(token_vault_address);
		pallet_cf_environment::SolanaTokenVaultUsdcAddress::<Runtime>::put(
			token_vault_usdc_address,
		);
		pallet_cf_environment::SolanaUpgradeManagerAddress::<Runtime>::put(upgrade_manager_address);
		pallet_cf_environment::SolanaUpgradeManagerSignerSeed::<Runtime>::put(
			upgrade_manager_signer_seed,
		);
		pallet_cf_environment::SolanaUpgradeManagerProgramDataAddress::<Runtime>::put(
			upgrade_manager_program_data_address,
		);
		pallet_cf_environment::SolanaVaultProgramDataAddress::<Runtime>::put(
			vault_program_data_address,
		);
		pallet_cf_environment::SolanaNonceAccounts::<Runtime>::put(nonce_accounts);
		pallet_cf_environment::SolanaGenesisHash::<Runtime>::put(genesis_hash);
		// TODO: Add for token support
		// pallet_cf_environment::SolanaSupportedAssets::<Runtime>::insert(SolUsdc, usdc_address);
		pallet_cf_chain_tracking::CurrentChainState::<Runtime, SolanaInstance>::put(ChainState {
			block_height: start_block_number,
			tracked_data: SolTrackedData { priority_fee: 0u32.into() },
		});
		pallet_cf_environment::SolanaVaultEmitEventAddress::<Runtime>::put(
			vault_emit_event_address,
		);

		pallet_cf_ingress_egress::DepositChannelLifetime::<Runtime, SolanaInstance>::put(
			deposit_channel_lifetime,
		);
		// We shoudln't be using this at all
		pallet_cf_ingress_egress::WitnessSafetyMargin::<Runtime, SolanaInstance>::put(1);

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
