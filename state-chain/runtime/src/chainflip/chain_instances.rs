//! Instance aliases for per-chain instantiable pallets in the chainflip runtime.
//!
//! Note these are a convenience to ensure we use the correct instances in definitions that use the
//! generated pallet components. They don't work inside the `construct_runtime!` macro itself.

use frame_support::instances::*;

/// Allows a type to be used as an alias for a pallet `Instance`.
pub trait PalletInstanceAlias {
	type Instance: Send + Sync + 'static;
}

impl PalletInstanceAlias for cf_chains::eth::Ethereum {
	type Instance = Instance1;
}

pub type EthereumInstance = <cf_chains::eth::Ethereum as PalletInstanceAlias>::Instance;

impl PalletInstanceAlias for cf_chains::dot::Polkadot {
	type Instance = Instance2;
}

pub type PolkadotInstance = <cf_chains::dot::Polkadot as PalletInstanceAlias>::Instance;

impl PalletInstanceAlias for cf_chains::btc::Bitcoin {
	type Instance = Instance3;
}

pub type BitcoinInstance = <cf_chains::btc::Bitcoin as PalletInstanceAlias>::Instance;
