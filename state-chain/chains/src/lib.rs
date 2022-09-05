#![cfg_attr(not(feature = "std"), no_std)]
use crate::benchmarking_value::BenchmarkValue;
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use eth::SchnorrVerificationComponents;
use ethereum_types::H256;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Blake2_256, Parameter, RuntimeDebug, StorageHasher,
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
		+ Saturating
		+ Into<u128>
		+ From<u128>
		+ FullCodec
		+ MaxEncodedLen;

	type TrackedData: Member + Parameter + MaxEncodedLen + Clone + Age<Self> + BenchmarkValue;

	type ChainAsset: Member + Parameter + MaxEncodedLen;

	type ChainAccount: Member + Parameter + MaxEncodedLen;
}

/// Measures the age of items associated with the Chain.
pub trait Age<C: Chain> {
	/// The creation block of this item.
	fn birth_block(&self) -> C::ChainBlockNumber;
}

/// Common crypto-related types and operations for some external chain.
pub trait ChainCrypto: Chain {
	/// The chain's `AggKey` format. The AggKey is the threshold key that controls the vault.
	/// TODO: Consider if Encode / Decode bounds are sufficient rather than To/From Vec<u8>
	type AggKey: TryFrom<Vec<u8>> + Into<Vec<u8>> + Member + Parameter + Copy + Ord + BenchmarkValue;
	type Payload: Member + Parameter + BenchmarkValue;
	type ThresholdSignature: Member + Parameter + BenchmarkValue;
	type TransactionHash: Member + Parameter + Default;
	type GovKey: Member + Parameter + Copy + BenchmarkValue;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool;

	/// We use the AggKey as the payload for keygen verification ceremonies.
	fn agg_key_to_payload(agg_key: Self::AggKey) -> Self::Payload;
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
	) -> Result<Self::TransactionHash, Self::ValidationError>;
}

/// A call or collection of calls that can be made to the Chainflip api on an external chain.
///
/// See [eth::api::EthereumApi] for an example implementation.
pub trait ApiCall<Abi: ChainAbi>: Parameter {
	/// Get the payload over which the threshold signature should be generated.
	fn threshold_signature_payload(&self) -> <Abi as ChainCrypto>::Payload;

	/// Add the threshold signature to the api call.
	fn signed(self, threshold_signature: &<Abi as ChainCrypto>::ThresholdSignature) -> Self;

	/// The call, encoded as a vector of bytes using the chain's native encoding.
	fn abi_encoded(&self) -> Vec<u8>;

	/// Checks we have updated the sig data to non-default values.
	fn is_signed(&self) -> bool;
}

/// Responsible for converting an api call into a raw unsigned transaction.
pub trait TransactionBuilder<Abi, Call>
where
	Abi: ChainAbi,
	Call: ApiCall<Abi>,
{
	/// Construct the unsigned outbound transaction from the *signed* api call.
	/// Doesn't include any time-sensitive data e.g. gas price.
	fn build_transaction(signed_call: &Call) -> Abi::UnsignedTransaction;

	/// Refresh any time-sensitive data e.g. gas price.
	fn refresh_unsigned_transaction(unsigned_tx: &mut Abi::UnsignedTransaction);
}

/// A general trait for initializing types required for Fetching Assets deposited by Users as part
/// of a swap for some External Chain.
/// #[derive(
#[derive(
	RuntimeDebug, Copy, Clone, Default, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
pub struct FetchAssetParams<T: Chain> {
	swap_id: [u8; 32],
	asset: T::ChainAsset,
}

/// A general trait for initializing types required for Transfering Assets to Users as part of a
/// swap for some External Chain.
#[derive(
	RuntimeDebug, Copy, Clone, Default, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
pub struct TransferAssetParams<T: Chain> {
	asset: T::ChainAsset,
	account: T::ChainAccount,
	amount: T::ChainAmount,
}

