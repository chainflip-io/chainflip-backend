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

#![cfg_attr(not(feature = "std"), no_std)]
#![feature(step_trait)]
#![feature(extract_if)]
#![feature(split_array)]
#![feature(impl_trait_in_assoc_type)]
use crate::{
	assets::any::Asset as AnyChainAsset,
	benchmarking_value::{BenchmarkValue, BenchmarkValueExtended},
	btc::BitcoinCrypto,
	dot::PolkadotCrypto,
	evm::EvmCrypto,
	none::NoneChainCrypto,
	sol::{
		api::SolanaTransactionBuildingError,
		sol_tx_core::instructions::program_instructions::swap_endpoints::types::CcmParams, SolSeed,
		SolanaCrypto, SolanaTransactionInId,
	},
};
use ccm_checker::{CcmValidityChecker, CcmValidityError, DecodedCcmAdditionalData};
use core::{fmt::Display, iter::Step};
use frame_support::storage::transactional;
use sol::api::VaultSwapAccountAndSender;
use sp_std::marker::PhantomData;

pub use address::ForeignChainAddress;
use address::{
	AddressConverter, AddressDerivationApi, AddressDerivationError, EncodedAddress,
	IntoForeignChainAddress, ToHumanreadableAddress,
};
use cf_primitives::{
	Affiliates, Asset, AssetAmount, BasisPoints, BlockNumber, BroadcastId, ChannelId,
	DcaParameters, EgressId, EthAmount, GasAmount, IngressOrEgress, TxId,
};
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member, RuntimeDebug},
	sp_runtime::{
		traits::{AtLeast32BitUnsigned, CheckedAdd, CheckedSub},
		BoundedVec, DispatchError,
	},
	Blake2_256, CloneNoBound, DebugNoBound, EqNoBound, Never, Parameter, PartialEqNoBound,
	StorageHasher,
};
use instances::{ChainCryptoInstanceAlias, ChainInstanceAlias};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{ConstU32, H256, U256};
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
pub use refund_parameters::*;

pub mod benchmarking_value;

pub mod any;
pub mod arb;
pub mod btc;
pub mod dot;
pub mod eth;
pub mod evm;
pub mod hub;
pub mod none;
pub mod sol;

pub mod address;
pub mod deposit_channel;
use cf_primitives::chains::assets::any::GetChainAssetMap;
pub use deposit_channel::*;
use strum::IntoEnumIterator;
pub mod ccm_checker;
pub mod cf_parameters;
pub mod instances;
pub mod refund_parameters;

#[cfg(feature = "mocks")]
pub mod mocks;

pub mod witness_period {
	use crate::ChainWitnessConfig;
	use codec::{Decode, Encode};
	use core::{
		iter::Step,
		ops::{Rem, Sub},
	};
	use derive_where::derive_where;
	use frame_support::{
		ensure,
		sp_runtime::traits::{One, Saturating},
	};
	use saturating_cast::SaturatingCast;
	use scale_info::TypeInfo;
	use serde::{Deserialize, Serialize};
	use sp_runtime::traits::Zero;
	use sp_std::ops::RangeInclusive;

	// So we can store a range-like object in storage, since this has encode and decode.
	#[derive(Encode, Decode, TypeInfo, Deserialize, Serialize)]
	#[derive_where(
		Debug,
		Clone,
		Copy,
		PartialEq,
		Eq,
		Default,
		PartialOrd,
		Ord;
		C::ChainBlockNumber
	)]
	pub struct BlockWitnessRange<C: ChainWitnessConfig> {
		root: C::ChainBlockNumber,
		_phantom: sp_std::marker::PhantomData<C>,
	}

	#[expect(clippy::result_unit_err)]
	impl<C: ChainWitnessConfig> BlockWitnessRange<C> {
		pub fn try_new(root: C::ChainBlockNumber) -> Result<Self, ()> {
			let result = Self { root, _phantom: Default::default() };
			result.check_is_valid()?;
			Ok(result)
		}
		pub fn check_is_valid(&self) -> Result<(), ()> {
			ensure!(C::WITNESS_PERIOD >= C::ChainBlockNumber::one(), ());
			ensure!(is_block_witness_root(C::WITNESS_PERIOD, self.root), ());
			Ok(())
		}
	}

	impl<C: ChainWitnessConfig> BlockWitnessRange<C> {
		pub fn into_range_inclusive(self) -> RangeInclusive<C::ChainBlockNumber> {
			self.root..=
				self.root
					.saturating_add(C::WITNESS_PERIOD.saturating_sub(C::ChainBlockNumber::one()))
		}

		pub fn root(&self) -> &C::ChainBlockNumber {
			&self.root
		}
	}

	fn block_witness_floor<
		I: Copy + Saturating + Sub<I, Output = I> + Rem<I, Output = I> + Eq + One,
	>(
		witness_period: I,
		block_number: I,
	) -> I {
		block_number - (block_number % witness_period)
	}

	pub fn is_block_witness_root<
		I: Copy + Saturating + Sub<I, Output = I> + Rem<I, Output = I> + Eq + One,
	>(
		witness_period: I,
		block_number: I,
	) -> bool {
		block_witness_root(witness_period, block_number) == block_number
	}

	pub fn block_witness_root<
		I: Copy + Saturating + Sub<I, Output = I> + Rem<I, Output = I> + Eq + One,
	>(
		witness_period: I,
		block_number: I,
	) -> I {
		block_witness_floor(witness_period, block_number)
	}

	pub fn block_witness_range<
		I: Copy + Saturating + Sub<I, Output = I> + Rem<I, Output = I> + Eq + One,
	>(
		witness_period: I,
		block_number: I,
	) -> core::ops::RangeInclusive<I> {
		let floored_block_number = block_witness_floor(witness_period, block_number);
		floored_block_number..=floored_block_number.saturating_add(witness_period - One::one())
	}

	impl<C: ChainWitnessConfig> Step for BlockWitnessRange<C> {
		fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
			if start.root > end.root {
				(0, None)
			} else {
				let distance = end.root - start.root;
				debug_assert!(distance % C::WITNESS_PERIOD == Zero::zero());
				let steps: u64 = (distance / C::WITNESS_PERIOD).into();
				let steps_usize: usize = steps.saturating_cast();
				let overflow_check = if steps_usize.saturating_cast::<u64>() == steps {
					Some(steps_usize)
				} else {
					None
				};
				(steps_usize, overflow_check)
			}
		}

		fn forward_checked(mut start: Self, count: usize) -> Option<Self> {
			start.root = start.root.saturating_add(C::WITNESS_PERIOD * (count as u32).into());
			Some(start)
		}

		fn backward_checked(mut start: Self, count: usize) -> Option<Self> {
			start.root = start.root.saturating_sub(C::WITNESS_PERIOD * (count as u32).into());
			Some(start)
		}
	}

	pub trait SaturatingStep {
		fn saturating_forward(self, count: usize) -> Self;
		fn saturating_backward(self, count: usize) -> Self;
	}

	impl<C: ChainWitnessConfig> SaturatingStep for BlockWitnessRange<C> {
		fn saturating_forward(mut self, count: usize) -> Self {
			self.root = self.root.saturating_add(C::WITNESS_PERIOD * (count as u32).into());
			self
		}

		fn saturating_backward(mut self, count: usize) -> Self {
			self.root = self.root.saturating_sub(C::WITNESS_PERIOD * (count as u32).into());
			self
		}
	}

	#[duplicate::duplicate_item(Integer; [ u8 ]; [ u16 ]; [ u32 ]; [ u64 ])]
	impl SaturatingStep for Integer {
		fn saturating_forward(self, count: usize) -> Self {
			self.saturating_add(count.saturating_cast::<Integer>())
		}
		fn saturating_backward(self, count: usize) -> Self {
			self.saturating_sub(count.saturating_cast::<Integer>())
		}
	}

	#[cfg(feature = "test")]
	use proptest::prelude::{any, Arbitrary, Strategy};

	#[cfg(feature = "test")]
	impl<C: ChainWitnessConfig> Arbitrary for BlockWitnessRange<C>
	where
		C::ChainBlockNumber: Arbitrary,
		<C::ChainBlockNumber as Arbitrary>::Strategy: Clone + Send + Sync,
	{
		type Parameters = ();
		type Strategy = impl Strategy<Value = Self> + Clone + Sync + Send;

		fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
			any::<C::ChainBlockNumber>().prop_map(|height| {
				BlockWitnessRange::<C>::try_new(block_witness_root(C::WITNESS_PERIOD, height))
					.unwrap()
			})
		}
	}
}

