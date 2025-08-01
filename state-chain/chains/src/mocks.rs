// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "std")]

use crate::{
	address::IntoForeignChainAddress,
	evm::{api::EvmReplayProtection, TransactionFee},
	IntoTransactionInIdForAnyChain, *,
};
use cf_utilities::SliceToArray;
use sp_core::{ConstBool, H160};
use sp_std::marker::PhantomData;
use std::cell::RefCell;

#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthereum;

pub type MockEthereumChannelId = u128;

#[derive(Clone)]
pub enum ChainChoice {
	Ethereum,
	Polkadot,
	Bitcoin,
}

thread_local! {
	static MOCK_KEY_HANDOVER_IS_REQUIRED: RefCell<bool> = const { RefCell::new(true) };
	static MOCK_VALID_METADATA: RefCell<bool> = const { RefCell::new(true) };
	static MOCK_BROADCAST_BARRIERS: RefCell<ChainChoice> = const { RefCell::new(ChainChoice::Ethereum) };
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

pub struct MockBroadcastBarriers;

impl MockBroadcastBarriers {
	pub fn set(value: ChainChoice) {
		MOCK_BROADCAST_BARRIERS.with(|v| *v.borrow_mut() = value);
	}
}

impl Get<ChainChoice> for MockBroadcastBarriers {
	fn get() -> ChainChoice {
		MOCK_BROADCAST_BARRIERS.with(|v| (*v.borrow()).clone())
	}
}

#[derive(
	Debug, Clone, Default, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize,
)]
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

impl IntoForeignChainAddress<MockEthereum> for u64 {
	fn into_foreign_chain_address(self) -> ForeignChainAddress {
		ForeignChainAddress::Eth(H160::repeat_byte(self as u8))
	}
}

// Chain implementation used for testing.
impl Chain for MockEthereum {
	const NAME: &'static str = "MockEthereum";
	const GAS_ASSET: Self::ChainAsset = assets::eth::Asset::Eth;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 1;

	type ChainCrypto = MockEthereumChainCrypto;

	type ChainBlockNumber = u64;
	type ChainAmount = EthAmount;
	type TransactionFee = TransactionFee;
	type TrackedData = MockTrackedData;
	type ChainAsset = assets::eth::Asset;
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = assets::eth::AssetMap<T>;
	type ChainAccount = u64;
	type DepositFetchId = MockEthereumChannelId;
	type DepositChannelState = MockLifecycleHooks;
	type DepositDetails = [u8; 4];
	type Transaction = MockTransaction;
	type TransactionMetadata = MockEthereumTransactionMetadata;
	type TransactionRef = u32;
	type ReplayProtectionParams = ();
	type ReplayProtection = EvmReplayProtection;

	fn input_asset_amount_using_reference_gas_asset_price(
		input_asset: Self::ChainAsset,
		required_gas: Self::ChainAmount,
	) -> Self::ChainAmount {
		<Ethereum as Chain>::input_asset_amount_using_reference_gas_asset_price(
			input_asset,
			required_gas,
		)
	}
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
			ForeignChainAddress::Eth(addr) => Ok(u64::from_be_bytes(addr.0[12..].copy_to_array())),
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

impl FeeEstimationApi<MockEthereum> for MockTrackedData {
	fn estimate_ingress_fee(
		&self,
		_asset: <MockEthereum as Chain>::ChainAsset,
	) -> <MockEthereum as Chain>::ChainAmount {
		unimplemented!("Unused for now.")
	}

	fn estimate_ingress_fee_vault_swap(&self) -> Option<<MockEthereum as Chain>::ChainAmount> {
		unimplemented!("Unused for now.")
	}

	fn estimate_egress_fee(
		&self,
		_asset: <MockEthereum as Chain>::ChainAsset,
	) -> <MockEthereum as Chain>::ChainAmount {
		unimplemented!("Unused for now.")
	}

	fn estimate_ccm_fee(
		&self,
		_asset: <MockEthereum as Chain>::ChainAsset,
		_gas_budget: GasAmount,
		_message_length: usize,
	) -> Option<<MockEthereum as Chain>::ChainAmount> {
		unimplemented!("Unused for now.")
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

#[derive(
	Copy, Clone, Debug, PartialEq, Eq, Default, Encode, Decode, TypeInfo, Serialize, Deserialize,
)]
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

impl IntoTransactionInIdForAnyChain<MockEthereumChainCrypto> for [u8; 4] {
	fn into_transaction_in_id_for_any_chain(self) -> TransactionInIdForAnyChain {
		TransactionInIdForAnyChain::MockEthereum(self)
	}
}

/// A key that should be not accepted as handover result
pub const BAD_AGG_KEY_POST_HANDOVER: MockAggKey = MockAggKey(*b"bad!");

#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockEthereumChainCrypto;
impl ChainCrypto for MockEthereumChainCrypto {
	const NAME: &'static str = "MockEthereum";
	type UtxoChain = ConstBool<false>;

	type AggKey = MockAggKey;
	type Payload = [u8; 4];
	type ThresholdSignature = MockThresholdSignature<Self::AggKey, Self::Payload>;
	type TransactionInId = [u8; 4];
	// The transaction out id is derived from the signed api call. Therefore we can use the
	// signature itself as a proxy for this.
	type TransactionOutId = Self::ThresholdSignature;
	type KeyHandoverIsRequired = MockKeyHandoverIsRequired;
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

	fn key_handover_is_required() -> bool {
		MockKeyHandoverIsRequired::get()
	}

