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

use crate::*;

pub mod api;

pub mod benchmarking;

#[cfg(feature = "std")]
pub mod serializable_address;

use cf_utilities::SliceToArray;
#[cfg(feature = "std")]
pub use serializable_address::*;

pub use cf_primitives::chains::Polkadot;
use cf_primitives::{PolkadotBlockNumber, TxId};
use codec::{Decode, Encode};
use core::str::FromStr;
use frame_support::sp_runtime::{
	generic::{Era, SignedPayload, UncheckedExtrinsic},
	traits::{
		AccountIdLookup, BlakeTwo256, DispatchInfoOf, Hash, SignedExtension, StaticLookup, Verify,
	},
	transaction_validity::{TransactionValidity, TransactionValidityError, ValidTransaction},
	MultiAddress, MultiSignature,
};
use scale_info::TypeInfo;
use sp_core::{sr25519, ConstBool, H256};

#[cfg_attr(feature = "std", derive(Hash))]
#[derive(Debug, Encode, Decode, TypeInfo, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct PolkadotSignature(sr25519::Signature);
impl PolkadotSignature {
	fn verify(&self, payload: &EncodedPolkadotPayload, signer: &PolkadotPublicKey) -> bool {
		self.0.verify(&payload.0[..], &sr25519::Public::from(*signer.aliased_ref()))
	}

	pub fn from_aliased(signature: [u8; 64]) -> Self {
		Self(sr25519::Signature::from(signature))
	}

	pub fn aliased_ref(&self) -> &[u8; 64] {
		&self.0 .0
	}
}

impl PartialOrd for PolkadotSignature {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for PolkadotSignature {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.aliased_ref().cmp(other.aliased_ref())
	}
}

pub type PolkadotBalance = u128;
pub type PolkadotIndex = u32;
pub type PolkadotExtrinsicIndex = u32;
pub type PolkadotHash = sp_core::H256;

#[cfg(feature = "std")]
#[derive(Clone)]
pub struct PolkadotPair(sr25519::Pair);
#[cfg(feature = "std")]
impl PolkadotPair {
	pub fn from_seed(seed: &[u8; 32]) -> Self {
		use sp_core::Pair;
		Self(sr25519::Pair::from_seed(seed))
	}

	pub fn sign(&self, payload: &EncodedPolkadotPayload) -> PolkadotSignature {
		use sp_core::Pair;
		PolkadotSignature(self.0.sign(&payload.0[..]))
	}

	pub fn public_key(&self) -> PolkadotPublicKey {
		use sp_core::Pair;
		PolkadotPublicKey::from_aliased(self.0.public().into())
	}
}

/// Alias to the opaque account ID type for this chain, actually a `AccountId32`. This is always
/// 32 bytes.
#[derive(
	Copy,
	Clone,
	Default,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
#[cfg_attr(
	feature = "std",
	serde(try_from = "SubstrateNetworkAddress", into = "SubstrateNetworkAddress")
)]
pub struct PolkadotAccountId(pub [u8; 32]);

impl TryFrom<Vec<u8>> for PolkadotAccountId {
	type Error = ();

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		if value.len() != 32 {
			return Err(())
		}
		Ok(Self(value.copy_to_array()))
	}
}

impl PolkadotAccountId {
	pub const fn from_aliased(account_id: [u8; 32]) -> Self {
		Self(account_id)
	}

	pub fn aliased_ref(&self) -> &[u8; 32] {
		&self.0
	}
}

pub type PolkadotAccountIdLookup = <AccountIdLookup<PolkadotAccountId, ()> as StaticLookup>::Source;

pub type PolkadotCallHasher = BlakeTwo256;

pub type PolkadotCallHash = <PolkadotCallHasher as Hash>::Output;

#[derive(
	Debug,
	Copy,
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	Default,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct RuntimeVersion {
	pub spec_version: PolkadotSpecVersion,
	pub transaction_version: PolkadotTransactionVersion,
}

pub type PolkadotSpecVersion = u32;
pub type PolkadotChannelId = u64;
pub type PolkadotTransactionVersion = u32;

#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub struct GenericUncheckedExtrinsic<Call, Extra: SignedExtension>(
	UncheckedExtrinsic<MultiAddress<PolkadotAccountId, ()>, Call, MultiSignature, Extra>,
);
impl<Call: Decode, Extra: SignedExtension> GenericUncheckedExtrinsic<Call, Extra> {
	pub fn new_signed(
		function: Call,
		signed: PolkadotAccountId,
		signature: PolkadotSignature,
		extra: Extra,
	) -> Self {
		Self(UncheckedExtrinsic::new_signed(
			function,
			MultiAddress::Id(signed),
			frame_support::sp_runtime::MultiSignature::Sr25519(signature.0),
			extra,
		))
	}

	pub fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		Ok(Self(UncheckedExtrinsic::decode(input)?))
	}

	pub fn signature(&self) -> Option<PolkadotSignature> {
		self.0.signature.as_ref().and_then(|signature| {
			if let MultiSignature::Sr25519(signature) = &signature.1 {
				Some(PolkadotSignature(*signature))
			} else {
				None
			}
		})
	}
}

pub type PolkadotUncheckedExtrinsic =
	GenericUncheckedExtrinsic<PolkadotRuntimeCall, PolkadotSignedExtra>;

