use crate::{
	eth::{api::EthereumReplayProtection, TransactionFee},
	*,
};
use sp_std::marker::PhantomData;
use std::cell::RefCell;

#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthereum;

pub type MockEthereumIngressId = u128;

// Chain implementation used for testing.
impl Chain for MockEthereum {
	const NAME: &'static str = "MockEthereum";
	type IngressFetchId = MockEthereumIngressId;
	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TrackedData = MockTrackedData;
	type TransactionFee = TransactionFee;
	type ChainAccount = u64;
	type ChainAsset = assets::eth::Asset;
	type EpochStartData = ();
}

impl IngressIdConstructor for MockEthereumIngressId {
	type Address = u64;

	fn deployed(_intent_id: u64, _address: Self::Address) -> Self {
		unimplemented!()
	}

	fn undeployed(_intent_id: u64, _address: Self::Address) -> Self {
		unimplemented!()
	}
}

#[derive(
	Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
pub struct MockTrackedData {
	pub age: u64,
	pub base_fee: AssetAmount,
	pub priority_fee: AssetAmount,
}

impl MockTrackedData {
	pub fn new(age: u64, base_fee: AssetAmount, priority_fee: AssetAmount) -> Self {
		Self { age, base_fee, priority_fee }
	}
	pub fn from_age(age: u64) -> Self {
		Self { age, base_fee: 0, priority_fee: 0 }
	}
}

impl Age for MockTrackedData {
	type BlockNumber = u64;

	fn birth_block(&self) -> Self::BlockNumber {
		self.age
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for MockTrackedData {
	fn benchmark_value() -> Self {
		Self { age: 1_000u64, base_fee: 1_000u128, priority_fee: 100u128 }
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

#[derive(
	Copy,
	Clone,
	Debug,
	PartialEq,
	Eq,
	Default,
	MaxEncodedLen,
	Encode,
	Decode,
	TypeInfo,
	Ord,
	PartialOrd,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct MockAggKey(pub [u8; 4]);

impl ChainCrypto for MockEthereum {
	type AggKey = MockAggKey;
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
		agg_key.0
	}
}

impl_default_benchmark_value!(MockAggKey);
impl_default_benchmark_value!([u8; 4]);
impl_default_benchmark_value!(MockThresholdSignature<MockAggKey, [u8; 4]>);
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

impl<Abi: ChainAbi<Transaction = MockTransaction>, Call: ApiCall<Abi>> TransactionBuilder<Abi, Call>
	for MockTransactionBuilder<Abi, Call>
{
	fn build_transaction(_signed_call: &Call) -> <Abi as ChainAbi>::Transaction {
		MockTransaction {}
	}

	fn refresh_unsigned_data(_unsigned_tx: &mut <Abi as ChainAbi>::Transaction) {
		// refresh nothing
	}

	fn is_valid_for_rebroadcast(_call: &Call, _payload: &<Abi as ChainCrypto>::Payload) -> bool {
		IS_VALID_BROADCAST.with(|is_valid| *is_valid.borrow())
	}
}
