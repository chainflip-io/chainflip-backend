#![cfg_attr(not(feature = "std"), no_std)]
#![feature(array_map)] // stable as of rust 1.55

use eth::SchnorrVerificationComponents;
use frame_support::pallet_prelude::Member;
use frame_support::Parameter;
use sp_std::convert::TryFrom;
use sp_std::prelude::*;

pub mod eth;

/// A trait representing all the types and constants that need to be implemented for supported blockchains.
pub trait Chain {
	/// The chain's `ChainId` - useful for serialization.
	const CHAIN_ID: ChainId;
}

pub trait ChainCrypto: Chain {
	/// The chain's `AggKey` format. The AggKey is the threshold key that controls the vault.
	type AggKey: Into<Vec<u8>> + TryFrom<Vec<u8>> + Member + Parameter;
	type Payload: Member + Parameter;
	type ThresholdSignature: Member + Parameter;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool;
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

impl ChainCrypto for Ethereum {
	type AggKey = eth::AggKey;
	type Payload = [u8; 32];
	type ThresholdSignature = SchnorrVerificationComponents;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool {
		agg_key
			.verify(payload, signature)
			.map_err(|e| {
				frame_support::debug::debug!("Ethereum signature verification failed: {:?}.", e)
			})
			.is_ok()
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
