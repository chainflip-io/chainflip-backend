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
#![feature(extract_if)]
#![feature(map_try_insert)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

mod benchmarking;

pub mod migrations;

#[cfg(test)]
mod mocks;
#[cfg(test)]
mod tests;
pub mod weights;

use cf_chains::{
	address::{
		AddressConverter, AddressDerivationApi, AddressDerivationError, IntoForeignChainAddress,
	},
	ccm_checker::DecodedCcmAdditionalData,
	instances::PalletInstanceAlias,
	AccountOrAddress, AllBatch, AllBatchError, CcmChannelMetadataChecked, CcmDepositMetadata,
	CcmDepositMetadataChecked, CcmDepositMetadataUnchecked, CcmMessage, Chain, ChainCrypto,
	ChannelLifecycleHooks, ChannelRefundParameters, ChannelRefundParametersForChain,
	ConsolidateCall, DepositChannel, DepositDetailsToTransactionInId, DepositOriginType,
	ExecutexSwapAndCall, ExecutexSwapAndCallError, FetchAssetParams, ForeignChainAddress,
	IntoTransactionInIdForAnyChain, RejectCall, SwapOrigin, TransferAssetParams,
};
use cf_primitives::{
	AccountRole, AffiliateShortId, Affiliates, Asset, BasisPoints, Beneficiaries, Beneficiary,
	BoostPoolTier, BroadcastId, ChannelId, DcaParameters, EgressCounter, EgressId, EpochIndex,
	ForeignChain, GasAmount, IngressOrEgress, PrewitnessedDepositId, SwapRequestId,
	ThresholdSignatureRequestId, SECONDS_PER_BLOCK,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode,
	lending::{BoostApi, BoostOutcome},
	AccountRoleRegistry, AdjustedFeeEstimationApi, AffiliateRegistry, AssetConverter,
	AssetWithholding, BalanceApi, Broadcaster, CcmAdditionalDataHandler, Chainflip,
	ChannelIdAllocator, DepositApi, EgressApi, EpochInfo, FeePayment,
	FetchesTransfersLimitProvider, GetBlockHeight, IngressEgressFeeApi, IngressSink, IngressSource,
	NetworkEnvironmentProvider, OnDeposit, ScheduledEgressDetails, SwapOutputAction,
	SwapParameterValidation, SwapRequestHandler, SwapRequestType,
};
use frame_support::{
	pallet_prelude::{OptionQuery, *},
	sp_runtime::{traits::Zero, DispatchError, Saturating},
	transactional, OrdNoBound, PartialOrdNoBound,
};
use frame_system::pallet_prelude::*;
use generic_typeinfo_derive::GenericTypeInfo;
pub use pallet::*;
use serde::{Deserialize, Serialize};
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{boxed::Box, vec, vec::Vec};
pub use weights::WeightInfo;

const MARKED_TX_EXPIRATION_BLOCKS: u32 = 3600 / SECONDS_PER_BLOCK as u32;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Default)]
pub enum BoostStatus<ChainAmount, BlockNumber> {
	// If a (pre-witnessed) deposit on a channel has been boosted, we record
	// its id, amount, and the pools that participated in boosting it.
	Boosted {
		prewitnessed_deposit_id: PrewitnessedDepositId,
		amount: ChainAmount,
	},
	#[default]
	NotBoosted,
	BoostPending {
		amount: ChainAmount,
		process_at_block: BlockNumber,
	},
}

#[derive(
	CloneNoBound, RuntimeDebugNoBound, Encode, Decode, TypeInfo, PartialEqNoBound, EqNoBound,
)]
#[scale_info(skip_type_params(T, I))]
enum BoostStatusLookup<T: Config<I>, I: 'static> {
	Vault { tx_id: TransactionInIdFor<T, I> },
	Channel { deposit_address: TargetChainAccount<T, I> },
}

impl<T: Config<I>, I: 'static> BoostStatusLookup<T, I> {
	pub fn resolve(&self) -> BoostStatus<TargetChainAmount<T, I>, BlockNumberFor<T>> {
		match self {
			BoostStatusLookup::Vault { tx_id } => BoostedVaultTransactions::<T, I>::get(tx_id),
			BoostStatusLookup::Channel { deposit_address } =>
				DepositChannelLookup::<T, I>::get(deposit_address)
					.map(|deposit_channel| deposit_channel.boost_status)
					.unwrap_or(BoostStatus::NotBoosted),
		}
	}

	pub fn set(&self, boost_status: BoostStatus<TargetChainAmount<T, I>, BlockNumberFor<T>>) {
		match self {
			BoostStatusLookup::Vault { tx_id } => {
				BoostedVaultTransactions::<T, I>::insert(tx_id, boost_status);
			},
			BoostStatusLookup::Channel { deposit_address } => {
				DepositChannelLookup::<T, I>::mutate(deposit_address, |details| {
					if let Some(details) = details {
						details.boost_status = boost_status;
					}
				});
			},
		}
	}
}

#[derive(Clone, RuntimeDebugNoBound, Encode, Decode, TypeInfo, PartialEq, Eq)]
#[scale_info(skip_type_params(T, I))]
struct PendingPrewitnessedDepositEntry<T: Config<I>, I: 'static> {
	boost_status_lookup: BoostStatusLookup<T, I>,
	deposit: PendingPrewitnessedDeposit<T, I>,
}

#[derive(Clone, RuntimeDebugNoBound, Encode, Decode, TypeInfo, PartialEq, Eq)]
#[scale_info(skip_type_params(T, I))]
struct PendingPrewitnessedDeposit<T: Config<I>, I: 'static> {
	block_height: TargetChainBlockNumber<T, I>,
	amount: TargetChainAmount<T, I>,
	asset: TargetChainAsset<T, I>,
	deposit_details: <T::TargetChain as Chain>::DepositDetails,
	deposit_address: Option<TargetChainAccount<T, I>>,
	action: ChannelActionForDeposit<T::AccountId, TargetChainAccount<T, I>>,
	boost_fee: u16,
	channel_id: Option<u64>,
	origin: DepositOrigin<T, I>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct TransactionRejectionStatus<BlockNumber> {
	expires_at: BlockNumber,
	/// We can't expire if the rejected tx has been prewitnessed. We need to wait until the
	/// rejection is processed.
	prewitnessed: bool,
}

/// Enum wrapper for fetch and egress requests.
#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo)]
pub enum FetchOrTransfer<C: Chain> {
	Fetch {
		asset: C::ChainAsset,
		deposit_address: C::ChainAccount,
		deposit_fetch_id: Option<C::DepositFetchId>,
		amount: C::ChainAmount,
	},
	Transfer {
		egress_id: EgressId,
		asset: C::ChainAsset,
		destination_address: C::ChainAccount,
		amount: C::ChainAmount,
	},
}

impl<C: Chain> FetchOrTransfer<C> {
	fn asset(&self) -> &C::ChainAsset {
		match self {
			FetchOrTransfer::Fetch { asset, .. } => asset,
			FetchOrTransfer::Transfer { asset, .. } => asset,
		}
	}
}

#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo)]
pub enum DepositFailedReason {
	BelowMinimumDeposit,
	/// The deposit was ignored because the amount provided was not high enough to pay for the fees
	/// required to process the requisite transactions.
	NotEnoughToPayFees,
	TransactionRejectedByBroker,
	DepositWitnessRejected(DispatchError),
	Unrefundable,
}

#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo)]
pub enum RefundReason {
	InvalidBrokerFees,
	InvalidRefundParameters,
	InvalidDcaParameters,
	CcmUnsupportedForTargetChain,
	CcmInvalidMetadata,
	InvalidDestinationAddress,
}

enum FullWitnessDepositOutcome {
	/// Either boost has been finalised, or it was still pending (due to a delay)
	/// and should no longer be scheduled for processing
	BoostConsumed,
	BoostNotConsumed,
}
mod deposit_origin {

	use super::*;

	#[derive(CloneNoBound, DebugNoBound, Encode, Decode, PartialEq, Eq, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub(super) enum DepositOrigin<T: Config<I>, I: 'static> {
		DepositChannel {
			deposit_address: TargetChainAccount<T, I>,
			channel_id: ChannelId,
			deposit_block_height: u64,
			broker_id: T::AccountId,
		},
		Vault {
			tx_id: TransactionInIdFor<T, I>,
			broker_id: Option<T::AccountId>,
		},
	}

	impl<T: Config<I>, I: 'static> DepositOrigin<T, I> {
		pub fn deposit_channel(
			deposit_address: TargetChainAccount<T, I>,
			channel_id: ChannelId,
			deposit_block_height: <T::TargetChain as Chain>::ChainBlockNumber,
			broker_id: T::AccountId,
		) -> Self {
			DepositOrigin::DepositChannel {
				deposit_address,
				channel_id,
				deposit_block_height: deposit_block_height.into(),
				broker_id,
			}
		}

		pub fn vault(tx_id: TransactionInIdFor<T, I>, broker_id: Option<T::AccountId>) -> Self {
			DepositOrigin::Vault { tx_id, broker_id }
		}

		pub fn broker_id(&self) -> Option<&T::AccountId> {
			match self {
				Self::DepositChannel { ref broker_id, .. } => Some(broker_id),
				Self::Vault { ref broker_id, .. } => broker_id.as_ref(),
			}
		}
	}

	impl<T: Config<I>, I: 'static> From<DepositOrigin<T, I>> for DepositOriginType {
		fn from(origin: DepositOrigin<T, I>) -> Self {
			match origin {
				DepositOrigin::DepositChannel { .. } => DepositOriginType::DepositChannel,
				DepositOrigin::Vault { .. } => DepositOriginType::Vault,
			}
		}
	}

	impl<T: Config<I>, I: 'static> From<DepositOrigin<T, I>> for SwapOrigin<T::AccountId> {
		fn from(origin: DepositOrigin<T, I>) -> SwapOrigin<T::AccountId> {
			match origin {
				DepositOrigin::Vault { tx_id, broker_id } => SwapOrigin::Vault {
					tx_id: tx_id.into_transaction_in_id_for_any_chain(),
					broker_id,
				},
				DepositOrigin::DepositChannel {
					deposit_address,
					channel_id,
					deposit_block_height,
					broker_id,
				} => SwapOrigin::DepositChannel {
					deposit_address: T::AddressConverter::to_encoded_address(
						deposit_address.clone().into_foreign_chain_address(),
					),
					channel_id,
					deposit_block_height,
					broker_id,
				},
			}
		}
	}
}

use deposit_origin::DepositOrigin;

/// Holds information about a transaction that is marked for rejection.
#[derive(RuntimeDebug, PartialEq, Eq, Encode, Decode, GenericTypeInfo, CloneNoBound)]
#[expand_name_with(<T::TargetChain as PalletInstanceAlias>::TYPE_INFO_SUFFIX)]
pub struct TransactionRejectionDetails<T: Config<I>, I: 'static> {
	pub deposit_address: Option<TargetChainAccount<T, I>>,
	#[skip_name_expansion]
	pub refund_address: ForeignChainAddress,
	pub asset: TargetChainAsset<T, I>,
	pub amount: TargetChainAmount<T, I>,
	pub deposit_details: <T::TargetChain as Chain>::DepositDetails,
	pub refund_ccm_metadata: Option<CcmDepositMetadataChecked<ForeignChainAddress>>,
}

/// Cross-chain messaging requests.
#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct CrossChainMessage<C: Chain> {
	pub egress_id: EgressId,
	pub asset: C::ChainAsset,
	pub amount: C::ChainAmount,
	pub destination_address: C::ChainAccount,
	pub message: CcmMessage,
	pub source_chain: ForeignChain,
	pub source_address: Option<ForeignChainAddress>,
	pub ccm_additional_data: DecodedCcmAdditionalData,
	pub gas_budget: GasAmount,
}

impl<C: Chain> CrossChainMessage<C> {
	fn asset(&self) -> C::ChainAsset {
		self.asset
	}
}

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(28);

impl_pallet_safe_mode! {
	PalletSafeMode<I>;
	boost_deposits_enabled,
	deposit_channel_creation_enabled,
	deposit_channel_witnessing_enabled,
	vault_deposit_witnessing_enabled,
}

/// Calls to the external chains that has failed to be broadcast/accepted by the target chain.
/// User can use information stored here to query for relevant information to broadcast
/// the call themselves.
#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct FailedForeignChainCall {
	/// Broadcast ID used in the broadcast pallet. Use it to query broadcast information,
	/// such as the threshold signature, the API call etc.
	pub broadcast_id: BroadcastId,
	/// The epoch the call originally failed in. Calls are cleaned from storage 2 epochs.
	pub original_epoch: EpochIndex,
}

