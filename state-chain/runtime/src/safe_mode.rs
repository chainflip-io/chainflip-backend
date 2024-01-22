//! For filtering runtime calls and other related utilities.

use crate::{BitcoinInstance, EthereumInstance, PolkadotInstance, Runtime, RuntimeCall};
use cf_traits::{impl_runtime_safe_mode, CallDispatchFilter};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

impl_runtime_safe_mode! {
	RuntimeSafeMode,
	pallet_cf_environment::RuntimeSafeMode<Runtime>,
	emissions: pallet_cf_emissions::PalletSafeMode,
	funding: pallet_cf_funding::PalletSafeMode,
	swapping: pallet_cf_swapping::PalletSafeMode,
	liquidity_provider: pallet_cf_lp::PalletSafeMode,
	validator: pallet_cf_validator::PalletSafeMode,
	pools: pallet_cf_pools::PalletSafeMode,
	reputation: pallet_cf_reputation::PalletSafeMode,
	vault_ethereum: pallet_cf_vaults::PalletSafeMode<EthereumInstance>,
	vault_bitcoin: pallet_cf_vaults::PalletSafeMode<BitcoinInstance>,
	vault_polkadot: pallet_cf_vaults::PalletSafeMode<PolkadotInstance>,
	broadcast_ethereum: pallet_cf_broadcast::PalletSafeMode<EthereumInstance>,
	broadcast_bitcoin: pallet_cf_broadcast::PalletSafeMode<BitcoinInstance>,
	broadcast_polkadot: pallet_cf_broadcast::PalletSafeMode<PolkadotInstance>,
	witnesser: pallet_cf_witnesser::PalletSafeMode<WitnesserCallPermission>,
}

/// Contains permissions for different Runtime calls.
/// This is done through the SafeMode::CodeAmber of the Witnesser pallet.
/// Only calls allowed here can be dispatched with Witnesser origin.
#[derive(
	Encode,
	Decode,
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

			_ => {
				cf_runtime_utilities::log_or_panic!(
					"All witnesser calls must be controllable through `WitnesserCallPermission`. Call: {:?}",
					call
				);
				#[allow(unreachable_code)]
				false
			},
		}
	}
}
