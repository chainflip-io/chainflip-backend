use crate::eth;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Supported assets
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub enum Asset {
	EthEth,
}

/// Address types for assets
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub enum AssetAddress {
	ETH(eth::Address),
}

/// Something that can derive an ingress address
pub trait AddressDerivation {
	type AddressType;
	/// Generates an ingress address
	fn generate_address(
		asset: Asset,
		vault_address: Self::AddressType,
		intent_id: u32,
	) -> Self::AddressType;
}