/// Definition of a chain as required by electoral system based witnessing
pub trait ChainWitnessConfig {
	type ChainBlockNumber: FullCodec
		+ Default
		+ Member
		+ Parameter
		+ Copy
		+ MaybeSerializeDeserialize
		+ AtLeast32BitUnsigned
		// this is used primarily for tests. We use u32 because it's the smallest block number we
		// use (and so we can always .into() into a larger type)
		+ Into<u64>
		+ MaxEncodedLen
		+ Display
		+ Unpin
		+ Step
		+ BenchmarkValue;

	const WITNESS_PERIOD: Self::ChainBlockNumber;
}

/// A trait representing all the types and constants that need to be implemented for supported
/// blockchains.
pub trait Chain: Member + Parameter + ChainInstanceAlias {
	const NAME: &'static str;

	const GAS_ASSET: Self::ChainAsset;

	const WITNESS_PERIOD: Self::ChainBlockNumber;

	const DEPRECATED: bool = false;

	/// Reference price of 1 full native token (e.g. 1 ETH, 1 BTC, 1 SOL)
	/// denominated in fineUSD (1e6 = $1.00).
	const REFERENCE_NATIVE_TOKEN_PRICE_IN_FINE_USD: Self::ChainAmount;

	/// Number of smallest units that make up 1 full token.
	/// For example: 1 ETH = 1_000_000_000_000_000_000 wei.
	const FINE_AMOUNT_PER_UNIT: Self::ChainAmount;

	/// Outputs the root block that witnesses the range of blocks after (not including)
	/// `block_number`
	fn checked_block_witness_next(
		block_number: Self::ChainBlockNumber,
	) -> Option<Self::ChainBlockNumber> {
		Self::block_witness_root(block_number).checked_add(&Self::WITNESS_PERIOD)
	}

	/// Outputs the period that witnesses blocks after `block_number`, if there is not such a
	/// period, it outputs the period that witnesses `block_number`
	fn saturating_block_witness_next(
		block_number: Self::ChainBlockNumber,
	) -> Self::ChainBlockNumber {
		let floored_block_number = Self::block_witness_root(block_number);
		floored_block_number
			.checked_add(&Self::WITNESS_PERIOD)
			.unwrap_or(floored_block_number)
	}

	/// Outputs the root block that witnesses the range of blocks before (not including)
	/// `block_number`
	fn checked_block_witness_previous(
		block_number: Self::ChainBlockNumber,
	) -> Option<Self::ChainBlockNumber> {
		Self::block_witness_root(block_number).checked_sub(&Self::WITNESS_PERIOD)
	}

	/// Checks this block is a root block of a witness range. A `witness root` is a block number
	/// used to identify the witness of a range of blocks, for example in Arbitrum `24` refers to
	/// the witness of all the blocks `24..=47`.
	fn is_block_witness_root(block_number: Self::ChainBlockNumber) -> bool {
		witness_period::is_block_witness_root(Self::WITNESS_PERIOD, block_number)
	}

