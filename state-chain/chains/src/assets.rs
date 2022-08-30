use crate::eth;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub enum Asset {
	EthEth,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub enum AssetAddress {
	ETH(eth::Address),
}

pub trait AddressDerivation {
	type AddressType;
	fn generate_address(
		asset: Asset,
		vault_address: Self::AddressType,
		intent_id: u32,
	) -> Self::AddressType;
}
