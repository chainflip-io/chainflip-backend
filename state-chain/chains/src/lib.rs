#![cfg_attr(not(feature = "std"), no_std)]
#![feature(array_map)] // stable as of rust 1.55

pub mod eth;

pub trait Chain {
	const CHAIN_ID: ChainId;
}

macro_rules! impl_chains {
	( $( $chain:ident ),+ $(,)? ) => {
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

			impl Chain for $chain {
				const CHAIN_ID: ChainId = ChainId::$chain;
			}
		)+
	};
}

impl_chains! {
	Ethereum,
}

impl<C: Chain> From<C> for ChainId {
	fn from(_: C) -> Self {
		C::CHAIN_ID
	}
}

#[cfg(test)]
mod test_chains {
	use super::*;

	#[test]
	fn test_conversion() {
		assert_eq!(ChainId::from(Ethereum), ChainId::Ethereum);
		assert_eq!(Ethereum::CHAIN_ID, ChainId::Ethereum);
	}
}