	/// Outputs the root block that's associated range includes the specified block. A `witness
	/// root` is a block number used to identify the witness of a range of blocks, for example in
	/// Arbitrum `24` refers to the witness of all the blocks `24..=47`.
	fn block_witness_root(block_number: Self::ChainBlockNumber) -> Self::ChainBlockNumber {
		witness_period::block_witness_root(Self::WITNESS_PERIOD, block_number)
	}

	/// Outputs the range of blocks this block will be witnessed in.
	fn block_witness_range(
		block_number: Self::ChainBlockNumber,
	) -> core::ops::RangeInclusive<Self::ChainBlockNumber> {
		witness_period::block_witness_range(Self::WITNESS_PERIOD, block_number)
	}

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
		+ Into<u64>
		+ MaxEncodedLen
		+ Display
		+ Unpin
		+ Step
		+ BenchmarkValue;

	type ChainAmount: Member
		+ Parameter
		+ Copy
		+ Unpin
		+ MaybeSerializeDeserialize
		+ Default
		+ AtLeast32BitUnsigned
		+ Into<AssetAmount>
		+ TryFrom<AssetAmount, Error: Debug>
		+ TryFrom<U256>
		+ Into<U256>
		+ FullCodec
		+ MaxEncodedLen
		+ BenchmarkValue;

	type TransactionFee: Member
		+ Parameter
		+ MaxEncodedLen
		+ BenchmarkValue
		+ Serialize
		+ for<'de> Deserialize<'de>;

	type TrackedData: Default
		+ MaybeSerializeDeserialize
		+ Member
		+ Parameter
		+ MaxEncodedLen
		+ Unpin
		+ BenchmarkValue
		+ FeeEstimationApi<Self>;

	type ChainAsset: Member
		+ Parameter
		+ MaxEncodedLen
		+ Copy
		+ MaybeSerializeDeserialize
		+ BenchmarkValue
		+ FullCodec
		+ Into<cf_primitives::Asset>
		+ Into<cf_primitives::ForeignChain>
		+ TryFrom<cf_primitives::Asset, Error: Debug>
		+ IntoEnumIterator
		+ Unpin
		+ Ord
		+ PartialOrd;

	type ChainAssetMap<
		T: Member
			+ Parameter
			+ MaxEncodedLen
			+ Copy
			+ BenchmarkValue
			+ FullCodec
			+ Unpin
	>: Member
		+ Parameter
		+ MaxEncodedLen
		+ Copy
		+ BenchmarkValue
		+ FullCodec
		+ Unpin
		+ GetChainAssetMap<T, Asset = Self::ChainAsset>;

	type ChainAccount: Member
		+ Parameter
		+ MaxEncodedLen
		+ BenchmarkValue
		+ BenchmarkValueExtended
		+ Debug
		+ Ord
		+ PartialOrd
		+ TryFrom<ForeignChainAddress>
		+ IntoForeignChainAddress<Self>
		+ Unpin
		+ ToHumanreadableAddress
		+ Serialize
		+ for<'a> Deserialize<'a>;

	type DepositFetchId: Member
		+ Parameter
		+ Copy
		+ BenchmarkValue
		+ BenchmarkValueExtended
		+ for<'a> From<&'a DepositChannel<Self>>;

	type DepositChannelState: Member + Parameter + ChannelLifecycleHooks + Unpin;

	/// Extra data associated with a deposit.
	type DepositDetails: Member
		+ Parameter
		+ BenchmarkValue
		+ DepositDetailsToTransactionInId<Self::ChainCrypto>
		+ Serialize
		+ for<'a> Deserialize<'a>
		+ Ord
		+ PartialOrd;

	type Transaction: Member + Parameter + BenchmarkValue + FeeRefundCalculator<Self>;

	type TransactionMetadata: Member
		+ Parameter
		+ TransactionMetadata<Self>
		+ BenchmarkValue
		+ Default
		+ Serialize
		+ for<'de> Deserialize<'de>;

	/// The type representing the transaction hash for this particular chain
	type TransactionRef: Member + Parameter + BenchmarkValue + Serialize + for<'de> Deserialize<'de>;

	/// Passed in to construct the replay protection.
	type ReplayProtectionParams: Member + Parameter;
	type ReplayProtection: Member + Parameter;
}

/// Common crypto-related types and operations for some external chain.
pub trait ChainCrypto: ChainCryptoInstanceAlias + Sized {
	const NAME: &'static str;
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
	type TransactionInId: Member
		+ Parameter
		+ Unpin
		+ IntoTransactionInIdForAnyChain<Self>
		+ BenchmarkValue
		+ Serialize
		+ for<'a> Deserialize<'a>
		+ Ord
		+ PartialOrd;

	/// Uniquely identifies a transaction on the outgoing direction.
	type TransactionOutId: Member
		+ Parameter
		+ Unpin
		+ BenchmarkValue
		+ Serialize
		+ for<'de> Deserialize<'de>;

	type KeyHandoverIsRequired: Get<bool>;

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

	/// Determines whether the chain crypto supports key handover.
	///
	/// By default, this is true for Utxo-based chains, false otherwise.
	fn key_handover_is_required() -> bool {
		Self::UtxoChain::get()
	}

	/// Provides chain specific functionality for providing the broadcast barriers on rotation tx
	/// broadcast
	fn maybe_broadcast_barriers_on_rotation(rotation_broadcast_id: BroadcastId)
		-> Vec<BroadcastId>;
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
	fn signed(
		self,
		threshold_signature: &<C as ChainCrypto>::ThresholdSignature,
		signer: <C as ChainCrypto>::AggKey,
	) -> Self;

	/// Construct the signed call, encoded according to the chain's native encoding.
	///
	/// Must be called after Self[Signed].
	fn chain_encoded(&self) -> Vec<u8>;

	/// Checks we have updated the sig data to non-default values.
	fn is_signed(&self) -> bool;

	/// Generates an identifier for the output of the transaction.
	fn transaction_out_id(&self) -> <C as ChainCrypto>::TransactionOutId;

	/// Refresh the replay protection data.
	fn refresh_replay_protection(&mut self);

