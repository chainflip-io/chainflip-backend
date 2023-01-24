#![cfg_attr(not(feature = "std"), no_std)]
use core::fmt::Display;

use crate::benchmarking_value::BenchmarkValue;
use cf_primitives::{chains::assets, AssetAmount, EthAmount, IntentId};
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
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
	vec,
};

pub use cf_primitives::chains::*;

pub mod benchmarking_value;

pub mod any;
pub mod dot;
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
		// this is used primarily for tests. We use u32 because it's the smallest block number we
		// use (and so we can always .into() into a larger type)
		+ From<u32>
		+ MaxEncodedLen
		+ Display;

	type ChainAmount: Member
		+ Parameter
		+ Copy
		+ Default
		+ Saturating
		+ Into<u128>
		+ From<u128>
		+ FullCodec
		+ MaxEncodedLen;

	type TransactionFee: Member + Parameter + MaxEncodedLen + BenchmarkValue;

	type TrackedData: Member + Parameter + MaxEncodedLen + Clone + Age<Self> + BenchmarkValue;

	type ChainAsset: Member
		+ Parameter
		+ MaxEncodedLen
		+ Copy
		+ BenchmarkValue
		+ Into<cf_primitives::Asset>
		+ Into<cf_primitives::ForeignChain>;

	type ChainAccount: Member
		+ Parameter
		+ MaxEncodedLen
		+ BenchmarkValue
		+ TryFrom<cf_primitives::ForeignChainAddress>
		+ Into<cf_primitives::ForeignChainAddress>;

	type EpochStartData: Member + Parameter + MaxEncodedLen;
}

/// Measures the age of items associated with the Chain.
pub trait Age<C: Chain> {
	/// The creation block of this item.
	fn birth_block(&self) -> C::ChainBlockNumber;
}

impl<C: Chain> Age<C> for () {
	fn birth_block(&self) -> C::ChainBlockNumber {
		unimplemented!()
	}
}

/// Common crypto-related types and operations for some external chain.
pub trait ChainCrypto: Chain {
	type KeyId: Member + Parameter;
	/// The chain's `AggKey` format. The AggKey is the threshold key that controls the vault.
	/// TODO: Consider if Encode / Decode bounds are sufficient rather than To/From Vec<u8>
	type AggKey: TryFrom<Vec<u8>>
		+ Into<Self::KeyId>
		+ Member
		+ Parameter
		+ Copy
		+ Ord
		+ Default // the "zero" address
		+ BenchmarkValue;
	type Payload: Member + Parameter + BenchmarkValue;
	type ThresholdSignature: Member + Parameter + BenchmarkValue;
	/// Must uniquely identify a transaction. On most chains this will be a transaction hash.
	/// However, for example, in the case of Polkadot, the blocknumber-extrinsic-index is the unique
	/// identifier.
	type TransactionId: Member + Parameter + BenchmarkValue;
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
	type Transaction: Member + Parameter + Default + BenchmarkValue + FeeRefundCalculator<Self>;
	type ReplayProtection: Member + Parameter;
}

/// Provides chain-specific replay protection data.
pub trait ReplayProtectionProvider<Abi: ChainAbi> {
	fn replay_protection() -> Abi::ReplayProtection;
}

/// A call or collection of calls that can be made to the Chainflip api on an external chain.
///
/// See [eth::api::EthereumApi] for an example implementation.
pub trait ApiCall<Abi: ChainAbi>: Parameter {
	/// Get the payload over which the threshold signature should be generated.
	fn threshold_signature_payload(&self) -> <Abi as ChainCrypto>::Payload;

	/// Add the threshold signature to the api call.
	fn signed(self, threshold_signature: &<Abi as ChainCrypto>::ThresholdSignature) -> Self;

	/// The call, encoded according to the chain's native encoding.
	fn chain_encoded(&self) -> Vec<u8>;

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
	fn build_transaction(signed_call: &Call) -> Abi::Transaction;

	/// Refresh any time-sensitive data e.g. gas price.
	fn refresh_unsigned_transaction(unsigned_tx: &mut Abi::Transaction);
}

/// Contains all the parameters required to fetch incoming transactions on an external chain.
#[derive(RuntimeDebug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct FetchAssetParams<C: Chain> {
	pub intent_id: IntentId,
	pub asset: <C as Chain>::ChainAsset,
}

/// Contains all the parameters required for transferring an asset on an external chain.
#[derive(RuntimeDebug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct TransferAssetParams<C: Chain> {
	pub asset: <C as Chain>::ChainAsset,
	pub to: <C as Chain>::ChainAccount,
	pub amount: AssetAmount,
}

pub trait IngressAddress {
	type AddressType;
	/// Returns an ingress address
	fn derive_address(self, vault_address: Self::AddressType, intent_id: u32) -> Self::AddressType;
}

/// Similar to [frame_support::StaticLookup] but with the `Key` as a type parameter instead of an
/// associated type.
///
/// This allows us to define multiple lookups on a single type.
///
/// TODO: Consider making the lookup infallible.
pub trait ChainEnvironment<
	LookupKey: codec::Codec + Clone + PartialEq + Debug + TypeInfo,
	LookupValue,
