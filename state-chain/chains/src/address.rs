use cf_primitives::{EthereumAddress, ForeignChain, PolkadotAccountId};

extern crate alloc;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::H160;

use sp_runtime::DispatchError;
use sp_std::vec::Vec;

use crate::btc::BitcoinScriptBounded;

pub type ScriptPubkeyBytes = Vec<u8>;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ForeignChainAddress {
	Eth(EthereumAddress),
	Dot([u8; 32]),
	Btc(BitcoinScriptBounded),
}
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, PartialOrd, Ord)]
pub enum EncodedAddress {
	Eth(Vec<u8>),
	Dot(Vec<u8>),
	Btc(Vec<u8>),
}

pub trait AddressConverter: Sized {
	fn to_encoded_address(address: ForeignChainAddress) -> Result<EncodedAddress, DispatchError>;
	#[allow(clippy::result_unit_err)]
	fn from_encoded_address(encoded_address: EncodedAddress) -> Result<ForeignChainAddress, ()>;
}

#[cfg(feature = "std")]
impl core::fmt::Display for EncodedAddress {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			EncodedAddress::Eth(addr) => {
				write!(f, "Eth(0x{})", hex::encode(&addr[..]))
			},
			EncodedAddress::Dot(addr) => {
				write!(f, "Dot(0x{})", hex::encode(&addr[..]))
			},
			EncodedAddress::Btc(addr) => {
				write!(
					f,
					"Btc({})",
					std::str::from_utf8(addr)
						.unwrap_or("The address cant be decoded from the utf8 encoded bytes")
				)
			},
		}
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AddressError {
	InvalidAddress,
}

impl TryFrom<ForeignChainAddress> for EthereumAddress {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Eth(addr) => Ok(addr),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}

impl TryFrom<ForeignChainAddress> for H160 {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Eth(addr) => Ok(addr.into()),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}

impl TryFrom<ForeignChainAddress> for [u8; 32] {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Dot(addr) => Ok(addr),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}

impl TryFrom<ForeignChainAddress> for PolkadotAccountId {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Dot(addr) => Ok(addr.into()),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}

impl TryFrom<ForeignChainAddress> for BitcoinScriptBounded {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Btc(bitcoin_script) => Ok(bitcoin_script),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}

// For MockEthereum
impl TryFrom<ForeignChainAddress> for u64 {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Eth(addr) => Ok(addr[0] as u64),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}
impl From<u64> for ForeignChainAddress {
	fn from(address: u64) -> ForeignChainAddress {
		ForeignChainAddress::Eth([address as u8; 20])
	}
}

impl From<EthereumAddress> for ForeignChainAddress {
	fn from(address: EthereumAddress) -> ForeignChainAddress {
		ForeignChainAddress::Eth(address)
	}
}

impl From<H160> for ForeignChainAddress {
	fn from(address: H160) -> ForeignChainAddress {
		ForeignChainAddress::Eth(address.to_fixed_bytes())
	}
}

impl From<[u8; 32]> for ForeignChainAddress {
	fn from(address: [u8; 32]) -> ForeignChainAddress {
		ForeignChainAddress::Dot(address)
	}
}

impl From<PolkadotAccountId> for ForeignChainAddress {
	fn from(address: PolkadotAccountId) -> ForeignChainAddress {
		ForeignChainAddress::Dot(address.into())
	}
}

impl From<BitcoinScriptBounded> for ForeignChainAddress {
	fn from(bitcoin_script: BitcoinScriptBounded) -> ForeignChainAddress {
		ForeignChainAddress::Btc(bitcoin_script)
	}
}

impl EncodedAddress {
	pub fn from_chain_bytes(chain: ForeignChain, bytes: Vec<u8>) -> Self {
		match chain {
			ForeignChain::Ethereum => EncodedAddress::Eth(bytes),
			ForeignChain::Polkadot => EncodedAddress::Dot(bytes),
			ForeignChain::Bitcoin => EncodedAddress::Btc(bytes),
		}
	}
}

impl From<ForeignChainAddress> for ForeignChain {
	fn from(address: ForeignChainAddress) -> ForeignChain {
		match address {
			ForeignChainAddress::Eth(_) => ForeignChain::Ethereum,
			ForeignChainAddress::Dot(_) => ForeignChain::Polkadot,
			ForeignChainAddress::Btc(_) => ForeignChain::Bitcoin,
		}
	}
}