	/// Returns the signer of this Apicall
	fn signer(&self) -> Option<<C as ChainCrypto>::AggKey>;
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
	fn requires_signature_refresh(
		call: &Call,
		payload: &<<C as Chain>::ChainCrypto as ChainCrypto>::Payload,
		maybe_current_onchain_key: Option<<<C as Chain>::ChainCrypto as ChainCrypto>::AggKey>,
	) -> RequiresSignatureRefresh<C::ChainCrypto, Call>;

	/// Calculate the Units of gas that is allowed to make this call.
	fn calculate_gas_limit(_call: &Call) -> Option<U256> {
		Default::default()
	}
}

pub trait TransactionMetadata<C: Chain> {
	fn extract_metadata(transaction: &C::Transaction) -> Self;
	fn verify_metadata(&self, expected_metadata: &Self) -> bool;
}

impl<C: Chain> TransactionMetadata<C> for () {
	fn extract_metadata(_transaction: &C::Transaction) -> Self {
		Default::default()
	}
	fn verify_metadata(&self, _expected_metadata: &Self) -> bool {
		true
	}
}

/// Contains all the parameters required to fetch incoming transactions on an external chain.
#[derive(RuntimeDebug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct FetchAssetParams<C: Chain> {
	pub deposit_fetch_id: <C as Chain>::DepositFetchId,
	pub asset: <C as Chain>::ChainAsset,
}

#[cfg(feature = "runtime-benchmarks")]
impl<C: Chain> BenchmarkValue for FetchAssetParams<C> {
	fn benchmark_value() -> Self {
		Self {
			deposit_fetch_id: BenchmarkValue::benchmark_value(),
			asset: BenchmarkValue::benchmark_value(),
		}
	}
}

/// Contains all the parameters required for transferring an asset on an external chain.
#[derive(RuntimeDebug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct TransferAssetParams<C: Chain> {
	pub asset: <C as Chain>::ChainAsset,
	pub amount: <C as Chain>::ChainAmount,
	pub to: <C as Chain>::ChainAccount,
}

#[cfg(feature = "runtime-benchmarks")]
impl<C: Chain> BenchmarkValue for TransferAssetParams<C> {
	fn benchmark_value() -> Self {
		Self {
			asset: BenchmarkValue::benchmark_value(),
			amount: BenchmarkValue::benchmark_value(),
			to: BenchmarkValue::benchmark_value(),
		}
	}
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

#[derive(RuntimeDebug, Clone, PartialEq, Eq)]
pub enum SetAggKeyWithAggKeyError {
	Failed,
	FinalTransactionExceededMaxLength,
	DispatchError(DispatchError),
}

#[derive(RuntimeDebug, Clone, PartialEq, Eq)]
pub enum SetGovKeyWithAggKeyError {
	FailedToBuildAPICall,
	VaultAccountNotSet,
	CurrentAggKeyUnavailable,
	NonceUnavailable,
	ComputePriceUnavailable,
	SolApiEnvironmentUnavailable,
	FailedToDecodeKey,
	UnsupportedChain,
	DispatchError(DispatchError),
}

/// Constructs the `SetAggKeyWithAggKey` api call.
pub trait SetAggKeyWithAggKey<C: ChainCrypto>: ApiCall<C> {
	/// DO NOT OVERRIDE THIS METHOD.
	///
	/// Instead implement the `new_unsigned_impl` method.
	///
	/// This method is executing `new_unsigned_impl` transactional to avoid undefined on-chain
	/// storage by rolling back all changes if the transaction fails.
	fn new_unsigned(
		maybe_old_key: Option<<C as ChainCrypto>::AggKey>,
		new_key: <C as ChainCrypto>::AggKey,
	) -> Result<Option<Self>, SetAggKeyWithAggKeyError> {
		transactional::with_storage_layer(|| Self::new_unsigned_impl(maybe_old_key, new_key))
	}

	/// This needs to be implemented for each chain and includes the logic for building the
	/// transaction. It should return an error if the transaction building has failed.
	fn new_unsigned_impl(
		maybe_old_key: Option<<C as ChainCrypto>::AggKey>,
		new_key: <C as ChainCrypto>::AggKey,
	) -> Result<Option<Self>, SetAggKeyWithAggKeyError>;
}

pub trait SetGovKeyWithAggKey<C: ChainCrypto>: ApiCall<C> {
	/// DO NOT OVERRIDE THIS METHOD.
	///
	/// Instead implement the `new_unsigned_impl` method.
	///
	/// This method is executing `new_unsigned_impl` transactional to avoid undefined on-chain
	/// storage by rolling back all changes if the transaction fails.
	fn new_unsigned(
		maybe_old_key: Option<<C as ChainCrypto>::GovKey>,
		new_key: <C as ChainCrypto>::GovKey,
	) -> Result<Self, SetGovKeyWithAggKeyError> {
		transactional::with_storage_layer(|| Self::new_unsigned_impl(maybe_old_key, new_key))
	}

	/// This needs to be implemented for each chain and includes the logic for building the
	/// transaction. It should return an error if the transaction building has failed.
	fn new_unsigned_impl(
		maybe_old_key: Option<<C as ChainCrypto>::GovKey>,
		new_key: <C as ChainCrypto>::GovKey,
	) -> Result<Self, SetGovKeyWithAggKeyError>;
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
}

pub trait FetchAndCloseSolanaVaultSwapAccounts: ApiCall<<Solana as Chain>::ChainCrypto> {
	/// DO NOT OVERRIDE THIS METHOD.
	///
	/// Instead implement the `new_unsigned_impl` method.
	///
	/// This method is executing `new_unsigned_impl` transactional to avoid undefined on-chain
	/// storage by rolling back all changes if the transaction fails.
	fn new_unsigned(
		accounts: Vec<VaultSwapAccountAndSender>,
	) -> Result<Self, SolanaTransactionBuildingError> {
		transactional::with_storage_layer(|| Self::new_unsigned_impl(accounts))
	}

