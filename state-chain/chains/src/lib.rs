#![cfg_attr(not(feature = "std"), no_std)]
use core::fmt::Display;

use crate::benchmarking_value::BenchmarkValue;
pub use address::ForeignChainAddress;
use cf_primitives::{
	chains::assets, AssetAmount, EgressId, EpochIndex, EthAmount, IntentId, KeyId, PublicKeyBytes,
};
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Blake2_256, Parameter, RuntimeDebug, StorageHasher,
};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, CheckedSub, Saturating},
	DispatchError,
};
use sp_std::{
	convert::{Into, TryFrom},
	fmt::Debug,
	prelude::*,
	vec,
	vec::Vec,
};

pub use cf_primitives::chains::*;

pub mod benchmarking_value;

pub mod any;
pub mod btc;
pub mod dot;
pub mod eth;

pub mod address;

#[cfg(feature = "std")]
pub mod mocks;

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
		+ Display
		+ CheckedSub;

	type ChainAmount: Member
		+ Parameter
		+ Copy
		+ Default
		+ Saturating
		+ Into<AssetAmount>
		+ FullCodec
		+ MaxEncodedLen
		+ BenchmarkValue;

	type TransactionFee: Member + Parameter + MaxEncodedLen + BenchmarkValue;

	type TrackedData: Member
		+ Parameter
		+ MaxEncodedLen
		+ Clone
		+ Age<BlockNumber = Self::ChainBlockNumber>
		+ BenchmarkValue;

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
		+ Debug
		+ TryFrom<ForeignChainAddress>
		+ Into<ForeignChainAddress>;

	type EpochStartData: Member + Parameter + MaxEncodedLen;

	type IngressFetchId: Member
		+ Parameter
		+ Copy
		+ BenchmarkValue
		+ IngressIdConstructor<Address = Self::ChainAccount>;
}

/// Measures the age of items associated with the Chain.
pub trait Age {
	type BlockNumber;

	/// The creation block of this item.
	fn birth_block(&self) -> Self::BlockNumber;
}

impl Age for () {
	type BlockNumber = u64;

	fn birth_block(&self) -> Self::BlockNumber {
		unimplemented!()
	}
}

/// Common crypto-related types and operations for some external chain.
pub trait ChainCrypto: Chain {
	/// The chain's `AggKey` format. The AggKey is the threshold key that controls the vault.
	/// TODO: Consider if Encode / Decode bounds are sufficient rather than To/From Vec<u8>
	type AggKey: TryFrom<PublicKeyBytes>
		+ Into<PublicKeyBytes>
		+ TryFrom<KeyId>
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

	fn agg_key_to_key_id(agg_key: Self::AggKey, epoch_index: EpochIndex) -> KeyId;
}

/// Common abi-related types and operations for some external chain.
pub trait ChainAbi: ChainCrypto {
	type Transaction: Member + Parameter + BenchmarkValue + FeeRefundCalculator<Self>;
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

	/// Construct the signed call, encoded according to the chain's native encoding.
	///
	/// Must be called after Self[Signed].
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

	/// Refresh any transaction data that is not signed over by the validators.
	///
	/// Note that calldata cannot be updated, or it would invalidate the signature.
	///
	/// A typical use case would be for updating the gas price on Ethereum transactions.
	fn refresh_unsigned_data(tx: &mut Abi::Transaction);

	/// Checks if the payload is still valid for the call.
	fn is_valid_for_rebroadcast(call: &Call, payload: &<Abi as ChainCrypto>::Payload) -> bool;
}

/// Contains all the parameters required to fetch incoming transactions on an external chain.
#[derive(RuntimeDebug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct FetchAssetParams<C: Chain> {
	pub ingress_fetch_id: <C as Chain>::IngressFetchId,
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
	fn new_unsigned(
		maybe_old_key: Option<<Abi as ChainCrypto>::GovKey>,
		new_key: <Abi as ChainCrypto>::GovKey,
	) -> Result<Self, ()>;
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

#[allow(clippy::result_unit_err)]
pub trait ExecutexSwapAndCall<Abi: ChainAbi>: ApiCall<Abi> {
	fn new_unsigned(
		egress_id: EgressId,
		transfer_param: TransferAssetParams<Abi>,
		from: ForeignChainAddress,
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

/// Helper trait to avoid matching over chains in the generic pallet.
pub trait IngressIdConstructor {
	type Address;
	/// Constructs the IngressId for the deployed case.
	fn deployed(intent_id: u64, address: Self::Address) -> Self;
	/// Constructs the IngressId for the undeployed case.
	fn undeployed(intent_id: u64, address: Self::Address) -> Self;
}

/// Metadata as part of a Cross Chain Message.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct CcmIngressMetadata {
	// Call data used after the message is egressed.
	pub message: Vec<u8>,
	// Amount of ingress funds to be used for gas.
	pub gas_budget: AssetAmount,
	// The address refunds will go to.
	pub refund_address: ForeignChainAddress,
}

#[cfg(feature = "std")]
impl std::str::FromStr for CcmIngressMetadata {
	type Err = String;

	fn from_str(_s: &str) -> Result<Self, Self::Err> {
		// TODO: check how from_str is used / should be implemented
		todo!()
	}
}
