use cf_chains::address::{AddressConverter, EncodedAddress, ForeignChainAddress};

pub struct MockAddressConverter;
impl AddressConverter for MockAddressConverter {
	fn from_encoded_address(
		encoded_address: cf_chains::address::EncodedAddress,
	) -> Result<ForeignChainAddress, ()> {
		Ok(match encoded_address {
			EncodedAddress::Eth(_) => ForeignChainAddress::Eth(Default::default()),
			EncodedAddress::Dot(_) => ForeignChainAddress::Dot(Default::default()),
			EncodedAddress::Btc(_) => ForeignChainAddress::Btc(Default::default()),
		})
	}
	fn to_encoded_address(
		address: ForeignChainAddress,
	) -> Result<cf_chains::address::EncodedAddress, sp_runtime::DispatchError> {
		Ok(match address {
			ForeignChainAddress::Eth(_) => EncodedAddress::Eth(Default::default()),
			ForeignChainAddress::Dot(_) => EncodedAddress::Dot(Default::default()),
			ForeignChainAddress::Btc(_) => EncodedAddress::Btc(Default::default()),
		})
	}
}
