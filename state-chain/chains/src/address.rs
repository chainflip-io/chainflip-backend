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

extern crate alloc;

use crate::{
	btc::ScriptPubkey,
	dot::PolkadotAccountId,
	eth::Address as EvmAddress,
	sol::{self, SolAddress, SolPubkey},
	Chain,
};
use cf_primitives::{
	chains::{Arbitrum, Assethub, Bitcoin, Ethereum, Polkadot, Solana},
	ChannelId, ForeignChain, NetworkEnvironment,
};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sp_core::H160;
use sp_std::{fmt::Debug, vec::Vec};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressDerivationError {
	MissingPolkadotVault,
	MissingBitcoinVault,
	BitcoinChannelIdTooLarge,
	MissingSolanaApiEnvironment,
	SolanaDerivationError(sol::AddressDerivationError),
	MissingAssethubVault,
}

impl From<sol::AddressDerivationError> for AddressDerivationError {
	fn from(value: sol::AddressDerivationError) -> Self {
		Self::SolanaDerivationError(value)
	}
}

/// Generates a deterministic deposit address for some combination of asset, chain and channel id.
pub trait AddressDerivationApi<C: Chain> {
	// TODO: should also take root pubkey (vault) as an argument?
	fn generate_address(
		source_asset: C::ChainAsset,
		channel_id: ChannelId,
	) -> Result<C::ChainAccount, AddressDerivationError>;

	fn generate_address_and_state(
		source_asset: C::ChainAsset,
		channel_id: ChannelId,
	) -> Result<(C::ChainAccount, C::DepositChannelState), AddressDerivationError>;
}

#[derive(
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	PartialOrd,
	Ord,
	Serialize,
	Deserialize,
)]
pub enum ForeignChainAddress {
	Eth(<Ethereum as Chain>::ChainAccount),
	Dot(<Polkadot as Chain>::ChainAccount),
	Btc(<Bitcoin as Chain>::ChainAccount),
	Arb(<Arbitrum as Chain>::ChainAccount),
	Sol(<Solana as Chain>::ChainAccount),
	Hub(<Assethub as Chain>::ChainAccount),
}

impl ForeignChainAddress {
	pub fn chain(&self) -> ForeignChain {
		match self {
			ForeignChainAddress::Eth(_) => ForeignChain::Ethereum,
			ForeignChainAddress::Dot(_) => ForeignChain::Polkadot,
			ForeignChainAddress::Btc(_) => ForeignChain::Bitcoin,
			ForeignChainAddress::Arb(_) => ForeignChain::Arbitrum,
			ForeignChainAddress::Sol(_) => ForeignChain::Solana,
			ForeignChainAddress::Hub(_) => ForeignChain::Assethub,
		}
	}
	pub fn raw_bytes(self) -> Vec<u8> {
		match self {
			ForeignChainAddress::Eth(source_address) => source_address.0.to_vec(),
			ForeignChainAddress::Arb(source_address) => source_address.0.to_vec(),
			ForeignChainAddress::Sol(source_address) => source_address.0.to_vec(),
			ForeignChainAddress::Dot(source_address) => source_address.aliased_ref().to_vec(),
			ForeignChainAddress::Btc(script_pubkey) => script_pubkey.bytes(),
			ForeignChainAddress::Hub(source_address) => source_address.aliased_ref().to_vec(),
		}
	}

	pub fn to_encoded_address(&self, network: NetworkEnvironment) -> EncodedAddress {
		to_encoded_address(self.clone(), || network)
	}
}

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum EncodedAddress {
	Eth([u8; 20]),
	Dot([u8; 32]),
	Btc(Vec<u8>),
	Arb([u8; 20]),
	Sol([u8; sol_prim::consts::SOLANA_ADDRESS_LEN]),
	Hub([u8; 32]),
}

pub trait AddressConverter: Sized {
	fn to_encoded_address(address: ForeignChainAddress) -> EncodedAddress;
	#[allow(clippy::result_unit_err)]
	fn try_from_encoded_address(encoded_address: EncodedAddress)
		-> Result<ForeignChainAddress, ()>;