>
{
	/// Attempt a lookup.
	fn lookup(s: LookupKey) -> Option<LookupValue>;
}
#[allow(clippy::result_unit_err)]
/// Constructs the `SetAggKeyWithAggKey` api call.
pub trait SetAggKeyWithAggKey<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(
		maybe_old_key: Option<<Abi as ChainCrypto>::AggKey>,
		new_key: <Abi as ChainCrypto>::AggKey,
	) -> Result<Self, ()>;
}

#[allow(clippy::result_unit_err)]
pub trait SetGovKeyWithAggKey<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(maybe_old_key: Option<Vec<u8>>, new_key: Vec<u8>) -> Result<Self, ()>;
}

pub trait SetCommKeyWithAggKey<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(new_comm_key: <Abi as ChainCrypto>::GovKey) -> Self;
}

/// Constructs the `UpdateFlipSupply` api call.
pub trait UpdateFlipSupply<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(
		new_total_supply: u128,
		block_number: u64,
		stake_manager_address: &[u8; 20],
	) -> Self;
}

/// Constructs the `RegisterClaim` api call.
pub trait RegisterClaim<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(node_id: &[u8; 32], amount: u128, address: &[u8; 20], expiry: u64) -> Self;

	fn amount(&self) -> u128;
}

#[allow(clippy::result_unit_err)]
pub trait AllBatch<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<Abi>>,
		transfer_params: Vec<TransferAssetParams<Abi>>,
	) -> Result<Self, ()>;
}

pub trait FeeRefundCalculator<C: Chain> {
	/// Takes the generic TransactionFee, allowing us to compare with the fee
	/// we expected (contained in self) and return the fee we want to refund
	/// the signing account.
	fn return_fee_refund(
		&self,
		fee_paid: <C as Chain>::TransactionFee,
	) -> <C as Chain>::ChainAmount;
}

pub mod mocks {
	use crate::{
		eth::{api::EthereumReplayProtection, TransactionFee},
		*,
	};
	use sp_std::marker::PhantomData;

	#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct MockEthereum;

	// Chain implementation used for testing.
	impl Chain for MockEthereum {
		type ChainBlockNumber = u64;
		type ChainAmount = EthAmount;
		type TrackedData = MockTrackedData;
		type TransactionFee = TransactionFee;
		type ChainAccount = u64; // Currently, we don't care about this since we don't use them in tests
		type ChainAsset = assets::eth::Asset;
		type EpochStartData = ();
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
	pub struct MockTransaction;

	impl FeeRefundCalculator<MockEthereum> for MockTransaction {
		fn return_fee_refund(
			&self,
			_fee_paid: <MockEthereum as Chain>::TransactionFee,
		) -> <MockEthereum as Chain>::ChainAmount {
			<MockEthereum as Chain>::ChainAmount::default()
		}
	}

	#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Encode, Decode, TypeInfo)]
	pub struct MockThresholdSignature<K, P> {
		pub signing_key: K,
		pub signed_payload: P,
	}

	impl ChainCrypto for MockEthereum {
		type KeyId = Vec<u8>;
		type AggKey = [u8; 4];
		type Payload = [u8; 4];
		type ThresholdSignature = MockThresholdSignature<Self::AggKey, Self::Payload>;
		type TransactionId = [u8; 4];
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

	impl_default_benchmark_value!([u8; 4]);
	impl_default_benchmark_value!(MockThresholdSignature<[u8; 4], [u8; 4]>);
	impl_default_benchmark_value!(u32);
	impl_default_benchmark_value!(MockTransaction);

	pub const ETH_TX_HASH: <MockEthereum as ChainCrypto>::TransactionId = [0xbc; 4];

	pub const ETH_TX_FEE: <MockEthereum as Chain>::TransactionFee =
		TransactionFee { effective_gas_price: 200, gas_used: 100 };

	impl ChainAbi for MockEthereum {
		type Transaction = MockTransaction;
		type ReplayProtection = EthereumReplayProtection;
	}

	#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct MockApiCall<C: ChainAbi>(C::Payload, Option<C::ThresholdSignature>);

	#[cfg(feature = "runtime-benchmarks")]
	impl<C: ChainCrypto + ChainAbi> BenchmarkValue for MockApiCall<C> {
		fn benchmark_value() -> Self {
			Self(C::Payload::benchmark_value(), Some(C::ThresholdSignature::benchmark_value()))
		}
	}

	impl<C: ChainAbi> MaxEncodedLen for MockApiCall<C> {
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

		fn chain_encoded(&self) -> Vec<u8> {
			vec![0, 1, 2]
		}

		fn is_signed(&self) -> bool {
			self.1.is_some()
		}
	}

	pub struct MockTransactionBuilder<Abi, Call>(PhantomData<(Abi, Call)>);

	impl<Abi: ChainAbi, Call: ApiCall<Abi>> TransactionBuilder<Abi, Call>
		for MockTransactionBuilder<Abi, Call>
	{
		fn build_transaction(_signed_call: &Call) -> <Abi as ChainAbi>::Transaction {
			Default::default()
		}

		fn refresh_unsigned_transaction(_unsigned_tx: &mut <Abi as ChainAbi>::Transaction) {
			// refresh nothing
		}
	}
}
