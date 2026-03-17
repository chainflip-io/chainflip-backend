use crate::Runtime;
use cf_chains::instances::TronInstance;
use cf_traits::SafeMode;
#[cfg(feature = "try-runtime")]
use codec::Encode;
use frame_support::{instances::*, traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub mod old {
	use crate::*;
	use frame_support::{instances::*, pallet_prelude::*};

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

	#[derive(
		Encode, Decode, DecodeWithMemTracking, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq,
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
		pub threshold_signature_evm: pallet_cf_threshold_signature::PalletSafeMode<Instance16>,
		pub threshold_signature_bitcoin: pallet_cf_threshold_signature::PalletSafeMode<Instance3>,
		pub threshold_signature_polkadot: pallet_cf_threshold_signature::PalletSafeMode<Instance15>,
		pub threshold_signature_solana: pallet_cf_threshold_signature::PalletSafeMode<Instance5>,
		pub broadcast_ethereum: pallet_cf_broadcast::PalletSafeMode<Instance1>,
		pub broadcast_bitcoin: pallet_cf_broadcast::PalletSafeMode<Instance3>,
		pub broadcast_polkadot: pallet_cf_broadcast::PalletSafeMode<Instance2>,
		pub broadcast_arbitrum: pallet_cf_broadcast::PalletSafeMode<Instance4>,
		pub broadcast_solana: pallet_cf_broadcast::PalletSafeMode<Instance5>,
		pub broadcast_assethub: pallet_cf_broadcast::PalletSafeMode<Instance6>,
		pub witnesser: pallet_cf_witnesser::PalletSafeMode<WitnesserCallPermission>,
		pub ingress_egress_ethereum: pallet_cf_ingress_egress::PalletSafeMode<Instance1>,
		pub ingress_egress_bitcoin: pallet_cf_ingress_egress::PalletSafeMode<Instance3>,
		pub ingress_egress_polkadot: pallet_cf_ingress_egress::PalletSafeMode<Instance2>,
		pub ingress_egress_arbitrum: pallet_cf_ingress_egress::PalletSafeMode<Instance4>,
		pub ingress_egress_solana: pallet_cf_ingress_egress::PalletSafeMode<Instance5>,
		pub ingress_egress_assethub: pallet_cf_ingress_egress::PalletSafeMode<Instance6>,
		pub elections_generic:
			crate::chainflip::witnessing::generic_elections::GenericElectionsSafeMode,
		pub ethereum_elections:
			crate::chainflip::witnessing::ethereum_elections::EthereumElectionsSafeMode,
		pub arbitrum_elections:
			crate::chainflip::witnessing::arbitrum_elections::ArbitrumElectionsSafeMode,
	}
}

/// Translate RuntimeSafeMode storage to add Tron-specific fields.
pub struct TronSafeModeUpdate;

impl OnRuntimeUpgrade for TronSafeModeUpdate {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🔧 Migrating RuntimeSafeMode to add Tron fields...");

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
									// Tron fields default to true (allowed)
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
						broadcast_tron: <pallet_cf_broadcast::PalletSafeMode<Instance7> as SafeMode>::code_green(),
						witnesser,
						ingress_egress_ethereum: old.ingress_egress_ethereum,
						ingress_egress_bitcoin: old.ingress_egress_bitcoin,
						ingress_egress_polkadot: old.ingress_egress_polkadot,
						ingress_egress_arbitrum: old.ingress_egress_arbitrum,
						ingress_egress_solana: old.ingress_egress_solana,
						ingress_egress_assethub: old.ingress_egress_assethub,
						ingress_egress_tron: <pallet_cf_ingress_egress::PalletSafeMode<Instance7> as SafeMode>::code_green(),
						elections_generic: old.elections_generic,
						ethereum_elections: old.ethereum_elections,
						arbitrum_elections: old.arbitrum_elections,
						tron_elections: <crate::chainflip::witnessing::tron_elections::TronElectionsSafeMode as SafeMode>::code_green(),
					}
				})
			},
		).map_err(|_| {
			log::warn!("⚠️ RuntimeSafeMode migration could not decode the existing storage!");
		});

		log::info!("🔧 RuntimeSafeMode migration completed.");
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let old = pallet_cf_environment::RuntimeSafeMode::<Runtime>::get();
		Ok(old.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use codec::Decode;
		let old = old::RuntimeSafeMode::decode(&mut _state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode old state"))?;
		let new = pallet_cf_environment::RuntimeSafeMode::<Runtime>::get();

		// Verify existing fields are preserved
		frame_support::ensure!(
			new.emissions == old.emissions,
			"emissions safe mode changed unexpectedly"
		);
		frame_support::ensure!(
			new.broadcast_ethereum == old.broadcast_ethereum,
			"broadcast_ethereum changed unexpectedly"
		);

		// Verify new Tron fields are CODE_GREEN
		frame_support::ensure!(
			new.broadcast_tron ==
				<pallet_cf_broadcast::PalletSafeMode<Instance7> as SafeMode>::code_green(),
			"broadcast_tron should be CODE_GREEN"
		);
		frame_support::ensure!(
			new.ingress_egress_tron ==
				<pallet_cf_ingress_egress::PalletSafeMode<Instance7> as SafeMode>::code_green(),
			"ingress_egress_tron should be CODE_GREEN"
		);

		Ok(())
	}
}