/// The payload being signed in transactions.
pub type PolkadotPayload = SignedPayload<PolkadotRuntimeCall, PolkadotSignedExtra>;

// test westend account 1 (CHAINFLIP-TEST)
// address: "5E2WfQFeafdktJ5AAF6ZGZ71Yj4fiJnHWRomVmeoStMNhoZe"
pub const RAW_SEED_1: [u8; 32] =
	hex_literal::hex!("858c1ee915090a119d4cb0774b908fa585ef7882f4648c577606490cc94f6e15");
pub const NONCE_1: u32 = 11; //correct nonce has to be provided for this account (see/track onchain)

// test westend account 2 (CHAINFLIP-TEST-2)
// address: "5GNn92C9ngX4sNp3UjqGzPbdRfbbV8hyyVVNZaH2z9e5kzxA"
pub const RAW_SEED_2: [u8; 32] =
	hex_literal::hex!("4b734882accd7a0e27b8b0d3cb7db79ab4da559d1d5f84f35fd218a1ee12ece4");
pub const NONCE_2: u32 = 18; //correct nonce has to be provided for this account (see/track onchain)

// test westend account 3 (CHAINFLIP-TEST-3)
// address: "5CLpD6DBg2hFToBJYKDB7bPVAf4TKw2F1Q2xbnzdHSikH3uK"
pub const RAW_SEED_3: [u8; 32] =
	hex_literal::hex!("ce7fec0dd410141c04e246a91f7ac909aa9707b56a8ecd33e794a49f1b5d70e6");
pub const NONCE_3: u32 = 0; //correct nonce has to be provided for this account (see/track onchain)

// FROM: https://github.com/paritytech/polkadot/blob/v0.9.33/runtime/polkadot/src/lib.rs
#[allow(clippy::unnecessary_cast)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum PolkadotProxyType {
	Any = 0,
	NonTransfer = 1,
	Governance = 2,
	Staking = 3,
	// Skip 4 as it is now removed (was SudoBalances)
	IdentityJudgement = 5,
	CancelProxy = 6,
	Auction = 7,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct EncodedPolkadotPayload(pub Vec<u8>);

#[derive(
	Clone, Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq, Eq, Serialize, Deserialize,
)]
pub struct PolkadotTrackedData {
	pub median_tip: PolkadotBalance,
	pub runtime_version: RuntimeVersion,
}

impl Default for PolkadotTrackedData {
	#[track_caller]
	fn default() -> Self {
		frame_support::print("You should not use the default chain tracking, as it's meaningless.");

		PolkadotTrackedData { median_tip: Default::default(), runtime_version: Default::default() }
	}
}

/// See https://wiki.polkadot.network/docs/learn-transaction-fees
///
/// Fee constants here already include the Multiplier.
pub mod fee_constants {
	// See https://wiki.polkadot.network/docs/learn-DOT.
	pub const MICRO_DOT: u128 = 10_000;
	pub const MILLI_DOT: u128 = 1_000 * MICRO_DOT;

	/// Taken from the Polkadot runtime.
	pub const BASE_FEE: u128 = MILLI_DOT;
	/// Taken from the Polkadot runtime. Should be 0.1 mDOT
	pub const LENGTH_FEE: u128 = MILLI_DOT / 10;

	pub mod fetch {
		pub use super::*;

		/// Estimated from the Polkadot runtime.
		pub const ADJUSTED_WEIGHT_FEE: u128 = 330 * MICRO_DOT;
		/// This should be a minor over-estimate. It's the length in bytes of an extrinsic that
		/// encodes a single fetch operation. In practice, multiple fetches and transfers might be
		/// encoded in the extrinsic, bringing the per-fetch average down.
		pub const EXTRINSIC_LENGTH: u128 = 184;

		pub const EXTRINSIC_FEE: u128 =
			BASE_FEE + LENGTH_FEE * EXTRINSIC_LENGTH + ADJUSTED_WEIGHT_FEE;
	}

	pub mod transfer {
		pub use super::*;

		/// Estimated from the Polkadot runtime.
		pub const ADJUSTED_WEIGHT_FEE: u128 = 245 * MICRO_DOT;
		/// This should be a minor over-estimate. It's the length in bytes of an extrinsic that
		/// encodes a single fetch operation. In practice, multiple fetches and transfers might be
		/// encoded in the extrinsic, bringing the per-fetch average down.
		pub const EXTRINSIC_LENGTH: u128 = 185;

		pub const EXTRINSIC_FEE: u128 =
			BASE_FEE + LENGTH_FEE * EXTRINSIC_LENGTH + ADJUSTED_WEIGHT_FEE;
	}
}

impl FeeEstimationApi<Polkadot> for PolkadotTrackedData {
	fn estimate_ingress_fee(
		&self,
		_asset: <Polkadot as Chain>::ChainAsset,
	) -> <Polkadot as Chain>::ChainAmount {
		use fee_constants::fetch::*;

		self.median_tip + fetch::EXTRINSIC_FEE
	}

	fn estimate_ingress_fee_vault_swap(&self) -> Option<<Polkadot as Chain>::ChainAmount> {
		None
	}

	fn estimate_egress_fee(
		&self,
		_asset: <Polkadot as Chain>::ChainAsset,
	) -> <Polkadot as Chain>::ChainAmount {
		use fee_constants::transfer::*;

		self.median_tip + transfer::EXTRINSIC_FEE
	}
}

