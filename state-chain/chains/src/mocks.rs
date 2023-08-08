#![cfg(debug_assertions)]

use crate::{
	eth::{api::EthereumReplayProtection, TransactionFee},
	*,
};
use cf_utilities::SliceToArray;
use sp_core::H160;
use sp_std::marker::PhantomData;
use std::cell::RefCell;

#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthereum;

pub type MockEthereumChannelId = u128;

thread_local! {
	static MOCK_KEY_HANDOVER_IS_REQUIRED: RefCell<bool> = RefCell::new(true);
	static MOCK_OPTIMISTICE_ACTIVATION: RefCell<bool> = RefCell::new(false);
}

pub struct MockKeyHandoverIsRequired;

impl MockKeyHandoverIsRequired {
	pub fn set(value: bool) {
		MOCK_KEY_HANDOVER_IS_REQUIRED.with(|v| *v.borrow_mut() = value);
	}
}

impl Get<bool> for MockKeyHandoverIsRequired {
	fn get() -> bool {
		MOCK_KEY_HANDOVER_IS_REQUIRED.with(|v| *v.borrow())
	}
}

pub struct MockOptimisticActivation;

impl MockOptimisticActivation {
	pub fn set(value: bool) {
		MOCK_OPTIMISTICE_ACTIVATION.with(|v| *v.borrow_mut() = value);
	}
}

impl Get<bool> for MockOptimisticActivation {
	fn get() -> bool {
		MOCK_OPTIMISTICE_ACTIVATION.with(|v| *v.borrow())
	}
}

// Chain implementation used for testing.
impl Chain for MockEthereum {
	const NAME: &'static str = "MockEthereum";

	type KeyHandoverIsRequired = MockKeyHandoverIsRequired;
	type OptimisticActivation = MockOptimisticActivation;

	type DepositFetchId = MockEthereumChannelId;
	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TrackedData = MockTrackedData;
	type TransactionFee = TransactionFee;
	type ChainAccount = u64;
	type ChainAsset = assets::eth::Asset;
	type EpochStartData = ();
	type DepositChannelState = MockLifecycleHooks;
	type DepositDetails = [u8; 4];
}

impl ToHumanreadableAddress for u64 {
	type Humanreadable = u64;

	fn to_humanreadable(
		&self,
		_network_environment: cf_primitives::NetworkEnvironment,
	) -> Self::Humanreadable {
		*self
	}
}

impl TryFrom<ForeignChainAddress> for u64 {
	type Error = ();

	fn try_from(address: ForeignChainAddress) -> Result<Self, Self::Error> {
		match address {
			ForeignChainAddress::Eth(addr) => Ok(u64::from_be_bytes(addr.0[12..].as_array())),
			_ => Err(()),
		}
	}
}

impl From<u64> for ForeignChainAddress {
	fn from(id: u64) -> Self {
		ForeignChainAddress::Eth(H160::from_low_u64_be(id))
	}
}

impl From<&DepositChannel<MockEthereum>> for MockEthereumChannelId {
	fn from(channel: &DepositChannel<MockEthereum>) -> Self {
		channel.channel_id as u128
	}
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct MockLifecycleHooks;

impl ChannelLifecycleHooks for MockLifecycleHooks {
	// Default implementation is fine.
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValueExtended for MockEthereumChannelId {
	fn benchmark_value_by_id(id: u8) -> Self {
		id.into()
	}
}

#[derive(
	Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct MockTrackedData {
	pub base_fee: AssetAmount,
	pub priority_fee: AssetAmount,
}

impl MockTrackedData {
	pub fn new(base_fee: AssetAmount, priority_fee: AssetAmount) -> Self {
		Self { base_fee, priority_fee }
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for MockTrackedData {
	fn benchmark_value() -> Self {
		Self { base_fee: 1_000u128, priority_fee: 100u128 }
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

#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
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
pub struct MockAggKey(pub [u8; 4]);

impl ChainCrypto for MockEthereum {
	type AggKey = MockAggKey;
	type Payload = [u8; 4];
	type ThresholdSignature = MockThresholdSignature<Self::AggKey, Self::Payload>;
	type TransactionInId = [u8; 4];
	// TODO: Use a different type here? So we can get better coverage
	type TransactionOutId = [u8; 4];
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

pub const MOCK_TRANSACTION_OUT_ID: [u8; 4] = [0xbc; 4];

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
	pub tx_out_id: C::TransactionOutId,
}

#[cfg(feature = "runtime-benchmarks")]
impl<C: ChainCrypto + ChainAbi> BenchmarkValue for MockApiCall<C> {
	fn benchmark_value() -> Self {
		Self {
			payload: C::Payload::benchmark_value(),
			sig: Some(C::ThresholdSignature::benchmark_value()),
			tx_out_id: C::TransactionOutId::benchmark_value(),
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

	fn transaction_out_id(&self) -> <C as ChainCrypto>::TransactionOutId {
		self.tx_out_id.clone()
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

	fn is_valid_for_rebroadcast(
		_call: &Call,
		_payload: &<Abi as ChainCrypto>::Payload,
		_current_key: &<Abi as ChainCrypto>::AggKey,
		_signature: &<Abi as ChainCrypto>::ThresholdSignature,
	) -> bool {
		IS_VALID_BROADCAST.with(|is_valid| *is_valid.borrow())
	}
}
