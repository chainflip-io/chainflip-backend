use crate::{safe_mode, Runtime};
use cf_chains::{
	arb::ArbitrumTrackedData,
	eth::Address,
	instances::{BitcoinInstance, EthereumInstance, PolkadotInstance, SolanaInstance},
	ChainState,
};
use cf_traits::SafeMode;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::prelude::*;

pub struct RenameEthereumToEvmThresholdSigner;

impl frame_support::traits::OnRuntimeUpgrade for RenameEthereumToEvmThresholdSigner {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("🖋️ Renaming EthereumThresholdSigner to EvmThresholdSigner");
		frame_support::storage::migration::move_pallet(
			b"EthereumThresholdSigner",
			b"EvmThresholdSigner",
		);
		Weight::zero()
	}
}

mod old {
	use super::*;
	use cf_chains::instances::{
		BitcoinCryptoInstance, BitcoinInstance, EthereumInstance, EvmInstance,
		PolkadotCryptoInstance, PolkadotInstance,
	};
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
		pub threshold_signature_ethereum:
			pallet_cf_threshold_signature::PalletSafeMode<EvmInstance>,
		pub threshold_signature_bitcoin:
			pallet_cf_threshold_signature::PalletSafeMode<BitcoinCryptoInstance>,
		pub threshold_signature_polkadot:
			pallet_cf_threshold_signature::PalletSafeMode<PolkadotCryptoInstance>,
		pub broadcast_ethereum: pallet_cf_broadcast::PalletSafeMode<EthereumInstance>,
		pub broadcast_bitcoin: pallet_cf_broadcast::PalletSafeMode<BitcoinInstance>,
		pub broadcast_polkadot: pallet_cf_broadcast::PalletSafeMode<PolkadotInstance>,
		pub witnesser: pallet_cf_witnesser::PalletSafeMode<safe_mode::WitnesserCallPermission>,
	}
}
pub struct ArbitrumIntegration;

impl OnRuntimeUpgrade for ArbitrumIntegration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		use cf_chains::{arb, assets::arb::Asset::ArbUsdc, instances::ArbitrumInstance};
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
							threshold_signature_evm: old.threshold_signature_ethereum,
							threshold_signature_bitcoin: old.threshold_signature_bitcoin,
							threshold_signature_polkadot: old.threshold_signature_polkadot,
							threshold_signature_solana: <pallet_cf_threshold_signature::PalletSafeMode<
								SolanaInstance,
							> as SafeMode>::CODE_GREEN,
							broadcast_ethereum: old.broadcast_ethereum,
							broadcast_bitcoin: old.broadcast_bitcoin,
							broadcast_polkadot: old.broadcast_polkadot,
							broadcast_arbitrum: <pallet_cf_broadcast::PalletSafeMode<
								ArbitrumInstance,
							> as SafeMode>::CODE_GREEN,
							broadcast_solana: <pallet_cf_broadcast::PalletSafeMode<
								SolanaInstance,
							> as SafeMode>::CODE_GREEN,
							// Set safe mode on for ingress-egress to disable boost features.
							ingress_egress_ethereum: <pallet_cf_ingress_egress::PalletSafeMode<EthereumInstance> as SafeMode>::CODE_RED,
							ingress_egress_bitcoin: <pallet_cf_ingress_egress::PalletSafeMode<BitcoinInstance> as SafeMode>::CODE_RED,
							ingress_egress_polkadot: <pallet_cf_ingress_egress::PalletSafeMode<PolkadotInstance> as SafeMode>::CODE_RED,
							ingress_egress_arbitrum: <pallet_cf_ingress_egress::PalletSafeMode<ArbitrumInstance> as SafeMode>::CODE_RED,
							ingress_egress_solana: <pallet_cf_ingress_egress::PalletSafeMode<SolanaInstance> as SafeMode>::CODE_RED,
							witnesser: old.witnesser,
						}
				})
			},
		));

		let (
			key_manager_address,
			vault_address,
			address_checker_address,
			chain_id,
			usdc_address,
			start_block_number,
			deposit_channel_lifetime,
		): (Address, Address, Address, u64, Address, u64, u64) =
			match cf_runtime_upgrade_utilities::genesis_hashes::genesis_hash::<Runtime>() {
				cf_runtime_upgrade_utilities::genesis_hashes::BERGHAIN => {
					log::warn!("Need to set up arbitrum integration for Berghain");
					(
						[0u8; 20].into(),
						[0u8; 20].into(),
						[0u8; 20].into(),
						arb::CHAIN_ID_MAINNET,
						[0u8; 20].into(),
						0,
						// state-chain/node/src/chain_spec/berghain.rs
						24 * 3600 * 4,
					)
				},
				cf_runtime_upgrade_utilities::genesis_hashes::PERSEVERANCE => {
					log::warn!("Need to set up arbitrum integration for Perseverance");
					(
						[1u8; 20].into(),
						[1u8; 20].into(),
						[1u8; 20].into(),
						arb::CHAIN_ID_ARBITRUM_SEPOLIA,
						[1u8; 20].into(),
						0,
						// state-chain/node/src/chain_spec/perseverance.rs
						2 * 60 * 60 * 4,
					)
				},
				cf_runtime_upgrade_utilities::genesis_hashes::SISYPHOS => {
					log::warn!("Need to set up arbitrum integration for Sisyphos");
					(
						[2u8; 20].into(),
						[2u8; 20].into(),
						[2u8; 20].into(),
						arb::CHAIN_ID_ARBITRUM_SEPOLIA,
						[2u8; 20].into(),
						0,
						// state-chain/node/src/chain_spec/sisyphos.rs
						2 * 60 * 60 * 4,
					)
				},
				_ => {
					// Assume testnet
					(
						hex_literal::hex!("5FbDB2315678afecb367f032d93F642f64180aa3").into(),
						hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512").into(),
						hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0").into(),
						412346,
						hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9").into(),
						0,
						// state-chain/node/src/chain_spec/testnet.rs
						2 * 60 * 60 * 4,
					)
				},
			};

		pallet_cf_environment::ArbitrumKeyManagerAddress::<Runtime>::put(key_manager_address);
		pallet_cf_environment::ArbitrumVaultAddress::<Runtime>::put(vault_address);
		pallet_cf_environment::ArbitrumAddressCheckerAddress::<Runtime>::put(
			address_checker_address,
		);
		pallet_cf_environment::ArbitrumChainId::<Runtime>::put(chain_id);
		pallet_cf_environment::ArbitrumSupportedAssets::<Runtime>::insert(ArbUsdc, usdc_address);
		pallet_cf_chain_tracking::CurrentChainState::<Runtime, ArbitrumInstance>::put(ChainState {
			block_height: start_block_number,
			tracked_data: ArbitrumTrackedData { base_fee: 0, gas_limit_multiplier: 1.into() },
		});
		pallet_cf_ingress_egress::DepositChannelLifetime::<Runtime, ArbitrumInstance>::put(
			deposit_channel_lifetime,
		);
		pallet_cf_ingress_egress::WitnessSafetyMargin::<Runtime, ArbitrumInstance>::put(1);

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
