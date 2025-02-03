use frame_support::traits::UncheckedOnRuntimeUpgrade;

use cf_chains::{CcmAdditionalData, CcmMessage};

use crate::Config;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use super::*;
	use cf_chains::{ChannelRefundParametersDecoded, ForeignChainAddress};
	use cf_primitives::{Asset, AssetAmount, Beneficiaries, SwapId};
	use frame_support::Twox64Concat;

	const MAX_CCM_MSG_LENGTH: u32 = 10_000;
	const MAX_CCM_CF_PARAM_LENGTH: u32 = 1_000;

	type CcmMessage = BoundedVec<u8, ConstU32<MAX_CCM_MSG_LENGTH>>;
	type CcmCfParameters = BoundedVec<u8, ConstU32<MAX_CCM_CF_PARAM_LENGTH>>;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
	pub struct CcmChannelMetadata {
		pub message: CcmMessage,
		pub gas_budget: AssetAmount,
		pub cf_parameters: CcmCfParameters,
	}

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
	pub struct CcmDepositMetadataGeneric<Address> {
		pub channel_metadata: CcmChannelMetadata,
		pub source_chain: ForeignChain,
		pub source_address: Option<Address>,
	}

	pub type CcmDepositMetadata = CcmDepositMetadataGeneric<ForeignChainAddress>;

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub enum GasSwapState {
		OutputReady { gas_budget: AssetAmount },
		Scheduled { gas_swap_id: SwapId },
		ToBeScheduled { gas_budget: AssetAmount, other_gas_asset: Asset },
	}

	#[allow(clippy::large_enum_variant)]
	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct CcmState {
		pub gas_swap_state: GasSwapState,
		pub ccm_deposit_metadata: CcmDepositMetadata,
	}

	#[allow(clippy::large_enum_variant)]
	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub enum SwapRequestState<T: Config> {
		UserSwap {
			ccm: Option<CcmState>,
			output_address: ForeignChainAddress,
			dca_state: DcaState,
			broker_fees: Beneficiaries<T::AccountId>,
		},
		NetworkFee,
		IngressEgressFee,
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct SwapRequest<T: Config> {
		pub id: SwapRequestId,
		pub input_asset: Asset,
		pub output_asset: Asset,
		pub refund_params: Option<ChannelRefundParametersDecoded>,
		pub state: SwapRequestState<T>,
	}

	#[frame_support::storage_alias]
	pub type SwapRequests<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, SwapRequestId, SwapRequest<T>>;

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub enum FeeType<T: Config> {
		NetworkFee,
		BrokerFee(Beneficiaries<T::AccountId>),
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct Swap<T: Config> {
		pub swap_id: SwapId,
		pub swap_request_id: SwapRequestId,
		pub from: Asset,
		pub to: Asset,
		pub input_amount: AssetAmount,
		pub fees: Vec<FeeType<T>>,
		pub refund_params: Option<SwapRefundParameters>,
	}

	#[frame_support::storage_alias]
	pub type SwapQueue<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, BlockNumberFor<T>, Vec<Swap<T>>, ValueQuery>;
}

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let swap_request_count = old::SwapRequests::<T>::iter().count() as u64;
		let scheduled_swaps_count: u64 =
			old::SwapQueue::<T>::iter().map(|(_, swaps)| swaps.len() as u64).sum();
		Ok((swap_request_count, scheduled_swaps_count).encode())
	}

	fn on_runtime_upgrade() -> Weight {
		crate::SwapRequests::<T>::translate_values::<old::SwapRequest<T>, _>(|old_swap_requests| {
			Some(SwapRequest {
				id: old_swap_requests.id,
				input_asset: old_swap_requests.input_asset,
				output_asset: old_swap_requests.output_asset,
				refund_params: old_swap_requests.refund_params,
				state: match old_swap_requests.state {
					old::SwapRequestState::UserSwap {
						ccm,
						output_address,
						dca_state,
						broker_fees,
					} => SwapRequestState::UserSwap {
						ccm_deposit_metadata: ccm.map(|old_ccm_state| CcmDepositMetadata {
							channel_metadata: CcmChannelMetadata {
								message: CcmMessage::try_from(
									old_ccm_state
										.ccm_deposit_metadata
										.channel_metadata
										.message
										.into_inner(),
								)
								.unwrap_or_default(),
								gas_budget: old_ccm_state
									.ccm_deposit_metadata
									.channel_metadata
									.gas_budget,
								ccm_additional_data: CcmAdditionalData::try_from(
									old_ccm_state
										.ccm_deposit_metadata
										.channel_metadata
										.cf_parameters
										.into_inner(),
								)
								.unwrap_or_default(),
							},
							source_chain: old_ccm_state.ccm_deposit_metadata.source_chain,
							source_address: old_ccm_state.ccm_deposit_metadata.source_address,
						}),
						output_address,
						dca_state,
						broker_fees,
					},
					old::SwapRequestState::NetworkFee => SwapRequestState::NetworkFee,
					old::SwapRequestState::IngressEgressFee => SwapRequestState::IngressEgressFee,
				},
			})
		});

		crate::SwapQueue::<T>::translate_values::<Vec<old::Swap<T>>, _>(|old_swaps| {
			Some(
				old_swaps
					.into_iter()
					.map(|swap| Swap {
						swap_id: swap.swap_id,
						swap_request_id: swap.swap_request_id,
						from: swap.from,
						to: swap.to,
						input_amount: swap.input_amount,
						fees: swap
							.fees
							.into_iter()
							.map(|fee| match fee {
								old::FeeType::NetworkFee =>
									FeeType::NetworkFee { min_fee_enforced: false },
								old::FeeType::BrokerFee(beneficiaries) =>
									FeeType::BrokerFee(beneficiaries),
							})
							.collect(),
						refund_params: swap.refund_params,
					})
					.collect(),
			)
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let (pre_swap_request_count, pre_scheduled_swap_count) =
			<(u64, u64)>::decode(&mut state.as_slice())
				.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_swap_request_count = crate::SwapRequests::<T>::iter().count() as u64;
		let post_scheduled_swaps_count: u64 =
			SwapQueue::<T>::iter().map(|(_, swaps)| swaps.len() as u64).sum();

		assert_eq!(pre_swap_request_count, post_swap_request_count);
		assert_eq!(pre_scheduled_swap_count, post_scheduled_swaps_count);
		Ok(())
	}
}
