#![cfg_attr(not(feature = "std"), no_std)]
use crate::benchmarking_value::BenchmarkValue;
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use eth::SchnorrVerificationComponents;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter, RuntimeDebug,
};
use scale_info::TypeInfo;
use sp_runtime::traits::{AtLeast32BitUnsigned, Saturating};
use sp_std::{
	convert::{Into, TryFrom},
	fmt::Debug,
	prelude::*,
};

pub mod benchmarking_value;

pub mod eth;

/// A trait representing all the types and constants that need to be implemented for supported
/// blockchains.
pub trait Chain: Member + Parameter {
	type ChainBlockNumber: FullCodec
		+ Member
		+ Parameter
		+ Copy
		+ MaybeSerializeDeserialize
		+ AtLeast32BitUnsigned
		+ From<u64>
		+ MaxEncodedLen;

	type ChainAmount: Member
		+ Parameter
		+ Copy
		+ Default
		+ Into<u128>
		+ From<u128>
		+ Saturating
		+ FullCodec
		+ MaxEncodedLen;

	type TrackedData: Member
		+ Parameter
		+ MaxEncodedLen
		+ Clone
		+ IndexedBy<Self::ChainBlockNumber>
		+ BenchmarkValue;
}

/// Something that can be ordered by an index.
pub trait IndexedBy<I: Ord> {
	fn index(&self) -> I;
}

/// Common crypto-related types and operations for some external chain.
pub trait ChainCrypto: Chain {
	/// The chain's `AggKey` format. The AggKey is the threshold key that controls the vault.
	/// TODO: Consider if Encode / Decode bounds are sufficient rather than To/From Vec<u8>
	type AggKey: TryFrom<Vec<u8>> + Into<Vec<u8>> + Member + Parameter + Copy + Ord + BenchmarkValue;
	type Payload: Member + Parameter + BenchmarkValue;
	type ThresholdSignature: Member + Parameter + BenchmarkValue;
	type TransactionHash: Member + Parameter + Default;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool;
}

/// Common abi-related types and operations for some external chain.
pub trait ChainAbi: ChainCrypto {
	type UnsignedTransaction: Member + Parameter + Default + BenchmarkValue;
	type SignedTransaction: Member + Parameter + BenchmarkValue;
	type SignerCredential: Member + Parameter + BenchmarkValue;
	type ReplayProtection: Member + Parameter + Default;
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
pub trait ApiCall<Abi: ChainAbi>: Parameter + MaxEncodedLen {
	/// Get the payload over which the threshold signature should be generated.
	fn threshold_signature_payload(&self) -> <Abi as ChainCrypto>::Payload;

	/// Add the threshold signature to the api call.
	fn signed(self, threshold_signature: &<Abi as ChainCrypto>::ThresholdSignature) -> Self;

	/// The call, encoded as a vector of bytes using the chain's native encoding.
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
	fn new_unsigned(
		replay_protection: Abi::ReplayProtection,
		new_key: <Abi as ChainCrypto>::AggKey,
	) -> Self;
}

/// Constructs the `UpdateFlipSupply` api call.
pub trait UpdateFlipSupply<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(
		replay_protection: Abi::ReplayProtection,
		new_total_supply: u128,
		block_number: u64,
		stake_manager_address: &[u8; 20],
	) -> Self;
}

/// Constructs the `RegisterClaim` api call.
pub trait RegisterClaim<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(
		replay_protection: Abi::ReplayProtection,
		node_id: &[u8; 32],
		amount: u128,
		address: &[u8; 20],
		expiry: u64,
	) -> Self;

	fn amount(&self) -> u128;
}

#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct Ethereum;

impl Chain for Ethereum {
	type ChainBlockNumber = u64;
	type ChainAmount = u128;
	type TrackedData = eth::TrackedData<Self>;
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
	use sp_std::marker::PhantomData;

	use crate::{eth::api::EthereumReplayProtection, *};

	#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct MockEthereum;

	// Chain implementation used for testing.
	impl Chain for MockEthereum {
		type ChainBlockNumber = u64;
		type ChainAmount = u128;
		type TrackedData = MockTrackedData;
	}

	#[derive(
		Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo,
	)]
	pub struct MockTrackedData(u64);

	impl IndexedBy<u64> for MockTrackedData {
		fn index(&self) -> u64 {
			self.0
		}
	}

	impl BenchmarkValue for MockTrackedData {
		fn benchmark_value() -> Self {
			Self(1_000)
		}
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Default)]
	pub struct MockUnsignedTransaction;

	impl MockUnsignedTransaction {
		/// Simulate a transaction signature.
		pub fn signed(self, signature: Validity) -> MockSignedTransation<Self> {
			MockSignedTransation::<Self> { transaction: self, signature }
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl BenchmarkValue for MockSignedTransation<MockUnsignedTransaction> {
		fn benchmark_value() -> Self {
			MockSignedTransation {
				transaction: MockUnsignedTransaction::default(),
				signature: Validity::Valid,
			}
		}
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
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

	#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum Validity {
		Valid,
		Invalid,
	}

	#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Encode, Decode, TypeInfo)]
	pub struct MockThresholdSignature<K, P> {
		pub signing_key: K,
		pub signed_payload: P,
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

	impl_default_benchmark_value!(Validity);
	impl_default_benchmark_value!([u8; 4]);
	impl_default_benchmark_value!(MockThresholdSignature<[u8; 4], [u8; 4]>);
	impl_default_benchmark_value!(u32);
	impl_default_benchmark_value!(MockUnsignedTransaction);

	impl ChainAbi for MockEthereum {
		type UnsignedTransaction = MockUnsignedTransaction;
		type SignedTransaction = MockSignedTransation<Self::UnsignedTransaction>;
		type SignerCredential = Validity;
		type ReplayProtection = EthereumReplayProtection;
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

	#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct MockApiCall<C: ChainCrypto>(C::Payload, Option<C::ThresholdSignature>);

	#[cfg(feature = "runtime-benchmarks")]
	impl<C: ChainCrypto> BenchmarkValue for MockApiCall<C> {
		fn benchmark_value() -> Self {
			Self(C::Payload::benchmark_value(), Some(C::ThresholdSignature::benchmark_value()))
		}
	}

	impl<C: ChainCrypto> MaxEncodedLen for MockApiCall<C> {
		fn max_encoded_len() -> usize {
			<[u8; 32]>::max_encoded_len() * 3
		}
	}

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
