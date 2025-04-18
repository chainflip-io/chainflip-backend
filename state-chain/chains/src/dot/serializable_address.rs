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

use super::*;
use anyhow::anyhow;
use frame_support::sp_runtime::AccountId32;
use sp_core::crypto::Ss58Codec;

#[derive(Debug, Clone)]
pub struct SubstrateNetworkAddress {
	format_specifier: ss58_registry::Ss58AddressFormat,
	account_id: AccountId32,
}

impl FromStr for PolkadotAccountId {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		from_ss58check_with_version(s)
			.and_then(TryInto::try_into)
			.or_else(|base58_err| {
				cf_utilities::clean_hex_address::<PolkadotAccountId>(s).map_err(|hex_err| {
					anyhow!(
						"Address is neither valid ss58: '{}' nor hex: '{}'",
						base58_err,
						hex_err.root_cause()
					)
				})
			})
	}
}

impl SubstrateNetworkAddress {
	pub fn polkadot(account_id: impl Into<AccountId32>) -> Self {
		Self {
			format_specifier: ss58_registry::Ss58AddressFormatRegistry::PolkadotAccount.into(),
			account_id: account_id.into(),
		}
	}
}

impl serde::Serialize for SubstrateNetworkAddress {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		use sp_core::crypto::Ss58Codec;
		serializer.serialize_str(&self.account_id.to_ss58check_with_version(self.format_specifier))
	}
}

fn from_ss58check_with_version(
	s: &str,
) -> Result<SubstrateNetworkAddress, sp_core::crypto::PublicError> {
	<AccountId32 as Ss58Codec>::from_ss58check_with_version(s).map(
		|(account_id, format_specifier)| SubstrateNetworkAddress { format_specifier, account_id },
	)
}

impl<'de> serde::Deserialize<'de> for SubstrateNetworkAddress {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		from_ss58check_with_version(&String::deserialize(deserializer)?)
			.map_err(|_| serde::de::Error::custom("Invalid SS58 address"))
	}
}

impl TryFrom<SubstrateNetworkAddress> for PolkadotAccountId {
	type Error = sp_core::crypto::PublicError;

	fn try_from(substrate_address: SubstrateNetworkAddress) -> Result<Self, Self::Error> {
		if substrate_address.format_specifier ==
			ss58_registry::Ss58AddressFormatRegistry::PolkadotAccount.into()
		{
			Ok(Self::from_aliased(*substrate_address.account_id.as_ref()))
		} else {
			Err(sp_core::crypto::PublicError::FormatNotAllowed)
		}
	}
}

impl From<PolkadotAccountId> for SubstrateNetworkAddress {
	fn from(account_id: PolkadotAccountId) -> Self {
		Self::polkadot(account_id.0)
	}
}

impl std::fmt::Display for SubstrateNetworkAddress {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(f, "{}", self.account_id.to_ss58check_with_version(self.format_specifier))
	}
}