	fn maybe_broadcast_barriers_on_rotation(
		rotation_broadcast_id: BroadcastId,
	) -> Vec<BroadcastId> {
		match MockBroadcastBarriers::get() {
			ChainChoice::Ethereum =>
				if rotation_broadcast_id > 1 {
					vec![rotation_broadcast_id - 1, rotation_broadcast_id]
				} else {
					vec![rotation_broadcast_id]
				},
			ChainChoice::Polkadot => vec![rotation_broadcast_id],
			ChainChoice::Bitcoin => vec![],
		}
	}
}

decl_instance_aliases!(
	MockEthereum => MockEthereumInstance, (),
	MockEthereumChainCrypto => MockEthereumCryptoInstance, (),
);
impl_instance_alias_traits!(
	MockEthereumChainCrypto => { MockEthereum },
);

impl_default_benchmark_value!(MockAggKey);
impl_default_benchmark_value!([u8; 4]);
impl_default_benchmark_value!(MockThresholdSignature<MockAggKey, [u8; 4]>);
impl_default_benchmark_value!(MockTransaction);

pub const ETH_TX_FEE: <MockEthereum as Chain>::TransactionFee =
	TransactionFee { effective_gas_price: 200, gas_used: 100 };

pub const MOCK_TX_METADATA: <MockEthereum as Chain>::TransactionMetadata =
	MockEthereumTransactionMetadata;

#[derive(Encode, Decode, TypeInfo, CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound)]
#[scale_info(skip_type_params(C))]
pub struct MockApiCall<C: ChainCrypto> {
	pub payload: <C as ChainCrypto>::Payload,
	pub signer_and_signature:
		Option<(<C as ChainCrypto>::AggKey, <C as ChainCrypto>::ThresholdSignature)>,
}

#[cfg(feature = "runtime-benchmarks")]
impl<C: ChainCrypto> BenchmarkValue for MockApiCall<C> {
	fn benchmark_value() -> Self {
		Self {
			payload: <C as ChainCrypto>::Payload::benchmark_value(),
			signer_and_signature: Some((
				<C as ChainCrypto>::AggKey::benchmark_value(),
				<C as ChainCrypto>::ThresholdSignature::benchmark_value(),
			)),
		}
	}
}

impl<C: ChainCrypto> MaxEncodedLen for MockApiCall<C> {
	fn max_encoded_len() -> usize {
		<[u8; 32]>::max_encoded_len() * 3
	}
}

impl<C: ChainCrypto + 'static> ApiCall<C> for MockApiCall<C>
where
	C::TransactionOutId: From<<C as ChainCrypto>::ThresholdSignature>,
{
	fn threshold_signature_payload(&self) -> <C as ChainCrypto>::Payload {
		self.payload.clone()
	}

	fn signed(
		self,
		threshold_signature: &<C as ChainCrypto>::ThresholdSignature,
		signer: <C as ChainCrypto>::AggKey,
	) -> Self {
		Self { signer_and_signature: Some((signer, threshold_signature.clone())), ..self }
	}

	fn chain_encoded(&self) -> Vec<u8> {
		vec![0, 1, 2]
	}

	fn is_signed(&self) -> bool {
		self.signer_and_signature.is_some()
	}

	#[track_caller]
	fn transaction_out_id(&self) -> <C as ChainCrypto>::TransactionOutId {
		self.signer_and_signature
			.as_ref()
			.expect("Should only be trying to get the transaction_out_id() after we've signed")
			.1
			.clone()
			.into()
	}

	fn refresh_replay_protection(&mut self) {
		REFRESHED_REPLAY_PROTECTION.with(|is_valid| *is_valid.borrow_mut() = true)
	}

	fn signer(&self) -> Option<<C as ChainCrypto>::AggKey> {
		self.signer_and_signature.as_ref().map(|(signer, _)| *signer)
	}
}

thread_local! {
	pub static REQUIRES_REFRESH: std::cell::RefCell<bool> = const { RefCell::new(false) };
	pub static REFRESHED_REPLAY_PROTECTION: std::cell::RefCell<bool> = const { RefCell::new(false) };
}

pub struct MockTransactionBuilder<C, Call>(PhantomData<(C, Call)>);

impl<C, Call> MockTransactionBuilder<C, Call> {
	pub fn set_requires_refresh() {
		REQUIRES_REFRESH.with(|is_valid| *is_valid.borrow_mut() = true)
	}
	pub fn set_refreshed_replay_protection() {
		REFRESHED_REPLAY_PROTECTION.with(|is_valid| *is_valid.borrow_mut() = false)
	}
	pub fn get_refreshed_replay_protection_state() -> bool {
		REFRESHED_REPLAY_PROTECTION.with(|is_valid| *is_valid.borrow())
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

	fn requires_signature_refresh(
		_call: &Call,
		_payload: &<<C as Chain>::ChainCrypto as ChainCrypto>::Payload,
		_maybe_current_on_chain_key: Option<<<C as Chain>::ChainCrypto as ChainCrypto>::AggKey>,
	) -> RequiresSignatureRefresh<C::ChainCrypto, Call> {
		if REQUIRES_REFRESH.with(|is_valid| *is_valid.borrow()) {
			RequiresSignatureRefresh::True(None)
		} else {
			RequiresSignatureRefresh::False
		}
	}
}

impl DepositDetailsToTransactionInId<MockEthereumChainCrypto> for [u8; 4] {
	fn deposit_ids(&self) -> Option<Vec<[u8; 4]>> {
		Some(vec![*self])
	}
}
