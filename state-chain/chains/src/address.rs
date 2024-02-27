extern crate alloc;

use crate::{
	btc::ScriptPubkey,
	dot::PolkadotAccountId,
	eth::Address as EthereumAddress,
	sol::{self, SolAddress},
	Chain,
};
use cf_primitives::{ChannelId, ForeignChain, NetworkEnvironment};
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
	SolanaDerivationError(sol::AddressDerivationError),
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
	Eth(EthereumAddress),
	Dot(PolkadotAccountId),
	Btc(ScriptPubkey),
	Sol(SolAddress),
}

impl ForeignChainAddress {
	pub fn chain(&self) -> ForeignChain {
		match self {
			ForeignChainAddress::Eth(_) => ForeignChain::Ethereum,
			ForeignChainAddress::Dot(_) => ForeignChain::Polkadot,
			ForeignChainAddress::Btc(_) => ForeignChain::Bitcoin,
			ForeignChainAddress::Sol(_) => ForeignChain::Solana,
		}
	}
}

#[derive(
	Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, PartialOrd, Ord,
)]
pub enum EncodedAddress {
	Eth([u8; 20]),
	Dot([u8; 32]),
	Btc(Vec<u8>),
	Sol([u8; crate::sol::consts::SOLANA_ADDRESS_SIZE]),
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
			EncodedAddress::Eth(addr) => write!(f, "0x{}", hex::encode(&addr[..])),
			EncodedAddress::Dot(addr) => write!(f, "0x{}", hex::encode(&addr[..])),
			EncodedAddress::Btc(addr) => write!(
				f,
				"{}",
				std::str::from_utf8(addr)
					.unwrap_or("The address cant be decoded from the utf8 encoded bytes")
			),
			EncodedAddress::Sol(addr) => core::fmt::Display::fmt(&SolAddress(*addr), f),
		}
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AddressError {
	InvalidAddress,
}

impl TryFrom<ForeignChainAddress> for H160 {
	type Error = AddressError;

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Eth(addr) => Ok(addr),
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

impl From<EthereumAddress> for ForeignChainAddress {
	fn from(address: EthereumAddress) -> ForeignChainAddress {
		ForeignChainAddress::Eth(address)
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

impl From<sol::AddressDerivationError> for AddressDerivationError {
	fn from(value: sol::AddressDerivationError) -> Self {
		Self::SolanaDerivationError(value)
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
			ForeignChain::Solana => Ok(EncodedAddress::Sol(
				bytes.try_into().map_err(|_| "Invalid Solana address length")?,
			)),
		}
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
		ForeignChainAddress::Sol(address) => EncodedAddress::Sol(address.into()),
	}
}

#[allow(clippy::result_unit_err)]
pub fn try_from_encoded_address<GetNetwork: FnOnce() -> NetworkEnvironment>(
	encoded_address: EncodedAddress,
	network_environment: GetNetwork,
) -> Result<ForeignChainAddress, ()> {
	match encoded_address {
		EncodedAddress::Eth(address_bytes) => Ok(ForeignChainAddress::Eth(address_bytes.into())),
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
	}
}

pub trait ToHumanreadableAddress {
	#[cfg(feature = "std")]
	/// A type that serializes the address in a human-readable way.
	type Humanreadable: Serialize + DeserializeOwned + Send + Sync + Debug + Clone;

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

impl ToHumanreadableAddress for EthereumAddress {
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
/// A type that serializes the address in a human-readable way. This can only be
/// serialized and not deserialized.
/// `deserialize` is not implemented for ForeignChainAddressHumanreadable
/// because it is not possible to deserialize a human-readable address without
/// further context around the asset and chain.
pub enum ForeignChainAddressHumanreadable {
	Eth(<EthereumAddress as ToHumanreadableAddress>::Humanreadable),
	Dot(<PolkadotAccountId as ToHumanreadableAddress>::Humanreadable),
	Btc(<ScriptPubkey as ToHumanreadableAddress>::Humanreadable),
	Sol(<SolAddress as ToHumanreadableAddress>::Humanreadable),
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
			ForeignChainAddress::Sol(address) =>
				ForeignChainAddressHumanreadable::Sol(address.to_humanreadable(network_environment)),
		}
	}
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
