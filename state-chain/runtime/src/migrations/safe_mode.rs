use cf_traits::SafeMode;
use frame_support::{traits::UncheckedOnRuntimeUpgrade, weights::Weight};

use crate::Runtime;

pub struct SafeModeMigration;

mod old {
	use crate::safe_mode::WitnesserCallPermission;
	use codec::{Decode, Encode, MaxEncodedLen};
	use frame_support::instances::*;
	use scale_info::TypeInfo;
	use sp_core::RuntimeDebug;

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct IngressEgressSafeMode {
		pub boost_deposits_enabled: bool,
		pub add_boost_funds_enabled: bool, // to be removed
		pub stop_boosting_enabled: bool,   // to be removed
		pub deposits_enabled: bool,
	}

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct RuntimeSafeMode {
		pub emissions: pallet_cf_emissions::PalletSafeMode,
		pub funding: pallet_cf_funding::PalletSafeMode,
		pub swapping: pallet_cf_swapping::PalletSafeMode,
		pub liquidity_provider: pallet_cf_lp::PalletSafeMode,
		pub validator: pallet_cf_validator::PalletSafeMode,
		pub pools: pallet_cf_pools::PalletSafeMode,
		pub trading_strategies: pallet_cf_trading_strategy::PalletSafeMode,
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
		pub ingress_egress_ethereum: IngressEgressSafeMode,
		pub ingress_egress_bitcoin: IngressEgressSafeMode,
		pub ingress_egress_polkadot: IngressEgressSafeMode,
		pub ingress_egress_arbitrum: IngressEgressSafeMode,
		pub ingress_egress_solana: IngressEgressSafeMode,
		pub ingress_egress_assethub: IngressEgressSafeMode,
	}
}

fn migrate_ingress_egress_safe_mode<I: 'static>(
	old: old::IngressEgressSafeMode,
) -> pallet_cf_ingress_egress::PalletSafeMode<I> {
	// Creating a default one first to get around _phantom being private
	let mut safe_mode = pallet_cf_ingress_egress::PalletSafeMode::CODE_GREEN;
	safe_mode.boost_deposits_enabled = old.boost_deposits_enabled;
	safe_mode.deposit_channel_witnessing_enabled = old.deposits_enabled;
	safe_mode.vault_deposit_witnessing_enabled = old.deposits_enabled;
	safe_mode.deposit_channel_creation_enabled = old.deposits_enabled;
	safe_mode
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
					ingress_egress_ethereum: migrate_ingress_egress_safe_mode(old.ingress_egress_ethereum),
					ingress_egress_bitcoin: migrate_ingress_egress_safe_mode(old.ingress_egress_bitcoin),
					ingress_egress_polkadot: migrate_ingress_egress_safe_mode(old.ingress_egress_polkadot),
					ingress_egress_arbitrum: migrate_ingress_egress_safe_mode(old.ingress_egress_arbitrum),
					ingress_egress_solana: migrate_ingress_egress_safe_mode(old.ingress_egress_solana),
					ingress_egress_assethub: migrate_ingress_egress_safe_mode(old.ingress_egress_assethub),
					lending_pools: pallet_cf_lending_pools::PalletSafeMode::CODE_GREEN,
				})
			},
		).map_err(|_| {
			log::warn!("Migration for Runtime Safe mode was not able to interpret the existing storage in the old format!")
		});

		Weight::zero()
	}
}
