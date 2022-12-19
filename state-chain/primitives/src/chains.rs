use super::*;
use frame_support::traits::Get;

pub mod assets;

macro_rules! chains {
	( $( $chain:ident ),+ ) => {
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
				$chain,
			)+
		}
	}
}

/// Can be any Chain.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct AnyChain;

chains! {
	Ethereum,
	Polkadot
}

#[test]
fn test_chains() {
	assert_eq!(Ethereum.as_ref(), &ForeignChain::Ethereum);
	assert_eq!(Polkadot.as_ref(), &ForeignChain::Polkadot);
}

#[test]
fn test_get_chain_identifier() {
	assert_eq!(Ethereum::get(), ForeignChain::Ethereum);
	assert_eq!(Polkadot::get(), ForeignChain::Polkadot);
}
