use codec::{Decode, Encode, MaxEncodedLen};
use digest::Digest;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::address;

use super::consts::{
	SOLANA_ADDRESS_SIZE, SOLANA_PDA_MARKER, SOLANA_PDA_MAX_SEEDS, SOLANA_PDA_MAX_SEED_LEN,
};

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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressDerivationError {
	NotAValidPoint,
	TooManySeeds,
	SeedTooLarge,
	// TODO: choose a better name
	BumpSeedBadLuck,
}

#[derive(Debug, Clone)]
pub struct DerivedAddressBuilder {
	program_id: [u8; SOLANA_ADDRESS_SIZE],
	hasher: Sha256,
	seeds_left: u8,
}

impl SolAddress {
	pub fn derive(&self) -> Result<DerivedAddressBuilder, AddressDerivationError> {
		if !bytes_are_curve_point(self) {
			return Err(AddressDerivationError::NotAValidPoint)
		}

		let builder = DerivedAddressBuilder {
			program_id: self.0,
			hasher: Sha256::new(),
			seeds_left: SOLANA_PDA_MAX_SEEDS - 1,
		};
		Ok(builder)
	}
}

impl DerivedAddressBuilder {
	pub fn seed(&mut self, seed: impl AsRef<[u8]>) -> Result<&mut Self, AddressDerivationError> {
		let Some(seeds_left) = self.seeds_left.checked_sub(1) else {
			return Err(AddressDerivationError::TooManySeeds)
		};

		let seed = seed.as_ref();
		if seed.len() > SOLANA_PDA_MAX_SEED_LEN {
			return Err(AddressDerivationError::SeedTooLarge)
		};

		self.seeds_left = seeds_left;
		self.hasher.update(seed);

		Ok(self)
	}

	pub fn chain_seed(mut self, seed: impl AsRef<[u8]>) -> Result<Self, AddressDerivationError> {
		self.seed(seed)?;
		Ok(self)
	}

	pub fn finish(self) -> Result<(SolAddress, u8), AddressDerivationError> {
		for bump in (0..=u8::MAX).rev() {
			let digest = self
				.hasher
				.clone()
				.chain_update([bump])
				.chain_update(&self.program_id[..])
				.chain_update(SOLANA_PDA_MARKER)
				.finalize();
			if !bytes_are_curve_point(&digest) {
				let address = SolAddress(digest.into());
				let pda = (address, bump);
				return Ok(pda)
			}
		}

		Err(AddressDerivationError::BumpSeedBadLuck)
	}
}

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

impl AsRef<[u8; SOLANA_ADDRESS_SIZE]> for SolAddress {
	fn as_ref(&self) -> &[u8; SOLANA_ADDRESS_SIZE] {
		&self.0
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

/// [Courtesy of Solana-SDK](https://docs.rs/solana-program/1.18.1/src/solana_program/pubkey.rs.html#163)
fn bytes_are_curve_point<T: AsRef<[u8; SOLANA_ADDRESS_SIZE]>>(bytes: T) -> bool {
	curve25519_dalek::edwards::CompressedEdwardsY::from_slice(bytes.as_ref())
		.decompress()
		.is_some()
}