#[derive(
	CloneNoBound,
	RuntimeDebugNoBound,
	PartialEqNoBound,
	EqNoBound,
	Encode,
	Decode,
	MaxEncodedLen,
	GenericTypeInfo,
)]
#[expand_name_with(<T::TargetChain as PalletInstanceAlias>::TYPE_INFO_SUFFIX)]
pub enum PalletConfigUpdate<T: Config<I>, I: 'static> {
	/// Set the fixed fee that is burned when opening a channel, denominated in Flipperinos.
	ChannelOpeningFee {
		fee: T::Amount,
	},
	/// Set the minimum deposit allowed for a particular asset.
	SetMinimumDeposit {
		asset: TargetChainAsset<T, I>,
		minimum_deposit: TargetChainAmount<T, I>,
	},
	/// Set the deposit channel lifetime. The time before the engines stop witnessing a channel.
	/// This is configurable primarily to allow for unpredictable block times in testnets.
	SetDepositChannelLifetime {
		lifetime: TargetChainBlockNumber<T, I>,
	},
	SetWitnessSafetyMargin {
		margin: TargetChainBlockNumber<T, I>,
	},
	SetBoostDelay {
		delay_blocks: BlockNumberFor<T>,
	},
	SetMaximumPreallocatedChannels {
		account_role: AccountRole,
		num_channels: u8,
	},
	SetIngressDelay {
		delay_blocks: BlockNumberFor<T>,
	},
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::{address::EncodedAddress, ExecutexSwapAndCall, TransferFallback};
	use cf_primitives::{BroadcastId, EpochIndex};
	use cf_traits::{OnDeposit, SwapParameterValidation};
	use cf_utilities::bounded_vec::map_bounded_vec;
	use core::marker::PhantomData;
	use frame_support::traits::{ConstU128, EnsureOrigin, IsType};
	use frame_system::WeightInfo as SystemWeightInfo;
	use sp_runtime::SaturatedConversion;
	use sp_std::{
		collections::{btree_map::BTreeMap, vec_deque::VecDeque},
		vec::Vec,
	};

	pub(crate) type ChannelRecycleQueue<T, I> =
		Vec<(TargetChainBlockNumber<T, I>, TargetChainAccount<T, I>)>;

	pub type TargetChainAsset<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAsset;
	pub type TargetChainAccount<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAccount;
	pub(crate) type TargetChainAmount<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAmount;
	pub(crate) type TargetChainBlockNumber<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::ChainBlockNumber;

	pub type TransactionInIdFor<T, I> =
		<<<T as Config<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionInId;

	#[derive(
		Clone,
		RuntimeDebug,
		PartialEq,
		Eq,
		Encode,
		Decode,
		MaxEncodedLen,
		Serialize,
		Deserialize,
		Ord,
		PartialOrd,
		GenericTypeInfo,
	)]
	#[expand_name_with(C::NAME)]
	pub struct DepositWitness<C: Chain> {
		pub deposit_address: C::ChainAccount,
		pub asset: C::ChainAsset,
		pub amount: C::ChainAmount,
		pub deposit_details: C::DepositDetails,
	}

	#[derive(
		CloneNoBound,
		RuntimeDebugNoBound,
		PartialEqNoBound,
		EqNoBound,
		Encode,
		Decode,
		Serialize,
		Deserialize,
		OrdNoBound,
		PartialOrdNoBound,
		GenericTypeInfo,
	)]
	#[serde(bound(serialize = "", deserialize = ""))]
	#[expand_name_with(<T::TargetChain as PalletInstanceAlias>::TYPE_INFO_SUFFIX)]
	pub struct VaultDepositWitness<T: Config<I>, I: 'static> {
		pub input_asset: TargetChainAsset<T, I>,
		pub deposit_address: Option<TargetChainAccount<T, I>>,
		#[skip_name_expansion]
		pub channel_id: Option<ChannelId>,
		pub deposit_amount: <T::TargetChain as Chain>::ChainAmount,
		pub deposit_details: <T::TargetChain as Chain>::DepositDetails,
		#[skip_name_expansion]
		pub output_asset: Asset,
		// Note we use EncodedAddress here rather than eg. ForeignChainAddress because this
		// value can be populated by the submitter of the vault deposit and is not verified
		// in the engine, so we need to verify on-chain.
		#[skip_name_expansion]
		pub destination_address: EncodedAddress,
		// Here, the `source_address` within the CCM Metadata is populated by the engines, so we
		// can use ForeignChainAddress here.
		#[skip_name_expansion]
		pub deposit_metadata: Option<CcmDepositMetadataUnchecked<ForeignChainAddress>>,
		pub tx_id: TransactionInIdFor<T, I>,
		#[skip_name_expansion]
		pub broker_fee: Option<Beneficiary<T::AccountId>>,
		#[skip_name_expansion]
		pub affiliate_fees: Affiliates<AffiliateShortId>,
		pub refund_params: ChannelRefundParametersForChain<T::TargetChain>,
		#[skip_name_expansion]
		pub dca_params: Option<DcaParameters>,
		#[skip_name_expansion]
		pub boost_fee: BasisPoints,
	}

	#[derive(
		CloneNoBound,
		RuntimeDebugNoBound,
		PartialEqNoBound,
		EqNoBound,
		Encode,
		Decode,
		GenericTypeInfo,
	)]
	#[expand_name_with(<T::TargetChain as PalletInstanceAlias>::TYPE_INFO_SUFFIX)]
	pub enum DepositFailedDetails<T: Config<I>, I: 'static> {
		#[replace_typename_with(DepositFailedDepositChannelVariant)]
		DepositChannel { deposit_witness: DepositWitness<T::TargetChain> },
		#[replace_typename_with(DepositFailedVaultVariant)]
		Vault { vault_witness: Box<VaultDepositWitness<T, I>> },
	}

	#[derive(CloneNoBound, RuntimeDebug, PartialEq, Eq, Encode, Decode, GenericTypeInfo)]
	#[expand_name_with(<T::TargetChain as PalletInstanceAlias>::TYPE_INFO_SUFFIX)]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		/// The owner of the deposit channel.
		#[skip_name_expansion]
		pub owner: T::AccountId,
		pub deposit_channel: DepositChannel<T::TargetChain>,
		/// The block number at which the deposit channel was opened, expressed as a block number
		/// on the external Chain.
		pub opened_at: TargetChainBlockNumber<T, I>,
		/// The last block on the target chain that the witnessing will witness it in. If funds are
		/// sent after this block, they will not be witnessed.
		pub expires_at: TargetChainBlockNumber<T, I>,
		/// The action to be taken when the DepositChannel is deposited to.
		#[skip_name_expansion]
		pub action: ChannelAction<T::AccountId, TargetChainAccount<T, I>>,
		/// The boost fee
		#[skip_name_expansion]
		pub boost_fee: BasisPoints,
		/// Boost status, indicating whether there is pending boost on the channel
		pub boost_status: BoostStatus<TargetChainAmount<T, I>, BlockNumberFor<T>>,
		pub is_marked_for_rejection: bool,
	}

	pub struct AmountAndFeesWithheld<A> {
		pub amount_after_fees: A,
		pub fees_withheld: A,
	}

	impl<A> AmountAndFeesWithheld<A> {
		pub fn map_amounts<B>(self, f: impl Fn(A) -> B) -> AmountAndFeesWithheld<B> {
			AmountAndFeesWithheld {
				amount_after_fees: f(self.amount_after_fees),
				fees_withheld: f(self.fees_withheld),
			}
		}
	}

	/// Determines the action to take when a deposit is made to a channel.
	#[allow(clippy::large_enum_variant)]
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[n_functor::derive_n_functor(CcmMetadata = ccm_metadata)]
	#[scale_info(skip_type_params(C))]
	#[allow(clippy::large_enum_variant)]
	pub enum ChannelAction<AccountId, ChainAccount, CcmMetadata = CcmChannelMetadataChecked> {
		Swap {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			#[map_with(|beneficiaries, f| map_bounded_vec(beneficiaries, |beneficiary: Beneficiary<_>| beneficiary.map(f)))]
			broker_fees: Beneficiaries<AccountId>,
			channel_metadata: Option<CcmMetadata>,
			#[map_with(|refund_parameters: ChannelRefundParameters<_,_>, f| refund_parameters.map(|address| address, |metadata: Option<_>| metadata.map(f)))]
			refund_params: ChannelRefundParameters<ForeignChainAddress, Option<CcmMetadata>>,
			dca_params: Option<DcaParameters>,
		},
		LiquidityProvision {
			lp_account: AccountId,
			refund_address: ForeignChainAddress,
		},
		Refund {
			reason: RefundReason,
			refund_address: ChainAccount,
			refund_ccm_metadata: Option<CcmMetadata>,
		},
		Unrefundable,
	}

	pub type ChannelActionForDeposit<AccountId, ChainAccount> =
		ChannelAction<AccountId, ChainAccount, CcmDepositMetadataChecked<ForeignChainAddress>>;

	/// Contains identifying information about the particular actions that have occurred for a
	/// particular deposit.
	#[derive(CloneNoBound, RuntimeDebugNoBound, PartialEqNoBound, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub enum DepositAction<T: Config<I>, I: 'static> {
		Swap {
			swap_request_id: SwapRequestId,
		},
		LiquidityProvision {
			lp_account: T::AccountId,
		},
		CcmTransfer {
			swap_request_id: SwapRequestId,
		},
		BoostersCredited {
			prewitnessed_deposit_id: PrewitnessedDepositId,
			network_fee_from_boost: TargetChainAmount<T, I>,
			// Optional since we only swap if the amount is non-zero
			network_fee_swap_request_id: Option<SwapRequestId>,
		},
		Refund {
			egress_id: Option<EgressId>,
			reason: RefundReason,
			amount: TargetChainAmount<T, I>,
		},
		Unrefundable,
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		pub deposit_channel_lifetime: TargetChainBlockNumber<T, I>,
		pub witness_safety_margin: Option<TargetChainBlockNumber<T, I>>,
		pub dust_limits: Vec<(TargetChainAsset<T, I>, TargetChainAmount<T, I>)>,
	}

	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			Self {
				deposit_channel_lifetime: Default::default(),
				witness_safety_margin: None,
				dust_limits: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			DepositChannelLifetime::<T, I>::put(self.deposit_channel_lifetime);
			WitnessSafetyMargin::<T, I>::set(self.witness_safety_margin);

			for (asset, dust_limit) in self.dust_limits.clone() {
				EgressDustLimit::<T, I>::set(asset, dust_limit.unique_saturated_into());
			}
		}
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The pallet dispatches calls, so it depends on the runtime's aggregated Call type.
		type RuntimeCall: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::RuntimeCall>;

		/// Sets if the pallet should automatically manage the closing of channels.
		const MANAGE_CHANNEL_LIFETIME: bool;

		/// If true, pre-allocated channels will only be taken from the pool of recycled channels.
		const ONLY_PREALLOCATE_FROM_POOL: bool;

		/// A hook to tell witnesses to start witnessing an opened channel.
		type IngressSource: IngressSource<
			Chain = Self::TargetChain,
			StateChainBlockNumber = BlockNumberFor<Self>,
		>;

		/// Marks which chain this pallet is interacting with.
		type TargetChain: Chain + Get<ForeignChain>;

		/// Generates deposit addresses.
		type AddressDerivation: AddressDerivationApi<Self::TargetChain>;

		/// A converter to convert address to and from human readable to internal address
		/// representation.
		type AddressConverter: AddressConverter;

		type Balance: BalanceApi<AccountId = Self::AccountId>;

		/// The type of the chain-native transaction.
		type ChainApiCall: AllBatch<Self::TargetChain>
			+ ExecutexSwapAndCall<Self::TargetChain>
			+ TransferFallback<Self::TargetChain>
			+ ConsolidateCall<Self::TargetChain>
			+ RejectCall<Self::TargetChain>;

		/// Get the latest chain state of the target chain.
		type ChainTracking: GetBlockHeight<Self::TargetChain>
			+ AdjustedFeeEstimationApi<Self::TargetChain>;

		/// A broadcaster instance.
		type Broadcaster: Broadcaster<
			Self::TargetChain,
			ApiCall = Self::ChainApiCall,
			Callback = <Self as Config<I>>::RuntimeCall,
		>;

		/// Provides callbacks for deposit lifecycle events.
		type DepositHandler: OnDeposit<Self::TargetChain>;

		type NetworkEnvironment: NetworkEnvironmentProvider;

		/// Allows assets to be converted through the AMM.
		type AssetConverter: AssetConverter;

		/// For paying the channel opening fee.
		type FeePayment: FeePayment<Amount = Self::Amount, AccountId = Self::AccountId>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;

		type SwapRequestHandler: SwapRequestHandler<AccountId = Self::AccountId>;

		type AssetWithholding: AssetWithholding;

		type FetchesTransfersLimitProvider: FetchesTransfersLimitProvider;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode<I>>;

		type SwapParameterValidation: SwapParameterValidation<AccountId = Self::AccountId>;

		type CcmAdditionalDataHandler: CcmAdditionalDataHandler;

		type AffiliateRegistry: AffiliateRegistry<AccountId = Self::AccountId>;

		type BoostApi: cf_traits::lending::BoostApi;

		#[pallet::constant]
		type AllowTransactionReports: Get<bool>;

		#[pallet::constant]
		type ScreeningBrokerId: Get<Self::AccountId>;
	}

	/// Lookup table for addresses to corresponding deposit channels.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type DepositChannelLookup<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		TargetChainAccount<T, I>,
		DepositChannelDetails<T, I>,
		OptionQuery,
	>;

	/// Stores the latest channel id used to generate an address.
	#[pallet::storage]
	pub type ChannelIdCounter<T: Config<I>, I: 'static = ()> =
		StorageValue<_, ChannelId, ValueQuery>;

	/// Stores the latest egress id used to generate an address.
	#[pallet::storage]
	pub type EgressIdCounter<T: Config<I>, I: 'static = ()> =
		StorageValue<_, EgressCounter, ValueQuery>;

	/// Scheduled fetch and egress for the Ethereum chain.
	#[pallet::storage]
	pub type ScheduledEgressFetchOrTransfer<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<FetchOrTransfer<T::TargetChain>>, ValueQuery>;

	/// Scheduled cross chain messages for the Ethereum chain.
	#[pallet::storage]
	pub type ScheduledEgressCcm<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<CrossChainMessage<T::TargetChain>>, ValueQuery>;

	/// Stores the list of assets that are not allowed to be egressed.
	#[pallet::storage]
	pub type DisabledEgressAssets<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, ()>;

	/// Stores address ready for use.
	#[pallet::storage]
	pub type DepositChannelPool<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, ChannelId, DepositChannel<T::TargetChain>>;

	/// Defines the minimum amount of Deposit allowed for each asset.
	#[pallet::storage]
	pub type MinimumDeposit<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, TargetChainAmount<T, I>, ValueQuery>;

	/// Defines the minimum amount aka. dust limit for a single egress i.e. *not* of a batch, but
	/// the outputs of each individual egress within that batch. If not set, defaults to 1.
	///
	/// This is required for bitcoin, for example, where any amount below 600 satoshis is considered
	/// dust and will be rejected by miners.
	#[pallet::storage]
	#[pallet::getter(fn egress_dust_limit)]
	pub type EgressDustLimit<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, u128, ValueQuery, ConstU128<1>>;

	#[pallet::storage]
	pub type DepositChannelLifetime<T: Config<I>, I: 'static = ()> =
		StorageValue<_, TargetChainBlockNumber<T, I>, ValueQuery>;

	/// Stores information about Calls to external chains that have failed to be broadcasted.
	/// These calls are signed and stored on-chain so that the user can broadcast the call
	/// themselves. These messages will be re-threshold-signed once during the next epoch, and
	/// removed from storage in the epoch after that.
	/// Hashmap: last_signed_epoch -> Vec<FailedForeignChainCall>
	#[pallet::storage]
	#[pallet::getter(fn failed_foreign_chain_calls)]
	pub type FailedForeignChainCalls<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, EpochIndex, Vec<FailedForeignChainCall>, ValueQuery>;

	#[pallet::storage]
	pub type DepositChannelRecycleBlocks<T: Config<I>, I: 'static = ()> =
		StorageValue<_, ChannelRecycleQueue<T, I>, ValueQuery>;

	// Determines the number of block confirmations is required for a block on
	// an external chain before CFE can submit any witness extrinsics for it.
	//
	// This storage item won't be used for any chains using the elections witnessing and
	// will be removed once all chains are migrated
	#[pallet::storage]
	#[pallet::getter(fn witness_safety_margin)]
	pub type WitnessSafetyMargin<T: Config<I>, I: 'static = ()> =
		StorageValue<_, TargetChainBlockNumber<T, I>, OptionQuery>;

	/// The fixed fee charged for opening a channel, in Flipperinos.
	#[pallet::storage]
	#[pallet::getter(fn channel_opening_fee)]
	pub type ChannelOpeningFee<T: Config<I>, I: 'static = ()> =
		StorageValue<_, T::Amount, ValueQuery>;

	/// How many blocks to wait before processing a prewitnessed deposit.
	#[pallet::storage]
	pub type BoostDelayBlocks<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// How many blocks to wait before processing a new deposit.
	#[pallet::storage]
	pub type IngressDelayBlocks<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// Stores the latest prewitnessed deposit id used.
	#[pallet::storage]
	pub type PrewitnessedDepositIdCounter<T: Config<I>, I: 'static = ()> =
		StorageValue<_, PrewitnessedDepositId, ValueQuery>;

	/// Stores the reporter and the tx_id against the BlockNumber when the report expires.
	#[pallet::storage]
	pub(crate) type TransactionsMarkedForRejection<T: Config<I>, I: 'static = ()> =
		StorageDoubleMap<
			_,
			Identity,
			T::AccountId,
			Blake2_128Concat,
			TransactionInIdFor<T, I>,
			TransactionRejectionStatus<BlockNumberFor<T>>,
			OptionQuery,
		>;

	/// Stores the block number when the report expires to gather with the reporter and the tx_id.
	#[pallet::storage]
	pub(crate) type ReportExpiresAt<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BlockNumberFor<T>,
		Vec<(T::AccountId, TransactionInIdFor<T, I>)>,
		ValueQuery,
	>;

	/// Stores the details of transactions that are scheduled for rejecting.
	#[pallet::storage]
	pub type ScheduledTransactionsForRejection<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<TransactionRejectionDetails<T, I>>, ValueQuery>;

	/// Stores the details of transactions that failed to be rejected.
	#[pallet::storage]
	pub type FailedRejections<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<TransactionRejectionDetails<T, I>>, ValueQuery>;

	/// Stores the whitelisted brokers.
	#[pallet::storage]
	pub type WhitelistedBrokers<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Identity, T::AccountId, (), ValueQuery>;

	/// Stores transaction ids that have been boosted but have not yet been finalised.
	#[pallet::storage]
	pub type BoostedVaultTransactions<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Identity,
		TransactionInIdFor<T, I>,
		BoostStatus<TargetChainAmount<T, I>, BlockNumberFor<T>>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub(super) type PendingPrewitnessedDeposits<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BlockNumberFor<T>,
		Vec<PendingPrewitnessedDepositEntry<T, I>>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type PendingDepositChannelDeposits<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BlockNumberFor<T>,
		Vec<(DepositWitness<T::TargetChain>, TargetChainBlockNumber<T, I>)>,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type PendingVaultDeposits<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		BlockNumberFor<T>,
		Vec<(VaultDepositWitness<T, I>, TargetChainBlockNumber<T, I>)>,
		ValueQuery,
	>;

	/// What the witnessing says we've processed up to. This allows us to expire channels safely. If
	/// the witnessing has processed up to a block, then we can safely recycle the channels.
	#[pallet::storage]
	pub type ProcessedUpTo<T: Config<I>, I: 'static = ()> =
		StorageValue<_, TargetChainBlockNumber<T, I>, ValueQuery>;

	/// Stores configuration param for maximum number of pre-allocated channels per account role.
	#[pallet::storage]
	#[pallet::getter(fn maximum_preallocated_channels)]
	pub type MaximumPreallocatedChannels<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, AccountRole, u8, ValueQuery>;

	/// Stores pre-allocated channels per account.
	#[pallet::storage]
	pub type PreallocatedChannels<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		VecDeque<DepositChannel<T::TargetChain>>,
		ValueQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		DepositFinalised {
			deposit_address: Option<TargetChainAccount<T, I>>,
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			block_height: TargetChainBlockNumber<T, I>,
			deposit_details: <T::TargetChain as Chain>::DepositDetails,
			// Ingress fee in the deposit asset. i.e. *NOT* the gas asset, if the deposit asset is
			// a non-gas asset.
			ingress_fee: TargetChainAmount<T, I>,
			max_boost_fee_bps: BasisPoints,
			action: DepositAction<T, I>,
			channel_id: Option<ChannelId>,
			origin_type: DepositOriginType,
		},
		AssetEgressStatusChanged {
			asset: TargetChainAsset<T, I>,
			disabled: bool,
		},
		CcmBroadcastRequested {
			broadcast_id: BroadcastId,
			egress_id: EgressId,
		},
		CcmEgressInvalid {
			egress_id: EgressId,
			error: ExecutexSwapAndCallError,
		},
		DepositFetchesScheduled {
			channel_id: ChannelId,
			asset: TargetChainAsset<T, I>,
		},
		BatchBroadcastRequested {
			broadcast_id: BroadcastId,
			egress_ids: Vec<EgressId>,
		},
		DepositFailed {
			block_height: TargetChainBlockNumber<T, I>,
			reason: DepositFailedReason,
			details: DepositFailedDetails<T, I>,
		},
		TransferFallbackRequested {
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			destination_address: TargetChainAccount<T, I>,
			broadcast_id: BroadcastId,
			egress_details: Option<ScheduledEgressDetails<T::TargetChain>>,
		},
		/// A CCM has failed to broadcast.
		CcmBroadcastFailed {
			broadcast_id: BroadcastId,
		},
		/// A failed CCM call has been re-threshold-signed for the current epoch.
		FailedForeignChainCallResigned {
			broadcast_id: BroadcastId,
			threshold_signature_id: ThresholdSignatureRequestId,
		},
		/// A failed CCM has been in the system storage for more than 1 epoch.
		/// It's broadcast data has been cleaned from storage.
		FailedForeignChainCallExpired {
			broadcast_id: BroadcastId,
		},
		UtxoConsolidation {
			broadcast_id: BroadcastId,
		},
		FailedToBuildAllBatchCall {
			error: AllBatchError,
		},
		ChannelOpeningFeePaid {
			fee: T::Amount,
		},
		DepositBoosted {
			deposit_address: Option<TargetChainAccount<T, I>>,
			asset: TargetChainAsset<T, I>,
			amounts: BTreeMap<BoostPoolTier, TargetChainAmount<T, I>>,
			deposit_details: <T::TargetChain as Chain>::DepositDetails,
			prewitnessed_deposit_id: PrewitnessedDepositId,
			channel_id: Option<ChannelId>,
			block_height: TargetChainBlockNumber<T, I>,
			// Ingress fee in the deposit asset. i.e. *NOT* the gas asset, if the deposit asset is
			// a non-gas asset. The ingress fee is taken *after* the boost fee.
			ingress_fee: TargetChainAmount<T, I>,
			max_boost_fee_bps: BasisPoints,
			// Total fee the user paid for their deposit to be boosted.
			boost_fee: TargetChainAmount<T, I>,
			action: DepositAction<T, I>,
			origin_type: DepositOriginType,
		},
		InsufficientBoostLiquidity {
			prewitnessed_deposit_id: PrewitnessedDepositId,
			asset: TargetChainAsset<T, I>,
			amount_attempted: TargetChainAmount<T, I>,
			channel_id: Option<ChannelId>,
			origin_type: DepositOriginType,
		},
		BoostedDepositLost {
			prewitnessed_deposit_id: PrewitnessedDepositId,
			amount: TargetChainAmount<T, I>,
		},
		TransactionRejectionRequestReceived {
			account_id: T::AccountId,
			tx_id: TransactionInIdFor<T, I>,
			expires_at: BlockNumberFor<T>,
		},
		TransactionRejectionRequestExpired {
			account_id: T::AccountId,
			tx_id: TransactionInIdFor<T, I>,
		},
		TransactionRejectedByBroker {
			broadcast_id: BroadcastId,
			tx_id: <T::TargetChain as Chain>::DepositDetails,
		},
		TransactionRejectionFailed {
			tx_id: <T::TargetChain as Chain>::DepositDetails,
		},
		UnknownBroker {
			broker_id: T::AccountId,
		},
		UnknownAffiliate {
			broker_id: T::AccountId,
			short_affiliate_id: AffiliateShortId,
		},
		InvalidCcmRefunded {
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			destination_address: TargetChainAccount<T, I>,
		},
		PalletConfigUpdated {
			update: PalletConfigUpdate<T, I>,
		},
		ChannelRejectionRequestReceived {
			account_id: T::AccountId,
			deposit_address: TargetChainAccount<T, I>,
		},
	}

	#[derive(CloneNoBound, PartialEqNoBound, EqNoBound)]
	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The deposit address is not valid. It may have expired or may never have been issued.
		InvalidDepositAddress,
		/// A deposit was made using the wrong asset.
		AssetMismatch,
		/// Channel ID has reached maximum
		ChannelIdsExhausted,
		/// Polkadot's Vault Account does not exist in storage.
		MissingPolkadotVault,
		/// Bitcoin's Vault key does not exist for the current epoch.
		MissingBitcoinVault,
		/// Channel ID is too large for Bitcoin address derivation
		BitcoinChannelIdTooLarge,
		/// The amount is below the minimum egress amount.
		BelowEgressDustLimit,
		/// Solana address derivation error.
		SolanaAddressDerivationError,
		/// Solana's Environment variables cannot be loaded via the SolanaEnvironment.
		MissingSolanaApiEnvironment,
		/// Disabled due to safe mode for the chain
		DepositChannelCreationDisabled,
		/// CCM parameters from a vault swap failed validity check.
		InvalidCcm,
		/// Unsupported chain
		UnsupportedChain,
		/// Transaction cannot be reported after being pre-witnessed or boosted.
		TransactionAlreadyPrewitnessed,
		/// Assethub's Vault Account does not exist in storage.
		MissingAssethubVault,
		/// The Chain is deprecated, support is being phased out.
		ChainDeprecated,
		/// A channel is marked by an invalid account (not owner and not whitelisted)
		CannotMarkChannel,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// Recycle addresses if we can
		fn on_idle(now: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut used_weight = Weight::zero();

			// Approximate weight calculation: r/w DepositChannelLookup + w DepositChannelPool
			let recycle_weight_per_address =
				frame_support::weights::constants::ParityDbWeight::get().reads_writes(1, 2);

			let maximum_addresses_to_recycle = remaining_weight
				.ref_time()
				.checked_div(recycle_weight_per_address.ref_time())
				.unwrap_or_default()
				.saturated_into::<usize>();

			// In some instances, like Solana, the channel lifetime is managed by the electoral
			// system.
			if T::MANAGE_CHANNEL_LIFETIME {
				let addresses_to_recycle =
					DepositChannelRecycleBlocks::<T, I>::mutate(|recycle_queue| {
						if recycle_queue.is_empty() {
							vec![]
						} else {
							Self::take_recyclable_addresses(
								recycle_queue,
								maximum_addresses_to_recycle,
								if T::TargetChain::NAME == "Bitcoin" {
									ProcessedUpTo::<T, I>::get()
								} else {
									T::ChainTracking::get_block_height()
								},
							)
						}
					});

				// Add weight for the DepositChannelRecycleBlocks read/write plus the
				// DepositChannelLookup read/writes in the for loop below
				used_weight = used_weight.saturating_add(
					frame_support::weights::constants::ParityDbWeight::get().reads_writes(
						(addresses_to_recycle.len() + 1) as u64,
						(addresses_to_recycle.len() + 1) as u64,
					),
				);

				for address in addresses_to_recycle {
					Self::recycle_channel(&mut used_weight, address);
				}
			}

			if T::AllowTransactionReports::get() {
				// A report gets cleaned up after approx 1 hour and needs to be re-reported by the
				// broker if necessary. This is needed as some kind of garbage collection mechanism.
				for (account_id, tx_id) in ReportExpiresAt::<T, I>::take(now) {
					let _ = TransactionsMarkedForRejection::<T, I>::try_mutate(
						&account_id,
						&tx_id,
						|status| {
							match status.take() {
								Some(TransactionRejectionStatus { prewitnessed, expires_at })
									if !prewitnessed && expires_at == now =>
								{
									Self::deposit_event(
										Event::<T, I>::TransactionRejectionRequestExpired {
											account_id: account_id.clone(),
											tx_id: tx_id.clone(),
										},
									);
									Ok(())
								},
								_ => {
									// Don't apply the mutation. We expect the pre-witnessed
									// transaction to eventually be fully witnessed.
									Err(())
								},
							}
						},
					);
				}
			}

			used_weight
		}

		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			for PendingPrewitnessedDepositEntry { boost_status_lookup, deposit } in
				PendingPrewitnessedDeposits::<T, I>::take(n)
			{
				let boost_status = boost_status_lookup.resolve();
				// Sanity/invariant check: boost status should be BoostPending
				if matches!(boost_status, BoostStatus::BoostPending { process_at_block, .. } if process_at_block == n)
				{
					boost_status_lookup.set(Self::process_prewitness_deposit_inner(deposit));
				} else {
					log::warn!(
						"An attempt to process prewitness deposit with status: {:?} ({:?})",
						boost_status,
						deposit
					);
				}
			}

			for (deposit_witness, block_height) in PendingDepositChannelDeposits::<T, I>::take(n) {
				Self::process_channel_deposit_full_witness_inner(&deposit_witness, block_height)
					.unwrap_or_else(|e| {
						Self::deposit_event(Event::<T, I>::DepositFailed {
							block_height,
							reason: DepositFailedReason::DepositWitnessRejected(e),
							details: DepositFailedDetails::DepositChannel { deposit_witness },
						});
					})
			}

			for (vault_deposit_witness, block_height) in PendingVaultDeposits::<T, I>::take(n) {
				Self::process_vault_swap_request_full_witness_inner(
					block_height,
					vault_deposit_witness,
				);
			}

			Weight::zero()
		}

		/// Take all scheduled Egress and send them out
		fn on_finalize(_n: BlockNumberFor<T>) {
			// Send all fetch/transfer requests as a batch. Revert storage if failed.
			if let Err(error) = Self::do_egress_scheduled_fetch_transfer() {
				Self::deposit_event(Event::<T, I>::FailedToBuildAllBatchCall { error });
			}

			if let Ok(egress_transaction) =
				<T::ChainApiCall as ConsolidateCall<T::TargetChain>>::consolidate_utxos()
			{
				let (broadcast_id, _) =
					T::Broadcaster::threshold_sign_and_broadcast(egress_transaction);
				Self::deposit_event(Event::<T, I>::UtxoConsolidation { broadcast_id });
			};

			// Egress all scheduled Cross chain messages
			Self::do_egress_scheduled_ccm();

			// Process failed external chain calls: re-sign or cull storage.
			// Take 1 call per block to avoid weight spike.
			let current_epoch = T::EpochInfo::epoch_index();
			if let Some(call) = FailedForeignChainCalls::<T, I>::mutate_exists(
				current_epoch.saturating_sub(1),
				|calls| {
					let next_call = calls.as_mut().and_then(Vec::pop);
					if calls.as_ref().map(Vec::len).unwrap_or_default() == 0 {
						// Ensures we remove the storage if there are no more calls.
						*calls = None;
					}
					next_call
				},
			) {
				match current_epoch.saturating_sub(call.original_epoch) {
					// The call is stale, clean up storage.
					n if n >= 2 => {
						T::Broadcaster::expire_broadcast(call.broadcast_id);
						Self::deposit_event(Event::<T, I>::FailedForeignChainCallExpired {
							broadcast_id: call.broadcast_id,
						});
					},
					// Previous epoch, signature is invalid. Re-sign but don't broadcast.
					1 => match T::Broadcaster::re_sign_broadcast(call.broadcast_id, false, false) {
						Ok(threshold_signature_id) => {
							Self::deposit_event(Event::<T, I>::FailedForeignChainCallResigned {
								broadcast_id: call.broadcast_id,
								threshold_signature_id,
							});
							FailedForeignChainCalls::<T, I>::append(current_epoch, call);
						},
						Err(e) => {
							// This can happen if a broadcast is still pending
							// since the previous epoch.
							// TODO: make sure this can't happen.
							log::warn!(
								"Failed CCM call for broadcast {} not re-signed: {:?}",
								call.broadcast_id,
								e
							);
						},
					},
					// Current epoch, shouldn't be possible.
					_ => {
						log_or_panic!(
							"Logic error: Found call for current epoch in previous epoch's failed calls: broadcast_id: {}.",
							call.broadcast_id,
						);
					},
				}
			}

			if T::AllowTransactionReports::get() {
				let mut deferred_rejections = Vec::new();

				for tx in ScheduledTransactionsForRejection::<T, I>::take() {
					let Ok(refund_address) = tx.refund_address.clone().try_into() else {
						FailedRejections::<T, I>::append(tx.clone());
						continue;
					};

					match tx.deposit_address {
						Some(ref deposit_address) => {
							if !DepositChannelLookup::<T, I>::contains_key(deposit_address) {
								Self::try_broadcast_rejection_refund_or_store_tx_details(
									tx.clone(),
									refund_address,
									None,
								);
								continue;
							}

							let maybe_fetch_id =
								DepositChannelLookup::<T, I>::mutate(deposit_address, |details| {
									details.as_mut().and_then(|details| {
										let can_fetch = details.deposit_channel.state.can_fetch();

										if can_fetch {
											let fetch_id = details.deposit_channel.fetch_id();
											details.deposit_channel.state.on_fetch_scheduled();
											Some(fetch_id)
										} else {
											None
										}
									})
								});

							if let Some(fetch_id) = maybe_fetch_id {
								Self::try_broadcast_rejection_refund_or_store_tx_details(
									tx,
									refund_address,
									Some(fetch_id),
								);
							} else {
								deferred_rejections.push(tx);
							}
						},
						None => {
							Self::try_broadcast_rejection_refund_or_store_tx_details(
								tx,
								refund_address,
								None,
							);
						},
					}
				}

				ScheduledTransactionsForRejection::<T, I>::put(deferred_rejections);
			}
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Callback for when a signature is accepted by the chain.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::finalise_ingress(addresses.len() as u32))]
		pub fn finalise_ingress(
			origin: OriginFor<T>,
			addresses: Vec<TargetChainAccount<T, I>>,
		) -> DispatchResult {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;

			for deposit_address in addresses {
				DepositChannelLookup::<T, I>::mutate(deposit_address, |deposit_channel_details| {
					deposit_channel_details
						.as_mut()
						.map(|details| details.deposit_channel.state.on_fetch_completed());
				});
			}
			Ok(())
		}

		/// Sets if an asset is not allowed to be sent out of the chain via Egress.
		/// Requires Governance
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::disable_asset_egress())]
		pub fn enable_or_disable_egress(
			origin: OriginFor<T>,
			asset: TargetChainAsset<T, I>,
			set_disabled: bool,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			let is_currently_disabled = DisabledEgressAssets::<T, I>::contains_key(asset);

			let do_disable = !is_currently_disabled && set_disabled;
			let do_enable = is_currently_disabled && !set_disabled;

			if do_disable {
				DisabledEgressAssets::<T, I>::insert(asset, ());
			} else if do_enable {
				DisabledEgressAssets::<T, I>::remove(asset);
			}

			if do_disable || do_enable {
				Self::deposit_event(Event::<T, I>::AssetEgressStatusChanged {
					asset,
					disabled: set_disabled,
				});
			}

			Ok(())
		}

		/// Called when funds have been deposited into the given address.
		///
		/// Requires `EnsurePrewitnessed` or `EnsureWitnessed` origin.
		///
		/// We calculate weight assuming the most expensive code path is taken, i.e. the deposit
		/// had been boosted and is now being finalised
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::boost_finalised().saturating_mul(deposit_witnesses.len() as u64))]
		pub fn process_deposits(
			origin: OriginFor<T>,
			deposit_witnesses: Vec<DepositWitness<T::TargetChain>>,
			block_height: TargetChainBlockNumber<T, I>,
		) -> DispatchResult {
			if T::EnsurePrewitnessed::ensure_origin(origin.clone()).is_ok() {
				for deposit_witness in deposit_witnesses {
					// TODO: emit event on error?
					let _ = Self::process_channel_deposit_prewitness(deposit_witness, block_height);
				}
			} else {
				T::EnsureWitnessed::ensure_origin(origin)?;

				for deposit_witness in deposit_witnesses {
					Self::process_channel_deposit_full_witness(deposit_witness, block_height);
				}
			}
			Ok(())
		}

		/// Stores information on failed Vault transfer.
		/// Requires Witness origin.
		#[pallet::weight(T::WeightInfo::vault_transfer_failed())]
		#[pallet::call_index(4)]
		pub fn vault_transfer_failed(
			origin: OriginFor<T>,
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			destination_address: TargetChainAccount<T, I>,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let current_epoch = T::EpochInfo::epoch_index();
			match <T::ChainApiCall as TransferFallback<T::TargetChain>>::new_unsigned(
				TransferAssetParams { asset, amount, to: destination_address.clone() },
			) {
				Ok(api_call) => {
					let (broadcast_id, _) = T::Broadcaster::threshold_sign(api_call);
					FailedForeignChainCalls::<T, I>::append(
						current_epoch,
						FailedForeignChainCall { broadcast_id, original_epoch: current_epoch },
					);
					Self::deposit_event(Event::<T, I>::TransferFallbackRequested {
						asset,
						amount,
						destination_address,
						broadcast_id,
						egress_details: None,
					});
				},
				// The only way this can fail is if the target chain is unsupported, which should
				// never happen.
				Err(err) => {
					log_or_panic!(
						"Failed to construct TransferFallback call. Asset: {:?}, amount: {:?}, Destination: {:?}, Error: {:?}",
						asset, amount, destination_address, err
					);
				},
			};
			Ok(())
		}

		/// Callback for when CCMs failed to be broadcasted. We will resign the call
		/// so the user can broadcast the CCM themselves.
		/// Requires Root origin.
		#[pallet::weight(T::WeightInfo::ccm_broadcast_failed())]
		#[pallet::call_index(5)]
		pub fn ccm_broadcast_failed(
			origin: OriginFor<T>,
			broadcast_id: BroadcastId,
		) -> DispatchResult {
			ensure_root(origin)?;

			let current_epoch = T::EpochInfo::epoch_index();

			// Stores the broadcast ID, so the user can use it to query for
			// information such as Threshold Signature etc.
			FailedForeignChainCalls::<T, I>::append(
				current_epoch,
				FailedForeignChainCall { broadcast_id, original_epoch: current_epoch },
			);

			Self::deposit_event(Event::<T, I>::CcmBroadcastFailed { broadcast_id });
			Ok(())
		}

		/// Apply a list of configuration updates to the pallet.
		///
		/// Requires Governance.
		#[pallet::call_index(6)]
		#[pallet::weight(<T as frame_system::Config>::SystemWeightInfo::set_storage(updates.len() as u32))]
		pub fn update_pallet_config(
			origin: OriginFor<T>,
			updates: BoundedVec<PalletConfigUpdate<T, I>, ConstU32<10>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			for update in updates {
				match update {
					PalletConfigUpdate::<T, I>::ChannelOpeningFee { fee } => {
						let fee = fee.unique_saturated_into();
						ChannelOpeningFee::<T, I>::set(fee);
					},
					PalletConfigUpdate::<T, I>::SetMinimumDeposit { asset, minimum_deposit } => {
						MinimumDeposit::<T, I>::insert(asset, minimum_deposit);
					},
					PalletConfigUpdate::<T, I>::SetDepositChannelLifetime { lifetime } => {
						DepositChannelLifetime::<T, I>::set(lifetime);
					},
					PalletConfigUpdate::SetWitnessSafetyMargin { margin } => {
						WitnessSafetyMargin::<T, I>::set(Some(margin));
					},
					PalletConfigUpdate::SetBoostDelay { delay_blocks } => {
						BoostDelayBlocks::<T, I>::set(delay_blocks);
					},
					PalletConfigUpdate::SetIngressDelay { delay_blocks } => {
						IngressDelayBlocks::<T, I>::set(delay_blocks);
					},
					PalletConfigUpdate::SetMaximumPreallocatedChannels {
						account_role,
						num_channels,
					} => {
						MaximumPreallocatedChannels::<T, I>::set(account_role, num_channels);
					},
				}
				Self::deposit_event(Event::<T, I>::PalletConfigUpdated { update });
			}

			Ok(())
		}

		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::mark_transaction_for_rejection())]
		pub fn mark_transaction_for_rejection(
			origin: OriginFor<T>,
			tx_id: TransactionInIdFor<T, I>,
		) -> DispatchResult {
			let account_id = T::AccountRoleRegistry::ensure_broker(origin)?;
			ensure!(T::AllowTransactionReports::get(), Error::<T, I>::UnsupportedChain);
			Self::mark_transaction_for_rejection_inner(account_id, tx_id)?;
			Ok(())
		}

		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::vault_swap_request())]
		pub fn vault_swap_request(
			origin: OriginFor<T>,
			block_height: TargetChainBlockNumber<T, I>,
			deposit: Box<VaultDepositWitness<T, I>>,
		) -> DispatchResult {
			if T::EnsureWitnessed::ensure_origin(origin.clone()).is_ok() {
				Self::process_vault_swap_request_full_witness(block_height, *deposit);
			} else {
				T::EnsurePrewitnessed::ensure_origin(origin)?;

				Self::process_vault_swap_request_prewitness(block_height, *deposit);
			}

			Ok(())
		}

		#[pallet::call_index(14)]
		#[pallet::weight(T::WeightInfo::mark_transaction_for_rejection())]
		pub fn mark_deposit_channel_for_rejection(
			origin: OriginFor<T>,
			deposit_address: TargetChainAccount<T, I>,
		) -> DispatchResult {
			let account_id = T::AccountRoleRegistry::ensure_broker(origin)?;
			ensure!(T::AllowTransactionReports::get(), Error::<T, I>::UnsupportedChain);
			Self::mark_deposit_channel_for_rejection_inner(account_id, deposit_address)?;
			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> IngressSink for Pallet<T, I> {
	type Account = TargetChainAccount<T, I>;
	type Asset = <T::TargetChain as Chain>::ChainAsset;
	type Amount = <T::TargetChain as Chain>::ChainAmount;
	type BlockNumber = <T::TargetChain as Chain>::ChainBlockNumber;
	type DepositDetails = <T::TargetChain as Chain>::DepositDetails;

	fn on_ingress(
		channel: Self::Account,
		asset: Self::Asset,
		amount: Self::Amount,
		block_number: Self::BlockNumber,
		deposit_details: Self::DepositDetails,
	) {
		Self::process_channel_deposit_full_witness(
			DepositWitness { deposit_address: channel, asset, amount, deposit_details },
			block_number,
		);
	}

	fn on_channel_closed(channel: Self::Account) {
		Self::recycle_channel(&mut Weight::zero(), channel);
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn mark_transaction_for_rejection_inner(
		account_id: T::AccountId,
		tx_id: TransactionInIdFor<T, I>,
	) -> DispatchResult {
		let lookup_id = {
			if WhitelistedBrokers::<T, I>::contains_key(&account_id) {
				T::ScreeningBrokerId::get()
			} else {
				account_id.clone()
			}
		};
		let expires_at = <frame_system::Pallet<T>>::block_number()
			.saturating_add(BlockNumberFor::<T>::from(MARKED_TX_EXPIRATION_BLOCKS));
		TransactionsMarkedForRejection::<T, I>::try_mutate(&lookup_id, &tx_id, |opt| {
			ensure!(
				!opt.replace(TransactionRejectionStatus { prewitnessed: false, expires_at })
					.map(|s| s.prewitnessed)
					.unwrap_or_default(),
				Error::<T, I>::TransactionAlreadyPrewitnessed
			);
			Ok::<_, DispatchError>(())
		})?;
		ReportExpiresAt::<T, I>::append(expires_at, (&lookup_id, &tx_id));
		Self::deposit_event(Event::<T, I>::TransactionRejectionRequestReceived {
			account_id,
			tx_id,
			expires_at,
		});
		Ok(())
	}
	fn mark_deposit_channel_for_rejection_inner(
		account_id: T::AccountId,
		deposit_address: TargetChainAccount<T, I>,
	) -> DispatchResult {
		DepositChannelLookup::<T, I>::try_mutate(&deposit_address, |channel| {
			if let Some(channel) = channel {
				ensure!(
					channel.owner == account_id ||
						WhitelistedBrokers::<T, I>::contains_key(&account_id),
					Error::<T, I>::CannotMarkChannel
				);
				ensure!(
					!matches!(channel.boost_status, BoostStatus::Boosted { .. }),
					Error::<T, I>::TransactionAlreadyPrewitnessed
				);

				channel.is_marked_for_rejection = true;
				Ok::<_, DispatchError>(())
			} else {
				Err(Error::<T, I>::InvalidDepositAddress.into())
			}
		})?;

		Self::deposit_event(Event::<T, I>::ChannelRejectionRequestReceived {
			account_id,
			deposit_address,
		});
		Ok(())
	}

	fn recycle_channel(used_weight: &mut Weight, address: TargetChainAccount<T, I>) {
		if let Some(DepositChannelDetails { deposit_channel, boost_status, .. }) =
			DepositChannelLookup::<T, I>::take(address)
		{
			if let Some(state) = deposit_channel.state.maybe_recycle() {
				DepositChannelPool::<T, I>::insert(
					deposit_channel.channel_id,
					DepositChannel { state, ..deposit_channel },
				);
				*used_weight = used_weight.saturating_add(
					frame_support::weights::constants::ParityDbWeight::get().reads_writes(0, 1),
				);
			}

			if let BoostStatus::Boosted { prewitnessed_deposit_id, amount } = boost_status {
				T::BoostApi::process_deposit_as_lost(
					prewitnessed_deposit_id,
					deposit_channel.asset.into(),
				);

				Self::deposit_event(Event::<T, I>::BoostedDepositLost {
					prewitnessed_deposit_id,
					amount,
				})
			}
		}
	}

	// This function is only doing the maybe_transfers_limit and not the maybe_fetches_limit checks that
	// the `do_egress_scheduled_*` are doing.
	fn try_broadcast_rejection_refund_or_store_tx_details(
		tx: TransactionRejectionDetails<T, I>,
		refund_address: TargetChainAccount<T, I>,
		deposit_fetch_id: Option<<T::TargetChain as Chain>::DepositFetchId>,
	) {
		if let Some(limit) = T::FetchesTransfersLimitProvider::maybe_transfers_limit() {
			// In case we don't have enough nonces we put the tx back to be retried next block
			if limit.is_zero() {
				ScheduledTransactionsForRejection::<T, I>::append(tx);
				return;
			}
		}
		let AmountAndFeesWithheld {
			amount_after_fees: amount_after_ingress_fees,
			fees_withheld: _,
		} = Self::withhold_ingress_or_egress_fee(
			if deposit_fetch_id.is_some() {
				IngressOrEgress::IngressDepositChannel
			} else {
				IngressOrEgress::IngressVaultSwap
			},
			tx.asset,
			tx.amount,
		);
		let AmountAndFeesWithheld { amount_after_fees: amount_to_refund, fees_withheld: _ } =
			Self::withhold_ingress_or_egress_fee(
				match tx.refund_ccm_metadata {
					Some(ref refund_ccm) => IngressOrEgress::EgressCcm {
						gas_budget: refund_ccm.channel_metadata.gas_budget,
						message_length: refund_ccm.channel_metadata.message.len(),
					},
					None => IngressOrEgress::Egress,
				},
				tx.asset,
				amount_after_ingress_fees,
			);

		// By building the ccm call, we will know if a separate "fetch" is required as part of the
		// refund.
		let no_ccm_refund = tx.refund_ccm_metadata.is_none();
		match <T::ChainApiCall as RejectCall<T::TargetChain>>::new_unsigned(
			tx.deposit_details.clone(),
			refund_address.clone(),
			no_ccm_refund.then_some(amount_to_refund),
			tx.asset,
			deposit_fetch_id,
		) {
			Ok(api_call) => {
				let broadcast_id = T::Broadcaster::threshold_sign_and_broadcast_with_callback(
					api_call,
					tx.deposit_address.clone().map(|deposit_address| {
						Call::finalise_ingress { addresses: vec![deposit_address] }.into()
					}),
					|_| None,
				);
				// Only emit TransactionRejectedByBroker for the actual egress (refund) transaction
				if no_ccm_refund {
					Self::deposit_event(Event::<T, I>::TransactionRejectedByBroker {
						broadcast_id,
						tx_id: tx.deposit_details.clone(),
					});
				}
			},
			Err(cf_chains::RejectError::NotRequired) => (),
			Err(_) =>
			// Only consider it failed if we are not doing a ccm refund
				if no_ccm_refund {
					FailedRejections::<T, I>::append(tx.clone());
					Self::deposit_event(Event::<T, I>::TransactionRejectionFailed {
						tx_id: tx.deposit_details.clone(),
					});
				},
		};

		if let Some(ref ccm_refund_metadata) = tx.refund_ccm_metadata {
			let handle_ccm_failure = || {
				FailedRejections::<T, I>::append(tx.clone());
				Self::deposit_event(Event::<T, I>::TransactionRejectionFailed {
					tx_id: tx.deposit_details.clone(),
				});
			};

			// Solana CCM refunds with ALT is not supported, since we don't want to witness ALTs
			// here.
			if ccm_refund_metadata
				.channel_metadata
				.ccm_additional_data
				.alt_addresses()
				.map(|alts| !alts.is_empty())
				.unwrap_or(false)
			{
				debug_assert!(false, "Solana refund CCM with ALTs is not supported");
				handle_ccm_failure();
				return;
			}

			if let Ok(api_call) =
				<T::ChainApiCall as ExecutexSwapAndCall<T::TargetChain>>::new_unsigned(
					TransferAssetParams {
						asset: tx.asset,
						amount: amount_to_refund,
						to: refund_address,
					},
					ccm_refund_metadata.source_chain,
					ccm_refund_metadata.source_address.clone(),
					ccm_refund_metadata.channel_metadata.gas_budget,
					ccm_refund_metadata.channel_metadata.message.to_vec(),
					ccm_refund_metadata.channel_metadata.ccm_additional_data.clone(),
				) {
				let broadcast_id = T::Broadcaster::threshold_sign_and_broadcast_with_callback(
					api_call,
					tx.deposit_address.clone().map(|deposit_address| {
						Call::finalise_ingress { addresses: vec![deposit_address] }.into()
					}),
					|_| None,
				);
				Self::deposit_event(Event::<T, I>::TransactionRejectedByBroker {
					broadcast_id,
					tx_id: tx.deposit_details.clone(),
				});
			} else {
				// NOTE: This could be appending a rejection that might have an already successful
				// fetch (RejectCall) but not the CCM refund. We must ensure that
				// FailedRejections are not being retried.
				handle_ccm_failure();
			}
		}
	}

	fn take_recyclable_addresses(
		channel_recycle_blocks: &mut ChannelRecycleQueue<T, I>,
		maximum_addresses_to_take: usize,
		current_block_height: TargetChainBlockNumber<T, I>,
	) -> Vec<TargetChainAccount<T, I>> {
		channel_recycle_blocks.sort_by_key(|(block, _)| *block);
		let partition_point = sp_std::cmp::min(
			channel_recycle_blocks.partition_point(|(block, _)| *block <= current_block_height),
			maximum_addresses_to_take,
		);
		channel_recycle_blocks
			.drain(..partition_point)
			.map(|(_, address)| address)
			.collect()
	}

	fn should_fetch_or_transfer(
		maybe_no_of_fetch_or_transfers_remaining: &mut Option<usize>,
	) -> bool {
		maybe_no_of_fetch_or_transfers_remaining
			.as_mut()
			.map(|no_of_fetch_or_transfers_remaining| {
				if *no_of_fetch_or_transfers_remaining != 0 {
					*no_of_fetch_or_transfers_remaining -= 1;
					true
				} else {
					false
				}
			})
			.unwrap_or(true)
	}

	/// Take all scheduled egress requests and send them out in an `AllBatch` call.
	///
	/// Note: Egress transactions with Blacklisted assets are not sent, and kept in storage.
	#[transactional]
	fn do_egress_scheduled_fetch_transfer() -> Result<(), AllBatchError> {
		let batch_to_send: Vec<_> =
			ScheduledEgressFetchOrTransfer::<T, I>::mutate(|requests: &mut Vec<_>| {
				let mut maybe_no_of_transfers_remaining =
					T::FetchesTransfersLimitProvider::maybe_transfers_limit();
				let mut maybe_no_of_fetches_remaining =
					T::FetchesTransfersLimitProvider::maybe_fetches_limit();
				// Filter out disabled assets and requests that are not ready to be egressed.
				requests
					.extract_if(.., |request| {
						!DisabledEgressAssets::<T, I>::contains_key(request.asset()) &&
							match request {
								FetchOrTransfer::Fetch {
									deposit_address,
									deposit_fetch_id,
									..
								} => {
									// Either:
									// 1. We always want to fetch
									// 2. We have a restriction on fetches, in which case we need to
									//    have fetches remaining.
									// And we must be able to fetch the channel (it must exist and
									// can_fetch must be true)
									if (maybe_no_of_fetches_remaining.is_none_or(|n| n > 0)) &&
										DepositChannelLookup::<T, I>::mutate(
											deposit_address,
											|details| {
												details
													.as_mut()
													.map(|details| {
														let can_fetch = details
															.deposit_channel
															.state
															.can_fetch();

														if can_fetch {
															deposit_fetch_id.replace(
																details.deposit_channel.fetch_id(),
															);
															details
																.deposit_channel
																.state
																.on_fetch_scheduled();
														}
														can_fetch
													})
													.unwrap_or(false)
											},
										) {
										if let Some(n) = maybe_no_of_fetches_remaining.as_mut() {
											*n = n.saturating_sub(1);
										}
										true
									} else {
										// If we have a restriction on fetches, but we have no fetch
										// slots remaining then we don't want to fetch any
										// more. OR:
										// If the channel is expired / `can_fetch` returns
										// false then we can't/shouldn't fetch.
										false
									}
								},
								FetchOrTransfer::Transfer { .. } => Self::should_fetch_or_transfer(
									&mut maybe_no_of_transfers_remaining,
								),
							}
					})
					.collect()
			});

		if batch_to_send.is_empty() {
			return Ok(())
		}

		let mut fetch_params: Vec<FetchAssetParams<<T as Config<I>>::TargetChain>> = vec![];
		let mut transfer_params = vec![];
		let mut addresses = vec![];

		for request in batch_to_send {
			match request {
				FetchOrTransfer::<T::TargetChain>::Fetch {
					asset,
					deposit_address,
					deposit_fetch_id,
					amount: _amount,
				} => {
					fetch_params.push(FetchAssetParams {
						deposit_fetch_id: deposit_fetch_id.expect("Checked in extract_if"),
						asset,
					});
					addresses.push(deposit_address.clone());
				},
				FetchOrTransfer::<T::TargetChain>::Transfer {
					asset,
					amount,
					destination_address,
					egress_id,
				} => {
					transfer_params.push((
						TransferAssetParams { asset, amount, to: destination_address },
						egress_id,
					));
				},
			}
		}

		// Construct and send the transaction.
		match <T::ChainApiCall as AllBatch<T::TargetChain>>::new_unsigned(
			fetch_params,
			transfer_params,
		) {
			Ok(egress_transactions) => {
				egress_transactions.into_iter().for_each(|(egress_transaction, egress_ids)| {
					let broadcast_id = T::Broadcaster::threshold_sign_and_broadcast_with_callback(
						egress_transaction,
						Some(Call::finalise_ingress { addresses: addresses.clone() }.into()),
						|_| None,
					);
					Self::deposit_event(Event::<T, I>::BatchBroadcastRequested {
						broadcast_id,
						egress_ids,
					});
				});
				Ok(())
			},
			Err(AllBatchError::NotRequired) => Ok(()),
			Err(other) => Err(other),
		}
	}

	/// Send all scheduled Cross Chain Messages out to the target chain.
	///
	/// Blacklisted assets are not sent and will remain in storage.
	fn do_egress_scheduled_ccm() {
		let mut maybe_no_of_transfers_remaining =
			T::FetchesTransfersLimitProvider::maybe_ccm_limit();

		let ccms_to_send: Vec<CrossChainMessage<T::TargetChain>> =
			ScheduledEgressCcm::<T, I>::mutate(|ccms: &mut Vec<_>| {
				// Filter out disabled assets, and take up to batch_size requests to be sent.
				ccms.extract_if(.., |ccm| {
					!DisabledEgressAssets::<T, I>::contains_key(ccm.asset()) &&
						Self::should_fetch_or_transfer(&mut maybe_no_of_transfers_remaining)
				})
				.collect()
			});
		for ccm in ccms_to_send {
			match <T::ChainApiCall as ExecutexSwapAndCall<T::TargetChain>>::new_unsigned(
				TransferAssetParams {
					asset: ccm.asset,
					amount: ccm.amount,
					to: ccm.destination_address.clone(),
				},
				ccm.source_chain,
				ccm.source_address.clone(),
				ccm.gas_budget,
				ccm.message.clone().to_vec(),
				ccm.ccm_additional_data.clone(),
			) {
				Ok(api_call) => {
					let broadcast_id = T::Broadcaster::threshold_sign_and_broadcast_with_callback(
						api_call,
						None,
						|broadcast_id| Some(Call::ccm_broadcast_failed { broadcast_id }.into()),
					);
					Self::deposit_event(Event::<T, I>::CcmBroadcastRequested {
						broadcast_id,
						egress_id: ccm.egress_id,
					});
				},
				Err(ExecutexSwapAndCallError::AuxDataNotReady) =>
					ScheduledEgressCcm::<T, I>::append(ccm),
				Err(error) => {
					Self::deposit_event(Event::<T, I>::CcmEgressInvalid {
						egress_id: ccm.egress_id,
						error,
					});

					if let Some(fallback_address) =
						ccm.ccm_additional_data.refund_address::<T::TargetChain>()
					{
						match Self::schedule_egress(
							ccm.asset,
							ccm.amount,
							fallback_address.clone(),
							None,
						) {
							Ok(egress_details) =>
								Self::deposit_event(Event::<T, I>::InvalidCcmRefunded {
									asset: ccm.asset,
									amount: egress_details.egress_amount,
									destination_address: fallback_address,
								}),
							Err(e) => log::warn!(
								"Cannot refund failed Ccm: failed to Egress. Error: {:?}",
								e
							),
						};
					}
				},
			};
		}
	}

	pub fn process_channel_deposit_prewitness(
		DepositWitness { deposit_address, asset, amount, deposit_details }: DepositWitness<
			T::TargetChain,
		>,
		block_height: TargetChainBlockNumber<T, I>,
	) -> DispatchResult {
		let DepositChannelDetails {
			deposit_channel, action, boost_fee, boost_status, owner, ..
		} = DepositChannelLookup::<T, I>::get(&deposit_address)
			.ok_or(Error::<T, I>::InvalidDepositAddress)?;

		let boost_status_lookup =
			BoostStatusLookup::Channel { deposit_address: deposit_address.clone() };
		if boost_status == BoostStatus::NotBoosted {
			let deposit = PendingPrewitnessedDeposit {
				block_height,
				amount,
				asset,
				deposit_details,
				deposit_address: Some(deposit_address.clone()),
				action: action.map(
					|account_id| account_id,
					|target_chain_account| target_chain_account,
					|channel_metadata| CcmDepositMetadataChecked {
						channel_metadata,
						source_chain: asset.into(),
						// For deposit channels we don't witness the source address
						source_address: None,
					},
				),
				boost_fee,
				channel_id: Some(deposit_channel.channel_id),
				origin: DepositOrigin::deposit_channel(
					deposit_address.clone(),
					deposit_channel.channel_id,
					block_height,
					owner.clone(),
				),
			};

			let delay = BoostDelayBlocks::<T, I>::get();

			let new_boost_status = if delay > Default::default() {
				let process_at_block = frame_system::Pallet::<T>::block_number() + delay;
				PendingPrewitnessedDeposits::<T, I>::append(
					process_at_block,
					PendingPrewitnessedDepositEntry {
						boost_status_lookup: boost_status_lookup.clone(),
						deposit,
					},
				);
				BoostStatus::BoostPending { amount, process_at_block }
			} else {
				Self::process_prewitness_deposit_inner(deposit)
			};

			if new_boost_status != boost_status {
				boost_status_lookup.set(new_boost_status);
			}
		}

		Ok(())
	}

	fn perform_channel_action(
		action: ChannelActionForDeposit<T::AccountId, TargetChainAccount<T, I>>,
		asset: TargetChainAsset<T, I>,
		amount_after_fees: TargetChainAmount<T, I>,
		origin: DepositOrigin<T, I>,
	) -> DepositAction<T, I> {
		match action.clone() {
			ChannelAction::LiquidityProvision { lp_account, .. } => {
				T::Balance::credit_account(&lp_account, asset.into(), amount_after_fees.into());
				DepositAction::LiquidityProvision { lp_account }
			},
			ChannelAction::Swap {
				destination_asset,
				destination_address,
				broker_fees,
				channel_metadata,
				refund_params,
				dca_params,
			} => {
				let swap_request_id = T::SwapRequestHandler::init_swap_request(
					asset.into(),
					amount_after_fees.into(),
					destination_asset,
					SwapRequestType::Regular {
						output_action: SwapOutputAction::Egress {
							ccm_deposit_metadata: channel_metadata,
							output_address: destination_address,
						},
					},
					broker_fees,
					Some(
						refund_params
							.map(AccountOrAddress::ExternalAddress, |refund_details| refund_details)
							.into(),
					),
					dca_params.clone(),
					origin.into(),
				);
				DepositAction::Swap { swap_request_id }
			},
			ChannelAction::Refund { refund_address, reason, refund_ccm_metadata } => {
				let egress_id = match Self::schedule_egress(
					asset,
					amount_after_fees,
					refund_address,
					refund_ccm_metadata,
				) {
					Ok(egress_details) => Some(egress_details.egress_id),
					Err(e) => {
						log::warn!("Failed to schedule egress for vault swap refund: {:?}", e);
						None
					},
				};
				DepositAction::Refund { egress_id, amount: amount_after_fees, reason }
			},
			ChannelAction::Unrefundable => DepositAction::Unrefundable,
		}
	}

	// A wrapper around `process_channel_deposit_full_witness_inner` that catches any
	// error and emits a rejection event
	pub fn process_channel_deposit_full_witness(
		deposit_witness: DepositWitness<T::TargetChain>,
		block_height: TargetChainBlockNumber<T, I>,
	) {
		let delay = IngressDelayBlocks::<T, I>::get();
		if delay > Default::default() {
			let process_at_block = frame_system::Pallet::<T>::block_number() + delay;
			PendingDepositChannelDeposits::<T, I>::append(
				process_at_block,
				(deposit_witness, block_height),
			);
		} else {
			Self::process_channel_deposit_full_witness_inner(&deposit_witness, block_height)
				.unwrap_or_else(|e| {
					Self::deposit_event(Event::<T, I>::DepositFailed {
						block_height,
						reason: DepositFailedReason::DepositWitnessRejected(e),
						details: DepositFailedDetails::DepositChannel { deposit_witness },
					});
				})
		}
	}

	/// Completes a single deposit request.
	#[transactional]
	fn process_channel_deposit_full_witness_inner(
		DepositWitness { deposit_address, asset, amount, deposit_details }: &DepositWitness<
			T::TargetChain,
		>,
		block_height: TargetChainBlockNumber<T, I>,
	) -> DispatchResult {
		let deposit_channel_details = DepositChannelLookup::<T, I>::get(deposit_address)
			.ok_or(Error::<T, I>::InvalidDepositAddress)?;

		ensure!(
			deposit_channel_details.deposit_channel.asset == *asset,
			Error::<T, I>::AssetMismatch
		);

		let channel_id = deposit_channel_details.deposit_channel.channel_id;

		if DepositChannelPool::<T, I>::get(channel_id).is_some() {
			log_or_panic!(
				"Deposit channel {} should not be in the recycled address pool if it's active",
				channel_id
			);
			#[cfg(not(debug_assertions))]
			return Err(Error::<T, I>::InvalidDepositAddress.into())
		}

		let deposit_origin = DepositOrigin::deposit_channel(
			deposit_address.clone(),
			channel_id,
			block_height,
			deposit_channel_details.owner.clone(),
		);

		match Self::process_full_witness_deposit_inner(
			Some(deposit_address.clone()),
			*asset,
			*amount,
			deposit_details.clone(),
			deposit_channel_details.boost_status,
			deposit_channel_details.boost_fee,
			Some(channel_id),
			deposit_channel_details.action.map(
				|account_id| account_id,
				|target_chain_account| target_chain_account,
				|channel_metadata| CcmDepositMetadataChecked {
					channel_metadata,
					source_chain: (*asset).into(),
					// For deposit channels we don't witness the source address
					source_address: None,
				},
			),
			block_height,
			deposit_origin,
		) {
			Ok(outcome) => {
				if matches!(outcome, FullWitnessDepositOutcome::BoostConsumed) {
					DepositChannelLookup::<T, I>::mutate(deposit_address, |details| {
						if let Some(details) = details {
							// Edge case: if a deposit is finalised while boost is still pending,
							// we cancel the pending boost.
							match details.boost_status {
								BoostStatus::BoostPending {
									process_at_block,
									amount: pending_amount,
								} if pending_amount == *amount => {
									Self::cancel_pending_prewitnessed_deposit(
										process_at_block,
										&BoostStatusLookup::Channel {
											deposit_address: deposit_address.clone(),
										},
									);
								},
								_ => {},
							}

							// This allows the channel to be boosted again:
							details.boost_status = BoostStatus::NotBoosted;
						}
					});
				}
			},
			Err(reason) => {
				Self::deposit_event(Event::<T, I>::DepositFailed {
					block_height,
					reason,
					details: DepositFailedDetails::DepositChannel {
						deposit_witness: DepositWitness {
							deposit_address: deposit_address.clone(),
							asset: *asset,
							amount: *amount,
							deposit_details: deposit_details.clone(),
						},
					},
				});
			},
		};

		Ok(())
	}

	// Look up the minimum broker fee that has been set by the broker and increase the given broker
	// fee to it if needed.
	fn enforce_broker_fee_minimum(
		broker_fee: Beneficiary<T::AccountId>,
	) -> Beneficiary<T::AccountId> {
		Beneficiary {
			bps: core::cmp::max(
				broker_fee.bps,
				T::SwapParameterValidation::get_minimum_vault_swap_fee_for_broker(
					&broker_fee.account,
				),
			),
			account: broker_fee.account,
		}
	}

	/// Returns the broker fee and affiliate fees for a vault swap.
	///
	/// If the broker fee is not set or the account is not a broker, the function returns None.
	fn assemble_broker_fees(
		broker_fee: Option<Beneficiary<T::AccountId>>,
		affiliate_fees: Affiliates<AffiliateShortId>,
	) -> Option<Beneficiaries<T::AccountId>> {
		broker_fee
			.filter(|Beneficiary { account, .. }| {
				if T::AccountRoleRegistry::has_account_role(account, AccountRole::Broker) {
					true
				} else {
					Self::deposit_event(Event::<T, I>::UnknownBroker {
						broker_id: account.clone(),
					});
					false
				}
			})
			.map(Self::enforce_broker_fee_minimum)
			.map(|primary_broker_fee| {
				let primary_broker = primary_broker_fee.account.clone();
				core::iter::once(primary_broker_fee.clone())
				.chain(affiliate_fees.into_iter().filter_map(
					|Beneficiary { account: short_affiliate_id, bps }| {
						if let Some(affiliate_id) = T::AffiliateRegistry::get_account_id(
							&primary_broker,
							short_affiliate_id,
						) {
							Some(Beneficiary { account: affiliate_id, bps })
						} else {
							// In case the entry not found, we ignore the entry, but process the
							// swap (to avoid having to refund it).
							Self::deposit_event(Event::<T, I>::UnknownAffiliate {
								broker_id: primary_broker.clone(),
								short_affiliate_id,
							});

							None
						}
					},
				))
				.collect::<Vec<_>>()
				.try_into()
				.expect(
					"must fit since affiliates are limited to 1 fewer element than beneficiaries",
				)
			})
	}

	fn process_prewitness_deposit_inner(
		PendingPrewitnessedDeposit {
			amount,
			asset,
			deposit_details,
			deposit_address,
			action,
			boost_fee,
			channel_id,
			block_height,
			origin,
		}: PendingPrewitnessedDeposit<T, I>,
	) -> BoostStatus<TargetChainAmount<T, I>, BlockNumberFor<T>> {
		if amount < MinimumDeposit::<T, I>::get(asset) {
			// We do not process/record pre-witnessed deposits for amounts smaller
			// than MinimumDeposit to match how this is done on finalisation
			return BoostStatus::NotBoosted;
		}

		if T::AllowTransactionReports::get() {
			if let (Some(tx_ids), Some(broker_id)) =
				(deposit_details.deposit_ids(), origin.broker_id())
			{
				let any_reported = [&T::ScreeningBrokerId::get(), broker_id]
					.into_iter()
					.flat_map(|account_id| {
						tx_ids.clone().into_iter().map(move |tx_id| {
							TransactionsMarkedForRejection::<T, I>::mutate(
								account_id,
								tx_id,
								|opt| {
									match opt.as_mut() {
										// Transaction has been reported, mark it as
										// pre-witnessed.
										Some(TransactionRejectionStatus {
											prewitnessed, ..
										}) => {
											*prewitnessed = true;
											true
										},
										// Transaction has not been reported.
										None => false,
									}
								},
							)
						})
					})
					// Collect to ensure all are processed before continuing.
					.collect::<Vec<_>>();
				if any_reported.contains(&true) {
					return BoostStatus::NotBoosted;
				}
				if deposit_address.as_ref().is_some_and(|deposit_addr| {
					DepositChannelLookup::<T, I>::get(deposit_addr)
						.is_some_and(|deposit_channel| deposit_channel.is_marked_for_rejection)
				}) {
					return BoostStatus::NotBoosted;
				}
			}
		}

		let prewitnessed_deposit_id =
			PrewitnessedDepositIdCounter::<T, I>::mutate(|id| -> PrewitnessedDepositId {
				*id = PrewitnessedDepositId(id.0.saturating_add(1));
				*id
			});

		// Only boost on non-zero fee and if the channel isn't already boosted:
		if T::SafeMode::get().boost_deposits_enabled && boost_fee > 0 {
			match T::BoostApi::try_boosting(
				prewitnessed_deposit_id,
				asset.into(),
				amount.into(),
				boost_fee,
			) {
				Ok(BoostOutcome { used_pools, total_fee }) => {
					let boost_fee_amount = total_fee.unique_saturated_into();
					let amount_after_boost_fee = amount.saturating_sub(boost_fee_amount);

					// Note that ingress fee is deducted at the time of boosting rather than the
					// time the deposit is finalised (which allows us to perform the channel
					// action immediately):
					let AmountAndFeesWithheld { amount_after_fees, fees_withheld: ingress_fee } =
						Self::conditionally_withhold_ingress_fee(
							asset,
							amount_after_boost_fee,
							&origin,
						);

					let used_pools = used_pools
						.into_iter()
						.map(|(fee, amount)| (fee, amount.unique_saturated_into()))
						.collect();

					let action = Self::perform_channel_action(
						action,
						asset,
						amount_after_fees,
						origin.clone(),
					);

					Self::deposit_event(Event::DepositBoosted {
						deposit_address,
						asset,
						amounts: used_pools,
						block_height,
						prewitnessed_deposit_id,
						channel_id,
						deposit_details,
						ingress_fee,
						max_boost_fee_bps: boost_fee,
						boost_fee: boost_fee_amount,
						action,
						origin_type: origin.into(),
					});

					BoostStatus::Boosted { prewitnessed_deposit_id, amount }
				},
				Err(_) => {
					Self::deposit_event(Event::InsufficientBoostLiquidity {
						prewitnessed_deposit_id,
						asset,
						amount_attempted: amount,
						channel_id,
						origin_type: origin.into(),
					});
					BoostStatus::NotBoosted
				},
			}
		} else {
			BoostStatus::NotBoosted
		}
	}

	pub fn process_vault_swap_request_prewitness(
		block_height: TargetChainBlockNumber<T, I>,
		vault_deposit_witness: VaultDepositWitness<T, I>,
	) {
		let VaultDepositWitness {
			input_asset: asset,
			deposit_address,
			channel_id,
			deposit_amount: amount,
			deposit_details,
			tx_id,
			broker_fee,
			boost_fee,
			..
		} = vault_deposit_witness.clone();

		let origin = DepositOrigin::vault(
			tx_id.clone(),
			broker_fee.as_ref().map(|Beneficiary { account, .. }| account.clone()),
		);

		match Self::derive_channel_action_from_vault_deposit_witness(vault_deposit_witness.clone())
		{
			action @ ChannelAction::Swap { .. } => {
				let boost_status_lookup = BoostStatusLookup::Vault { tx_id: tx_id.clone() };
				let boost_status = boost_status_lookup.resolve();
				if boost_status == BoostStatus::NotBoosted {
					let deposit = PendingPrewitnessedDeposit {
						block_height,
						amount,
						asset,
						deposit_details,
						deposit_address,
						action,
						boost_fee,
						channel_id,
						origin,
					};

					let delay = BoostDelayBlocks::<T, I>::get();

					let new_boost_status = if delay > 0u32.into() {
						let process_at_block = frame_system::Pallet::<T>::block_number() + delay;

						PendingPrewitnessedDeposits::<T, I>::append(
							process_at_block,
							PendingPrewitnessedDepositEntry {
								boost_status_lookup: boost_status_lookup.clone(),
								deposit,
							},
						);
						BoostStatus::BoostPending { amount, process_at_block }
					} else {
						// Process immediately
						Self::process_prewitness_deposit_inner(deposit)
					};

					if boost_status != new_boost_status {
						boost_status_lookup.set(new_boost_status);
					}
				}
			},
			_ => {
				// we don't process refunds on prewitness
			},
		}
	}

	fn process_full_witness_deposit_inner(
		deposit_address: Option<TargetChainAccount<T, I>>,
		asset: TargetChainAsset<T, I>,
		deposit_amount: TargetChainAmount<T, I>,
		deposit_details: <T::TargetChain as Chain>::DepositDetails,
		boost_status: BoostStatus<TargetChainAmount<T, I>, BlockNumberFor<T>>,
		max_boost_fee_bps: BasisPoints,
		channel_id: Option<u64>,
		action: ChannelActionForDeposit<T::AccountId, TargetChainAccount<T, I>>,
		block_height: TargetChainBlockNumber<T, I>,
		origin: DepositOrigin<T, I>,
	) -> Result<FullWitnessDepositOutcome, DepositFailedReason> {
		// Deposits can only fail or be rejected if we haven't already boosted.
		if !matches!(boost_status, BoostStatus::Boosted { .. }) {
			if deposit_amount < MinimumDeposit::<T, I>::get(asset) {
				// If the deposit amount is below the minimum allowed, the deposit is ignored.
				// TODO: track these funds somewhere, for example add them to the withheld fees.
				return Err(DepositFailedReason::BelowMinimumDeposit);
			}
			if T::AllowTransactionReports::get() {
				let is_marked_by_broker_or_screening_id = {
					let transaction_marked = if let (Some(tx_ids), Some(broker_id)) =
						(deposit_details.deposit_ids(), origin.broker_id())
					{
						!tx_ids
							.iter()
							.filter_map(|tx_id| {
								// The transaction may have been marked by a whitelisted broker
								// (screening_id) or, by the channel owner if the owner is not
								// whitelisted.
								let screening_id = T::ScreeningBrokerId::get();
								match (
									TransactionsMarkedForRejection::<T, I>::take(
										&screening_id,
										tx_id,
									),
									TransactionsMarkedForRejection::<T, I>::take(broker_id, tx_id),
								) {
									(None, None) => None,
									_ => Some(()),
								}
							})
							// Collect to ensure that the iterator is fully consumed.
							.collect::<Vec<_>>()
							.is_empty()
					} else {
						false
					};

					let channel_marked = if let Some(ref deposit_address) = deposit_address {
						DepositChannelLookup::<T, I>::get(deposit_address)
							.is_some_and(|deposit_channel| deposit_channel.is_marked_for_rejection)
					} else {
						false
					};

					transaction_marked || channel_marked
				};

				if is_marked_by_broker_or_screening_id {
					let (refund_address, refund_ccm_metadata) = match &action {
						ChannelAction::Swap { refund_params, .. } => (
							refund_params.refund_address.clone(),
							refund_params.refund_ccm_metadata.clone(),
						),
						ChannelAction::LiquidityProvision { refund_address, .. } =>
							(refund_address.clone(), None),
						ChannelAction::Refund { refund_address, refund_ccm_metadata, .. } => (
							refund_address.clone().into_foreign_chain_address(),
							refund_ccm_metadata.clone(),
						),
						ChannelAction::Unrefundable =>
							return Err(DepositFailedReason::Unrefundable),
					};

					ScheduledTransactionsForRejection::<T, I>::append(
						TransactionRejectionDetails {
							deposit_address: deposit_address.clone(),
							refund_address,
							amount: deposit_amount,
							asset,
							deposit_details: deposit_details.clone(),
							refund_ccm_metadata,
						},
					);

					return Err(DepositFailedReason::TransactionRejectedByBroker);
				}
			}
		}

		match &origin {
			DepositOrigin::DepositChannel { deposit_address, channel_id, .. } => {
				ScheduledEgressFetchOrTransfer::<T, I>::append(
					FetchOrTransfer::<T::TargetChain>::Fetch {
						asset,
						deposit_address: deposit_address.clone(),
						deposit_fetch_id: None,
						amount: deposit_amount,
					},
				);
				Self::deposit_event(Event::<T, I>::DepositFetchesScheduled {
					channel_id: *channel_id,
					asset,
				});
			},
			DepositOrigin::Vault { .. } => {
				// Vault deposits don't need to be fetched
			},
		}

		// Add the deposit to the balance.
		T::DepositHandler::on_deposit_made(deposit_details.clone());

		enum ActionToPerform {
			FinaliseBoost { prewitnessed_deposit_id: PrewitnessedDepositId },
			PerformChannelAction { deposit_outcome: FullWitnessDepositOutcome },
		}

		// We received a deposit on a channel. If channel has been boosted earlier
		// (i.e. awaiting finalisation), *and* the boosted amount matches the amount
		// in this deposit, finalise the boost by crediting boost pools with the deposit.
		// Process as non-boosted deposit otherwise:
		let action_to_perform = match boost_status {
			// If there is a boosted amount that matches the deposit amount, we can finalise the
			// boost:
			BoostStatus::Boosted { prewitnessed_deposit_id, amount }
				if amount == deposit_amount =>
				ActionToPerform::FinaliseBoost { prewitnessed_deposit_id },
			// If there is a pending amount matching the deposit amount, we can cancel the pending
			// boost:
			BoostStatus::BoostPending { amount, .. } if amount == deposit_amount =>
				ActionToPerform::PerformChannelAction {
					deposit_outcome: FullWitnessDepositOutcome::BoostConsumed,
				},
			// Anything else we consider as a non-boosted deposit:
			_ => ActionToPerform::PerformChannelAction {
				deposit_outcome: FullWitnessDepositOutcome::BoostNotConsumed,
			},
		};

		match action_to_perform {
			ActionToPerform::FinaliseBoost { prewitnessed_deposit_id } => {
				let network_fee_from_boost =
					T::BoostApi::finalise_boost(prewitnessed_deposit_id, asset.into()).network_fee;

				let network_fee_swap_request_id = if network_fee_from_boost > 0u32.into() {
					// NOTE: if asset is FLIP, we shouldn't need to swap, but it should still work,
					// and it seems easiest to not write a special case (esp if we only support
					// boost for BTC)
					Some(T::SwapRequestHandler::init_network_fee_swap_request(
						asset.into(),
						network_fee_from_boost,
					))
				} else {
					None
				};

				Self::deposit_event(Event::DepositFinalised {
					deposit_address,
					asset,
					amount: deposit_amount,
					block_height,
					deposit_details,
					// no ingress fee as it was already charged at the time of boosting
					ingress_fee: 0u32.into(),
					max_boost_fee_bps,
					action: DepositAction::BoostersCredited {
						prewitnessed_deposit_id,
						network_fee_from_boost: network_fee_from_boost.unique_saturated_into(),
						network_fee_swap_request_id,
					},
					channel_id,
					origin_type: origin.into(),
				});

				Ok(FullWitnessDepositOutcome::BoostConsumed)
			},
			ActionToPerform::PerformChannelAction { deposit_outcome } => {
				let AmountAndFeesWithheld { amount_after_fees, fees_withheld } =
					Self::conditionally_withhold_ingress_fee(asset, deposit_amount, &origin);

				if amount_after_fees.is_zero() {
					Err(DepositFailedReason::NotEnoughToPayFees)
				} else {
					// Processing as a non-boosted deposit:
					let action = Self::perform_channel_action(
						action,
						asset,
						amount_after_fees,
						origin.clone(),
					);

					Self::deposit_event(Event::DepositFinalised {
						deposit_address,
						asset,
						amount: deposit_amount,
						block_height,
						deposit_details,
						ingress_fee: fees_withheld,
						max_boost_fee_bps,
						action,
						channel_id,
						origin_type: origin.into(),
					});

					Ok(deposit_outcome)
				}
			},
		}
	}

	fn derive_channel_action_from_vault_deposit_witness(
		vault_deposit_witness: VaultDepositWitness<T, I>,
	) -> ChannelActionForDeposit<T::AccountId, TargetChainAccount<T, I>> {
		let VaultDepositWitness {
			input_asset: source_asset,
			deposit_address,
			output_asset: destination_asset,
			destination_address,
			deposit_metadata,
			broker_fee,
			affiliate_fees,
			refund_params,
			dca_params,
			..
		} = vault_deposit_witness.clone();

		// ------ refund parameters -----

		let refund_address = refund_params.refund_address.clone();
		let Ok(checked_refund_params) =
			refund_params.map_refund_address_to_foreign_chain_address().into_checked(
				deposit_address.clone().map(|addr| addr.into_foreign_chain_address()),
				source_asset.into(),
			)
		else {
			return ChannelAction::Unrefundable;
		};

		let refund_action = |reason| ChannelAction::Refund {
			reason,
			refund_address,
			refund_ccm_metadata: checked_refund_params.refund_ccm_metadata.clone(),
		};

		// ------ fees -----
		let Some(broker_fees) = Self::assemble_broker_fees(broker_fee, affiliate_fees.clone())
		else {
			return refund_action(RefundReason::InvalidBrokerFees);
		};

		if T::SwapParameterValidation::validate_broker_fees(&broker_fees).is_err() {
			return refund_action(RefundReason::InvalidBrokerFees);
		}

		// ------ egress ccm metadata -----
		let destination_address_internal =
			match T::AddressConverter::decode_and_validate_address_for_asset(
				destination_address.clone(),
				destination_asset,
			) {
				Ok(address) => address,
				Err(_) => {
					return refund_action(RefundReason::InvalidDestinationAddress);
				},
			};

		let channel_metadata = if let Some(metadata) = deposit_metadata.clone() {
			let destination_chain: ForeignChain = (destination_asset).into();
			if !destination_chain.ccm_support() {
				return refund_action(RefundReason::CcmUnsupportedForTargetChain);
			}

			match metadata.to_checked(destination_asset, destination_address_internal.clone()) {
				Ok(decoded) => Some(CcmDepositMetadataChecked {
					channel_metadata: decoded.channel_metadata,
					source_chain: source_asset.into(),
					source_address: decoded.source_address.clone(),
				}),
				Err(_) => return refund_action(RefundReason::CcmInvalidMetadata),
			}
		} else {
			None
		};

		if T::SwapParameterValidation::validate_refund_params(
			source_asset.into(),
			destination_asset,
			checked_refund_params.retry_duration,
			checked_refund_params.max_oracle_price_slippage,
		)
		.is_err()
		{
			return refund_action(RefundReason::InvalidRefundParameters)
		}

		if let Some(params) = &dca_params {
			if T::SwapParameterValidation::validate_dca_params(params).is_err() {
				return refund_action(RefundReason::InvalidDcaParameters);
			}
		}

		ChannelAction::Swap {
			destination_asset,
			destination_address: destination_address_internal,
			broker_fees,
			channel_metadata,
			refund_params: checked_refund_params,
			dca_params,
		}
	}

	pub fn process_vault_swap_request_full_witness(
		block_height: TargetChainBlockNumber<T, I>,
		vault_deposit_witness: VaultDepositWitness<T, I>,
	) {
		let delay = IngressDelayBlocks::<T, I>::get();
		if delay > Default::default() {
			let process_at_block = frame_system::Pallet::<T>::block_number() + delay;
			PendingVaultDeposits::<T, I>::append(
				process_at_block,
				(vault_deposit_witness, block_height),
			);
		} else {
			Self::process_vault_swap_request_full_witness_inner(block_height, vault_deposit_witness)
		}
	}

	pub fn process_vault_swap_request_full_witness_inner(
		block_height: TargetChainBlockNumber<T, I>,
		vault_deposit_witness: VaultDepositWitness<T, I>,
	) {
		let VaultDepositWitness {
			input_asset: source_asset,
			deposit_address,
			channel_id,
			deposit_amount,
			deposit_details,
			tx_id,
			broker_fee,
			boost_fee,
			..
		} = vault_deposit_witness.clone();

		let action =
			Self::derive_channel_action_from_vault_deposit_witness(vault_deposit_witness.clone());

		match Self::process_full_witness_deposit_inner(
			deposit_address.clone(),
			source_asset,
			deposit_amount,
			deposit_details.clone(),
			BoostedVaultTransactions::<T, I>::get(&tx_id),
			boost_fee,
			channel_id,
			action,
			block_height,
			DepositOrigin::vault(
				tx_id.clone(),
				broker_fee.as_ref().map(|Beneficiary { account, .. }| account.clone()),
			),
		) {
			Ok(outcome) =>
				if matches!(outcome, FullWitnessDepositOutcome::BoostConsumed) {
					// Clean up any state associated with prewitnessing of this deposit.
					match BoostedVaultTransactions::<T, I>::take(&tx_id) {
						BoostStatus::BoostPending { amount, process_at_block }
							if amount == deposit_amount =>
						{
							Self::cancel_pending_prewitnessed_deposit(
								process_at_block,
								&BoostStatusLookup::Vault { tx_id: tx_id.clone() },
							);
						},
						_ => {},
					}

					BoostedVaultTransactions::<T, I>::take(&tx_id);
				},
			Err(reason) => {
				Self::deposit_event(Event::<T, I>::DepositFailed {
					block_height,
					reason,
					details: DepositFailedDetails::Vault {
						vault_witness: Box::new(vault_deposit_witness),
					},
				});
			},
		}
	}

	fn expiry_and_recycle_block_height(
	) -> (TargetChainBlockNumber<T, I>, TargetChainBlockNumber<T, I>, TargetChainBlockNumber<T, I>)
	{
		// Goals:
		// 1. When chain tracking reaches a particular block number, we want to be able to process
		//   that block immediately on the CFE.
		// 2. The CFE's need to have a consistent view of what we want to witness (channels) in
		//    order to
		//   come to consensus.

		// We open deposit channels for the block after the current chain tracking block, so that
		// the set of channels open at the *current chain tracking* block does not change after
		// chain tracking reaches that block. This achieves the second goal. We achieve the first
		// goal by using this in conjunction with waiting until the chain tracking reaches a
		// particular block before we process it on the CFE.

		// This relates directly to the code in
		// `engine/src/witness/common/chunked_chain_source/chunked_by_vault/deposit_addresses.rs`
		// and `engine/src/witness/common/chunked_chain_source/chunked_by_vault/monitored_items.rs`
		let current_height =
			T::ChainTracking::get_block_height() + <T::TargetChain as Chain>::WITNESS_PERIOD;
		debug_assert!(<T::TargetChain as Chain>::is_block_witness_root(current_height));

		let lifetime = DepositChannelLifetime::<T, I>::get();

		let expiry_height = <T::TargetChain as Chain>::saturating_block_witness_next(
			current_height.saturating_add(lifetime),
		);
		let recycle_height = <T::TargetChain as Chain>::saturating_block_witness_next(
			expiry_height.saturating_add(lifetime),
		);

		debug_assert!(current_height < expiry_height);
		debug_assert!(expiry_height < recycle_height);

		(current_height, expiry_height, recycle_height)
	}

	/// Generates a new deposit channel for the given asset
	#[allow(clippy::type_complexity)]
	fn generate_new_channel(
		source_asset: TargetChainAsset<T, I>,
	) -> Result<DepositChannel<T::TargetChain>, DispatchError> {
		Ok(DepositChannel::generate_new::<T::AddressDerivation>(
			Self::allocate_next_channel_id()?,
			source_asset,
		)
		.map_err(|e| match e {
			AddressDerivationError::MissingPolkadotVault => Error::<T, I>::MissingPolkadotVault,
			AddressDerivationError::MissingBitcoinVault => Error::<T, I>::MissingBitcoinVault,
			AddressDerivationError::BitcoinChannelIdTooLarge =>
				Error::<T, I>::BitcoinChannelIdTooLarge,
			AddressDerivationError::SolanaDerivationError { .. } =>
				Error::<T, I>::SolanaAddressDerivationError,
			AddressDerivationError::MissingSolanaApiEnvironment =>
				Error::<T, I>::MissingSolanaApiEnvironment,
			AddressDerivationError::MissingAssethubVault => Error::<T, I>::MissingAssethubVault,
		})?)
	}

	/// Opens a channel for the given asset and registers it with the given action.
	///
	/// May re-use an existing channel, depending on pallet configuration. The channel is added to
	/// the preallocated channels list up to the maximum number of channels allowed for the
	/// requester.
	///
	/// The requester must have enough FLIP available to pay the channel opening fee.
	#[allow(clippy::type_complexity)]
	fn open_channel(
		requester: &T::AccountId,
		source_asset: TargetChainAsset<T, I>,
		action: ChannelAction<T::AccountId, TargetChainAccount<T, I>>,
		boost_fee: BasisPoints,
	) -> Result<
		(DepositChannel<T::TargetChain>, TargetChainBlockNumber<T, I>, T::Amount),
		DispatchError,
	> {
		ensure!(!T::TargetChain::DEPRECATED, Error::<T, I>::ChainDeprecated);
		ensure!(
			T::SafeMode::get().deposit_channel_creation_enabled,
			Error::<T, I>::DepositChannelCreationDisabled
		);

		let channel_opening_fee = ChannelOpeningFee::<T, I>::get();
		T::FeePayment::try_burn_fee(requester, channel_opening_fee)?;
		Self::deposit_event(Event::<T, I>::ChannelOpeningFeePaid { fee: channel_opening_fee });

		let deposit_channel = PreallocatedChannels::<T, I>::mutate(requester, |queue| {
			// Always fill up the list to one above capacity, then pop the first channel.
			for _ in queue.len()..=
				MaximumPreallocatedChannels::<T, I>::get(T::AccountRoleRegistry::account_role(
					requester,
				)) as usize
			{
				if let Some((_id, channel)) = DepositChannelPool::<T, I>::drain().next() {
					queue.push_back(channel);
				} else if T::ONLY_PREALLOCATE_FROM_POOL {
					break;
				} else {
					queue.push_back(Self::generate_new_channel(source_asset)?);
				};
			}
			let mut deposit_channel = if let Some(deposit_channel) = queue.pop_front() {
				deposit_channel
			} else {
				// If the queue is empty, generate a new channel.
				Self::generate_new_channel(source_asset)?
			};
			// Make sure to set the asset, in case the channel was allocated from the pre-allocated
			// channels list or from the deposit channels pool.
			deposit_channel.asset = source_asset;
			Ok::<_, DispatchError>(deposit_channel)
		})?;

		let (current_height, expiry_height, recycle_height) =
			Self::expiry_and_recycle_block_height();

		if T::MANAGE_CHANNEL_LIFETIME {
			DepositChannelRecycleBlocks::<T, I>::append((
				recycle_height,
				deposit_channel.address.clone(),
			));
		}

		DepositChannelLookup::<T, I>::insert(
			&deposit_channel.address,
			DepositChannelDetails {
				owner: requester.clone(),
				deposit_channel: deposit_channel.clone(),
				opened_at: current_height,
				expires_at: expiry_height,
				action,
				boost_fee,
				boost_status: BoostStatus::NotBoosted,
				is_marked_for_rejection: false,
			},
		);
		<T::IngressSource as IngressSource>::open_channel(
			deposit_channel.address.clone(),
			deposit_channel.asset,
			expiry_height,
			<frame_system::Pallet<T>>::block_number(),
		)?;

		Ok((deposit_channel, expiry_height, channel_opening_fee))
	}

	pub fn get_failed_call(broadcast_id: BroadcastId) -> Option<FailedForeignChainCall> {
		let epoch = T::EpochInfo::epoch_index();
		FailedForeignChainCalls::<T, I>::get(epoch)
			.iter()
			.find(|ccm| ccm.broadcast_id == broadcast_id)
			.cloned()
	}

	// Withholds ingress fee, but only after checking the origin
	fn conditionally_withhold_ingress_fee(
		asset: TargetChainAsset<T, I>,
		available_amount: TargetChainAmount<T, I>,
		origin: &DepositOrigin<T, I>,
	) -> AmountAndFeesWithheld<TargetChainAmount<T, I>> {
		match origin {
			DepositOrigin::DepositChannel { .. } => Self::withhold_ingress_or_egress_fee(
				IngressOrEgress::IngressDepositChannel,
				asset,
				available_amount,
			),
			DepositOrigin::Vault { .. } => Self::withhold_ingress_or_egress_fee(
				IngressOrEgress::IngressVaultSwap,
				asset,
				available_amount,
			),
		}
	}

	/// Withholds the fee for a given amount.
	///
	/// Returns the remaining amount after the fee has been withheld, and the fee itself, both
	/// measured in units of the input asset. A swap may be scheduled to convert the fee into the
	/// gas asset.
	#[allow(clippy::redundant_pattern_matching)]
	pub fn withhold_ingress_or_egress_fee(
		ingress_or_egress: IngressOrEgress,
		asset: TargetChainAsset<T, I>,
		available_amount: TargetChainAmount<T, I>,
	) -> AmountAndFeesWithheld<TargetChainAmount<T, I>> {
		let fee_estimate = T::ChainTracking::estimate_fee(asset, ingress_or_egress);

		let fees_withheld = if asset == <T::TargetChain as Chain>::GAS_ASSET {
			// No need to schedule a swap for gas, it's already in the gas asset.
			Self::accrue_withheld_fee(asset, sp_std::cmp::min(fee_estimate, available_amount));
			fee_estimate
		} else {
			let transaction_fee = sp_std::cmp::min(
				T::AssetConverter::calculate_input_for_gas_output::<T::TargetChain>(
					asset,
					fee_estimate,
				),
				available_amount,
			);

			if !transaction_fee.is_zero() {
				T::SwapRequestHandler::init_ingress_egress_fee_swap_request::<T::TargetChain>(
					asset,
					transaction_fee,
				);
			}

			transaction_fee
		};

		AmountAndFeesWithheld {
			amount_after_fees: available_amount.saturating_sub(fees_withheld),
			fees_withheld,
		}
	}

	/// If a Ccm failed, we want to refund the user their assets.
	/// This function will schedule a transfer to the fallback address, and emit an event on
	/// success. IMPORTANT: Currently only used for Solana.
	pub fn do_ccm_fallback(
		broadcast_id: BroadcastId,
		fallback: TransferAssetParams<T::TargetChain>,
	) {
		match Self::schedule_egress(
			fallback.asset,
			fallback.amount,
			fallback.to.clone(),
			None,
		) {
			Ok(egress_details) => Self::deposit_event(Event::<T, I>::TransferFallbackRequested {
				asset: fallback.asset,
				amount: fallback.amount,
				destination_address: fallback.to,
				broadcast_id,
				egress_details: Some(egress_details),
			}),
			Err(e) => log::error!("Ccm fallback failed to schedule the fallback egress: Target chain: {:?}, broadcast_id: {:?}, error: {:?}", T::TargetChain::get(), broadcast_id, e),
		}
	}

	fn allocate_next_channel_id() -> Result<ChannelId, Error<T, I>> {
		ChannelIdCounter::<T, I>::try_mutate::<_, Error<T, I>, _>(|id| {
			*id = id.checked_add(1).ok_or(Error::<T, I>::ChannelIdsExhausted)?;
			Ok(*id)
		})
	}

	fn cancel_pending_prewitnessed_deposit(
		process_at_block: BlockNumberFor<T>,
		boost_status_lookup: &BoostStatusLookup<T, I>,
	) {
		let _ = PendingPrewitnessedDeposits::<T, I>::try_mutate_exists(
			process_at_block,
			|pending_deposits| {
				let (extracted, remaining) = pending_deposits
					.as_mut()
					.map(|deposits| {
						let extracted = deposits
							.extract_if(.., |deposit| {
								&deposit.boost_status_lookup == boost_status_lookup
							})
							.count();
						let remaining = deposits.len();
						(extracted, remaining)
					})
					.unwrap_or((0, 0));
				if remaining == 0 {
					// If none are remaining, delete the storage.
					Ok(None)
				} else if extracted > 0 {
					// If anything was removed, mutate the storage.
					Ok(Some(()))
				} else {
					// Otherwise, we don't need to write anything.
					Err(())
				}
			},
		);
	}

	// TODO: Write test

	// This should only be used if we're using ProcessedUpTo to track the block height.
	pub fn active_deposit_channels_at(
		opened_at_or_before: TargetChainBlockNumber<T, I>,
		expires_after: TargetChainBlockNumber<T, I>,
	) -> Vec<DepositChannelDetails<T, I>> {
		debug_assert!(<T::TargetChain as Chain>::is_block_witness_root(opened_at_or_before));

		DepositChannelLookup::<T, I>::iter_values()
			.filter(|details| {
				details.opened_at <= opened_at_or_before && details.expires_at >= expires_after
			})
			.collect()
	}
}

impl<T: Config<I>, I: 'static> EgressApi<T::TargetChain> for Pallet<T, I> {
	type EgressError = Error<T, I>;

	fn schedule_egress(
		asset: TargetChainAsset<T, I>,
		amount: TargetChainAmount<T, I>,
		destination_address: TargetChainAccount<T, I>,
		maybe_ccm_deposit_metadata: Option<CcmDepositMetadataChecked<ForeignChainAddress>>,
	) -> Result<ScheduledEgressDetails<T::TargetChain>, Error<T, I>> {
		EgressIdCounter::<T, I>::try_mutate(|id_counter| {
			*id_counter = id_counter.saturating_add(1);
			let egress_id = (<T as Config<I>>::TargetChain::get(), *id_counter);

			match maybe_ccm_deposit_metadata {
				Some(CcmDepositMetadata {
					channel_metadata:
						CcmChannelMetadataChecked { message, gas_budget, ccm_additional_data, .. },
					source_chain,
					source_address,
					..
				}) => {
					let AmountAndFeesWithheld { amount_after_fees, fees_withheld } =
						Self::withhold_ingress_or_egress_fee(
							IngressOrEgress::EgressCcm {
								gas_budget,
								message_length: message.len(),
							},
							asset,
							amount,
						);

					// Handle any action required from the CCM additional data.
					T::CcmAdditionalDataHandler::handle_ccm_additional_data(
						ccm_additional_data.clone(),
					);

					ScheduledEgressCcm::<T, I>::append(CrossChainMessage {
						egress_id,
						asset,
						amount: amount_after_fees,
						destination_address: destination_address.clone(),
						message,
						ccm_additional_data,
						source_chain,
						source_address,
						gas_budget,
					});

					Ok(ScheduledEgressDetails::new(*id_counter, amount_after_fees, fees_withheld))
				},
				None => {
					let AmountAndFeesWithheld { amount_after_fees, fees_withheld } =
						Self::withhold_ingress_or_egress_fee(
							IngressOrEgress::Egress,
							asset,
							amount,
						);

					if amount_after_fees >=
						EgressDustLimit::<T, I>::get(asset).unique_saturated_into() ||
						// We always want to benchmark the success case.
						cfg!(all(feature = "runtime-benchmarks", not(test)))
					{
						let egress_details = ScheduledEgressDetails::new(
							*id_counter,
							amount_after_fees,
							fees_withheld,
						);

						ScheduledEgressFetchOrTransfer::<T, I>::append({
							FetchOrTransfer::<T::TargetChain>::Transfer {
								asset,
								destination_address: destination_address.clone(),
								amount: amount_after_fees,
								egress_id: egress_details.egress_id,
							}
						});

						Ok(egress_details)
					} else {
						// TODO: Consider tracking the ignored egresses somewhere.
						// For example, store the egress and try it again later when fees have
						// dropped?
						Err(Error::<T, I>::BelowEgressDustLimit)
					}
				},
			}
		})
	}
}

impl<T: Config<I>, I: 'static> ChannelIdAllocator for Pallet<T, I> {
	fn allocate_private_channel_id() -> Result<ChannelId, DispatchError> {
		Ok(Self::allocate_next_channel_id()?)
	}
}