#[derive(
	Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq, Serialize, Deserialize,
)]
pub struct PolkadotTransactionId {
	pub block_number: PolkadotBlockNumber,
	pub extrinsic_index: u32,
}

impl Chain for Polkadot {
	const NAME: &'static str = "Polkadot";
	const GAS_ASSET: Self::ChainAsset = assets::dot::Asset::Dot;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 1;

	type ChainCrypto = PolkadotCrypto;
	type ChainBlockNumber = PolkadotBlockNumber;
	type ChainAmount = PolkadotBalance;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = PolkadotTrackedData;
	type ChainAsset = assets::dot::Asset;
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = assets::dot::AssetMap<T>;
	type ChainAccount = PolkadotAccountId;
	type DepositFetchId = PolkadotChannelId;
	type DepositChannelState = PolkadotChannelState;
	type DepositDetails = PolkadotExtrinsicIndex;
	type Transaction = PolkadotTransactionData;
	type TransactionMetadata = ();
	type TransactionRef = PolkadotTransactionId;
	type ReplayProtectionParams = ResetProxyAccountNonce;
	type ReplayProtection = PolkadotReplayProtection;

	fn input_asset_amount_using_reference_gas_asset_price(
		_input_asset: Self::ChainAsset,
		required_gas: Self::ChainAmount,
	) -> Self::ChainAmount {
		required_gas
	}
}

pub type ResetProxyAccountNonce = bool;

#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq, Eq, Default)]
pub struct PolkadotChannelState;

