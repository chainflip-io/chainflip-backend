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

use super::*;
pub use frame_support::traits::Get;
use sp_std::{fmt, fmt::Display, str::FromStr};

pub mod assets;

macro_rules! chains {
	( $( $chain:ident = $index:literal),+ ) => {
		$(
			#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Ord, PartialOrd)]
			pub struct $chain;

			impl AsRef<ForeignChain> for $chain {
				fn as_ref(&self) -> &ForeignChain {
					&ForeignChain::$chain
				}
			}

			impl Get<ForeignChain> for $chain {
				fn get() -> ForeignChain {
					ForeignChain::$chain
				}
			}

			impl From<$chain> for ForeignChain {
				fn from(_: $chain) -> ForeignChain {
					ForeignChain::$chain
				}
			}
		)+

		#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo, MaxEncodedLen, Copy, Hash)]
		#[derive(Serialize, Deserialize)]
		#[repr(u32)]
		pub enum ForeignChain {
			$(
				$chain = $index,
			)+
		}

		impl ForeignChain {
			pub fn iter() -> impl Iterator<Item = Self> {
				[
					$( ForeignChain::$chain, )+
				].into_iter()
			}
		}

		impl TryFrom<u32> for ForeignChain {
			type Error = &'static str;

			fn try_from(index: u32) -> Result<Self, Self::Error> {
				match index {
					$(
						x if x == Self::$chain as u32 => Ok(Self::$chain),
					)+
					_ => Err("Invalid foreign chain"),
				}
			}
		}

		impl FromStr for ForeignChain {
			type Err = &'static str;

			fn from_str(s: &str) -> Result<Self, Self::Err> {
				let s = s.to_lowercase();
				$(
					if s == stringify!($chain).to_lowercase() {
						return Ok(ForeignChain::$chain);
					}
				)+
				Err("Unrecognized Chain")
			}
		}

		impl Display for ForeignChain {
			fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
				match self {
					$(
						ForeignChain::$chain => write!(f, "{}", stringify!($chain)),
					)+
				}
			}
		}
	}
}

// !!!!! IMPORTANT !!!!!
// Do not change these indices.
chains! {
	Ethereum = 1,
	Polkadot = 2,
	Bitcoin = 3,
	Arbitrum = 4,
	Solana = 5,
	Assethub = 6
}

/// Can be any Chain.
#[derive(
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Copy,
	Serialize,
	Deserialize,
)]
pub struct AnyChain;

impl ForeignChain {
	pub const fn gas_asset(self) -> assets::any::Asset {
		match self {
			ForeignChain::Ethereum => assets::any::Asset::Eth,
			ForeignChain::Polkadot => assets::any::Asset::Dot,
			ForeignChain::Bitcoin => assets::any::Asset::Btc,
			ForeignChain::Arbitrum => assets::any::Asset::ArbEth,
			ForeignChain::Solana => assets::any::Asset::Sol,
			ForeignChain::Assethub => assets::any::Asset::HubDot,
		}
	}
	pub const fn ccm_support(self) -> bool {
		match self {
			ForeignChain::Ethereum => true,
			ForeignChain::Polkadot => false,
			ForeignChain::Bitcoin => false,
			ForeignChain::Arbitrum => true,
			ForeignChain::Solana => true,
			ForeignChain::Assethub => true,
		}
	}
}

#[test]
fn chain_as_u32() {
	assert_eq!(ForeignChain::Ethereum as u32, 1);
	assert_eq!(ForeignChain::Polkadot as u32, 2);
	assert_eq!(ForeignChain::Bitcoin as u32, 3);
	assert_eq!(ForeignChain::Arbitrum as u32, 4);
	assert_eq!(ForeignChain::Solana as u32, 5);
	assert_eq!(ForeignChain::Assethub as u32, 6);
}

#[test]
fn chain_id_to_chain() {
	assert_eq!(ForeignChain::try_from(1), Ok(ForeignChain::Ethereum));
	assert_eq!(ForeignChain::try_from(2), Ok(ForeignChain::Polkadot));
	assert_eq!(ForeignChain::try_from(3), Ok(ForeignChain::Bitcoin));
	assert_eq!(ForeignChain::try_from(4), Ok(ForeignChain::Arbitrum));
	assert_eq!(ForeignChain::try_from(5), Ok(ForeignChain::Solana));
	assert_eq!(ForeignChain::try_from(6), Ok(ForeignChain::Assethub));
	assert!(ForeignChain::try_from(7).is_err());
}

#[test]
fn test_chains() {
	assert_eq!(Ethereum.as_ref(), &ForeignChain::Ethereum);
	assert_eq!(Polkadot.as_ref(), &ForeignChain::Polkadot);
	assert_eq!(Bitcoin.as_ref(), &ForeignChain::Bitcoin);
	assert_eq!(Arbitrum.as_ref(), &ForeignChain::Arbitrum);
	assert_eq!(Solana.as_ref(), &ForeignChain::Solana);
	assert_eq!(Assethub.as_ref(), &ForeignChain::Assethub);
}

#[test]
fn test_get_chain_identifier() {
	assert_eq!(Ethereum::get(), ForeignChain::Ethereum);
	assert_eq!(Polkadot::get(), ForeignChain::Polkadot);
	assert_eq!(Bitcoin::get(), ForeignChain::Bitcoin);
	assert_eq!(Arbitrum::get(), ForeignChain::Arbitrum);
	assert_eq!(Solana::get(), ForeignChain::Solana);
	assert_eq!(Assethub::get(), ForeignChain::Assethub);
}

#[test]
fn test_chain_to_and_from_str() {
	assert_eq!(
		ForeignChain::from_str(ForeignChain::Ethereum.to_string().as_str()).unwrap(),
		ForeignChain::Ethereum
	);
	assert_eq!(
		ForeignChain::from_str(ForeignChain::Polkadot.to_string().as_str()).unwrap(),
		ForeignChain::Polkadot
	);
	assert_eq!(
		ForeignChain::from_str(ForeignChain::Bitcoin.to_string().as_str()).unwrap(),
		ForeignChain::Bitcoin
	);
	assert_eq!(
		ForeignChain::from_str(ForeignChain::Arbitrum.to_string().as_str()).unwrap(),
		ForeignChain::Arbitrum
	);
	assert_eq!(
		ForeignChain::from_str(ForeignChain::Solana.to_string().as_str()).unwrap(),
		ForeignChain::Solana
	);
	assert_eq!(
		ForeignChain::from_str(ForeignChain::Assethub.to_string().as_str()).unwrap(),
		ForeignChain::Assethub
	);
}