	/// This needs to be implemented for each chain and includes the logic for building the
	/// transaction. It should return an error if the transaction building has failed.
	fn new_unsigned_impl(
		accounts: Vec<VaultSwapAccountAndSender>,
	) -> Result<Self, SolanaTransactionBuildingError>;
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum AllBatchError {
	/// Empty transaction - the call is not required.
	NotRequired,

	/// The token address lookup failed. The token is not supported on the target chain.
	UnsupportedToken,

	/// The vault account is not set.
	VaultAccountNotSet,

	/// The Aggregate key lookup failed
	AggKeyNotSet,

	/// Unable to select Utxos.
	UtxoSelectionFailed,

	/// Failed to build Solana transaction.
	FailedToBuildSolanaTransaction(SolanaTransactionBuildingError),

	/// Some other DispatchError occurred.
	DispatchError(DispatchError),
}

impl From<DispatchError> for AllBatchError {
	fn from(e: DispatchError) -> Self {
		AllBatchError::DispatchError(e)
	}
}

impl From<DispatchError> for ExecutexSwapAndCallError {
	fn from(e: DispatchError) -> Self {
		ExecutexSwapAndCallError::DispatchError(e)
	}
}

impl From<DispatchError> for SetAggKeyWithAggKeyError {
	fn from(e: DispatchError) -> Self {
		SetAggKeyWithAggKeyError::DispatchError(e)
	}
}

impl From<DispatchError> for SetGovKeyWithAggKeyError {
	fn from(e: DispatchError) -> Self {
		SetGovKeyWithAggKeyError::DispatchError(e)
	}
}

impl From<DispatchError> for SolanaTransactionBuildingError {
	fn from(e: DispatchError) -> Self {
		SolanaTransactionBuildingError::DispatchError(e)
	}
}

impl From<DispatchError> for TransferFallbackError {
	fn from(e: DispatchError) -> Self {
		TransferFallbackError::DispatchError(e)
	}
}

#[derive(Debug)]
pub enum ConsolidationError {
	NotRequired,
	Other,
}

#[derive(Debug)]
pub enum RejectError {
	NotSupportedForAsset,
	NotRequired,
	Other,
	FailedToBuildRejection,
}

impl From<AllBatchError> for RejectError {
	fn from(e: AllBatchError) -> Self {
		match e {
			AllBatchError::UnsupportedToken => RejectError::NotSupportedForAsset,
			AllBatchError::NotRequired => RejectError::NotRequired,
			_ => RejectError::Other,
		}
	}
}
pub trait ConsolidateCall<C: Chain>: ApiCall<C::ChainCrypto> {
	fn consolidate_utxos() -> Result<Self, ConsolidationError>;
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
pub enum FetchForRejection<C: Chain> {
	Fetch { deposit_fetch_id: C::DepositFetchId },
	NotRequired,
}

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
pub enum TransferForRejection<C: Chain> {
	Transfer { address: C::ChainAccount, amount: C::ChainAmount },
	TransferWillBeCcmCallAndIsHandledSeparately,
}

pub trait RejectCall<C: Chain>: ApiCall<C::ChainCrypto> {
	fn new_unsigned(
		_deposit_details: C::DepositDetails,
		_asset: C::ChainAsset,
		_fetch_request: FetchForRejection<C>,
		_refund_request: TransferForRejection<C>,
	) -> Result<Self, RejectError> {
		Err(RejectError::NotSupportedForAsset)
	}
}

pub trait AllBatch<C: Chain>: ApiCall<C::ChainCrypto> {
	/// DO NOT OVERRIDE THIS METHOD.
	///
	/// Instead implement the `new_unsigned_impl` method.
	///
	/// This method is executing `new_unsigned_impl` transactional to avoid undefined on-chain
	/// storage by rolling back all changes if the transaction fails.
	fn new_unsigned_impl(
		fetch_params: Vec<FetchAssetParams<C>>,
		transfer_params: Vec<(TransferAssetParams<C>, EgressId)>,
	) -> Result<Vec<(Self, Vec<EgressId>)>, AllBatchError>;

	/// This needs to be implemented for each chain and includes the logic for building the
	/// transaction. It should return an error if the transaction building has failed.
	fn new_unsigned(
		fetch_params: Vec<FetchAssetParams<C>>,
		transfer_params: Vec<(TransferAssetParams<C>, EgressId)>,
	) -> Result<Vec<(Self, Vec<EgressId>)>, AllBatchError> {
		transactional::with_storage_layer(|| Self::new_unsigned_impl(fetch_params, transfer_params))
	}
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum ExecutexSwapAndCallError {
	/// The chain does not support CCM functionality.
	Unsupported,
	/// Failed to build CCM for the Solana chain.
	FailedToBuildCcmForSolana(SolanaTransactionBuildingError),
	/// Some other DispatchError occurred.
	DispatchError(DispatchError),
	/// No vault account exists yet.
	NoVault,
	/// Auxiliary data required to build the transaction is not ready.
	AuxDataNotReady,
}

pub trait ExecutexSwapAndCall<C: Chain>: ApiCall<C::ChainCrypto> {
	/// DO NOT OVERRIDE THIS METHOD.
	///
	/// Instead implement the `new_unsigned_impl` method.
	///
	/// This method is executing `new_unsigned_impl` transactional to avoid undefined on-chain
	/// storage by rolling back all changes if the transaction fails.
	fn new_unsigned(
		transfer_param: TransferAssetParams<C>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: GasAmount,
		message: Vec<u8>,
		ccm_additional_data: DecodedCcmAdditionalData,
	) -> Result<Self, ExecutexSwapAndCallError> {
		transactional::with_storage_layer(|| {
			Self::new_unsigned_impl(
				transfer_param,
				source_chain,
				source_address,
				gas_budget,
				message,
				ccm_additional_data,
			)
		})
	}

