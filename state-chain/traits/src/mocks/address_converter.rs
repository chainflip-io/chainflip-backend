use cf_chains::{
	address::{
		to_encoded_address, try_from_encoded_address, AddressConverter, ForeignChainAddress,
	},
	btc::BitcoinNetwork,
};

pub struct MockAddressConverter;
impl AddressConverter for MockAddressConverter {
	fn try_from_encoded_address(
		encoded_address: cf_chains::address::EncodedAddress,
	) -> Result<ForeignChainAddress, ()> {
		try_from_encoded_address(encoded_address, || BitcoinNetwork::Mainnet)
	}
	fn to_encoded_address(address: ForeignChainAddress) -> cf_chains::address::EncodedAddress {
		to_encoded_address(address, || BitcoinNetwork::Mainnet)
	}
}
