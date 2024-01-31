use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

const SOLANA_TRANSACTION_SIZE: usize = 64;

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
pub struct SolTransaction {
	#[serde(with = "::serde_bytes")]
	bytes: [u8; SOLANA_TRANSACTION_SIZE],
}
