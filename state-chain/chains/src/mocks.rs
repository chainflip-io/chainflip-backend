use crate::{
	eth::{api::EthereumReplayProtection, EthereumIngressId, TransactionFee},
	*,
};
use sp_std::marker::PhantomData;
use std::cell::RefCell;

#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthereum;

// Chain implementation used for testing.
impl Chain for MockEthereum {
	type IngressFetchId = EthereumIngressId;
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
impl_default_benchmark_value!(MockTransaction);

pub const ETH_TX_HASH: <MockEthereum as ChainCrypto>::TransactionId = [0xbc; 4];

pub const ETH_TX_FEE: <MockEthereum as Chain>::TransactionFee =
	TransactionFee { effective_gas_price: 200, gas_used: 100 };

impl ChainAbi for MockEthereum {
	type Transaction = MockTransaction;
	type ReplayProtection = EthereumReplayProtection;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockApiCall<C: ChainAbi> {
	pub payload: C::Payload,
	pub sig: Option<C::ThresholdSignature>,
}

#[cfg(feature = "runtime-benchmarks")]
impl<C: ChainCrypto + ChainAbi> BenchmarkValue for MockApiCall<C> {
	fn benchmark_value() -> Self {
		Self {
			payload: C::Payload::benchmark_value(),
			sig: Some(C::ThresholdSignature::benchmark_value()),
		}
	}
}

impl<C: ChainAbi> MaxEncodedLen for MockApiCall<C> {
	fn max_encoded_len() -> usize {
		<[u8; 32]>::max_encoded_len() * 3
	}
}

impl<C: ChainAbi> ApiCall<C> for MockApiCall<C> {
	fn threshold_signature_payload(&self) -> <C as ChainCrypto>::Payload {
		self.payload.clone()
	}

	fn signed(self, threshold_signature: &<C as ChainCrypto>::ThresholdSignature) -> Self {
		Self { sig: Some(threshold_signature.clone()), ..self }
	}

	fn chain_encoded(&self) -> Vec<u8> {
		vec![0, 1, 2]
	}

	fn is_signed(&self) -> bool {
		self.sig.is_some()
	}
}

thread_local! {
	pub static IS_VALID_BROADCAST: std::cell::RefCell<bool> = RefCell::new(true);
}

pub struct MockTransactionBuilder<Abi, Call>(PhantomData<(Abi, Call)>);

impl<Abi, Call> MockTransactionBuilder<Abi, Call> {
	pub fn set_invalid_for_rebroadcast() {
		IS_VALID_BROADCAST.with(|is_valid| *is_valid.borrow_mut() = false)
	}
}

impl<Abi: ChainAbi, Call: ApiCall<Abi>> TransactionBuilder<Abi, Call>
	for MockTransactionBuilder<Abi, Call>
{
	fn build_transaction(_signed_call: &Call) -> <Abi as ChainAbi>::Transaction {
		Default::default()
	}

	fn refresh_unsigned_transaction(_unsigned_tx: &mut <Abi as ChainAbi>::Transaction) {
		// refresh nothing
	}

	fn is_valid_for_rebroadcast(_call: &Call) -> bool {
		IS_VALID_BROADCAST.with(|is_valid| *is_valid.borrow())
	}
}
