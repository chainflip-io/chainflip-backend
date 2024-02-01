use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

use crate::address;

use super::consts::SOLANA_PUBLIC_KEY_SIZE;

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
pub struct SolPublicKey(#[serde(with = "::serde_bytes")] [u8; SOLANA_PUBLIC_KEY_SIZE]);

impl From<address::ForeignChainAddress> for SolPublicKey {
	fn from(_address: address::ForeignChainAddress) -> Self {
		unimplemented!()
	}
}
impl From<SolPublicKey> for address::ForeignChainAddress {
	fn from(_value: SolPublicKey) -> Self {
		unimplemented!()
	}
}
impl address::ToHumanreadableAddress for SolPublicKey {
	#[cfg(feature = "std")]
	type Humanreadable = Self;

	#[cfg(feature = "std")]
	fn to_humanreadable(
		&self,
		_network_environment: cf_primitives::NetworkEnvironment,
	) -> Self::Humanreadable {
		*self
	}
}
