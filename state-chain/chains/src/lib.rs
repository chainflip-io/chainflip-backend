#![cfg_attr(not(feature = "std"), no_std)]
#![feature(step_trait)]
use core::{fmt::Display, iter::Step};

use crate::benchmarking_value::{BenchmarkValue, BenchmarkValueExtended};
pub use address::ForeignChainAddress;
use address::{AddressDerivationApi, ToHumanreadableAddress};
use cf_primitives::{AssetAmount, ChannelId, EgressId, EthAmount, TransactionHash};
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	sp_runtime::{
		traits::{AtLeast32BitUnsigned, CheckedSub},
		BoundedVec, DispatchError,
	},
	Blake2_256, CloneNoBound, DebugNoBound, EqNoBound, Parameter, PartialEqNoBound, RuntimeDebug,
	StorageHasher,
};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{ConstU32, U256};
use sp_std::{
	cmp::Ord,
	convert::{Into, TryFrom},
	fmt::Debug,
	prelude::*,
	vec,
	vec::Vec,
};

pub use cf_primitives::chains::*;
pub use frame_support::traits::Get;

pub mod benchmarking_value;

pub mod any;
pub mod btc;
pub mod dot;
pub mod eth;
pub mod evm;
pub mod none;

pub mod address;
pub mod deposit_channel;
pub use deposit_channel::*;

pub mod mocks;

/// A trait representing all the types and constants that need to be implemented for supported
/// blockchains.
pub trait Chain: Member + Parameter {
	const NAME: &'static str;

	type ChainCrypto: ChainCrypto;

	type ChainBlockNumber: FullCodec
		+ Default
		+ Member
		+ Parameter
		+ Copy
		+ MaybeSerializeDeserialize
		+ AtLeast32BitUnsigned
		// this is used primarily for tests. We use u32 because it's the smallest block number we
		// use (and so we can always .into() into a larger type)
		+ From<u32>
		+ Into<u64>
		+ MaxEncodedLen
		+ Display
		+ CheckedSub
		+ Unpin
		+ Step
		+ BenchmarkValue;

	type ChainAmount: Member
		+ Parameter
		+ Copy
		+ Default
		+ AtLeast32BitUnsigned
		+ Into<AssetAmount>
		+ FullCodec
		+ MaxEncodedLen
		+ BenchmarkValue;

	type TransactionFee: Member + Parameter + MaxEncodedLen + BenchmarkValue;

	type TrackedData: Default
		+ MaybeSerializeDeserialize
		+ Member
		+ Parameter
		+ MaxEncodedLen
		+ Unpin
		+ BenchmarkValue;

	type ChainAsset: Member
		+ Parameter
		+ MaxEncodedLen
		+ Copy
		+ BenchmarkValue
		+ FullCodec
		+ Into<cf_primitives::Asset>
		+ Into<cf_primitives::ForeignChain>
		+ Unpin;

	type ChainAccount: Member
		+ Parameter
		+ MaxEncodedLen
		+ BenchmarkValue
		+ BenchmarkValueExtended
		+ Debug
		+ Ord
		+ PartialOrd
		+ TryFrom<ForeignChainAddress>
		+ Into<ForeignChainAddress>
		+ Unpin
		+ ToHumanreadableAddress;

	type EpochStartData: Member + Parameter + MaxEncodedLen;

	type DepositFetchId: Member
		+ Parameter
		+ Copy
		+ BenchmarkValue
		+ BenchmarkValueExtended
		+ for<'a> From<&'a DepositChannel<Self>>;

	type DepositChannelState: Member + Parameter + ChannelLifecycleHooks + Unpin;

	/// Extra data associated with a deposit.
	type DepositDetails: Member + Parameter + BenchmarkValue;

	type Transaction: Member + Parameter + BenchmarkValue + FeeRefundCalculator<Self>;
	/// Passed in to construct the replay protection.
	type ReplayProtectionParams: Member + Parameter;
	type ReplayProtection: Member + Parameter;
}

/// Common crypto-related types and operations for some external chain.
pub trait ChainCrypto {
	type UtxoChain: Get<bool>;

	/// The chain's `AggKey` format. The AggKey is the threshold key that controls the vault.
	type AggKey: MaybeSerializeDeserialize
		+ Member
		+ Parameter
		+ Copy
		+ Ord
		+ Default // the "zero" address
		+ BenchmarkValue;
	type Payload: Member + Parameter + BenchmarkValue;
	type ThresholdSignature: Member + Parameter + BenchmarkValue;

	/// Uniquely identifies a transaction on the incoming direction.
	type TransactionInId: Member + Parameter + Unpin + BenchmarkValue;

	/// Uniquely identifies a transaction on the outoing direction.
	type TransactionOutId: Member + Parameter + Unpin + BenchmarkValue;

