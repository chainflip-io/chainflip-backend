use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

use super::consts::SOLANA_SIGNATURE_SIZE;

#[derive(
	Debug,
	Clone,
	Copy,
	PartialEq,
	Eq,
	TypeInfo,
	Encode,
	Decode,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct SolSignature(#[serde(with = "::serde_bytes")] [u8; SOLANA_SIGNATURE_SIZE]);

impl From<[u8; SOLANA_SIGNATURE_SIZE]> for SolSignature {
	fn from(value: [u8; SOLANA_SIGNATURE_SIZE]) -> Self {
		Self(value)
	}
}
impl From<SolSignature> for [u8; SOLANA_SIGNATURE_SIZE] {
	fn from(value: SolSignature) -> Self {
		value.0
	}
}

impl core::str::FromStr for SolSignature {
	type Err = &'static str;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let bytes = base58::FromBase58::from_base58(s).map_err(|_| "bad base58")?;
		Ok(Self(bytes.try_into().map_err(|_| "invalid length")?))
	}
}

impl core::fmt::Display for SolSignature {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(f, "{}", base58::ToBase58::to_base58(&self.0[..]))
	}
}
