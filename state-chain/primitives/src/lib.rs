//! Chainflip Primitives
//!
//! Primitive types to be used across Chainflip's various crates

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub enum Chain {
	Eth,
	Dot,
}

/// These assets can exist on several chains
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub enum Asset {
	Eth,
	Usdc,
	Flip,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub struct ChainAsset {
	chain: Chain,
	asset: Asset,
}
