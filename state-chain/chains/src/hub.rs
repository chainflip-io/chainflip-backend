use core::str::FromStr;

use crate::*;

pub mod api;
pub mod benchmarking;

#[cfg(feature = "std")]
pub use crate::dot::serializable_address::*;
use dot::{
	fee_constants, polkadot_sdk_types, EncodedPolkadotPayload, GenericUncheckedExtrinsic,
	PolkadotAccountId, PolkadotAccountIdLookup, PolkadotBalance, PolkadotCallHash,
	PolkadotChannelId, PolkadotChannelState, PolkadotCheckMortality, PolkadotCheckNonce,
	PolkadotExtrinsicIndex, PolkadotHash, PolkadotIndex, PolkadotProxyType, PolkadotPublicKey,
	PolkadotReplayProtection, PolkadotSignature, PolkadotSpecVersion, PolkadotTransactionData,
	PolkadotTransactionId, PolkadotTransactionVersion, ResetProxyAccountNonce, RuntimeVersion,
};

pub use cf_primitives::chains::Assethub;
use cf_primitives::PolkadotBlockNumber;
use codec::{Decode, Encode};
use frame_support::{
	pallet_prelude::{TransactionValidity, TransactionValidityError, ValidTransaction},
	sp_runtime::generic::Era,
};
use scale_info::TypeInfo;
use sp_runtime::{
	generic::SignedPayload,
	traits::{DispatchInfoOf, SignedExtension},
};

impl Chain for Assethub {
	const NAME: &'static str = "Assethub";
	const GAS_ASSET: Self::ChainAsset = assets::hub::Asset::HubDot;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 1;

	type ChainCrypto = PolkadotCrypto;
	type ChainBlockNumber = PolkadotBlockNumber;
	type ChainAmount = PolkadotBalance;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = AssethubTrackedData;
	type ChainAsset = assets::hub::Asset;
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = assets::hub::AssetMap<T>;
	type ChainAccount = PolkadotAccountId;
	type DepositFetchId = PolkadotChannelId;
	type DepositChannelState = PolkadotChannelState;
	type DepositDetails = PolkadotExtrinsicIndex;
	type Transaction = PolkadotTransactionData;
	type TransactionMetadata = ();
	type TransactionRef = PolkadotTransactionId;
	type ReplayProtectionParams = ResetProxyAccountNonce;
	type ReplayProtection = PolkadotReplayProtection;
}

/// The payload being signed in transactions.
pub type AssethubPayload = SignedPayload<AssethubRuntimeCall, AssethubSignedExtra>;

pub type AssethubUncheckedExtrinsic =
	GenericUncheckedExtrinsic<AssethubRuntimeCall, AssethubSignedExtra>;

/// The builder for creating and signing assethub extrinsics, and creating signature payload
#[derive(Debug, Encode, Decode, TypeInfo, Eq, PartialEq, Clone)]
pub struct AssethubExtrinsicBuilder {
	pub extrinsic_call: AssethubRuntimeCall,
	pub replay_protection: PolkadotReplayProtection,
	pub signer_and_signature: Option<(PolkadotPublicKey, PolkadotSignature)>,
}

#[derive(Debug, Encode, Decode, Copy, Clone, Eq, PartialEq, TypeInfo)]
pub struct AssethubChargeAssetTxPayment {
	#[codec(compact)]
	tip: u128,
	asset_id: Option<u32>,
}

#[derive(Debug, Encode, Decode, Copy, Clone, Eq, PartialEq, TypeInfo)]
pub struct AssethubSignedExtra(
	pub  (
		(),
		(),
		(),
		(),
		PolkadotCheckMortality,
		PolkadotCheckNonce,
		(),
		AssethubChargeAssetTxPayment,
		polkadot_sdk_types::CheckMetadataHash,
	),
);

impl SignedExtension for AssethubSignedExtra {
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
		polkadot_sdk_types::MetadataHash,
	);
	type Pre = ();
	const IDENTIFIER: &'static str = "AssethubSignedExtra";

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

impl AssethubExtrinsicBuilder {
	pub fn new(
		replay_protection: PolkadotReplayProtection,
		extrinsic_call: AssethubRuntimeCall,
	) -> Self {
		Self { extrinsic_call, replay_protection, signer_and_signature: None }
	}

