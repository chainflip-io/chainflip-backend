use core::str::FromStr;
use digest::Digest;
use sha2::Sha256;

use crate::{address::Address, consts, AccountBump};

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

	pub fn finish(self) -> Result<(Address, AccountBump), PdaError> {
		for bump in (0..=AccountBump::MAX).rev() {
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

pub fn derive_fetch_account(
	vault_program: Address,
	deposit_channel: Address,
) -> Result<Address, PdaError> {
	let (fetch_account, _) =
		Pda::from_address(vault_program)?.chain_seed(deposit_channel)?.finish()?;

	Ok(fetch_account)
}

#[allow(dead_code)]
/// Derive a Associated Token Account (ATA) of a main account.
pub fn derive_associated_token_account(
	address: Address,
	mint_pubkey: Address,
) -> Result<(Address, AccountBump), PdaError> {
	let associated_token_program_id = Address::from_str(consts::ASSOCIATED_TOKEN_PROGRAM_ID)
		.expect("Associated token program ID must be valid");
	let token_program_id =
		Address::from_str(consts::TOKEN_PROGRAM_ID).expect("Token program ID must be valid");

	Pda::from_address(associated_token_program_id)?
		.chain_seed(address)?
		.chain_seed(token_program_id)?
		.chain_seed(mint_pubkey)?
		.finish()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_sol_derive_fetch_account() {
		let fetch_account = derive_fetch_account(
			Address::from_str("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf").unwrap(),
			Address::from_str("HAMxiXdEJxiBHabZAUm8PSLvWQM2GHi5PArVZvUCeDab").unwrap(),
		)
		.unwrap();
		assert_eq!(
			fetch_account,
			Address::from_str("HGgUaHpsmZpB3pcYt8PE89imca6BQBRqYtbVQQqsso3o").unwrap()
		);
	}

	#[test]
	fn derive_associated_token_account_on_curve() {
		let wallet_address =
			Address::from_str("HfasueN6RNPjSM6rKGH5dga6kS2oUF8siGH3m4MXPURp").unwrap();
		let mint_pubkey =
			Address::from_str("24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p").unwrap();

		assert_eq!(
			derive_associated_token_account(wallet_address, mint_pubkey).unwrap(),
			(Address::from_str("BeRexE9vZSdQMNg65PAnhy3rRPUxF6oWsxyNegYxySZD").unwrap(), 253u8)
		);
	}

	#[test]
	fn derive_associated_token_account_off_curve() {
		let pda_address =
			Address::from_str("9j17hjg8wR2uFxJAJDAFahwsgTCNx35sc5qXSxDmuuF6").unwrap();
		let mint_pubkey =
			Address::from_str("24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p").unwrap();

		assert_eq!(
			derive_associated_token_account(pda_address, mint_pubkey).unwrap(),
			(Address::from_str("DUjCLckPi4g7QAwBEwuFL1whpgY6L9fxwXnqbWvS2pcW").unwrap(), 251u8)
		);
	}
}