	fn decode_and_validate_address_for_asset(
		encoded_address: EncodedAddress,
		asset: cf_primitives::Asset,
	) -> Result<ForeignChainAddress, AddressError>;
}

#[cfg(feature = "std")]
impl core::fmt::Display for EncodedAddress {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			EncodedAddress::Eth(addr) | EncodedAddress::Arb(addr) =>
				write!(f, "0x{}", hex::encode(&addr[..])),
			EncodedAddress::Dot(addr) => write!(f, "0x{}", hex::encode(&addr[..])),
			EncodedAddress::Btc(addr) => write!(
				f,
				"{}",
				std::str::from_utf8(addr)
					.unwrap_or("The address cant be decoded from the utf8 encoded bytes")
			),
			EncodedAddress::Sol(addr) => core::fmt::Display::fmt(&SolAddress(*addr), f),
			EncodedAddress::Hub(addr) => write!(f, "0x{}", hex::encode(&addr[..])),
		}
	}
}

impl TryFrom<EncodedAddress> for SolPubkey {
	type Error = ();
	fn try_from(value: EncodedAddress) -> Result<Self, Self::Error> {
		if let EncodedAddress::Sol(bytes) = value {
			Ok(SolPubkey(bytes))
		} else {
			Err(())
		}
	}
}

impl From<SolAddress> for EncodedAddress {
	fn from(from: SolAddress) -> EncodedAddress {
		EncodedAddress::Sol(from.0)
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AddressError {
	InvalidAddress,
	InvalidAddressForChain,
}

impl TryFrom<ForeignChainAddress> for H160 {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Eth(addr) | ForeignChainAddress::Arb(addr) => Ok(addr),
			_ => Err(AddressError::InvalidAddress),
		}
	}
}

impl TryFrom<ForeignChainAddress> for PolkadotAccountId {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Dot(addr) | ForeignChainAddress::Hub(addr) => Ok(addr),
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
pub trait IntoForeignChainAddress<C: Chain> {
	fn into_foreign_chain_address(self) -> ForeignChainAddress;
}

impl<C: Chain> IntoForeignChainAddress<C> for ForeignChainAddress {
	fn into_foreign_chain_address(self) -> ForeignChainAddress {
		self
	}
}

impl IntoForeignChainAddress<Ethereum> for EvmAddress {
	fn into_foreign_chain_address(self) -> ForeignChainAddress {
		ForeignChainAddress::Eth(self)
	}
}

impl IntoForeignChainAddress<Arbitrum> for EvmAddress {
	fn into_foreign_chain_address(self) -> ForeignChainAddress {
		ForeignChainAddress::Arb(self)
	}
}

impl IntoForeignChainAddress<Polkadot> for PolkadotAccountId {
	fn into_foreign_chain_address(self) -> ForeignChainAddress {
		ForeignChainAddress::Dot(self)
	}
}

impl IntoForeignChainAddress<Assethub> for PolkadotAccountId {
	fn into_foreign_chain_address(self) -> ForeignChainAddress {
		ForeignChainAddress::Hub(self)
	}
}

impl IntoForeignChainAddress<Bitcoin> for ScriptPubkey {
	fn into_foreign_chain_address(self) -> ForeignChainAddress {
		ForeignChainAddress::Btc(self)
	}
}

impl IntoForeignChainAddress<Solana> for SolAddress {
	fn into_foreign_chain_address(self) -> ForeignChainAddress {
		ForeignChainAddress::Sol(self)
	}
}

impl EncodedAddress {
	pub fn inner_bytes(&self) -> &[u8] {
		match self {
			EncodedAddress::Eth(inner) => &inner[..],
			EncodedAddress::Dot(inner) => &inner[..],
			EncodedAddress::Btc(inner) => &inner[..],
			EncodedAddress::Arb(inner) => &inner[..],
			EncodedAddress::Sol(inner) => &inner[..],
			EncodedAddress::Hub(inner) => &inner[..],
		}
	}
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
			ForeignChain::Arbitrum => {
				if bytes.len() != 20 {
					return Err("Invalid Arbitrum address length")
				}
				let mut address = [0u8; 20];
				address.copy_from_slice(&bytes);
				Ok(EncodedAddress::Arb(address))
			},
			ForeignChain::Solana => Ok(EncodedAddress::Sol(
				bytes.try_into().map_err(|_| "Invalid Solana address length")?,
			)),
			ForeignChain::Assethub => {
				if bytes.len() != 32 {
					return Err("Invalid Assethub address length")
				}
				let mut address = [0u8; 32];
				address.copy_from_slice(&bytes);
				Ok(EncodedAddress::Hub(address))
			},
		}
	}

	pub fn chain(&self) -> ForeignChain {
		match self {
			EncodedAddress::Eth(_) => ForeignChain::Ethereum,
			EncodedAddress::Dot(_) => ForeignChain::Polkadot,
			EncodedAddress::Btc(_) => ForeignChain::Bitcoin,
			EncodedAddress::Arb(_) => ForeignChain::Arbitrum,
			EncodedAddress::Sol(_) => ForeignChain::Solana,
			EncodedAddress::Hub(_) => ForeignChain::Assethub,
		}
	}
	pub fn into_vec(self) -> Vec<u8> {
		match self {
			EncodedAddress::Eth(bytes) => bytes.to_vec(),
			EncodedAddress::Arb(bytes) => bytes.to_vec(),
			EncodedAddress::Sol(bytes) => bytes.to_vec(),
			EncodedAddress::Dot(bytes) => bytes.to_vec(),
			EncodedAddress::Btc(byte_vec) => byte_vec,
			EncodedAddress::Hub(bytes) => bytes.to_vec(),
		}
	}

	pub fn from_chain_account<C: Chain>(
		account: C::ChainAccount,
		network: NetworkEnvironment,
	) -> Self {
		account.into_foreign_chain_address().to_encoded_address(network)
	}
}