/// Initialize TronElections pallet so the engine recognises validators as authorities.
pub struct TronElectionsInit;

impl OnRuntimeUpgrade for TronElectionsInit {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(().encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let result =
			pallet_cf_elections::Pallet::<Runtime, crate::TronInstance>::internally_initialize(
				crate::chainflip::witnessing::tron_elections::initial_state(),
			);
		if result.is_err() {
			log::error!("Failed to initialize Tron election pallet");
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use crate::chainflip::witnessing::tron_elections::TRON_MAINNET_SAFETY_BUFFER;
		use pallet_cf_elections::{
			electoral_systems::{
				block_height_witnesser::BlockHeightWitnesserSettings,
				block_witnesser::state_machine::BlockWitnesserSettings,
			},
			ElectoralUnsynchronisedSettings, SharedDataReferenceLifetime,
		};

		let unsynchronized_settings =
			ElectoralUnsynchronisedSettings::<Runtime, crate::TronInstance>::get();
		assert_eq!(
			unsynchronized_settings,
			Some((
				BlockHeightWitnesserSettings { safety_buffer: TRON_MAINNET_SAFETY_BUFFER },
				BlockWitnesserSettings {
					max_ongoing_elections: 15,
					max_optimistic_elections: 1,
					safety_margin: 1,
					safety_buffer: TRON_MAINNET_SAFETY_BUFFER,
				},
				BlockWitnesserSettings {
					max_ongoing_elections: 15,
					max_optimistic_elections: 1,
					safety_margin: 1,
					safety_buffer: TRON_MAINNET_SAFETY_BUFFER,
				},
				BlockWitnesserSettings {
					max_ongoing_elections: 15,
					max_optimistic_elections: 1,
					safety_margin: 1,
					safety_buffer: TRON_MAINNET_SAFETY_BUFFER,
				},
				(),
			))
		);

		let lifetime = SharedDataReferenceLifetime::<Runtime, crate::TronInstance>::get();
		assert_eq!(lifetime, 8);

		Ok(())
	}
}

/// Initialize Tron ingress-egress pallet values (deposit channel lifetime).
/// These are normally set via GenesisConfig but must be set via migration when adding a new chain.
/// Note: WitnessSafetyMargin is deprecated for chains using elections-based witnessing (see
/// comment on WitnessSafetyMargin storage item), so we only set the channel lifetime here.
pub struct TronIngressEgressInit;

impl OnRuntimeUpgrade for TronIngressEgressInit {
	fn on_runtime_upgrade() -> Weight {
		use cf_runtime_utilities::genesis_hashes;

		// Values from each chain_spec (node/src/chain_spec/{berghain,testnet,devnet}.rs).
		let deposit_channel_lifetime: u64 = match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => 24 * 3600 / 3,
			genesis_hashes::PERSEVERANCE | genesis_hashes::SISYPHOS => 2 * 60 * 60 / 3,
			_ => 10 * 60 * 2,
		};

		log::info!(
			"🔧 Initializing Tron ingress-egress: deposit_channel_lifetime={}",
			deposit_channel_lifetime,
		);

		pallet_cf_ingress_egress::DepositChannelLifetime::<Runtime, TronInstance>::put(
			deposit_channel_lifetime,
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let lifetime =
			pallet_cf_ingress_egress::DepositChannelLifetime::<Runtime, TronInstance>::get();
		frame_support::ensure!(lifetime > 0, "Tron deposit channel lifetime must be non-zero");

		Ok(())
	}
}

/// Initialize TronChainTracking with initial chain state.
pub struct TronChainstate;

impl OnRuntimeUpgrade for TronChainstate {
	fn on_runtime_upgrade() -> Weight {
		if pallet_cf_chain_tracking::CurrentChainState::<Runtime, TronInstance>::get().is_none() {
			log::info!("🔧 Initializing TronChainTracking with block_height 0...");
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, TronInstance>::put(
				cf_chains::ChainState {
					block_height: 0u64,
					tracked_data: cf_chains::tron::TronTrackedData::new(),
				},
			);
		}
		Weight::zero()
	}
}
