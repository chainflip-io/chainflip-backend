use super::*;
use frame_support::traits::Get;

pub mod assets;

macro_rules! chains {
	( $( $chain:ident = $value:expr),+ ) => {
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
		)+

		#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
		#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
		pub enum ForeignChain {
			$(
				$chain = $value,
			)+
		}
	}
}

chains! {
	Ethereum = 1,
	Polkadot = 2,
	Bitcoin = 3
}

/// Can be any Chain.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct AnyChain;

impl ForeignChain {
	pub fn gas_asset(self) -> assets::any::Asset {
		match self {
			ForeignChain::Ethereum => assets::any::Asset::Eth,
			ForeignChain::Polkadot => assets::any::Asset::Dot,
			ForeignChain::Bitcoin => assets::any::Asset::Btc,
		}
	}
}

impl TryFrom<u32> for ForeignChain {
	type Error = &'static str;

	fn try_from(value: u32) -> Result<Self, Self::Error> {
		match value {
			x if x == Self::Ethereum as u32 => Ok(Self::Ethereum),
			x if x == Self::Polkadot as u32 => Ok(Self::Polkadot),
			x if x == Self::Bitcoin as u32 => Ok(Self::Bitcoin),
			_ => Err("Invalid foreign chain"),
		}
	}
}

#[test]
fn chain_id_to_chain() {
	assert_eq!(ForeignChain::try_from(1), Ok(ForeignChain::Ethereum));
	assert_eq!(ForeignChain::try_from(2), Ok(ForeignChain::Polkadot));
	assert_eq!(ForeignChain::try_from(3), Ok(ForeignChain::Bitcoin));
	assert!(ForeignChain::try_from(4).is_err());
}

#[test]
fn test_chains() {
	assert_eq!(Ethereum.as_ref(), &ForeignChain::Ethereum);
	assert_eq!(Polkadot.as_ref(), &ForeignChain::Polkadot);
	assert_eq!(Bitcoin.as_ref(), &ForeignChain::Bitcoin);
}

#[test]
fn test_get_chain_identifier() {
	assert_eq!(Ethereum::get(), ForeignChain::Ethereum);
	assert_eq!(Polkadot::get(), ForeignChain::Polkadot);
	assert_eq!(Bitcoin::get(), ForeignChain::Bitcoin);
}