	type GovKey: Member + Parameter + Copy + BenchmarkValue;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payload: &Self::Payload,
		signature: &Self::ThresholdSignature,
	) -> bool;

	/// We use the AggKey as the payload for keygen verification ceremonies.
	fn agg_key_to_payload(agg_key: Self::AggKey, for_handover: bool) -> Self::Payload;

	/// For a chain that supports key handover, check that the key produced during
	/// the handover ceremony (stored in new_key) matches the current key. (Defaults
	/// to always trivially returning `true` for chains without handover.)
	fn handover_key_matches(_current_key: &Self::AggKey, _new_key: &Self::AggKey) -> bool {
		true
	}

	/// Determines whether threshold signatures are made with a specific fixed key, or whether the
	/// key is refreshed if we need to retry the signature.
	///
	/// By default, this is true for Utxo-based chains, true otherwise.
	fn sign_with_specific_key() -> bool {
		Self::UtxoChain::get()
	}

	/// Determines whether the chain crypto allows for optimistic activation of new aggregate keys.
	///
	/// By default, this is true for Utxo-based chains, false otherwise.
	fn optimistic_activation() -> bool {
		Self::UtxoChain::get()
	}

	/// Determines whether the chain crypto supports key handover.
	///
	/// By default, this is true for Utxo-based chains, false otherwise.
	fn key_handover_is_required() -> bool {
		Self::UtxoChain::get()
	}
}

/// Provides chain-specific replay protection data.
pub trait ReplayProtectionProvider<C: Chain> {
	fn replay_protection(params: C::ReplayProtectionParams) -> C::ReplayProtection;
}

/// A call or collection of calls that can be made to the Chainflip api on an external chain.
///
/// See [eth::api::EthereumApi] for an example implementation.
pub trait ApiCall<C: ChainCrypto>: Parameter {
	/// Get the payload over which the threshold signature should be generated.
	fn threshold_signature_payload(&self) -> <C as ChainCrypto>::Payload;

	/// Add the threshold signature to the api call.
	fn signed(self, threshold_signature: &<C as ChainCrypto>::ThresholdSignature) -> Self;

	/// Construct the signed call, encoded according to the chain's native encoding.
	///
	/// Must be called after Self[Signed].
	fn chain_encoded(&self) -> Vec<u8>;

	/// Checks we have updated the sig data to non-default values.
	fn is_signed(&self) -> bool;

	/// Generates an identifier for the output of the transaction.
	fn transaction_out_id(&self) -> <C as ChainCrypto>::TransactionOutId;
}

/// Responsible for converting an api call into a raw unsigned transaction.
pub trait TransactionBuilder<C, Call>
where
	C: Chain,
	Call: ApiCall<C::ChainCrypto>,
{
	/// Construct the unsigned outbound transaction from the *signed* api call.
	/// Doesn't include any time-sensitive data e.g. gas price.
	fn build_transaction(signed_call: &Call) -> C::Transaction;

	/// Refresh any transaction data that is not signed over by the validators.
	///
	/// Note that calldata cannot be updated, or it would invalidate the signature.
	///
	/// A typical use case would be for updating the gas price on Ethereum transactions.
	fn refresh_unsigned_data(tx: &mut C::Transaction);

	/// Checks if the payload is still valid for the call.
	fn is_valid_for_rebroadcast(
		call: &Call,
		payload: &<<C as Chain>::ChainCrypto as ChainCrypto>::Payload,
		current_key: &<<C as Chain>::ChainCrypto as ChainCrypto>::AggKey,
		signature: &<<C as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature,
	) -> bool;

	/// Calculate the Units of gas that is allowed to make this call.
	fn calculate_gas_limit(_call: &Call) -> Option<U256> {
		Default::default()
	}
}

/// Contains all the parameters required to fetch incoming transactions on an external chain.
#[derive(RuntimeDebug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct FetchAssetParams<C: Chain> {
	pub deposit_fetch_id: <C as Chain>::DepositFetchId,
	pub asset: <C as Chain>::ChainAsset,
}

/// Contains all the parameters required for transferring an asset on an external chain.
#[derive(RuntimeDebug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct TransferAssetParams<C: Chain> {
	pub asset: <C as Chain>::ChainAsset,
	pub amount: <C as Chain>::ChainAmount,
	pub to: <C as Chain>::ChainAccount,
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

pub enum SetAggKeyWithAggKeyError {
	Failed,
	NotRequired,
}

/// Constructs the `SetAggKeyWithAggKey` api call.
#[allow(clippy::result_unit_err)]
pub trait SetAggKeyWithAggKey<C: ChainCrypto>: ApiCall<C> {
	fn new_unsigned(
		maybe_old_key: Option<<C as ChainCrypto>::AggKey>,
		new_key: <C as ChainCrypto>::AggKey,
	) -> Result<Self, SetAggKeyWithAggKeyError>;
}