/// Constructs the `SetAggKeyWithAggKey` api call.
pub trait SetAggKeyWithAggKey<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(
		replay_protection: Abi::ReplayProtection,
		new_key: <Abi as ChainCrypto>::AggKey,
	) -> Self;
}

pub trait SetGovKeyWithAggKey<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(
		replay_protection: Abi::ReplayProtection,
		new_gov_key: <Abi as ChainCrypto>::GovKey,
	) -> Self;
}

pub trait SetCommKeyWithAggKey<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(
		replay_protection: Abi::ReplayProtection,
		new_comm_key: <Abi as ChainCrypto>::GovKey,
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

pub trait AllBatch<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(
		replay_protection: Abi::ReplayProtection,
		fetch_params: Vec<FetchAssetParams<Abi>>,
		transfer_params: Vec<TransferAssetParams<Abi>>,
	) -> Self;
}

#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct Ethereum;

impl Chain for Ethereum {
	type ChainBlockNumber = u64;
	type ChainAmount = u128;
	type TrackedData = eth::TrackedData<Self>;
	type ChainAccount = eth::Address;
	type ChainAsset = eth::Address;
}

impl ChainCrypto for Ethereum {
	type AggKey = eth::AggKey;
	type Payload = eth::H256;
	type ThresholdSignature = SchnorrVerificationComponents;
	type TransactionHash = eth::H256;
	type GovKey = eth::Address;

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

	fn agg_key_to_payload(agg_key: Self::AggKey) -> Self::Payload {
		H256(Blake2_256::hash(&agg_key.to_pubkey_compressed()))
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
		type ChainAccount = (); // Currently, we don't care about this since we don't use them in tests
		type ChainAsset = (); // Currently, we don't care about this since we don't use them in tests
	}

	#[derive(
		Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo,
	)]
	pub struct MockTrackedData(pub u64);

	impl Age<MockEthereum> for MockTrackedData {
		fn birth_block(&self) -> u64 {
			self.0
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl BenchmarkValue for [u8; 32] {
		fn benchmark_value() -> Self {
			[1u8; 32]
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
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
		type GovKey = [u8; 32];

		fn verify_threshold_signature(
			agg_key: &Self::AggKey,
			payload: &Self::Payload,
			signature: &Self::ThresholdSignature,
		) -> bool {
			signature.signing_key == *agg_key && signature.signed_payload == *payload
		}

		fn agg_key_to_payload(agg_key: Self::AggKey) -> Self::Payload {
			agg_key
		}
	}

	impl_default_benchmark_value!(Validity);
	impl_default_benchmark_value!([u8; 4]);
	impl_default_benchmark_value!(MockThresholdSignature<[u8; 4], [u8; 4]>);
	impl_default_benchmark_value!(u32);
	impl_default_benchmark_value!(MockUnsignedTransaction);

	pub const ETH_TX_HASH: <MockEthereum as ChainCrypto>::TransactionHash = [0xbc; 4];

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
		) -> Result<Self::TransactionHash, Self::ValidationError> {
			if *unsigned_tx == signed_tx.transaction &&
				signed_tx.signature.is_valid() &&
				signer_credential.is_valid()
			{
				Ok(ETH_TX_HASH)
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

		fn abi_encoded(&self) -> Vec<u8> {
			self.encode()
		}

		fn is_signed(&self) -> bool {
			self.1.is_some()
		}
	}

	pub struct MockTransactionBuilder<Abi, Call>(PhantomData<(Abi, Call)>);

	impl<Abi: ChainAbi, Call: ApiCall<Abi>> TransactionBuilder<Abi, Call>
		for MockTransactionBuilder<Abi, Call>
	{
		fn build_transaction(_signed_call: &Call) -> <Abi as ChainAbi>::UnsignedTransaction {
			Default::default()
		}

		fn refresh_unsigned_transaction(_unsigned_tx: &mut <Abi as ChainAbi>::UnsignedTransaction) {
			// refresh nothing
		}
	}
}
