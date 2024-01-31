use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

const SOLANA_TRANSACTION_SIZE: usize = 64;
const SOLANA_PUBLIC_KEY_SIZE: usize = 32;

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
pub struct SolTransaction(#[serde(with = "::serde_bytes")] [u8; SOLANA_TRANSACTION_SIZE]);

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
pub struct SolPublicKey(#[serde(with = "::serde_bytes")] [u8; SOLANA_PUBLIC_KEY_SIZE]);
