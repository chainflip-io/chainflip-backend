use cf_chains::address::{
	decode_and_validate_address_for_asset, to_encoded_address, try_from_encoded_address,
	AddressConverter, AddressError, EncodedAddress, ForeignChainAddress,
};
use cf_primitives::{Asset, NetworkEnvironment};

pub struct MockAddressConverter;
impl AddressConverter for MockAddressConverter {
	fn try_from_encoded_address(
		encoded_address: EncodedAddress,
	) -> Result<ForeignChainAddress, ()> {
		try_from_encoded_address(encoded_address, || NetworkEnvironment::Mainnet)
	}
	fn to_encoded_address(address: ForeignChainAddress) -> EncodedAddress {
		to_encoded_address(address, || NetworkEnvironment::Mainnet)
	}
	fn decode_and_validate_address_for_asset(
		address: EncodedAddress,
		asset: Asset,
	) -> Result<ForeignChainAddress, AddressError> {
		decode_and_validate_address_for_asset(address, asset, || NetworkEnvironment::Mainnet)
	}
}