pub fn to_encoded_address<GetNetwork: FnOnce() -> NetworkEnvironment>(
	address: ForeignChainAddress,
	network_environment: GetNetwork,
) -> EncodedAddress {
	match address {
		ForeignChainAddress::Eth(address) => EncodedAddress::Eth(address.0),
		ForeignChainAddress::Dot(address) => EncodedAddress::Dot(*address.aliased_ref()),
		ForeignChainAddress::Btc(script_pubkey) => EncodedAddress::Btc(
			script_pubkey.to_address(&network_environment().into()).as_bytes().to_vec(),
		),
		ForeignChainAddress::Arb(address) => EncodedAddress::Arb(address.0),
		ForeignChainAddress::Sol(address) => EncodedAddress::Sol(address.into()),
		ForeignChainAddress::Hub(address) => EncodedAddress::Hub(*address.aliased_ref()),
	}
}

#[allow(clippy::result_unit_err)]
pub fn try_from_encoded_address<GetNetwork: FnOnce() -> NetworkEnvironment>(
	encoded_address: EncodedAddress,
	network_environment: GetNetwork,
) -> Result<ForeignChainAddress, ()> {
	match encoded_address {
		EncodedAddress::Eth(address_bytes) => Ok(ForeignChainAddress::Eth(address_bytes.into())),
		EncodedAddress::Arb(address_bytes) => Ok(ForeignChainAddress::Arb(address_bytes.into())),
		EncodedAddress::Dot(address_bytes) =>
			Ok(ForeignChainAddress::Dot(PolkadotAccountId::from_aliased(address_bytes))),
		EncodedAddress::Btc(address_bytes) => Ok(ForeignChainAddress::Btc(
			ScriptPubkey::try_from_address(
				sp_std::str::from_utf8(&address_bytes[..]).map_err(|_| ())?,
				&network_environment().into(),
			)
			.map_err(|_| ())?,
		)),
		EncodedAddress::Sol(address_bytes) => Ok(ForeignChainAddress::Sol(address_bytes.into())),
		EncodedAddress::Hub(address_bytes) =>
			Ok(ForeignChainAddress::Hub(PolkadotAccountId::from_aliased(address_bytes))),
	}
}

