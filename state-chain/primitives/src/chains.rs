use super::*;
pub use frame_support::traits::Get;
use sp_std::str::FromStr;

pub mod assets;

macro_rules! chains {
	( $( $chain:ident = $value:literal),+ ) => {
		$(
			#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
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

		#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
		#[derive(Serialize, Deserialize)]
		#[repr(u32)]
		pub enum ForeignChain {
			$(
				$chain = $value,
			)+
		}

		impl TryFrom<u32> for ForeignChain {
			type Error = &'static str;

			fn try_from(value: u32) -> Result<Self, Self::Error> {
				match value {
					$(
						x if x == Self::$chain as u32 => Ok(Self::$chain),
					)+
					_ => Err("Invalid foreign chain"),
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
	Arbitrum = 4
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
	pub fn gas_asset(self) -> assets::any::Asset {
		match self {
			ForeignChain::Ethereum => assets::any::Asset::Eth,
			ForeignChain::Polkadot => assets::any::Asset::Dot,
			ForeignChain::Bitcoin => assets::any::Asset::Btc,
			ForeignChain::Arbitrum => assets::any::Asset::ArbEth,
		}
	}
}

impl FromStr for ForeignChain {
	type Err = &'static str;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.to_lowercase().as_str() {
			"ethereum" => Ok(ForeignChain::Ethereum),
			"bitcoin" => Ok(ForeignChain::Bitcoin),
			"polkadot" => Ok(ForeignChain::Polkadot),
			_ => Err("Unrecognized Chain"),
		}
	}
}

#[test]
fn chain_as_u32() {
	assert_eq!(ForeignChain::Ethereum as u32, 1);
	assert_eq!(ForeignChain::Polkadot as u32, 2);
	assert_eq!(ForeignChain::Bitcoin as u32, 3);
	assert_eq!(ForeignChain::Arbitrum as u32, 4);
}

#[test]
fn chain_id_to_chain() {
	assert_eq!(ForeignChain::try_from(1), Ok(ForeignChain::Ethereum));
	assert_eq!(ForeignChain::try_from(2), Ok(ForeignChain::Polkadot));
	assert_eq!(ForeignChain::try_from(3), Ok(ForeignChain::Bitcoin));
	assert_eq!(ForeignChain::try_from(4), Ok(ForeignChain::Arbitrum));
	assert!(ForeignChain::try_from(5).is_err());
}

#[test]
fn test_chains() {
	assert_eq!(Ethereum.as_ref(), &ForeignChain::Ethereum);
	assert_eq!(Polkadot.as_ref(), &ForeignChain::Polkadot);
	assert_eq!(Bitcoin.as_ref(), &ForeignChain::Bitcoin);
	assert_eq!(Arbitrum.as_ref(), &ForeignChain::Arbitrum);
}

#[test]
fn test_get_chain_identifier() {
	assert_eq!(Ethereum::get(), ForeignChain::Ethereum);
	assert_eq!(Polkadot::get(), ForeignChain::Polkadot);
	assert_eq!(Bitcoin::get(), ForeignChain::Bitcoin);
	assert_eq!(Arbitrum::get(), ForeignChain::Arbitrum);
}