	pub fn signature(&self) -> Option<PolkadotSignature> {
		self.signer_and_signature.as_ref().map(|(_, signature)| signature.clone())
	}

	fn extra(&self) -> AssethubSignedExtra {
		// TODO: use chain data to estimate fees
		const TIP: PolkadotBalance = 0;
		AssethubSignedExtra((
			(),
			(),
			(),
			(),
			PolkadotCheckMortality(Era::Immortal),
			PolkadotCheckNonce(self.replay_protection.nonce),
			(),
			AssethubChargeAssetTxPayment { tip: TIP, asset_id: None },
			polkadot_sdk_types::CheckMetadataHash::default(),
		))
	}

	pub fn get_signature_payload(
		&self,
		spec_version: u32,
		transaction_version: u32,
	) -> <<Assethub as Chain>::ChainCrypto as ChainCrypto>::Payload {
		EncodedPolkadotPayload(
			AssethubPayload::from_raw(
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

	pub fn get_signed_unchecked_extrinsic(&self) -> Option<AssethubUncheckedExtrinsic> {
		self.signer_and_signature.as_ref().map(|(signer, signature)| {
			AssethubUncheckedExtrinsic::new_signed(
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

#[derive(
	Clone, Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq, Eq, Serialize, Deserialize,
)]
pub struct AssethubTrackedData {
	pub median_tip: PolkadotBalance,
	pub runtime_version: RuntimeVersion,
}

impl Default for AssethubTrackedData {
	#[track_caller]
	fn default() -> Self {
		frame_support::print("You should not use the default chain tracking, as it's meaningless.");

		AssethubTrackedData { median_tip: Default::default(), runtime_version: Default::default() }
	}
}

impl FeeEstimationApi<Assethub> for AssethubTrackedData {
	fn estimate_ingress_fee(
		&self,
		_asset: <Assethub as Chain>::ChainAsset,
	) -> <Assethub as Chain>::ChainAmount {
		use fee_constants::fetch::*;

		self.median_tip + fetch::EXTRINSIC_FEE
	}

	fn estimate_egress_fee(
		&self,
		_asset: <Assethub as Chain>::ChainAsset,
	) -> <Assethub as Chain>::ChainAmount {
		use fee_constants::transfer::*;

		self.median_tip + transfer::EXTRINSIC_FEE
	}
}

impl FeeRefundCalculator<Assethub> for PolkadotTransactionData {
	fn return_fee_refund(
		&self,
		fee_paid: <Assethub as Chain>::TransactionFee,
	) -> <Assethub as Chain>::ChainAmount {
		fee_paid
	}
}

// The Assethub Runtime type that is expected by the assethub runtime
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub enum AssethubRuntimeCall {
	#[codec(index = 0u8)]
	System(SystemCall),
	#[codec(index = 10u8)]
	Balances(BalancesCall),
	#[codec(index = 40u8)]
	Utility(UtilityCall),
	#[codec(index = 42u8)]
	Proxy(ProxyCall),
	#[codec(index = 50u8)]
	Assets(AssetsCall),
}

#[allow(non_camel_case_types)]
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub enum SystemCall {}

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
		calls: Vec<AssethubRuntimeCall>,
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
		call: Box<AssethubRuntimeCall>,
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
		calls: Vec<AssethubRuntimeCall>,
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
		calls: Vec<AssethubRuntimeCall>,
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
		call: Box<AssethubRuntimeCall>,
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
		call: Box<AssethubRuntimeCall>,
	},
}

#[allow(non_camel_case_types)]
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub enum AssetsCall {
	#[codec(index = 8u8)]
	transfer {
		#[allow(missing_docs)]
		#[codec(compact)]
		id: PolkadotIndex,
		#[allow(missing_docs)]
		dest: PolkadotAccountIdLookup,
		#[allow(missing_docs)]
		#[codec(compact)]
		value: PolkadotBalance,
	},
}

#[cfg(test)]
pub(crate) const TEST_RUNTIME_VERSION: RuntimeVersion =
	RuntimeVersion { spec_version: 1003004, transaction_version: 15 };