pub fn decode_and_validate_address_for_asset<GetNetwork: FnOnce() -> NetworkEnvironment>(
	encoded_address: EncodedAddress,
	asset: cf_primitives::Asset,
	network_environment: GetNetwork,
) -> Result<ForeignChainAddress, AddressError> {
	let address = try_from_encoded_address(encoded_address, network_environment)
		.map_err(|_| AddressError::InvalidAddress)?;

	frame_support::ensure!(
		address.chain() == ForeignChain::from(asset),
		AddressError::InvalidAddressForChain
	);

	Ok(address)
}

pub trait ToHumanreadableAddress {
	#[cfg(feature = "std")]
	/// A type that serializes the address in a human-readable way.
	type Humanreadable: Serialize
		+ DeserializeOwned
		+ std::fmt::Display
		+ Send
		+ Sync
		+ Debug
		+ Clone;

	#[cfg(feature = "std")]
	fn to_humanreadable(&self, network_environment: NetworkEnvironment) -> Self::Humanreadable;
}

impl ToHumanreadableAddress for ScriptPubkey {
	#[cfg(feature = "std")]
	type Humanreadable = String;

	#[cfg(feature = "std")]
	fn to_humanreadable(&self, network_environment: NetworkEnvironment) -> Self::Humanreadable {
		self.to_address(&network_environment.into())
	}
}

impl ToHumanreadableAddress for EvmAddress {
	#[cfg(feature = "std")]
	type Humanreadable = Self;

	#[cfg(feature = "std")]
	fn to_humanreadable(&self, _network_environment: NetworkEnvironment) -> Self::Humanreadable {
		*self
	}
}

impl ToHumanreadableAddress for PolkadotAccountId {
	#[cfg(feature = "std")]
	type Humanreadable = crate::dot::SubstrateNetworkAddress;

	#[cfg(feature = "std")]
	fn to_humanreadable(&self, _network_environment: NetworkEnvironment) -> Self::Humanreadable {
		crate::dot::SubstrateNetworkAddress::polkadot(*self.aliased_ref())
	}
}

#[cfg(feature = "std")]
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
/// A type that serializes the address in a human-readable way.
///
/// This can only be serialized and not deserialized.
/// `deserialize` is not implemented for ForeignChainAddressHumanreadable
/// because it is not possible to deserialize a human-readable address without
/// further context around the asset and chain.
pub enum ForeignChainAddressHumanreadable {
	Eth(<EvmAddress as ToHumanreadableAddress>::Humanreadable),
	Dot(<PolkadotAccountId as ToHumanreadableAddress>::Humanreadable),
	Btc(<ScriptPubkey as ToHumanreadableAddress>::Humanreadable),
	Arb(<EvmAddress as ToHumanreadableAddress>::Humanreadable),
	Sol(<SolAddress as ToHumanreadableAddress>::Humanreadable),
	Hub(<PolkadotAccountId as ToHumanreadableAddress>::Humanreadable),
}

#[cfg(feature = "std")]
impl std::fmt::Display for ForeignChainAddressHumanreadable {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ForeignChainAddressHumanreadable::Eth(address) |
			ForeignChainAddressHumanreadable::Arb(address) => write!(f, "{:#x}", address),
			ForeignChainAddressHumanreadable::Dot(address) |
			ForeignChainAddressHumanreadable::Hub(address) => write!(f, "{}", address),
			ForeignChainAddressHumanreadable::Btc(address) |
			ForeignChainAddressHumanreadable::Sol(address) => write!(f, "{}", address),
		}
	}
}

#[cfg(feature = "std")]
impl<'de> Deserialize<'de> for ForeignChainAddressHumanreadable {
	fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		unimplemented!("Deserialization of ForeignChainAddressHumanreadable is not implemented")
	}
}