impl<T: Config<I>, I: 'static> DepositApi<T::TargetChain> for Pallet<T, I> {
	type AccountId = T::AccountId;
	type Amount = T::Amount;

	// This should be callable by the LP pallet.
	fn request_liquidity_deposit_address(
		lp_account: T::AccountId,
		source_asset: TargetChainAsset<T, I>,
		boost_fee: BasisPoints,
		refund_address: ForeignChainAddress,
	) -> Result<
		(ChannelId, ForeignChainAddress, <T::TargetChain as Chain>::ChainBlockNumber, Self::Amount),
		DispatchError,
	> {
		let (deposit_channel, expiry_block, channel_opening_fee) = Self::open_channel(
			&lp_account,
			source_asset,
			ChannelAction::LiquidityProvision { lp_account: lp_account.clone(), refund_address },
			boost_fee,
		)?;

		Ok((
			deposit_channel.channel_id,
			deposit_channel.address.into_foreign_chain_address(),
			expiry_block,
			channel_opening_fee,
		))
	}

	// This should only be callable by the broker.
	fn request_swap_deposit_address(
		source_asset: TargetChainAsset<T, I>,
		destination_asset: Asset,
		destination_address: ForeignChainAddress,
		broker_fees: Beneficiaries<Self::AccountId>,
		broker_id: T::AccountId,
		channel_metadata: Option<CcmChannelMetadataChecked>,
		boost_fee: BasisPoints,
		refund_params: ChannelRefundParametersForChain<T::TargetChain>,
		dca_params: Option<DcaParameters>,
	) -> Result<
		(ChannelId, ForeignChainAddress, <T::TargetChain as Chain>::ChainBlockNumber, Self::Amount),
		DispatchError,
	> {
		T::SwapParameterValidation::validate_refund_params(
			source_asset.into(),
			destination_asset,
			refund_params.retry_duration,
			refund_params.max_oracle_price_slippage,
		)?;

		if let Some(params) = &dca_params {
			T::SwapParameterValidation::validate_dca_params(params)?;
		}

		let (deposit_channel, expiry_height, channel_opening_fee) = Self::open_channel(
			&broker_id,
			source_asset,
			ChannelAction::Swap {
				destination_asset,
				destination_address,
				broker_fees,
				channel_metadata,
				refund_params: refund_params
					.map_refund_address_to_foreign_chain_address()
					.into_checked(None, source_asset.into())?
					// we map the CcmDepositMetadata to a CcmChannelMetadata, forgetting the other
					// fields:
					.map(
						|address| address,
						|maybe_deposit_metadata| {
							maybe_deposit_metadata.map(|metadata| metadata.channel_metadata)
						},
					),
				dca_params,
			},
			boost_fee,
		)?;

		Ok((
			deposit_channel.channel_id,
			deposit_channel.address.into_foreign_chain_address(),
			expiry_height,
			channel_opening_fee,
		))
	}
}

impl<T: Config<I>, I: 'static> IngressEgressFeeApi<T::TargetChain> for Pallet<T, I> {
	fn accrue_withheld_fee(
		_asset: <T::TargetChain as Chain>::ChainAsset,
		fee: TargetChainAmount<T, I>,
	) {
		if !fee.is_zero() {
			T::AssetWithholding::withhold_assets(
				<T::TargetChain as Chain>::GAS_ASSET.into(),
				fee.into(),
			);
		}
	}
}
