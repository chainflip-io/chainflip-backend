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
#[derive(Debug, Encode, Decode, TypeInfo, Eq, PartialEq, Clone)]
pub struct PolkadotSignature(sr25519::Signature);
impl PolkadotSignature {
	fn verify(&self, payload: &EncodedPolkadotPayload, signer: &PolkadotPublicKey) -> bool {
		self.0.verify(&payload.0[..], &sr25519::Public(*signer.aliased_ref()))
	}

	pub const fn from_aliased(signature: [u8; 64]) -> Self {
		Self(sr25519::Signature(signature))
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
pub struct PolkadotAccountId([u8; 32]);

impl TryFrom<Vec<u8>> for PolkadotAccountId {
	type Error = ();

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		if value.len() != 32 {
			return Err(())
		}
		Ok(Self(value.as_array()))
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
pub struct PolkadotUncheckedExtrinsic(
	UncheckedExtrinsic<
		MultiAddress<PolkadotAccountId, ()>,
		PolkadotRuntimeCall,
		MultiSignature,
		PolkadotSignedExtra,
	>,
);
impl PolkadotUncheckedExtrinsic {
	pub fn new_signed(
		function: PolkadotRuntimeCall,
		signed: PolkadotAccountId,
		signature: PolkadotSignature,
		extra: PolkadotSignedExtra,
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
				Some(PolkadotSignature(signature.clone()))
			} else {
				None
			}
		})
	}
}

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

#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq, Eq)]
pub struct EpochStartData {
	pub vault_account: PolkadotAccountId,
}

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
		panic!("You should not use the default chain tracking, as it's meaningless.")
	}
}

/// See https://wiki.polkadot.network/docs/learn-transaction-fees
///
/// Fee constants here already include the Multiplier.
mod fee_constants {
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

	fn estimate_egress_fee(
		&self,
		_asset: <Polkadot as Chain>::ChainAsset,
	) -> <Polkadot as Chain>::ChainAmount {
		use fee_constants::transfer::*;

		self.median_tip + transfer::EXTRINSIC_FEE
	}
}

impl Chain for Polkadot {
	const NAME: &'static str = "Polkadot";
	const GAS_ASSET: Self::ChainAsset = assets::dot::Asset::Dot;

	type ChainCrypto = PolkadotCrypto;

	type ChainBlockNumber = PolkadotBlockNumber;
	type ChainAmount = PolkadotBalance;
	type TrackedData = PolkadotTrackedData;
	type ChainAccount = PolkadotAccountId;
	type TransactionFee = Self::ChainAmount;
	type ChainAsset = assets::dot::Asset;
	type EpochStartData = EpochStartData;
	type DepositFetchId = PolkadotChannelId;
	type DepositChannelState = PolkadotChannelState;
	type DepositDetails = ();
	type Transaction = PolkadotTransactionData;
	type TransactionMetadata = ();
	type ReplayProtectionParams = ResetProxyAccountNonce;
	type ReplayProtection = PolkadotReplayProtection;
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
	type UtxoChain = ConstBool<false>;

	type AggKey = PolkadotPublicKey;
	type Payload = EncodedPolkadotPayload;
	type ThresholdSignature = PolkadotSignature;
	type TransactionInId = TxId;
	type TransactionOutId = PolkadotSignature;

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
	extrinsic_call: PolkadotRuntimeCall,
	replay_protection: PolkadotReplayProtection,
	signature: Option<PolkadotSignature>,
}

impl PolkadotExtrinsicBuilder {
	pub fn new(
		replay_protection: PolkadotReplayProtection,
		extrinsic_call: PolkadotRuntimeCall,
	) -> Self {
		Self { extrinsic_call, replay_protection, signature: None }
	}

