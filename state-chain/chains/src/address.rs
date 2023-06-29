extern crate alloc;

use crate::{
	btc::{BitcoinNetwork, ScriptPubkey},
	dot::PolkadotAccountId,
};
use cf_primitives::{EthereumAddress, ForeignChain};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::H160;
use sp_std::{str::FromStr, vec::Vec};

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ForeignChainAddress {
	Eth(EthereumAddress),
	Dot(PolkadotAccountId),
	Btc(ScriptPubkey),
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

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, PartialOrd, Ord)]
pub enum EncodedAddress {
	Eth([u8; 20]),
	Dot([u8; 32]),
	Btc(Vec<u8>),
}

pub trait AddressConverter: Sized {
	fn to_encoded_address(address: ForeignChainAddress) -> EncodedAddress;
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

impl FromStr for EncodedAddress {
	type Err = &'static str;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (chain, addr) =
			{
				let split = s.split(':').collect::<Vec<&str>>();
				if split.len() != 2 {
					return Err("Wrong format. Encoded address should be in the format of '<chain>:<address>'");
				}
				(split[0].trim(), split[1].trim())
			};
		let decoded_chain = ForeignChain::from_str(chain)?;
		match decoded_chain {
			ForeignChain::Bitcoin =>
				EncodedAddress::from_chain_bytes(ForeignChain::Bitcoin, addr.as_bytes().to_vec()),
			_ => EncodedAddress::from_chain_bytes(
				decoded_chain,
				hex::decode(addr).map_err(|_| "Failed to decode input string as Hex address.")?,
			),
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

impl TryFrom<ForeignChainAddress> for PolkadotAccountId {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Dot(addr) => Ok(addr),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}

impl TryFrom<ForeignChainAddress> for ScriptPubkey {
	type Error = AddressError;

	fn try_from(foreign_chain_address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match foreign_chain_address {
			ForeignChainAddress::Btc(script_pubkey) => Ok(script_pubkey),
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

impl From<PolkadotAccountId> for ForeignChainAddress {
	fn from(account_id: PolkadotAccountId) -> ForeignChainAddress {
		ForeignChainAddress::Dot(account_id)
	}
}

impl From<ScriptPubkey> for ForeignChainAddress {
	fn from(script_pubkey: ScriptPubkey) -> ForeignChainAddress {
		ForeignChainAddress::Btc(script_pubkey)
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

pub fn to_encoded_address<GetBitcoinNetwork: FnOnce() -> BitcoinNetwork>(
	address: ForeignChainAddress,
	bitcoin_network: GetBitcoinNetwork,
) -> EncodedAddress {
	match address {
		ForeignChainAddress::Eth(address) => EncodedAddress::Eth(address),
		ForeignChainAddress::Dot(address) => EncodedAddress::Dot(*address.aliased_ref()),
		ForeignChainAddress::Btc(script_pubkey) =>
			EncodedAddress::Btc(script_pubkey.to_address(&bitcoin_network()).as_bytes().to_vec()),
	}
}

#[allow(clippy::result_unit_err)]
pub fn try_from_encoded_address<GetBitcoinNetwork: FnOnce() -> BitcoinNetwork>(
	encoded_address: EncodedAddress,
	bitcoin_network: GetBitcoinNetwork,
) -> Result<ForeignChainAddress, ()> {
	match encoded_address {
		EncodedAddress::Eth(address_bytes) => Ok(ForeignChainAddress::Eth(address_bytes)),
		EncodedAddress::Dot(address_bytes) =>
			Ok(ForeignChainAddress::Dot(PolkadotAccountId::from_aliased(address_bytes))),
		EncodedAddress::Btc(address_bytes) => Ok(ForeignChainAddress::Btc(
			ScriptPubkey::try_from_address(
				sp_std::str::from_utf8(&address_bytes[..]).map_err(|_| ())?,
				&bitcoin_network(),
			)
			.map_err(|_| ())?,
		)),
	}
}

#[test]
fn encode_and_decode_address() {
	#[track_caller]
	fn test(address: &str, case_sensitive: bool) {
		let network = || BitcoinNetwork::Mainnet;
		let encoded_addr = EncodedAddress::Btc(address.as_bytes().to_vec());
		let foreign_chain_addr = try_from_encoded_address(encoded_addr.clone(), network).unwrap();
		let recovered_addr = to_encoded_address(foreign_chain_addr, network);
		if case_sensitive {
			assert_eq!(recovered_addr, encoded_addr, "{recovered_addr} != {encoded_addr}");
		} else {
			assert!(
				recovered_addr.to_string().eq_ignore_ascii_case(&encoded_addr.to_string()),
				"{recovered_addr} != {encoded_addr}"
			);
		}
	}
	for addr in [
		"bc1p4syuuy97f96lfah764w33ru9v5u3uk8n8jk9xsq684xfl8sxu82sdcvdcx",
		"3P14159f73E4gFr7JterCCQh9QjiTjiZrG",
		"BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4",
		"BC1SW50QGDZ25J",
		"bc1zw508d6qejxtdg4y5r3zarvaryvaxxpcs",
		"bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0",
	] {
		test(addr, false);
	}
	for addr in [
		"1AGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62i",
		"1Q1pE5vPGEEMqRcVRMbtBK842Y6Pzo6nK9",
		"1BNGaR29FmfAqidXmD9HLwsGv9p5WVvvhq",
		"17NdbrSGoUotzeGCcMMCqnFkEvLymoou9j",
		"16UwLL9Risc3QfPqBUvKofHmBQ7wMtjvM",
		"1111111111111111111114oLvT2",
	] {
		test(addr, true);
	}
}

#[test]
fn can_create_encoded_address_from_str() {
	let valid_input = vec![
		(
			"Ethereum:b0B267E4afCFbbA8A05961bD5F4607B6cdea0E37",
			EncodedAddress::Eth(hex_literal::hex!("b0B267E4afCFbbA8A05961bD5F4607B6cdea0E37")),
		),
		(
			"Polkadot:d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d",
			EncodedAddress::Dot(hex_literal::hex!(
				"d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
			)),
		),
		(
			"Bitcoin:12pdGSdrR1FEA3pgmLPjX4RvYWm4xAdnGp",
			EncodedAddress::Btc("12pdGSdrR1FEA3pgmLPjX4RvYWm4xAdnGp".as_bytes().to_vec()),
		),
	];

	for (input_str, result_address) in valid_input {
		assert_eq!(
			EncodedAddress::from_str(input_str)
				.expect("String to EncodedAddress Conversion must succeed in valid cases."),
			result_address
		);
	}
}
