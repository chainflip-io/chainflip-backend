use cf_primitives::{EthereumAddress, ForeignChain, PolkadotAccountId, MAX_BTC_ADDRESS_LENGTH};

extern crate alloc;
use alloc::string::{String, ToString};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::BoundedVec;
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::{ConstU32, H160};

use sp_std::vec::Vec;

use crate::btc::{
	ingress_address::derive_btc_ingress_address, scriptpubkey_from_address, BitcoinNetwork,
	BitcoinScript, Error,
};

pub type ScriptPubkeyBytes = Vec<u8>;

pub type BitcoinAddress = BoundedVec<u8, ConstU32<MAX_BTC_ADDRESS_LENGTH>>;

/// The seed data required to generate a Bitcoin address. We don't pass in network
/// here, as we assume the same network for all addresses.
#[derive(
	Default, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, PartialOrd, Ord,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BitcoinAddressSeed {
	pub pubkey_x: [u8; 32],
	pub salt: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BitcoinAddressData {
	pub address_for: BitcoinAddressFor,
	pub network: BitcoinNetwork,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum BitcoinAddressFor {
	// When we ingress, we derive an address from our pubkey_x and a salt which creates an address
	// that can then be used by the user to send BTC to us.
	Ingress(BitcoinAddressSeed),
	// When we egress, we are provided with an address from the user.
	// We then create a lock script over that address.
	Egress(BitcoinAddress),
}

impl BitcoinAddressData {
	pub fn to_scriptpubkey(&self) -> Result<BitcoinScript, Error> {
		scriptpubkey_from_address(&self.to_address_string(), self.network)
	}

	pub fn seed(&self) -> Option<BitcoinAddressSeed> {
		match &self.address_for {
			BitcoinAddressFor::Ingress(seed) => Some(seed.clone()),
			BitcoinAddressFor::Egress(_) => None,
		}
	}

	pub fn to_address_string(&self) -> String {
		match &self.address_for {
			BitcoinAddressFor::Ingress(seed) =>
				derive_btc_ingress_address(seed.pubkey_x, seed.salt, self.network),
			BitcoinAddressFor::Egress(address) =>
				sp_std::str::from_utf8(&address[..]).unwrap().to_string(),
		}
	}
}

impl Default for BitcoinAddressData {
	fn default() -> Self {
		BitcoinAddressData {
			address_for: BitcoinAddressFor::Ingress(BitcoinAddressSeed::default()),
			network: BitcoinNetwork::Mainnet,
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ForeignChainAddress {
	Eth(EthereumAddress),
	Dot([u8; 32]),
	Btc(BitcoinAddressData),
}

#[cfg(feature = "std")]
impl core::fmt::Display for ForeignChainAddress {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			ForeignChainAddress::Eth(addr) => {
				write!(f, "Eth(0x{})", hex::encode(addr))
			},
			ForeignChainAddress::Dot(addr) => {
				write!(f, "Dot(0x{})", hex::encode(addr))
			},
			ForeignChainAddress::Btc(addr) => {
				write!(f, "Btc({})", addr.to_address_string())
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

impl TryFrom<ForeignChainAddress> for BitcoinAddressData {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Btc(address) => Ok(address),
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

impl From<BitcoinAddressData> for ForeignChainAddress {
	fn from(address: BitcoinAddressData) -> ForeignChainAddress {
		ForeignChainAddress::Btc(address)
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