/// Polkadot channels should always be recycled because we are limited to u16::MAX channels.
impl ChannelLifecycleHooks for PolkadotChannelState {
	fn maybe_recycle(self) -> Option<Self> {
		Some(self)
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolkadotCrypto;
impl ChainCrypto for PolkadotCrypto {
	const NAME: &'static str = "Polkadot";
	type UtxoChain = ConstBool<false>;

	type AggKey = PolkadotPublicKey;
	type Payload = EncodedPolkadotPayload;
	type ThresholdSignature = PolkadotSignature;
	type TransactionInId = TxId;
	type TransactionOutId = PolkadotSignature;
	type KeyHandoverIsRequired = ConstBool<false>;

	type GovKey = PolkadotPublicKey;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool {
		signature.verify(payload, agg_key)
	}

	fn agg_key_to_payload(agg_key: Self::AggKey, _for_handover: bool) -> Self::Payload {
		EncodedPolkadotPayload(Blake2_256::hash(&agg_key.aliased_ref()[..]).to_vec())
	}

	fn maybe_broadcast_barriers_on_rotation(
		rotation_broadcast_id: BroadcastId,
	) -> Vec<BroadcastId> {
		// For polkadot, we need to pause future epoch broadcasts until all the previous epoch
		// broadcasts (including the rotation tx) has successfully broadcasted.
		vec![rotation_broadcast_id]
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct PolkadotTransactionData {
	pub encoded_extrinsic: Vec<u8>,
}

impl FeeRefundCalculator<Polkadot> for PolkadotTransactionData {
	fn return_fee_refund(
		&self,
		fee_paid: <Polkadot as Chain>::TransactionFee,
	) -> <Polkadot as Chain>::ChainAmount {
		fee_paid
	}
}

pub struct CurrentVaultAndProxy {
	pub vault_account: PolkadotAccountId,
	pub proxy_account: PolkadotAccountId,
}

/// The builder for creating and signing polkadot extrinsics, and creating signature payload
#[derive(Debug, Encode, Decode, TypeInfo, Eq, PartialEq, Clone)]
pub struct PolkadotExtrinsicBuilder {
	pub extrinsic_call: PolkadotRuntimeCall,
	pub replay_protection: PolkadotReplayProtection,
	pub signer_and_signature: Option<(PolkadotPublicKey, PolkadotSignature)>,
}

impl PolkadotExtrinsicBuilder {
	pub fn new(
		replay_protection: PolkadotReplayProtection,
		extrinsic_call: PolkadotRuntimeCall,
	) -> Self {
		Self { extrinsic_call, replay_protection, signer_and_signature: None }
	}

	pub fn signature(&self) -> Option<PolkadotSignature> {
		self.signer_and_signature.as_ref().map(|(_, signature)| signature.clone())
	}

	fn extra(&self) -> PolkadotSignedExtra {
		// TODO: use chain data to estimate fees
		const TIP: PolkadotBalance = 0;
		PolkadotSignedExtra((
			(),
			(),
			(),
			(),
			PolkadotCheckMortality(Era::Immortal),
			PolkadotCheckNonce(self.replay_protection.nonce),
			(),
			PolkadotChargeTransactionPayment(TIP),
			(),
			polkadot_sdk_types::CheckMetadataHash::default(),
		))
	}

	pub fn get_signature_payload(
		&self,
		spec_version: u32,
		transaction_version: u32,
	) -> <<Polkadot as Chain>::ChainCrypto as ChainCrypto>::Payload {
		EncodedPolkadotPayload(
			PolkadotPayload::from_raw(
				self.extrinsic_call.clone(),
				self.extra(),
				(
					(),
					spec_version,
					transaction_version,
					self.replay_protection.genesis_hash,
					self.replay_protection.genesis_hash,
					(),
					(),
					(),
					(),
					None,
				),
			)
			.encode(),
		)
	}

	pub fn insert_signer_and_signature(
		&mut self,
		signer: PolkadotAccountId,
		signature: PolkadotSignature,
	) {
		self.signer_and_signature.replace((signer, signature));
	}

	pub fn get_signed_unchecked_extrinsic(&self) -> Option<PolkadotUncheckedExtrinsic> {
		self.signer_and_signature.as_ref().map(|(signer, signature)| {
			PolkadotUncheckedExtrinsic::new_signed(
				self.extrinsic_call.clone(),
				*signer,
				signature.clone(),
				self.extra(),
			)
		})
	}

	pub fn is_signed(&self) -> bool {
		self.signer_and_signature.is_some()
	}

	pub fn refresh_replay_protection(&mut self, replay_protection: PolkadotReplayProtection) {
		self.signer_and_signature = None;
		self.replay_protection = replay_protection;
	}
}

// The Polkadot Runtime type that is expected by the polkadot runtime
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub enum PolkadotRuntimeCall {
	#[codec(index = 0u8)]
	System(SystemCall),
	#[codec(index = 5u8)] // INDEX FOR WESTEND: 4, FOR POLKADOT: 5
	Balances(BalancesCall),
	#[codec(index = 26u8)] // INDEX FOR WESTEND: 16, FOR POLKADOT: 26
	Utility(UtilityCall),
	#[codec(index = 29u8)] // INDEX FOR WESTEND: 22, FOR POLKADOT: 29
	Proxy(ProxyCall),
}

#[allow(non_camel_case_types)]
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub enum SystemCall {}

impl DepositDetailsToTransactionInId<PolkadotCrypto> for u32 {}

#[allow(non_camel_case_types)]
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub enum BalancesCall {
	/// Transfer some liquid free balance to another account.
	///
	/// `transfer_allow_death` will set the `FreeBalance` of the sender and receiver.
	/// If the sender's account is below the existential deposit as a result
	/// of the transfer, the account will be reaped.
	///
	/// The dispatch origin for this call must be `Signed` by the transactor.
	#[codec(index = 0u8)]
	transfer_allow_death {
		#[allow(missing_docs)]
		dest: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		#[codec(compact)]
		value: PolkadotBalance,
	},
	/// Transfer the entire transferable balance from the caller account.
	///
	/// NOTE: This function only attempts to transfer _transferable_ balances. This means that
	/// any locked, reserved, or existential deposits (when `keep_alive` is `true`), will not be
	/// transferred by this function. To ensure that this function results in a killed account,
	/// you might need to prepare the account by removing any reference counters, storage
	/// deposits, etc...
	///
	/// The dispatch origin of this call must be Signed.
	///
	/// - `dest`: The recipient of the transfer.
	/// - `keep_alive`: A boolean to determine if the `transfer_all` operation should send all of
	///   the funds the account has, causing the sender account to be killed (false), or transfer
	///   everything except at least the existential deposit, which will guarantee to keep the
	///   sender account alive (true). # <weight>
	/// - O(1). Just like transfer, but reading the user's transferable balance first. #</weight>
	#[codec(index = 4u8)]
	transfer_all {
		#[allow(missing_docs)]
		dest: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		keep_alive: bool,
	},
}

#[allow(non_camel_case_types)]
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub enum UtilityCall {
	/// Send a batch of dispatch calls.
	///
	/// May be called from any origin.
	///
	/// - `calls`: The calls to be dispatched from the same origin. The number of call must not
	///   exceed the constant: `batched_calls_limit` (available in constant metadata).
	///
	/// If origin is root then call are dispatch without checking origin filter. (This includes
	/// bypassing `frame_system::Config::BaseCallFilter`).
	///
	/// # <weight>
	/// - Complexity: O(C) where C is the number of calls to be batched.
	/// # </weight>
	///
	/// This will return `Ok` in all circumstances. To determine the success of the batch, an
	/// event is deposited. If a call failed and the batch was interrupted, then the
	/// `BatchInterrupted` event is deposited, along with the number of successful calls made
	/// and the error of the failed call. If all were successful, then the `BatchCompleted`
	/// event is deposited.
	#[codec(index = 0u8)]
	batch {
		#[allow(missing_docs)]
		calls: Vec<PolkadotRuntimeCall>,
	},
	/// Send a call through an indexed pseudonym of the sender.
	///
	/// Filter from origin are passed along. The call will be dispatched with an origin which
	/// use the same filter as the origin of this call.
	///
	/// NOTE: If you need to ensure that any account-based filtering is not honored (i.e.
	/// because you expect `proxy` to have been used prior in the call stack and you do not want
	/// the call restrictions to apply to any sub-accounts), then use `as_multi_threshold_1`
	/// in the Multisig pallet instead.
	///
	/// NOTE: Prior to version *12, this was called `as_limited_sub`.
	///
	/// The dispatch origin for this call must be _Signed_.
	#[codec(index = 1u8)]
	as_derivative {
		#[allow(missing_docs)]
		index: u16,
		#[allow(missing_docs)]
		call: Box<PolkadotRuntimeCall>,
	},
	/// Send a batch of dispatch calls and atomically execute them.
	/// The whole transaction will rollback and fail if any of the calls failed.
	///
	/// May be called from any origin.
	///
	/// - `calls`: The calls to be dispatched from the same origin. The number of call must not
	///   exceed the constant: `batched_calls_limit` (available in constant metadata).
	///
	/// If origin is root then call are dispatch without checking origin filter. (This includes
	/// bypassing `frame_system::Config::BaseCallFilter`).
	///
	/// # <weight>
	/// - Complexity: O(C) where C is the number of calls to be batched.
	/// # </weight>
	#[codec(index = 2u8)]
	batch_all {
		#[allow(missing_docs)]
		calls: Vec<PolkadotRuntimeCall>,
	},
	/// Send a batch of dispatch calls.
	/// Unlike `batch`, it allows errors and won't interrupt.
	///
	/// May be called from any origin.
	///
	/// - `calls`: The calls to be dispatched from the same origin. The number of call must not
	///   exceed the constant: `batched_calls_limit` (available in constant metadata).
	///
	/// If origin is root then call are dispatch without checking origin filter. (This includes
	/// bypassing `frame_system::Config::BaseCallFilter`).
	///
	/// # <weight>
	/// - Complexity: O(C) where C is the number of calls to be batched.
	/// # </weight>
	#[codec(index = 4u8)]
	force_batch {
		#[allow(missing_docs)]
		calls: Vec<PolkadotRuntimeCall>,
	},
}

#[allow(non_camel_case_types)]
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub enum ProxyCall {
	/// Dispatch the given `call` from an account that the sender is authorised for through
	/// `add_proxy`.
	///
	/// Removes any corresponding announcement(s).
	///
	/// The dispatch origin for this call must be _Signed_.
	///
	/// Parameters:
	/// - `real`: The account that the proxy will make a call on behalf of.
	/// - `force_proxy_type`: Specify the exact proxy type to be used and checked for this call.
	/// - `call`: The call to be made by the `real` account.
	///
	/// # <weight>
	/// Weight is a function of the number of proxies the user has (P).
	/// # </weight>
	#[codec(index = 0u8)]
	proxy {
		#[allow(missing_docs)]
		real: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		force_proxy_type: Option<PolkadotProxyType>,
		#[allow(missing_docs)]
		call: Box<PolkadotRuntimeCall>,
	},
	/// Register a proxy account for the sender that is able to make calls on its behalf.
	///
	/// The dispatch origin for this call must be _Signed_.
	///
	/// Parameters:
	/// - `proxy`: The account that the `caller` would like to make a proxy.
	/// - `proxy_type`: The permissions allowed for this proxy account.
	/// - `delay`: The announcement period required of the initial proxy. This will generally be
	///   set to zero.
	///
	/// # <weight>
	/// Weight is a function of the number of proxies the user has (P).
	/// # </weight>
	#[codec(index = 1u8)]
	add_proxy {
		#[allow(missing_docs)]
		delegate: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		proxy_type: PolkadotProxyType,
		#[allow(missing_docs)]
		delay: PolkadotBlockNumber,
	},
	/// Unregister a proxy account for the sender.
	///
	/// The dispatch origin for this call must be _Signed_.
	///
	/// Parameters:
	/// - `proxy`: The account that the `caller` would like to remove as a proxy.
	/// - `proxy_type`: The permissions currently enabled for the removed proxy account.
	///
	/// # <weight>
	/// Weight is a function of the number of proxies the user has (P).
	/// # </weight>
	#[codec(index = 2u8)]
	remove_proxy {
		#[allow(missing_docs)]
		delegate: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		proxy_type: PolkadotProxyType,
		#[allow(missing_docs)]
		delay: PolkadotBlockNumber,
	},
	/// Unregister all proxy accounts for the sender.
	///
	/// The dispatch origin for this call must be _Signed_.
	///
	/// WARNING: This may be called on accounts created by `anonymous`, however if done, then
	/// the unreserved fees will be inaccessible. **All access to this account will be lost.**
	///
	/// # <weight>
	/// Weight is a function of the number of proxies the user has (P).
	/// # </weight>
	#[codec(index = 3u8)]
	remove_proxies {},
	/// Spawn a fresh new account that is guaranteed to be otherwise inaccessible, and
	/// initialize it with a proxy of `proxy_type` for `origin` sender.
	///
	/// Requires a `Signed` origin.
	///
	/// - `proxy_type`: The type of the proxy that the sender will be registered as over the new
	///   account. This will almost always be the most permissive `ProxyType` possible to allow for
	///   maximum flexibility.
	/// - `index`: A disambiguation index, in case this is called multiple times in the same
	///   transaction (e.g. with `utility::batch`). Unless you're using `batch` you probably just
	///   want to use `0`.
	/// - `delay`: The announcement period required of the initial proxy. Will generally be zero.
	///
	/// Fails with `Duplicate` if this has already been called in this transaction, from the same
	/// sender, with the same parameters.
	///
	/// Fails if there are insufficient funds to pay for deposit.
	///
	/// # <weight>
	/// Weight is a function of the number of proxies the user has (P).
	/// # </weight>
	/// TODO: Might be over counting 1 read
	#[codec(index = 4u8)]
	create_pure {
		#[allow(missing_docs)]
		proxy_type: PolkadotProxyType,
		#[allow(missing_docs)]
		delay: PolkadotBlockNumber,
		#[allow(missing_docs)]
		index: u16,
	},
	/// Removes a previously spawned anonymous proxy.
	///
	/// WARNING: **All access to this account will be lost.** Any funds held in it will be
	/// inaccessible.
	///
	/// Requires a `Signed` origin, and the sender account must have been created by a call to
	/// `anonymous` with corresponding parameters.
	///
	/// - `spawner`: The account that originally called `anonymous` to create this account.
	/// - `index`: The disambiguation index originally passed to `anonymous`. Probably `0`.
	/// - `proxy_type`: The proxy type originally passed to `anonymous`.
	/// - `height`: The height of the chain when the call to `anonymous` was processed.
	/// - `ext_index`: The extrinsic index in which the call to `anonymous` was processed.
	///
	/// Fails with `NoPermission` in case the caller is not a previously created anonymous
	/// account whose `anonymous` call has corresponding parameters.
	///
	/// # <weight>
	/// Weight is a function of the number of proxies the user has (P).
	/// # </weight>
	#[codec(index = 5u8)]
	kill_pure {
		#[allow(missing_docs)]
		spawner: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		proxy_type: PolkadotProxyType,
		#[allow(missing_docs)]
		index: u16,
		#[allow(missing_docs)]
		#[codec(compact)]
		height: PolkadotBlockNumber,
		#[allow(missing_docs)]
		#[codec(compact)]
		ext_index: u32,
	},
	/// Publish the hash of a proxy-call that will be made in the future.
	///
	/// This must be called some number of blocks before the corresponding `proxy` is attempted
	/// if the delay associated with the proxy relationship is greater than zero.
	///
	/// No more than `MaxPending` announcements may be made at any one time.
	///
	/// This will take a deposit of `AnnouncementDepositFactor` as well as
	/// `AnnouncementDepositBase` if there are no other pending announcements.
	///
	/// The dispatch origin for this call must be _Signed_ and a proxy of `real`.
	///
	/// Parameters:
	/// - `real`: The account that the proxy will make a call on behalf of.
	/// - `call_hash`: The hash of the call to be made by the `real` account.
	///
	/// # <weight>
	/// Weight is a function of:
	/// - A: the number of announcements made.
	/// - P: the number of proxies the user has.
	/// # </weight>
	#[codec(index = 6u8)]
	announce {
		#[allow(missing_docs)]
		real: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		call_hash: PolkadotCallHash,
	},
	/// Remove a given announcement.
	///
	/// May be called by a proxy account to remove a call they previously announced and return
	/// the deposit.
	///
	/// The dispatch origin for this call must be _Signed_.
	///
	/// Parameters:
	/// - `real`: The account that the proxy will make a call on behalf of.
	/// - `call_hash`: The hash of the call to be made by the `real` account.
	///
	/// # <weight>
	/// Weight is a function of:
	/// - A: the number of announcements made.
	/// - P: the number of proxies the user has.
	/// # </weight>
	#[codec(index = 7u8)]
	remove_announcement {
		#[allow(missing_docs)]
		real: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		call_hash: PolkadotCallHash,
	},
	/// Remove the given announcement of a delegate.
	///
	/// May be called by a target (proxied) account to remove a call that one of their delegates
	/// (`delegate`) has announced they want to execute. The deposit is returned.
	///
	/// The dispatch origin for this call must be _Signed_.
	///
	/// Parameters:
	/// - `delegate`: The account that previously announced the call.
	/// - `call_hash`: The hash of the call to be made.
	///
	/// # <weight>
	/// Weight is a function of:
	/// - A: the number of announcements made.
	/// - P: the number of proxies the user has.
	/// # </weight>
	#[codec(index = 8u8)]
	reject_announcement {
		#[allow(missing_docs)]
		delegate: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		call_hash: PolkadotCallHash,
	},
	/// Dispatch the given `call` from an account that the sender is authorized for through
	/// `add_proxy`.
	///
	/// Removes any corresponding announcement(s).
	///
	/// The dispatch origin for this call must be _Signed_.
	///
	/// Parameters:
	/// - `real`: The account that the proxy will make a call on behalf of.
	/// - `force_proxy_type`: Specify the exact proxy type to be used and checked for this call.
	/// - `call`: The call to be made by the `real` account.
	///
	/// # <weight>
	/// Weight is a function of:
	/// - A: the number of announcements made.
	/// - P: the number of proxies the user has.
	/// # </weight>
	#[codec(index = 9u8)]
	proxy_announced {
		#[allow(missing_docs)]
		delegate: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		real: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		force_proxy_type: Option<PolkadotProxyType>,
		#[allow(missing_docs)]
		call: Box<PolkadotRuntimeCall>,
	},
}
#[derive(Debug, Encode, Decode, Copy, Clone, Eq, PartialEq, TypeInfo)]
pub struct PolkadotChargeTransactionPayment(#[codec(compact)] pub PolkadotBalance);

#[derive(Debug, Encode, Decode, Copy, Clone, Eq, PartialEq, TypeInfo)]
pub struct PolkadotCheckNonce(#[codec(compact)] pub PolkadotIndex);

#[derive(Debug, Encode, Decode, Copy, Clone, Eq, PartialEq, TypeInfo)]
pub struct PolkadotCheckMortality(pub Era);

/// Temporarily copied from https://github.com/chainflip-io/polkadot-sdk/blob/8dbe4ee80734bba6644c7e5f879a363ce7c0a19f/substrate/frame/metadata-hash-extension/src/lib.rs
/// TODO: import it from polkadot-sdk once we update to a more recent version.
pub mod polkadot_sdk_types {
	use super::*;

	/// The mode of [`CheckMetadataHash`].
	#[derive(Decode, Encode, PartialEq, Debug, TypeInfo, Clone, Copy, Eq)]
	enum Mode {
		Disabled,
		Enabled,
	}

	pub type MetadataHash = Option<[u8; 32]>;

	#[derive(Encode, Decode, Copy, Clone, Eq, PartialEq, TypeInfo, DebugNoBound)]
	pub struct CheckMetadataHash {
		mode: Mode,
		#[codec(skip)]
		metadata_hash: MetadataHash,
	}

	impl Default for CheckMetadataHash {
		fn default() -> Self {
			Self { mode: Mode::Disabled, metadata_hash: None }
		}
	}
}

#[derive(Debug, Encode, Decode, Copy, Clone, Eq, PartialEq, TypeInfo)]
pub struct PolkadotSignedExtra(
	pub  (
		(),
		(),
		(),
		(),
		PolkadotCheckMortality,
		PolkadotCheckNonce,
		(),
		PolkadotChargeTransactionPayment,
		(),
		polkadot_sdk_types::CheckMetadataHash,
	),
);

impl SignedExtension for PolkadotSignedExtra {
	type AccountId = PolkadotAccountId;
	type Call = ();
	type AdditionalSigned = (
		(),
		PolkadotSpecVersion,
		PolkadotTransactionVersion,
		PolkadotHash,
		PolkadotHash,
		(),
		(),
		(),
		(),
		polkadot_sdk_types::MetadataHash,
	);
	type Pre = ();
	const IDENTIFIER: &'static str = "PolkadotSignedExtra";

	// This is a dummy implementation of additional_signed required by SignedPayload. This is never
	// actually used since the extrinsic builder that constructs the payload uses its own
	// additional_signed and constructs payload from raw.
	fn additional_signed(
		&self,
	) -> sp_std::result::Result<Self::AdditionalSigned, TransactionValidityError> {
		Ok((
			(),
			9300,
			15,
			H256::from_str("91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3")
				.unwrap(),
			H256::from_str("91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3")
				.unwrap(),
			(),
			(),
			(),
			(),
			polkadot_sdk_types::MetadataHash::None,
		))
	}

	fn pre_dispatch(
		self,
		_who: &Self::AccountId,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> Result<(), TransactionValidityError> {
		Ok(())
	}

	fn validate(
		&self,
		_who: &Self::AccountId,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> TransactionValidity {
		Ok(<ValidTransaction as Default>::default())
	}
}

pub type PolkadotPublicKey = PolkadotAccountId;

#[derive(Debug, Encode, Decode, TypeInfo, Eq, PartialEq, Clone)]
pub struct PolkadotReplayProtection {
	pub genesis_hash: PolkadotHash,
	pub signer: PolkadotAccountId,
	pub nonce: PolkadotIndex,
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValueExtended for PolkadotChannelId {
	fn benchmark_value_by_id(id: u8) -> Self {
		Self::from(id)
	}
}

#[cfg(test)]
pub(crate) const TEST_RUNTIME_VERSION: RuntimeVersion =
	RuntimeVersion { spec_version: 9340, transaction_version: 16 };

#[cfg(test)]
mod test_polkadot_extrinsics {
	use super::*;

	#[test]
	fn decode_into_unchecked_extrinsic() {
		// These extrinsic bytes were taken from real polkadot extrinsics
		#[allow(clippy::single_element_loop)]
		for mut bytes in [
			// Single fetch.
			&hex_literal::hex!(
				"
				dd02840022fd62bede1c2c45822a5007893b90c1d6c3407d9eb275aeb4890541cd893f6e01
				c215c3a4cae054916d032cbf162efe92ca5fbe9953b686a5e1fe734db7d3f31503598c2fbb
				ded5940b2cb40a7ae459d5cd769bb9460771e5aec5ae53367b268e004400001d00006d07de
				61ff7c24ff7e3688df5e0e20e619454b8c82975dbd2afda98efa281f1301001a04041a010e
				000504006d07de61ff7c24ff7e3688df5e0e20e619454b8c82975dbd2afda98efa281f1300
			"
			)[..],
			// TODO: add more examples.
		] {
			PolkadotUncheckedExtrinsic::decode(&mut bytes).expect("Should decode extrinsic bytes");
		}
	}

	#[ignore]
	#[test]
	fn create_test_extrinsic() {
		let keypair_1 = PolkadotPair::from_seed(&RAW_SEED_1);
		let keypair_2 = PolkadotPair::from_seed(&RAW_SEED_2);

		let account_id_1: PolkadotAccountId = keypair_1.public_key();
		let account_id_2: PolkadotAccountId = keypair_2.public_key();

		let test_runtime_call: PolkadotRuntimeCall =
			PolkadotRuntimeCall::Balances(BalancesCall::transfer_allow_death {
				dest: PolkadotAccountIdLookup::from(account_id_2),
				value: 35_000_000_000u128, //0.035 WND
			});

		println!("Account id 1: {account_id_1:?}");

		println!(
			"CallHash: 0x{}",
			test_runtime_call.using_encoded(|encoded| hex::encode(Blake2_256::hash(encoded)))
		);
		println!("Encoded Call: 0x{}", hex::encode(test_runtime_call.encode()));

		let mut extrinsic_builder = PolkadotExtrinsicBuilder::new(
			PolkadotReplayProtection {
				nonce: 12,
				signer: account_id_1,
				genesis_hash: Default::default(),
			},
			test_runtime_call,
		);
		extrinsic_builder.insert_signer_and_signature(
			keypair_1.public_key(),
			keypair_1.sign(&extrinsic_builder.get_signature_payload(
				TEST_RUNTIME_VERSION.spec_version,
				TEST_RUNTIME_VERSION.transaction_version,
			)),
		);

		assert!(extrinsic_builder.is_signed());

		println!(
			"encoded extrinsic: {:?}",
			extrinsic_builder.get_signed_unchecked_extrinsic().unwrap().encode()
		);
	}

	#[test]
	fn fee_estimation_doesnt_overflow() {
		let ingress_fee = PolkadotTrackedData {
			median_tip: Default::default(),
			runtime_version: Default::default(),
		}
		.estimate_ingress_fee(assets::dot::Asset::Dot);

		let egress_fee = PolkadotTrackedData {
			median_tip: Default::default(),
			runtime_version: Default::default(),
		}
		.estimate_egress_fee(assets::dot::Asset::Dot);

		// The values are not important. This test serves more as a sanity check that
		// the fees are valid, and a reference to compare against the actual fees. These values must
		// be updated if we update the fee calculation.
		assert_eq!(ingress_fee, 197_300_000u128);
		assert_eq!(egress_fee, 197_450_000u128);
	}

	#[test]
	fn refresh_replay_protection() {
		let keypair = PolkadotPair::from_seed(&RAW_SEED_1);
		let account_id: PolkadotAccountId = keypair.public_key();
		let test_runtime_call: PolkadotRuntimeCall =
			PolkadotRuntimeCall::Balances(BalancesCall::transfer_allow_death {
				dest: PolkadotAccountIdLookup::from(account_id),
				value: 35_000_000_000u128, //0.035 WND
			});

		let mut extrinsic_builder = PolkadotExtrinsicBuilder::new(
			PolkadotReplayProtection {
				nonce: 12,
				signer: account_id,
				genesis_hash: Default::default(),
			},
			test_runtime_call,
		);

		extrinsic_builder.insert_signer_and_signature(
			keypair.public_key(),
			keypair.sign(&extrinsic_builder.get_signature_payload(
				TEST_RUNTIME_VERSION.spec_version,
				TEST_RUNTIME_VERSION.transaction_version,
			)),
		);

		let new_replay_protection = PolkadotReplayProtection {
			nonce: 13,
			signer: account_id,
			genesis_hash: Default::default(),
		};

		extrinsic_builder.refresh_replay_protection(new_replay_protection.clone());

		assert_eq!(new_replay_protection, extrinsic_builder.replay_protection);
		assert!(!extrinsic_builder.is_signed());
	}

	#[ignore]
	#[test]
	fn with_metadata_hash_extension() {
		let mut ext = PolkadotExtrinsicBuilder::new(
			PolkadotReplayProtection {
				genesis_hash: H256::from_str(
					"0x91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3",
				)
				.unwrap(),
				signer: PolkadotAccountId::from_aliased(hex_literal::hex!(
					"22fd62bede1c2c45822a5007893b90c1d6c3407d9eb275aeb4890541cd893f6e"
				)),
				nonce: 17,
			},
			PolkadotRuntimeCall::Proxy(ProxyCall::proxy {
				real: PolkadotAccountIdLookup::from(PolkadotAccountId::from_aliased(
					hex_literal::hex!(
						"6d07de61ff7c24ff7e3688df5e0e20e619454b8c82975dbd2afda98efa281f13"
					),
				)),
				force_proxy_type: Some(PolkadotProxyType::Any),
				call: Box::new(PolkadotRuntimeCall::Utility(UtilityCall::force_batch {
					calls: vec![PolkadotRuntimeCall::Utility(UtilityCall::as_derivative {
						index: 14,
						call: Box::new(PolkadotRuntimeCall::Balances(BalancesCall::transfer_all {
							dest: PolkadotAccountIdLookup::from(PolkadotAccountId::from_aliased(
								hex_literal::hex!(
								"6d07de61ff7c24ff7e3688df5e0e20e619454b8c82975dbd2afda98efa281f13"
							),
							)),
							keep_alive: false,
						})),
					})],
				})),
			}),
		);
		let keypair = PolkadotPair::from_seed(&RAW_SEED_1);
		ext.insert_signer_and_signature(
			keypair.public_key(),
			keypair.sign(&ext.get_signature_payload(
				TEST_RUNTIME_VERSION.spec_version,
				TEST_RUNTIME_VERSION.transaction_version,
			)),
		);

		println!(
			"Encoded extrinsic: 0x{}",
			hex::encode(ext.get_signed_unchecked_extrinsic().unwrap().encode())
		);
	}
}
