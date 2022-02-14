#![cfg_attr(not(feature = "std"), no_std)]
#![feature(array_map)] // stable as of rust 1.55

use eth::SchnorrVerificationComponents;
use frame_support::{pallet_prelude::Member, Parameter};
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::{
	convert::{Into, TryFrom},
	fmt::Debug,
	prelude::*,
};

pub mod eth;

/// A trait representing all the types and constants that need to be implemented for supported
/// blockchains.
pub trait Chain: Member + Parameter {}

/// Common crypto-related types and operations for some external chain.
pub trait ChainCrypto: Chain {
	/// The chain's `AggKey` format. The AggKey is the threshold key that controls the vault.
	/// TODO: Consider if Encode / Decode bounds are sufficient rather than To/From Vec<u8>
	type AggKey: TryFrom<Vec<u8>> + Into<Vec<u8>> + Member + Parameter + Copy + Ord;
	type Payload: Member + Parameter;
	type ThresholdSignature: Member + Parameter;
	type TransactionHash: Member + Parameter;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool;
}

/// Common abi-related types and operations for some external chain.
pub trait ChainAbi: ChainCrypto {
	type UnsignedTransaction: Member + Parameter + Default;
	type SignedTransaction: Member + Parameter;
	type SignerCredential: Member + Parameter;
	type Nonce: Member + Parameter + AtLeast32BitUnsigned;
	type ValidationError;

	/// Verify the signed transaction when it is submitted to the state chain by the nominated
	/// signer.
	///
	/// 'Verification' here is loosely defined as whatever is deemed necessary to accept the
	/// validaty of the returned transaction for this `Chain` and can include verification of the
	/// byte encoding, the transaction content, metadata, signer idenity, etc.
	fn verify_signed_transaction(
		unsigned_tx: &Self::UnsignedTransaction,
		signed_tx: &Self::SignedTransaction,
		signer_credential: &Self::SignerCredential,
	) -> Result<(), Self::ValidationError>;
}

/// A call or collection of calls that can be made to the Chainflip api on an external chain.
///
/// See [eth::api::EthereumApi] for an example implementation.
pub trait ApiCall<Abi: ChainAbi>: Parameter {
	/// Get the payload over which the threshold signature should be generated.
	fn threshold_signature_payload(&self) -> <Abi as ChainCrypto>::Payload;

	/// Add the threshold signature to the api call.
	fn signed(self, threshold_signature: &<Abi as ChainCrypto>::ThresholdSignature) -> Self;

	///
	fn encoded(&self) -> Vec<u8>;
}

/// Responsible for converting an api call into a raw unsigned transaction.
pub trait TransactionBuilder<Abi, Call>
where
	Abi: ChainAbi,
	Call: ApiCall<Abi>,
{
	/// Construct the unsigned outbound transaction from the *signed* api call.
	fn build_transaction(signed_call: &Call) -> Abi::UnsignedTransaction;
}

/// Constructs the `SetAggKeyWithAggKey` api call.
pub trait SetAggKeyWithAggKey<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(nonce: Abi::Nonce, new_key: <Abi as ChainCrypto>::AggKey) -> Self;
}

/// Constructs the `UpdateFlipSupply` api call.
pub trait UpdateFlipSupply<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(nonce: Abi::Nonce, new_total_supply: u128, block_number: u64) -> Self;
}

/// Constructs the `RegisterClaim` api call.
pub trait RegisterClaim<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(
		nonce: Abi::Nonce,
		node_id: &[u8; 32],
		amount: u128,
		address: &[u8; 20],
		expiry: u64,
	) -> Self;

	fn amount(&self) -> u128;
}

macro_rules! impl_chains {
	( $( $chain:ident ),+ $(,)? ) => {
		use codec::{Decode, Encode};
		use sp_runtime::RuntimeDebug;

		$(
			#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode)]
			pub struct $chain;

			impl Chain for $chain {}
		)+
	};
}

impl_chains! {
	Ethereum,
}

impl ChainCrypto for Ethereum {
	type AggKey = eth::AggKey;
	type Payload = eth::H256;
	type ThresholdSignature = SchnorrVerificationComponents;
	type TransactionHash = eth::H256;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool {
		agg_key
			.verify(payload.as_fixed_bytes(), signature)
			.map_err(|e| log::debug!("Ethereum signature verification failed: {:?}.", e))
			.is_ok()
	}
}

pub mod mocks {
	use std::marker::PhantomData;

	use crate::*;

	// Chain implementation used for testing.
	impl_chains! {
		MockEthereum,
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, Default)]
	pub struct MockUnsignedTransaction;

	impl MockUnsignedTransaction {
		/// Simulate a transaction signature.
		pub fn signed(self, signature: Validity) -> MockSignedTransation<Self> {
			MockSignedTransation::<Self> { transaction: self, signature }
		}
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
	pub struct MockSignedTransation<Unsigned> {
		transaction: Unsigned,
		signature: Validity,
	}

	impl Default for Validity {
		fn default() -> Self {
			Self::Invalid
		}
	}

	impl Validity {
		pub fn is_valid(&self) -> bool {
			*self == Self::Valid
		}
	}

	#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
	pub enum Validity {
		Valid,
		Invalid,
	}

	#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
	pub struct MockThresholdSignature<K, P> {
		signing_key: K,
		signed_payload: P,
	}

	impl ChainCrypto for MockEthereum {
		type AggKey = [u8; 4];
		type Payload = [u8; 4];
		type ThresholdSignature = MockThresholdSignature<Self::AggKey, Self::Payload>;
		type TransactionHash = [u8; 4];

		fn verify_threshold_signature(
			agg_key: &Self::AggKey,
			payload: &Self::Payload,
			signature: &Self::ThresholdSignature,
		) -> bool {
			signature.signing_key == *agg_key && signature.signed_payload == *payload
		}
	}

	impl ChainAbi for MockEthereum {
		type UnsignedTransaction = MockUnsignedTransaction;
		type SignedTransaction = MockSignedTransation<Self::UnsignedTransaction>;
		type SignerCredential = Validity;
		type Nonce = u32;
		type ValidationError = &'static str;

		fn verify_signed_transaction(
			unsigned_tx: &Self::UnsignedTransaction,
			signed_tx: &Self::SignedTransaction,
			signer_credential: &Self::SignerCredential,
		) -> Result<(), Self::ValidationError> {
			if *unsigned_tx == signed_tx.transaction &&
				signed_tx.signature.is_valid() &&
				signer_credential.is_valid()
			{
				Ok(())
			} else {
				Err("MockEthereum::ValidationError")
			}
		}
	}

	#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
	pub struct MockApiCall<C: ChainCrypto>(C::Payload, Option<C::ThresholdSignature>);

	impl<C: ChainAbi> ApiCall<C> for MockApiCall<C> {
		fn threshold_signature_payload(&self) -> <C as ChainCrypto>::Payload {
			self.0.clone()
		}

		fn signed(self, threshold_signature: &<C as ChainCrypto>::ThresholdSignature) -> Self {
			Self(self.0, Some(threshold_signature.clone()))
		}

		fn encoded(&self) -> Vec<u8> {
			self.encode()
		}
	}

	pub struct MockTransactionBuilder<Abi, Call>(PhantomData<(Abi, Call)>);

	impl<Abi: ChainAbi, Call: ApiCall<Abi>> TransactionBuilder<Abi, Call>
		for MockTransactionBuilder<Abi, Call>
	{
		fn build_transaction(_signed_call: &Call) -> <Abi as ChainAbi>::UnsignedTransaction {
			Default::default()
		}
	}
}