	/// This needs to be implemented for each chain and includes the logic for building the
	/// transaction. It should return an error if the transaction building has failed.
	fn new_unsigned_impl(
		transfer_param: TransferAssetParams<C>,
		source_chain: ForeignChain,
		source_address: Option<ForeignChainAddress>,
		gas_budget: GasAmount,
		message: Vec<u8>,
		ccm_additional_data: DecodedCcmAdditionalData,
	) -> Result<Self, ExecutexSwapAndCallError>;
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum TransferFallbackError {
	/// The chain does not support this functionality.
	Unsupported,
	/// Failed to lookup the given token address, so the asset is invalid.
	CannotLookupTokenAddress,
	/// Some other DispatchError occurred.
	DispatchError(DispatchError),
}

pub trait TransferFallback<C: Chain>: ApiCall<C::ChainCrypto> {
	/// DO NOT OVERRIDE THIS METHOD.
	///
	/// Instead implement the `new_unsigned_impl` method.
	///
	/// This method is executing `new_unsigned_impl` transactional to avoid undefined on-chain
	/// storage by rolling back all changes if the transaction fails.
	fn new_unsigned(transfer_param: TransferAssetParams<C>) -> Result<Self, TransferFallbackError> {
		transactional::with_storage_layer(|| Self::new_unsigned_impl(transfer_param))
	}

