use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

use crate::address;

use super::consts::SOLANA_ADDRESS_SIZE;

#[derive(
	Default,
	Debug,
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	TypeInfo,
	Encode,
	Decode,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct SolAddress(#[serde(with = "::serde_bytes")] pub [u8; SOLANA_ADDRESS_SIZE]);

impl From<[u8; SOLANA_ADDRESS_SIZE]> for SolAddress {
	fn from(value: [u8; SOLANA_ADDRESS_SIZE]) -> Self {
		Self(value)
	}
}
impl From<SolAddress> for [u8; SOLANA_ADDRESS_SIZE] {
	fn from(value: SolAddress) -> Self {
		value.0
	}
}

impl TryFrom<address::ForeignChainAddress> for SolAddress {
	type Error = address::AddressError;
	fn try_from(value: address::ForeignChainAddress) -> Result<Self, Self::Error> {
		if let address::ForeignChainAddress::Sol(value) = value {
			Ok(value)
		} else {
			Err(address::AddressError::InvalidAddress)
		}
	}
}
impl From<SolAddress> for address::ForeignChainAddress {
	fn from(value: SolAddress) -> Self {
		address::ForeignChainAddress::Sol(value)
	}
}

impl core::str::FromStr for SolAddress {
	type Err = address::AddressError;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let bytes = base58::FromBase58::from_base58(s)
			.map_err(|_| address::AddressError::InvalidAddress)?;
		Ok(Self(bytes.try_into().map_err(|_| address::AddressError::InvalidAddress)?))
	}
}

impl core::fmt::Display for SolAddress {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(f, "{}", base58::ToBase58::to_base58(&self.0[..]))
	}
}

impl address::ToHumanreadableAddress for SolAddress {
	#[cfg(feature = "std")]
	type Humanreadable = String;

	#[cfg(feature = "std")]
	fn to_humanreadable(
		&self,
		_network_environment: cf_primitives::NetworkEnvironment,
	) -> Self::Humanreadable {
		self.to_string()
	}
}
