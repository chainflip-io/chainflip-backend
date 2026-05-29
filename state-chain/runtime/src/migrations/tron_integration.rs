use crate::{chainflip::witnessing::tron_elections::TRON_MAINNET_SAFETY_MARGIN, Runtime};
use cf_chains::instances::{EthereumInstance, TronInstance};
#[cfg(feature = "try-runtime")]
use codec::Encode;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

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
		use crate::chainflip::witnessing::tron_elections::{
			TRON_MAINNET_SAFETY_BUFFER, TRON_MAINNET_SAFETY_MARGIN,
		};
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
					safety_margin: TRON_MAINNET_SAFETY_MARGIN,
					safety_buffer: TRON_MAINNET_SAFETY_BUFFER,
				},
				BlockWitnesserSettings {
					max_ongoing_elections: 15,
					max_optimistic_elections: 1,
					safety_margin: TRON_MAINNET_SAFETY_MARGIN,
					safety_buffer: TRON_MAINNET_SAFETY_BUFFER,
				},
				BlockWitnesserSettings {
					max_ongoing_elections: 15,
					max_optimistic_elections: 1,
					safety_margin: TRON_MAINNET_SAFETY_MARGIN,
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
/// This sets both the channel lifetime and the witness safety margin.
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
			"🔧 Initializing Tron ingress-egress: deposit_channel_lifetime={}, ingress_safety_margin={}",
			deposit_channel_lifetime,
			TRON_MAINNET_SAFETY_MARGIN,
		);

		pallet_cf_ingress_egress::DepositChannelLifetime::<Runtime, TronInstance>::put(
			deposit_channel_lifetime,
		);

		pallet_cf_ingress_egress::WitnessSafetyMargin::<Runtime, TronInstance>::put(
			TRON_MAINNET_SAFETY_MARGIN as u64,
		);

		for id in
			pallet_cf_ingress_egress::WhitelistedBrokers::<Runtime, EthereumInstance>::iter_keys()
		{
			pallet_cf_ingress_egress::WhitelistedBrokers::<Runtime, TronInstance>::insert(id, ());
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let lifetime =
			pallet_cf_ingress_egress::DepositChannelLifetime::<Runtime, TronInstance>::get();
		frame_support::ensure!(lifetime > 0, "Tron deposit channel lifetime must be non-zero");

		let safety_margin =
			pallet_cf_ingress_egress::WitnessSafetyMargin::<Runtime, TronInstance>::get();
		frame_support::ensure!(
			safety_margin == Some(TRON_MAINNET_SAFETY_MARGIN as u64),
			"Tron safety margin not set correctly during migration"
		);
		frame_support::ensure!(
			pallet_cf_ingress_egress::WhitelistedBrokers::<Runtime, TronInstance>::iter_keys()
				.count() > 0,
			"Tron whitelisted brokers not migrated correctly"
		);

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
