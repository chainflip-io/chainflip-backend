#![cfg(debug_assertions)]

use crate::{
	evm::{api::EvmReplayProtection, TransactionFee},
	*,
};
use cf_utilities::SliceToArray;
use sp_core::{ConstBool, H160};
use sp_std::marker::PhantomData;
use std::cell::RefCell;

#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthereum;

pub type MockEthereumChannelId = u128;

thread_local! {
	static MOCK_KEY_HANDOVER_IS_REQUIRED: RefCell<bool> = RefCell::new(true);
	static MOCK_OPTIMISTIC_ACTIVATION: RefCell<bool> = RefCell::new(false);
	static MOCK_SIGN_WITH_SPECIFIC_KEY: RefCell<bool> = RefCell::new(false);
	static MOCK_VALID_METADATA: RefCell<bool> = RefCell::new(true);
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
		MOCK_OPTIMISTIC_ACTIVATION.with(|v| *v.borrow_mut() = value);
	}
}

impl Get<bool> for MockOptimisticActivation {
	fn get() -> bool {
		MOCK_OPTIMISTIC_ACTIVATION.with(|v| *v.borrow())
	}
}

pub struct MockFixedKeySigningRequests;

impl MockFixedKeySigningRequests {
	pub fn set(value: bool) {
		MOCK_SIGN_WITH_SPECIFIC_KEY.with(|v| *v.borrow_mut() = value);
	}
}

impl Get<bool> for MockFixedKeySigningRequests {
	fn get() -> bool {
		MOCK_SIGN_WITH_SPECIFIC_KEY.with(|v| *v.borrow())
	}
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthereumTransactionMetadata;

impl TransactionMetadata<MockEthereum> for MockEthereumTransactionMetadata {
	fn extract_metadata(_transaction: &<MockEthereum as Chain>::Transaction) -> Self {
		Default::default()
	}

	fn verify_metadata(&self, _expected_metadata: &Self) -> bool {
		MOCK_VALID_METADATA.with(|cell| *cell.borrow())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for MockEthereumTransactionMetadata {
	fn benchmark_value() -> Self {
		Default::default()
	}
}

impl MockEthereumTransactionMetadata {
	pub fn set_validity(valid: bool) {
		MOCK_VALID_METADATA.with(|cell| *cell.borrow_mut() = valid);
	}
}

// Chain implementation used for testing.
impl Chain for MockEthereum {
	const NAME: &'static str = "MockEthereum";
	type ChainCrypto = MockEthereumChainCrypto;

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
	type Transaction = MockTransaction;
	type TransactionMetadata = MockEthereumTransactionMetadata;
	type ReplayProtectionParams = ();
	type ReplayProtection = EvmReplayProtection;
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
	Copy,
	Clone,
	RuntimeDebug,
	Default,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
)]
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
		fee_paid: <MockEthereum as Chain>::TransactionFee,
	) -> <MockEthereum as Chain>::ChainAmount {
		fee_paid.effective_gas_price * fee_paid.gas_used
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Encode, Decode, TypeInfo)]
pub struct MockThresholdSignature<K, P> {
	pub signing_key: K,
	pub signed_payload: P,
}

#[derive(
	serde::Serialize,
	serde::Deserialize,
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

/// A key that should be not accepted as handover result
pub const BAD_AGG_KEY_POST_HANDOVER: MockAggKey = MockAggKey(*b"bad!");

#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthereumChainCrypto;
impl ChainCrypto for MockEthereumChainCrypto {
	type UtxoChain = ConstBool<false>;

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

	fn agg_key_to_payload(agg_key: Self::AggKey, _for_handover: bool) -> Self::Payload {
		agg_key.0
	}

	fn handover_key_matches(_current_key: &Self::AggKey, new_key: &Self::AggKey) -> bool {
		// In tests we don't look to the current key, but instead
		// compare to some "bad" value for simplicity
		new_key != &BAD_AGG_KEY_POST_HANDOVER
	}

	fn sign_with_specific_key() -> bool {
		MockFixedKeySigningRequests::get()
	}

	fn optimistic_activation() -> bool {
		MockOptimisticActivation::get()
	}

	fn key_handover_is_required() -> bool {
		MockKeyHandoverIsRequired::get()
	}
}

impl_default_benchmark_value!(MockAggKey);
impl_default_benchmark_value!([u8; 4]);
impl_default_benchmark_value!(MockThresholdSignature<MockAggKey, [u8; 4]>);
impl_default_benchmark_value!(MockTransaction);

pub const MOCK_TRANSACTION_OUT_ID: [u8; 4] = [0xbc; 4];

pub const ETH_TX_FEE: <MockEthereum as Chain>::TransactionFee =
	TransactionFee { effective_gas_price: 200, gas_used: 100 };

pub const MOCK_TX_METADATA: <MockEthereum as Chain>::TransactionMetadata =
	MockEthereumTransactionMetadata;

#[derive(Encode, Decode, TypeInfo, CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound)]
#[scale_info(skip_type_params(C))]
pub struct MockApiCall<C: ChainCrypto> {
	pub payload: <C as ChainCrypto>::Payload,
	pub sig: Option<<C as ChainCrypto>::ThresholdSignature>,
	pub tx_out_id: <C as ChainCrypto>::TransactionOutId,
}

#[cfg(feature = "runtime-benchmarks")]
impl<C: ChainCrypto> BenchmarkValue for MockApiCall<C> {
	fn benchmark_value() -> Self {
		Self {
			payload: <C as ChainCrypto>::Payload::benchmark_value(),
			sig: Some(<C as ChainCrypto>::ThresholdSignature::benchmark_value()),
			tx_out_id: <C as ChainCrypto>::TransactionOutId::benchmark_value(),
		}
	}
}

impl<C: ChainCrypto> MaxEncodedLen for MockApiCall<C> {
	fn max_encoded_len() -> usize {
		<[u8; 32]>::max_encoded_len() * 3
	}
}

impl<C: ChainCrypto + 'static> ApiCall<C> for MockApiCall<C> {
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

pub struct MockTransactionBuilder<C, Call>(PhantomData<(C, Call)>);

impl<C, Call> MockTransactionBuilder<C, Call> {
	pub fn set_invalid_for_rebroadcast() {
		IS_VALID_BROADCAST.with(|is_valid| *is_valid.borrow_mut() = false)
	}
}

impl<C: Chain<Transaction = MockTransaction>, Call: ApiCall<C::ChainCrypto>>
	TransactionBuilder<C, Call> for MockTransactionBuilder<C, Call>
{
	fn build_transaction(_signed_call: &Call) -> <C as Chain>::Transaction {
		MockTransaction {}
	}

	fn refresh_unsigned_data(_unsigned_tx: &mut <C as Chain>::Transaction) {
		// refresh nothing
	}

	fn is_valid_for_rebroadcast(
		_call: &Call,
		_payload: &<<C as Chain>::ChainCrypto as ChainCrypto>::Payload,
		_current_key: &<<C as Chain>::ChainCrypto as ChainCrypto>::AggKey,
		_signature: &<<C as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature,
	) -> bool {
		IS_VALID_BROADCAST.with(|is_valid| *is_valid.borrow())
	}
}