	/// This needs to be implemented for each chain and includes the logic for building the
	/// transaction. It should return an error if the transaction building has failed.
	fn new_unsigned_impl(
		transfer_param: TransferAssetParams<C>,
	) -> Result<Self, TransferFallbackError>;
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

#[derive(Debug, Clone, TypeInfo, Encode, Decode, PartialEq, Eq)]
pub enum TransactionInIdForAnyChain {
	Evm(H256),
	Bitcoin(H256),
	Polkadot(TxId),
	Solana(SolanaTransactionInId),
	None,
	#[cfg(feature = "mocks")]
	MockEthereum([u8; 4]),
}

#[cfg(feature = "std")]
impl std::fmt::Display for TransactionInIdForAnyChain {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Bitcoin(hash) | Self::Evm(hash) => write!(f, "{:#x}", hash),
			Self::Polkadot(transaction_id) =>
				write!(f, "{}-{}", transaction_id.block_number, transaction_id.extrinsic_index),
			Self::Solana((address, id)) => write!(f, "{address}-{id}",),
			#[cfg(feature = "mocks")]
			Self::MockEthereum(id) => write!(f, "{:?}", id),
			Self::None => write!(f, "None"),
		}
	}
}

pub trait IntoTransactionInIdForAnyChain<C: ChainCrypto<TransactionInId = Self>> {
	fn into_transaction_in_id_for_any_chain(self) -> TransactionInIdForAnyChain;
}

impl IntoTransactionInIdForAnyChain<EvmCrypto> for H256 {
	fn into_transaction_in_id_for_any_chain(self) -> TransactionInIdForAnyChain {
		TransactionInIdForAnyChain::Evm(self)
	}
}

impl IntoTransactionInIdForAnyChain<BitcoinCrypto> for H256 {
	fn into_transaction_in_id_for_any_chain(self) -> TransactionInIdForAnyChain {
		TransactionInIdForAnyChain::Bitcoin(self)
	}
}

impl IntoTransactionInIdForAnyChain<SolanaCrypto> for SolanaTransactionInId {
	fn into_transaction_in_id_for_any_chain(self) -> TransactionInIdForAnyChain {
		TransactionInIdForAnyChain::Solana(self)
	}
}

impl IntoTransactionInIdForAnyChain<PolkadotCrypto> for TxId {
	fn into_transaction_in_id_for_any_chain(self) -> TransactionInIdForAnyChain {
		TransactionInIdForAnyChain::Polkadot(self)
	}
}

impl IntoTransactionInIdForAnyChain<NoneChainCrypto> for () {
	fn into_transaction_in_id_for_any_chain(self) -> TransactionInIdForAnyChain {
		TransactionInIdForAnyChain::None
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum SwapOrigin<AccountId> {
	DepositChannel {
		deposit_address: address::EncodedAddress,
		channel_id: ChannelId,
		deposit_block_height: u64,
		broker_id: AccountId,
	},
	Vault {
		tx_id: TransactionInIdForAnyChain,
		broker_id: Option<AccountId>,
	},
	Internal,
	OnChainAccount(AccountId),
}

impl<AccountId> SwapOrigin<AccountId> {
	pub fn broker_id(&self) -> Option<&AccountId> {
		match self {
			Self::DepositChannel { ref broker_id, .. } => Some(broker_id),
			Self::Vault { ref broker_id, .. } => broker_id.as_ref(),
			Self::Internal => None,
			Self::OnChainAccount(_) => None,
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum DepositOriginType {
	DepositChannel,
	Vault,
}

pub const MAX_CCM_MSG_LENGTH: u32 = 15_000;
pub const MAX_CCM_ADDITIONAL_DATA_LENGTH: u32 = 3_000;

pub type CcmMessage = BoundedVec<u8, ConstU32<MAX_CCM_MSG_LENGTH>>;

#[derive(
	Clone,
	Debug,
	Default,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
	PartialOrd,
	Ord,
)]
pub struct CcmAdditionalData(
	#[cfg_attr(
		feature = "std",
		serde(with = "bounded_hex", default, skip_serializing_if = "Vec::is_empty")
	)]
	pub BoundedVec<u8, ConstU32<MAX_CCM_ADDITIONAL_DATA_LENGTH>>,
);

impl From<BoundedVec<u8, ConstU32<MAX_CCM_ADDITIONAL_DATA_LENGTH>>> for CcmAdditionalData {
	fn from(bytes: BoundedVec<u8, ConstU32<MAX_CCM_ADDITIONAL_DATA_LENGTH>>) -> Self {
		Self(bytes)
	}
}

impl TryFrom<Vec<u8>> for CcmAdditionalData {
	type Error =
		<BoundedVec<u8, ConstU32<MAX_CCM_ADDITIONAL_DATA_LENGTH>> as TryFrom<Vec<u8>>>::Error;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		Ok(Self(BoundedVec::try_from(value)?))
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for CcmAdditionalData {
	fn benchmark_value() -> Self {
		Self::default()
	}
}

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
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	Serialize,
	Deserialize,
	MaxEncodedLen,
	PartialOrd,
	Ord,
)]
pub struct CcmChannelMetadata<AdditionalData> {
	/// Call data used after the message is egressed.
	#[cfg_attr(feature = "std", serde(with = "bounded_hex"))]
	pub message: CcmMessage,
	/// User funds designated to be used for gas.
	#[cfg_attr(feature = "std", serde(with = "cf_utilities::serde_helpers::number_or_hex"))]
	pub gas_budget: GasAmount,
	/// Additional parameters for the cross chain message.
	pub ccm_additional_data: AdditionalData,
}
impl CcmChannelMetadataUnchecked {
	pub fn to_checked(
		self,
		egress_asset: Asset,
		destination_address: ForeignChainAddress,
	) -> Result<CcmChannelMetadataChecked, CcmValidityError> {
		Ok(CcmChannelMetadata {
			message: self.message.clone(),
			gas_budget: self.gas_budget,
			ccm_additional_data: CcmValidityChecker::check_and_decode(
				&self,
				egress_asset,
				destination_address,
			)?,
		})
	}
}

pub type CcmChannelMetadataUnchecked = CcmChannelMetadata<CcmAdditionalData>;
pub type CcmChannelMetadataChecked = CcmChannelMetadata<DecodedCcmAdditionalData>;

#[cfg(feature = "runtime-benchmarks")]
impl<D: BenchmarkValue> BenchmarkValue for CcmChannelMetadata<D> {
	fn benchmark_value() -> Self {
		Self {
			message: BenchmarkValue::benchmark_value(),
			gas_budget: BenchmarkValue::benchmark_value(),
			ccm_additional_data: BenchmarkValue::benchmark_value(),
		}
	}
}

impl<T> From<CcmChannelMetadata<T>> for CcmParams {
	fn from(ccm: CcmChannelMetadata<T>) -> Self {
		CcmParams { message: ccm.message.to_vec(), gas_amount: ccm.gas_budget as u64 }
	}
}

#[derive(
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	Serialize,
	Deserialize,
	PartialOrd,
	Ord,
	MaxEncodedLen,
)]
pub struct CcmDepositMetadata<Address, AdditionalData> {
	pub channel_metadata: CcmChannelMetadata<AdditionalData>,
	pub source_chain: ForeignChain,
	pub source_address: Option<Address>,
}

#[cfg(feature = "runtime-benchmarks")]
impl<Address: BenchmarkValue, AdditionalData: BenchmarkValue> BenchmarkValue
	for CcmDepositMetadata<Address, AdditionalData>
{
	fn benchmark_value() -> Self {
		Self {
			channel_metadata: BenchmarkValue::benchmark_value(),
			source_chain: BenchmarkValue::benchmark_value(),
			source_address: Some(BenchmarkValue::benchmark_value()),
		}
	}
}

pub type CcmDepositMetadataUnchecked<Address> = CcmDepositMetadata<Address, CcmAdditionalData>;
pub type CcmDepositMetadataChecked<Address> = CcmDepositMetadata<Address, DecodedCcmAdditionalData>;

impl<A> CcmDepositMetadata<A, CcmAdditionalData> {
	pub fn to_checked(
		self,
		egress_asset: Asset,
		destination_address: ForeignChainAddress,
	) -> Result<CcmDepositMetadata<A, DecodedCcmAdditionalData>, CcmValidityError> {
		Ok(CcmDepositMetadata {
			source_address: self.source_address,
			channel_metadata: self
				.channel_metadata
				.to_checked(egress_asset, destination_address)?,
			source_chain: self.source_chain,
		})
	}
}

impl<AdditionalData> CcmDepositMetadata<ForeignChainAddress, AdditionalData> {
	pub fn to_encoded<Converter: AddressConverter>(
		self,
	) -> CcmDepositMetadata<EncodedAddress, AdditionalData> {
		CcmDepositMetadata {
			channel_metadata: self.channel_metadata,
			source_chain: self.source_chain,
			source_address: self.source_address.map(Converter::to_encoded_address),
		}
	}
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

pub trait FeeEstimationApi<C: Chain> {
	fn estimate_fee(
		&self,
		asset: C::ChainAsset,
		ingress_or_egress: IngressOrEgress,
	) -> C::ChainAmount;
}

impl<C: Chain> FeeEstimationApi<C> for () {
	fn estimate_fee(
		&self,
		_asset: C::ChainAsset,
		_ingress_or_egress: IngressOrEgress,
	) -> C::ChainAmount {
		Default::default()
	}
}

/// Defines an interface for a retry policy.
pub trait RetryPolicy {
	type BlockNumber;
	type AttemptCount;
	/// Returns the delay for the given attempt count. If None, no delay is applied.
	fn next_attempt_delay(retry_attempts: Self::AttemptCount) -> Option<Self::BlockNumber>;
}

pub struct DefaultRetryPolicy;
impl RetryPolicy for DefaultRetryPolicy {
	type BlockNumber = u32;
	type AttemptCount = u32;

