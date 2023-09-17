//! For filtering runtime calls and other related utilities.

use crate::{Runtime, RuntimeCall};
use cf_traits::{impl_runtime_safe_mode, CallDispatchFilter};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::Get;
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
	vault: pallet_cf_vaults::PalletSafeMode,
	witnesser: pallet_cf_witnesser::PalletSafeMode<WitnesserCallPermission>,
	broadcast: pallet_cf_broadcast::PalletSafeMode,
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
	frame_support::RuntimeDebug,
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

pub struct ChainflipCallFilter;
impl CallDispatchFilter<RuntimeCall> for ChainflipCallFilter {
	fn should_dispatch(call: &RuntimeCall) -> bool {
		match <RuntimeSafeMode as Get<
			pallet_cf_witnesser::PalletSafeMode<WitnesserCallPermission>,
		>>::get()
		{
			pallet_cf_witnesser::PalletSafeMode::CodeGreen => true,
			pallet_cf_witnesser::PalletSafeMode::CodeRed => false,
			pallet_cf_witnesser::PalletSafeMode::CodeAmber(permission) => match call {
				RuntimeCall::Governance(..) => permission.governance,
				RuntimeCall::Funding(..) => permission.funding,
				RuntimeCall::Swapping(..) => permission.swapping,

				RuntimeCall::EthereumBroadcaster(..) => permission.ethereum_broadcast,
				RuntimeCall::EthereumChainTracking(..) => permission.ethereum_chain_tracking,
				RuntimeCall::EthereumIngressEgress(..) => permission.ethereum_ingress_egress,
				RuntimeCall::EthereumVault(..) => permission.ethereum_vault,

				RuntimeCall::PolkadotBroadcaster(..) => permission.polkadot_broadcast,
				RuntimeCall::PolkadotChainTracking(..) => permission.polkadot_chain_tracking,
				RuntimeCall::PolkadotIngressEgress(..) => permission.polkadot_ingress_egress,
				RuntimeCall::PolkadotVault(..) => permission.polkadot_vault,

				RuntimeCall::BitcoinBroadcaster(..) => permission.bitcoin_broadcast,
				RuntimeCall::BitcoinChainTracking(..) => permission.bitcoin_chain_tracking,
				RuntimeCall::BitcoinIngressEgress(..) => permission.bitcoin_ingress_egress,
				RuntimeCall::BitcoinVault(..) => permission.bitcoin_vault,

				_ => {
					log::warn!("All witnesser calls must be controllable through `WitnesserCallPermission` during SafeMode: CodeAmber. Call: {:?}", call);
					false
				},
			},
		}
	}
}
