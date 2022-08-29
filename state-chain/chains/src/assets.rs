use crate::{eth, IngressAddress};
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

impl IngressAddress for Asset {
	type AddressType = AssetAddress;
	fn derive_address(self, vault_key: Self::AddressType, intent_id: u32) -> Self::AddressType {
		match self {
			Asset::EthEth => todo!(),
		}
	}
}
