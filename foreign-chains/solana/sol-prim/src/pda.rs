use digest::Digest;
use sha2::Sha256;

use crate::{address::Address, consts};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "std-error", derive(thiserror::Error))]
pub enum PdaError {
	#[cfg_attr(feature = "std-error", error("not a valid point"))]
	NotAValidPoint,
	#[cfg_attr(feature = "std-error", error("too many seeds"))]
	TooManySeeds,
	#[cfg_attr(feature = "std-error", error("seed too large"))]
	SeedTooLarge,
	// TODO: choose a better name
	#[cfg_attr(feature = "std-error", error("bad luck bumping seed"))]
	BumpSeedBadLuck,
}

#[derive(Debug, Clone)]
pub struct Pda {
	program_id: Address,
	hasher: Sha256,
	seeds_left: u8,
}

impl Pda {
	pub fn from_address(program_id: Address) -> Result<Self, PdaError> {
		if !bytes_are_curve_point(program_id) {
			return Err(PdaError::NotAValidPoint)
		}

		let builder = Self {
			program_id,
			hasher: Sha256::new(),
			seeds_left: consts::SOLANA_PDA_MAX_SEEDS - 1,
		};
		Ok(builder)
	}

	pub fn seed(&mut self, seed: impl AsRef<[u8]>) -> Result<&mut Self, PdaError> {
		let Some(seeds_left) = self.seeds_left.checked_sub(1) else {
			return Err(PdaError::TooManySeeds)
		};

		let seed = seed.as_ref();
		if seed.len() > consts::SOLANA_PDA_MAX_SEED_LEN {
			return Err(PdaError::SeedTooLarge)
		};

		self.seeds_left = seeds_left;
		self.hasher.update(seed);

		Ok(self)
	}

	pub fn chain_seed(mut self, seed: impl AsRef<[u8]>) -> Result<Self, PdaError> {
		self.seed(seed)?;
		Ok(self)
	}

	pub fn finish(self) -> Result<(Address, u8), PdaError> {
		for bump in (0..=u8::MAX).rev() {
			let digest = self
				.hasher
				.clone()
				.chain_update([bump])
				.chain_update(self.program_id)
				.chain_update(consts::SOLANA_PDA_MARKER)
				.finalize();
			if !bytes_are_curve_point(digest) {
				let address = Address(digest.into());
				let pda = (address, bump);
				return Ok(pda)
			}
		}

		Err(PdaError::BumpSeedBadLuck)
	}
}

/// [Courtesy of Solana-SDK](https://docs.rs/solana-program/1.18.1/src/solana_program/pubkey.rs.html#163)
fn bytes_are_curve_point<T: AsRef<[u8; consts::SOLANA_ADDRESS_LEN]>>(bytes: T) -> bool {
	curve25519_dalek::edwards::CompressedEdwardsY::from_slice(bytes.as_ref())
		.decompress()
		.is_some()
}
