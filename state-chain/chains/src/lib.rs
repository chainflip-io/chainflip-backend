#![cfg_attr(not(feature = "std"), no_std)]

pub mod eth;

pub trait Chain {}

macro_rules! impl_chains {
	( $( $chain:ident ),+ ) => {
		use codec::{Decode, Encode};
		use sp_runtime::RuntimeDebug;

		#[derive(Copy, Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
		pub enum ChainId {
			$(
				$chain,
			)+
		}
		$(
			#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode)]
			pub struct $chain;

			impl Chain for $chain {}

			impl From<$chain> for ChainId {
				fn from(_: $chain) -> Self {
					ChainId::$chain
				}
			}
		)+
	};
}

impl_chains! {
	Ethereum
}

#[cfg(test)]
mod test_chains {
	use super::*;

	#[test]
	fn test_conversion() {
		assert_eq!(ChainId::from(Ethereum), ChainId::Ethereum);
	}
}