#[allow(clippy::result_unit_err)]
pub trait SetGovKeyWithAggKey<C: ChainCrypto>: ApiCall<C> {
	fn new_unsigned(
		maybe_old_key: Option<<C as ChainCrypto>::GovKey>,
		new_key: <C as ChainCrypto>::GovKey,
	) -> Result<Self, ()>;
}

pub trait SetCommKeyWithAggKey<C: ChainCrypto>: ApiCall<C> {
	fn new_unsigned(new_comm_key: <C as ChainCrypto>::GovKey) -> Self;
}

/// Constructs the `UpdateFlipSupply` api call.
pub trait UpdateFlipSupply<C: ChainCrypto>: ApiCall<C> {
	fn new_unsigned(new_total_supply: u128, block_number: u64) -> Self;
}

/// Constructs the `RegisterRedemption` api call.
pub trait RegisterRedemption: ApiCall<<Ethereum as Chain>::ChainCrypto> {
	fn new_unsigned(
		node_id: &[u8; 32],
		amount: u128,
		address: &[u8; 20],
		expiry: u64,
		executor: Option<eth::Address>,
	) -> Self;

	fn amount(&self) -> u128;
}

#[derive(Debug)]
pub enum AllBatchError {
	NotRequired,
	Other,
}

#[allow(clippy::result_unit_err)]
pub trait AllBatch<C: Chain>: ApiCall<C::ChainCrypto> {
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<C>>,
		transfer_params: Vec<TransferAssetParams<C>>,
	) -> Result<Self, AllBatchError>;
}

#[allow(clippy::result_unit_err)]
pub trait ExecutexSwapAndCall<C: Chain>: ApiCall<C::ChainCrypto> {
	fn new_unsigned(
		egress_id: EgressId,
		transfer_param: TransferAssetParams<C>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: C::ChainAmount,
		message: Vec<u8>,
	) -> Result<Self, DispatchError>;
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

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SwapOrigin {
	DepositChannel {
		deposit_address: address::EncodedAddress,
		channel_id: ChannelId,
		deposit_block_height: u64,
	},
	Vault {
		tx_hash: TransactionHash,
	},
}

pub const MAX_CCM_MSG_LENGTH: u32 = 10_000;
pub const MAX_CCM_CF_PARAM_LENGTH: u32 = 10_000;

pub type CcmMessage = BoundedVec<u8, ConstU32<MAX_CCM_MSG_LENGTH>>;
pub type CcmCfParameters = BoundedVec<u8, ConstU32<MAX_CCM_CF_PARAM_LENGTH>>;

#[cfg(feature = "std")]
mod bounded_hex {
	use super::*;
	use sp_core::Get;

	pub fn serialize<S: serde::Serializer, Size>(
		bounded: &BoundedVec<u8, Size>,
		serializer: S,
	) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(&hex::encode(bounded))
	}

	pub fn deserialize<'de, D: serde::Deserializer<'de>, Size: Get<u32>>(
		deserializer: D,
	) -> Result<BoundedVec<u8, Size>, D::Error> {
		let hex_str = String::deserialize(deserializer)?;
		let bytes =
			hex::decode(hex_str.trim_start_matches("0x")).map_err(serde::de::Error::custom)?;
		BoundedVec::try_from(bytes).map_err(|input| {
			serde::de::Error::invalid_length(
				input.len(),
				&format!("{} bytes", Size::get()).as_str(),
			)
		})
	}
}

/// Deposit channel Metadata for Cross-Chain-Message.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize, MaxEncodedLen,
)]
pub struct CcmChannelMetadata {
	/// Call data used after the message is egressed.
	#[cfg_attr(feature = "std", serde(with = "bounded_hex"))]
	pub message: CcmMessage,
	/// User funds designated to be used for gas.
	#[cfg_attr(feature = "std", serde(with = "cf_utilities::serde_helpers::number_or_hex"))]
	pub gas_budget: AssetAmount,
	/// Additonal parameters for the cross chain message.
	#[cfg_attr(
		feature = "std",
		serde(with = "bounded_hex", default, skip_serializing_if = "Vec::is_empty")
	)]
	pub cf_parameters: CcmCfParameters,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize)]
pub struct CcmDepositMetadata {
	pub source_chain: ForeignChain,
	pub source_address: Option<ForeignChainAddress>,
	pub channel_metadata: CcmChannelMetadata,
}

#[derive(
	PartialEqNoBound,
	EqNoBound,
	CloneNoBound,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	DebugNoBound,
	Serialize,
	Deserialize,
)]
pub struct ChainState<C: Chain> {
	pub block_height: C::ChainBlockNumber,
	pub tracked_data: C::TrackedData,
}