	pub fn signature(&self) -> Option<PolkadotSignature> {
		self.signature.clone()
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
				),
			)
			.encode(),
		)
	}

	pub fn insert_signature(&mut self, signature: PolkadotSignature) {
		self.signature.replace(signature);
	}

	pub fn get_signed_unchecked_extrinsic(&self) -> Option<PolkadotUncheckedExtrinsic> {
		self.signature.as_ref().map(|signature| {
			PolkadotUncheckedExtrinsic::new_signed(
				self.extrinsic_call.clone(),
				self.replay_protection.signer,
				signature.clone(),
				self.extra(),
			)
		})
	}

	pub fn is_signed(&self) -> bool {
		self.signature.is_some()
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

#[allow(non_camel_case_types)]
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub enum BalancesCall {
	/// Transfer some liquid free balance to another account.
	///
	/// `transfer` will set the `FreeBalance` of the sender and receiver.
	/// If the sender's account is below the existential deposit as a result
	/// of the transfer, the account will be reaped.
	///
	/// The dispatch origin for this call must be `Signed` by the transactor.
	///
	/// # <weight>
	/// - Dependent on arguments but not critical, given proper implementations for input config
	///   types. See related functions below.
	/// - It contains a limited number of reads and writes internally and no complex computation.
	///
	/// Related functions:
	///
	///   - `ensure_can_withdraw` is always called internally but has a bounded complexity.
	///   - Transferring balances to accounts that did not exist before will cause
	///     `T::OnNewAccount::on_new_account` to be called.
	///   - Removing enough funds from an account will trigger `T::DustRemoval::on_unbalanced`.
	///   - `transfer_keep_alive` works the same way as `transfer`, but has an additional check
	///     that the transfer will not kill the origin account.
	/// ---------------------------------
	/// - Origin account is already in memory, so no DB operations for them.
	/// # </weight>
	#[codec(index = 0u8)]
	transfer {
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
	/// - `delay`: The announcement period required of the initial proxy. Will generally be
	/// zero.
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
	/// - `proxy_type`: The type of the proxy that the sender will be registered as over the
	/// new account. This will almost always be the most permissive `ProxyType` possible to
	/// allow for maximum flexibility.
	/// - `index`: A disambiguation index, in case this is called multiple times in the same
	/// transaction (e.g. with `utility::batch`). Unless you're using `batch` you probably just
	/// want to use `0`.
	/// - `delay`: The announcement period required of the initial proxy. Will generally be
	/// zero.
	///
	/// Fails with `Duplicate` if this has already been called in this transaction, from the
	/// same sender, with the same parameters.
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
pub struct PolkadotChargeTransactionPayment(#[codec(compact)] PolkadotBalance);

#[derive(Debug, Encode, Decode, Copy, Clone, Eq, PartialEq, TypeInfo)]
pub struct PolkadotCheckNonce(#[codec(compact)] pub PolkadotIndex);

#[derive(Debug, Encode, Decode, Copy, Clone, Eq, PartialEq, TypeInfo)]
pub struct PolkadotCheckMortality(pub Era);

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

#[cfg(debug_assertions)]
pub const TEST_RUNTIME_VERSION: RuntimeVersion =
	RuntimeVersion { spec_version: 9340, transaction_version: 16 };

#[cfg(test)]
mod test_polkadot_extrinsics {
	use super::*;

	#[test]
	fn decode_into_unchecked_extrinsic() {
		// These extrinsic bytes were taken from real polkadot extrinsics
		let tests = [
			vec![
				217u8, 2, 132, 0, 204, 244, 17, 138, 147, 245, 170, 200, 63, 156, 208, 149, 110,
				196, 92, 172, 208, 18, 154, 161, 101, 9, 136, 32, 24, 224, 82, 32, 192, 44, 47, 57,
				1, 32, 166, 154, 89, 119, 246, 7, 38, 66, 211, 127, 100, 163, 122, 246, 84, 202,
				17, 78, 163, 200, 19, 43, 106, 49, 143, 86, 195, 159, 227, 118, 115, 227, 169, 9,
				97, 166, 49, 79, 126, 77, 71, 89, 238, 7, 58, 148, 10, 231, 91, 245, 106, 134, 193,
				131, 146, 1, 45, 189, 96, 26, 184, 146, 131, 0, 0, 0, 29, 0, 0, 74, 39, 43, 218,
				234, 215, 148, 117, 64, 37, 43, 141, 168, 78, 215, 71, 242, 11, 51, 73, 71, 152,
				37, 203, 138, 113, 49, 248, 102, 199, 158, 244, 1, 0, 26, 4, 4, 26, 1, 1, 0, 5, 4,
				0, 74, 39, 43, 218, 234, 215, 148, 117, 64, 37, 43, 141, 168, 78, 215, 71, 242, 11,
				51, 73, 71, 152, 37, 203, 138, 113, 49, 248, 102, 199, 158, 244, 0,
			],
			vec![
				121, 8, 132, 0, 204, 244, 17, 138, 147, 245, 170, 200, 63, 156, 208, 149, 110, 196,
				92, 172, 208, 18, 154, 161, 101, 9, 136, 32, 24, 224, 82, 32, 192, 44, 47, 57, 1,
				84, 216, 144, 8, 107, 255, 131, 148, 77, 46, 236, 161, 47, 65, 179, 130, 104, 234,
				83, 208, 133, 54, 252, 198, 32, 98, 231, 23, 32, 223, 158, 37, 113, 39, 128, 26,
				71, 238, 62, 48, 216, 232, 58, 100, 9, 178, 71, 216, 103, 218, 253, 161, 13, 133,
				18, 152, 232, 222, 119, 193, 50, 148, 133, 141, 0, 4, 0, 29, 0, 0, 74, 39, 43, 218,
				234, 215, 148, 117, 64, 37, 43, 141, 168, 78, 215, 71, 242, 11, 51, 73, 71, 152,
				37, 203, 138, 113, 49, 248, 102, 199, 158, 244, 1, 0, 26, 4, 40, 26, 1, 9, 0, 5, 4,
				0, 74, 39, 43, 218, 234, 215, 148, 117, 64, 37, 43, 141, 168, 78, 215, 71, 242, 11,
				51, 73, 71, 152, 37, 203, 138, 113, 49, 248, 102, 199, 158, 244, 0, 26, 1, 11, 0,
				5, 4, 0, 74, 39, 43, 218, 234, 215, 148, 117, 64, 37, 43, 141, 168, 78, 215, 71,
				242, 11, 51, 73, 71, 152, 37, 203, 138, 113, 49, 248, 102, 199, 158, 244, 0, 26, 1,
				10, 0, 5, 4, 0, 74, 39, 43, 218, 234, 215, 148, 117, 64, 37, 43, 141, 168, 78, 215,
				71, 242, 11, 51, 73, 71, 152, 37, 203, 138, 113, 49, 248, 102, 199, 158, 244, 0,
				26, 1, 8, 0, 5, 4, 0, 74, 39, 43, 218, 234, 215, 148, 117, 64, 37, 43, 141, 168,
				78, 215, 71, 242, 11, 51, 73, 71, 152, 37, 203, 138, 113, 49, 248, 102, 199, 158,
				244, 0, 26, 1, 3, 0, 5, 4, 0, 74, 39, 43, 218, 234, 215, 148, 117, 64, 37, 43, 141,
				168, 78, 215, 71, 242, 11, 51, 73, 71, 152, 37, 203, 138, 113, 49, 248, 102, 199,
				158, 244, 0, 26, 1, 5, 0, 5, 4, 0, 74, 39, 43, 218, 234, 215, 148, 117, 64, 37, 43,
				141, 168, 78, 215, 71, 242, 11, 51, 73, 71, 152, 37, 203, 138, 113, 49, 248, 102,
				199, 158, 244, 0, 26, 1, 2, 0, 5, 4, 0, 74, 39, 43, 218, 234, 215, 148, 117, 64,
				37, 43, 141, 168, 78, 215, 71, 242, 11, 51, 73, 71, 152, 37, 203, 138, 113, 49,
				248, 102, 199, 158, 244, 0, 26, 1, 6, 0, 5, 4, 0, 74, 39, 43, 218, 234, 215, 148,
				117, 64, 37, 43, 141, 168, 78, 215, 71, 242, 11, 51, 73, 71, 152, 37, 203, 138,
				113, 49, 248, 102, 199, 158, 244, 0, 26, 1, 7, 0, 5, 4, 0, 74, 39, 43, 218, 234,
				215, 148, 117, 64, 37, 43, 141, 168, 78, 215, 71, 242, 11, 51, 73, 71, 152, 37,
				203, 138, 113, 49, 248, 102, 199, 158, 244, 0, 26, 1, 4, 0, 5, 4, 0, 74, 39, 43,
				218, 234, 215, 148, 117, 64, 37, 43, 141, 168, 78, 215, 71, 242, 11, 51, 73, 71,
				152, 37, 203, 138, 113, 49, 248, 102, 199, 158, 244, 0,
			],
			vec![
				221, 2, 132, 0, 204, 244, 17, 138, 147, 245, 170, 200, 63, 156, 208, 149, 110, 196,
				92, 172, 208, 18, 154, 161, 101, 9, 136, 32, 24, 224, 82, 32, 192, 44, 47, 57, 1,
				200, 78, 45, 101, 48, 19, 155, 165, 69, 100, 122, 205, 219, 131, 91, 65, 66, 170,
				61, 13, 161, 114, 7, 11, 131, 177, 140, 62, 103, 153, 252, 18, 210, 191, 208, 86,
				61, 86, 217, 117, 127, 246, 180, 48, 214, 147, 58, 248, 233, 191, 10, 42, 37, 29,
				228, 232, 55, 80, 241, 113, 77, 126, 212, 139, 0, 8, 0, 29, 0, 0, 74, 39, 43, 218,
				234, 215, 148, 117, 64, 37, 43, 141, 168, 78, 215, 71, 242, 11, 51, 73, 71, 152,
				37, 203, 138, 113, 49, 248, 102, 199, 158, 244, 1, 0, 26, 4, 4, 5, 0, 0, 0, 72, 60,
				242, 234, 28, 242, 48, 175, 91, 19, 67, 23, 112, 3, 45, 92, 152, 192, 32, 128, 83,
				192, 130, 139, 252, 127, 61, 7, 188, 82, 102, 7, 64, 85, 100, 5, 128,
			],
			vec![
				45, 4, 132, 0, 204, 244, 17, 138, 147, 245, 170, 200, 63, 156, 208, 149, 110, 196,
				92, 172, 208, 18, 154, 161, 101, 9, 136, 32, 24, 224, 82, 32, 192, 44, 47, 57, 1,
				20, 152, 207, 160, 55, 187, 5, 113, 7, 208, 84, 47, 190, 91, 88, 68, 62, 57, 31,
				104, 223, 192, 49, 28, 111, 86, 14, 178, 135, 30, 46, 14, 168, 54, 184, 233, 126,
				246, 184, 109, 239, 170, 56, 183, 246, 177, 192, 191, 155, 156, 149, 102, 224, 39,
				144, 118, 117, 83, 76, 179, 146, 169, 91, 133, 0, 12, 0, 29, 0, 0, 74, 39, 43, 218,
				234, 215, 148, 117, 64, 37, 43, 141, 168, 78, 215, 71, 242, 11, 51, 73, 71, 152,
				37, 203, 138, 113, 49, 248, 102, 199, 158, 244, 1, 0, 26, 4, 12, 5, 0, 0, 104, 234,
				42, 87, 45, 71, 66, 169, 83, 127, 86, 95, 110, 56, 251, 79, 105, 212, 23, 12, 203,
				214, 208, 176, 129, 190, 181, 248, 190, 28, 11, 6, 11, 29, 56, 246, 123, 70, 3, 5,
				0, 0, 120, 112, 253, 81, 144, 212, 233, 27, 0, 48, 154, 238, 82, 149, 219, 107, 54,
				192, 135, 124, 162, 21, 208, 18, 242, 77, 64, 233, 6, 149, 242, 40, 7, 160, 129,
				23, 163, 117, 5, 0, 0, 4, 41, 131, 33, 217, 48, 147, 189, 214, 149, 115, 254, 142,
				12, 203, 134, 243, 61, 205, 219, 227, 102, 234, 101, 59, 7, 10, 94, 12, 215, 106,
				45, 11, 90, 10, 235, 19, 70, 3,
			],
		];

		tests.into_iter().for_each(|bytes| {
			let mut bytes = bytes.as_slice();
			PolkadotUncheckedExtrinsic::decode(&mut bytes).expect("Should decode extrinsic bytes");
		})
	}

	#[ignore]
	#[test]
	fn create_test_extrinsic() {
		let keypair_1 = PolkadotPair::from_seed(&RAW_SEED_1);
		let keypair_2 = PolkadotPair::from_seed(&RAW_SEED_2);

		let account_id_1: PolkadotAccountId = keypair_1.public_key();
		let account_id_2: PolkadotAccountId = keypair_2.public_key();

		let test_runtime_call: PolkadotRuntimeCall =
			PolkadotRuntimeCall::Balances(BalancesCall::transfer {
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
		extrinsic_builder.insert_signature(keypair_1.sign(
			&extrinsic_builder.get_signature_payload(
				TEST_RUNTIME_VERSION.spec_version,
				TEST_RUNTIME_VERSION.transaction_version,
			),
		));

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
}
