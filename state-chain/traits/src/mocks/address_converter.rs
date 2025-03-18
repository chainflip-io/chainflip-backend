// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

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
