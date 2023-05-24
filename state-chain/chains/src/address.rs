use cf_primitives::{EthereumAddress, ForeignChain, PolkadotAccountId};

extern crate alloc;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::H160;

use sp_runtime::DispatchError;
use sp_std::vec::Vec;

use crate::btc::{
	deposit_address::derive_btc_deposit_address_from_script, scriptpubkey_from_address,
	BitcoinNetwork, BitcoinScriptBounded,
};

pub type ScriptPubkeyBytes = Vec<u8>;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ForeignChainAddress {
	Eth(EthereumAddress),
	Dot([u8; 32]),
	Btc(BitcoinScriptBounded),
}

impl ForeignChainAddress {
	pub fn chain(&self) -> ForeignChain {
		match self {
			ForeignChainAddress::Eth(_) => ForeignChain::Ethereum,
			ForeignChainAddress::Dot(_) => ForeignChain::Polkadot,
			ForeignChainAddress::Btc(_) => ForeignChain::Bitcoin,
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, PartialOrd, Ord)]
pub enum EncodedAddress {
	Eth([u8; 20]),
	Dot([u8; 32]),
	Btc(Vec<u8>),
}

pub trait AddressConverter: Sized {
	fn try_to_encoded_address(
		address: ForeignChainAddress,
	) -> Result<EncodedAddress, DispatchError>;
	#[allow(clippy::result_unit_err)]
	fn try_from_encoded_address(encoded_address: EncodedAddress)
		-> Result<ForeignChainAddress, ()>;
}

#[cfg(feature = "std")]
impl core::fmt::Display for EncodedAddress {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			EncodedAddress::Eth(addr) => {
				write!(f, "0x{}", hex::encode(&addr[..]))
			},
			EncodedAddress::Dot(addr) => {
				write!(f, "0x{}", hex::encode(&addr[..]))
			},
			EncodedAddress::Btc(addr) => {
				write!(
					f,
					"{}",
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
	pub fn from_chain_bytes(chain: ForeignChain, bytes: Vec<u8>) -> Result<Self, &'static str> {
		match chain {
			ForeignChain::Ethereum => {
				if bytes.len() != 20 {
					return Err("Invalid Ethereum address length")
				}
				let mut address = [0u8; 20];
				address.copy_from_slice(&bytes);
				Ok(EncodedAddress::Eth(address))
			},
			ForeignChain::Polkadot => {
				if bytes.len() != 32 {
					return Err("Invalid Polkadot address length")
				}
				let mut address = [0u8; 32];
				address.copy_from_slice(&bytes);
				Ok(EncodedAddress::Dot(address))
			},
			ForeignChain::Bitcoin => Ok(EncodedAddress::Btc(bytes)),
		}
	}
}

pub fn try_to_encoded_address<GetBitcoinNetwork: FnOnce() -> BitcoinNetwork>(
	address: ForeignChainAddress,
	bitcoin_network: GetBitcoinNetwork,
) -> Result<EncodedAddress, DispatchError> {
	match address {
		ForeignChainAddress::Eth(address) => Ok(EncodedAddress::Eth(address)),
		ForeignChainAddress::Dot(address) => Ok(EncodedAddress::Dot(address)),
		ForeignChainAddress::Btc(address) => Ok(EncodedAddress::Btc(
			// TODO: This only works for our own addresses, not for arbitrary addresses.
			cf_chains::btc::deposit_address::legacy_derive_btc_deposit_address_from_script(
				address.into(),
				Environment::bitcoin_network(),
			)
			.bytes()
			.collect::<Vec<u8>>(),
		)),
	}
}

#[allow(clippy::result_unit_err)]
pub fn try_from_encoded_address<GetBitcoinNetwork: FnOnce() -> BitcoinNetwork>(
	encoded_address: EncodedAddress,
	bitcoin_network: GetBitcoinNetwork,
) -> Result<ForeignChainAddress, ()> {
	match encoded_address {
		EncodedAddress::Eth(address_bytes) => Ok(ForeignChainAddress::Eth(address_bytes)),
		EncodedAddress::Dot(address_bytes) => Ok(ForeignChainAddress::Dot(address_bytes)),
		EncodedAddress::Btc(address_bytes) => Ok(ForeignChainAddress::Btc(
			scriptpubkey_from_address(
				sp_std::str::from_utf8(&address_bytes[..]).map_err(|_| ())?,
				bitcoin_network(),
			)
			.map_err(|_| ())?
			.try_into()
			.expect(
				"bitcoin scripts constructed from supported addresses should not exceed 128 bytes",
			),
		)),
	}
}
