use super::*;

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
	Polkadot,
	Any
}

#[test]
fn test_chains() {
	assert_eq!(Ethereum.as_ref(), &ForeignChain::Ethereum);
	assert_eq!(Polkadot.as_ref(), &ForeignChain::Polkadot);
	assert_eq!(Any.as_ref(), &ForeignChain::Any);
}