impl ToHumanreadableAddress for ForeignChainAddress {
	#[cfg(feature = "std")]
	type Humanreadable = ForeignChainAddressHumanreadable;

	#[cfg(feature = "std")]
	fn to_humanreadable(&self, network_environment: NetworkEnvironment) -> Self::Humanreadable {
		match self {
			ForeignChainAddress::Eth(address) =>
				ForeignChainAddressHumanreadable::Eth(address.to_humanreadable(network_environment)),
			ForeignChainAddress::Dot(address) =>
				ForeignChainAddressHumanreadable::Dot(address.to_humanreadable(network_environment)),
			ForeignChainAddress::Btc(address) =>
				ForeignChainAddressHumanreadable::Btc(address.to_humanreadable(network_environment)),
			ForeignChainAddress::Arb(address) =>
				ForeignChainAddressHumanreadable::Arb(address.to_humanreadable(network_environment)),
			ForeignChainAddress::Sol(address) =>
				ForeignChainAddressHumanreadable::Sol(address.to_humanreadable(network_environment)),
			ForeignChainAddress::Hub(address) =>
				ForeignChainAddressHumanreadable::Hub(address.to_humanreadable(network_environment)),
		}
	}
}

#[cfg(feature = "std")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressString(String);

#[cfg(feature = "std")]
impl From<String> for AddressString {
	fn from(s: String) -> Self {
		Self(s)
	}
}

#[cfg(feature = "std")]
impl From<AddressString> for String {
	fn from(s: AddressString) -> Self {
		s.0
	}
}

#[cfg(feature = "std")]
impl sp_std::str::FromStr for AddressString {
	type Err = frame_support::Never;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(s.to_string()))
	}
}

#[cfg(feature = "std")]
impl AddressString {
	pub fn try_parse_to_encoded_address(
		self,
		chain: ForeignChain,
	) -> anyhow::Result<EncodedAddress> {
		clean_foreign_chain_address(chain, self.0.as_str())
	}

	pub fn try_parse_to_foreign_chain_address(
		self,
		chain: ForeignChain,
		network: NetworkEnvironment,
	) -> anyhow::Result<ForeignChainAddress> {
		try_from_encoded_address(self.try_parse_to_encoded_address(chain)?, move || network)
			.map_err(|_| anyhow::anyhow!("Failed to parse address"))
	}

	pub fn from_encoded_address<T: std::borrow::Borrow<EncodedAddress>>(address: T) -> Self {
		Self(address.borrow().to_string())
	}
}

#[cfg(feature = "std")]
impl sp_std::fmt::Display for AddressString {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "{}", self.0)
	}
}

/// Sanitize the given address (hex or base58) and turn it into a EncodedAddress of the given
/// chain.
#[cfg(feature = "std")]
pub fn clean_foreign_chain_address(
	chain: ForeignChain,
	address: &str,
) -> anyhow::Result<EncodedAddress> {
	use core::str::FromStr;

	use cf_utilities::clean_hex_address;

	Ok(match chain {
		ForeignChain::Ethereum => EncodedAddress::Eth(clean_hex_address(address)?),
		ForeignChain::Polkadot =>
			EncodedAddress::Dot(PolkadotAccountId::from_str(address).map(|id| *id.aliased_ref())?),
		ForeignChain::Bitcoin => EncodedAddress::Btc(address.as_bytes().to_vec()),
		ForeignChain::Arbitrum => EncodedAddress::Arb(clean_hex_address(address)?),
		ForeignChain::Solana => match SolAddress::from_str(address) {
			Ok(sol_address) => EncodedAddress::Sol(sol_address.into()),
			Err(_) => EncodedAddress::Sol(clean_hex_address(address)?),
		},
		ForeignChain::Assethub =>
			EncodedAddress::Hub(PolkadotAccountId::from_str(address).map(|id| *id.aliased_ref())?),
	})
}

#[test]
fn encode_and_decode_address() {
	#[track_caller]
	fn test(address: &str, case_sensitive: bool) {
		let network = || NetworkEnvironment::Mainnet;
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
