/// cf-rpc-types module defines all RPC related types
/// Common types are defined in here
use cf_chains::{address::ToHumanreadableAddress, Chain};
use cf_primitives::{Asset, FlipBalance};
use frame_support::{Deserialize, Serialize};

/// Defines all broker related RPC types
pub mod broker;
/// Defines all LP related RPC types
pub mod lp;

pub type RedemptionAmount = pallet_cf_funding::RedemptionAmount<FlipBalance>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapChannelInfo<C: Chain> {
	pub deposit_address: <C::ChainAccount as ToHumanreadableAddress>::Humanreadable,
	pub source_asset: Asset,
	pub destination_asset: Asset,
}
