use cf_chains::{
	address::{
		try_from_encoded_address, try_to_encoded_address, AddressConverter, ForeignChainAddress,
	},
	btc::BitcoinNetwork,
};

pub struct MockAddressConverter;
impl AddressConverter for MockAddressConverter {
	fn try_from_encoded_address(
		encoded_address: cf_chains::address::EncodedAddress,
	) -> Result<ForeignChainAddress, ()> {
		try_from_encoded_address(encoded_address, BitcoinNetwork::Mainnet)
	}
	fn try_to_encoded_address(
		address: ForeignChainAddress,
	) -> Result<cf_chains::address::EncodedAddress, sp_runtime::DispatchError> {
		try_to_encoded_address(address, BitcoinNetwork::Mainnet)
	}
}
