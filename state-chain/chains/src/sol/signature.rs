use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

use super::consts::SOLANA_TRANSACTION_SIZE;

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
pub struct SolSignature(#[serde(with = "::serde_bytes")] [u8; SOLANA_TRANSACTION_SIZE]);