	fn next_attempt_delay(_retry_attempts: Self::AttemptCount) -> Option<Self::BlockNumber> {
		Some(10u32)
	}
}

pub enum RequiresSignatureRefresh<C: ChainCrypto, Api: ApiCall<C>> {
	True(Option<Api>),
	False,
	_Phantom(PhantomData<C>, Never),
}

pub trait DepositDetailsToTransactionInId<C: ChainCrypto> {
	fn deposit_ids(&self) -> Option<Vec<C::TransactionInId>> {
		None
	}
}

#[derive(
	Clone, Debug, Encode, Decode, PartialEq, Eq, TypeInfo, Serialize, Deserialize, PartialOrd, Ord,
)]
pub struct EvmVaultSwapExtraParameters<Address, Amount> {
	pub input_amount: Amount,
	pub refund_parameters: ChannelRefundParametersUnchecked<Address>,
}
impl<Address: Clone, Amount> EvmVaultSwapExtraParameters<Address, Amount> {
	pub fn try_map_address<AddressOther, E>(
		self,
		f: impl Fn(Address) -> Result<AddressOther, E>,
	) -> Result<EvmVaultSwapExtraParameters<AddressOther, Amount>, E> {
		Ok(EvmVaultSwapExtraParameters {
			input_amount: self.input_amount,
			refund_parameters: self.refund_parameters.try_map_address(f)?,
		})
	}

	pub fn try_map_amounts<AmountOther, E>(
		self,
		f: impl Fn(Amount) -> Result<AmountOther, E>,
	) -> Result<EvmVaultSwapExtraParameters<Address, AmountOther>, E> {
		Ok(EvmVaultSwapExtraParameters {
			input_amount: f(self.input_amount)?,
			refund_parameters: self.refund_parameters,
		})
	}
}

#[derive(
	Clone, Debug, Encode, Decode, PartialEq, Eq, TypeInfo, Serialize, Deserialize, PartialOrd, Ord,
)]
#[serde(tag = "chain")]
pub enum VaultSwapExtraParameters<Address, Amount> {
	Bitcoin {
		min_output_amount: Amount,
		retry_duration: BlockNumber,
		max_oracle_price_slippage: Option<u8>,
	},
	Ethereum(EvmVaultSwapExtraParameters<Address, Amount>),
	Arbitrum(EvmVaultSwapExtraParameters<Address, Amount>),
	Solana {
		from: Address,
		#[cfg_attr(feature = "std", serde(with = "bounded_hex"))]
		seed: SolSeed,
		input_amount: Amount,
		refund_parameters: ChannelRefundParametersUnchecked<Address>,
		from_token_account: Option<Address>,
	},
}

impl<Address: Clone, Amount> VaultSwapExtraParameters<Address, Amount> {
	/// Try map address type parameters into another type.
	/// Typically used to convert RPC supported types into internal types.
	pub fn try_map_address<AddressOther, E>(
		self,
		f: impl Fn(Address) -> Result<AddressOther, E>,
	) -> Result<VaultSwapExtraParameters<AddressOther, Amount>, E> {
		Ok(match self {
			VaultSwapExtraParameters::Bitcoin {
				min_output_amount,
				retry_duration,
				max_oracle_price_slippage,
			} => VaultSwapExtraParameters::Bitcoin {
				min_output_amount,
				retry_duration,
				max_oracle_price_slippage,
			},
			VaultSwapExtraParameters::Ethereum(extra_parameter) =>
				VaultSwapExtraParameters::Ethereum(extra_parameter.try_map_address(f)?),
			VaultSwapExtraParameters::Arbitrum(extra_parameter) =>
				VaultSwapExtraParameters::Arbitrum(extra_parameter.try_map_address(f)?),
			VaultSwapExtraParameters::Solana {
				from,
				seed,
				input_amount,
				refund_parameters,
				from_token_account,
			} => VaultSwapExtraParameters::Solana {
				from: f(from)?,
				seed,
				input_amount,
				refund_parameters: refund_parameters.try_map_address(&f)?,
				from_token_account: from_token_account.map(&f).transpose()?,
			},
		})
	}

	/// Try map numerical parameters into another type.
	/// Typically used to convert RPC supported types into internal types.
	pub fn try_map_amounts<NumberOther, E>(
		self,
		f: impl Fn(Amount) -> Result<NumberOther, E>,
	) -> Result<VaultSwapExtraParameters<Address, NumberOther>, E> {
		Ok(match self {
			VaultSwapExtraParameters::Bitcoin {
				min_output_amount,
				retry_duration,
				max_oracle_price_slippage,
			} => VaultSwapExtraParameters::Bitcoin {
				min_output_amount: f(min_output_amount)?,
				retry_duration,
				max_oracle_price_slippage,
			},
			VaultSwapExtraParameters::Ethereum(extra_parameter) =>
				VaultSwapExtraParameters::Ethereum(extra_parameter.try_map_amounts(f)?),
			VaultSwapExtraParameters::Arbitrum(extra_parameter) =>
				VaultSwapExtraParameters::Arbitrum(extra_parameter.try_map_amounts(f)?),
			VaultSwapExtraParameters::Solana {
				from,
				seed,
				input_amount,
				refund_parameters,
				from_token_account,
			} => VaultSwapExtraParameters::Solana {
				from,
				seed,
				input_amount: f(input_amount)?,
				refund_parameters,
				from_token_account,
			},
		})
	}
}

/// Type used internally within the State chain.
pub type VaultSwapExtraParametersEncoded = VaultSwapExtraParameters<EncodedAddress, AssetAmount>;

#[derive(Clone, Debug, Encode, Decode, Serialize, Deserialize, TypeInfo)]
pub struct VaultSwapInput<Address, Amount> {
	pub source_asset: AnyChainAsset,
	pub destination_asset: AnyChainAsset,
	pub destination_address: Address,
	pub broker_commission: BasisPoints,
	pub extra_parameters: VaultSwapExtraParameters<Address, Amount>,
	pub channel_metadata: Option<CcmChannelMetadataUnchecked>,
	pub boost_fee: BasisPoints,
	pub affiliate_fees: Affiliates<cf_primitives::AccountId>,
	pub dca_parameters: Option<DcaParameters>,
}

/// Type used internally within the State chain.
pub type VaultSwapInputEncoded = VaultSwapInput<EncodedAddress, AssetAmount>;
