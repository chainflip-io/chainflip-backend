use cf_chains::ChannelRefundParameters;
use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::Config;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use super::*;
	use cf_chains::{CcmDepositMetadata, ChannelRefundParametersDecoded, ForeignChainAddress};
	use cf_primitives::{Asset, Beneficiaries};
	use frame_support::Twox64Concat;

	#[allow(clippy::large_enum_variant)]
	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub enum SwapRequestState<T: Config> {
		UserSwap {
			ccm_deposit_metadata: Option<CcmDepositMetadata>,
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
		// This is to be moved into swap request state
		pub refund_params: Option<ChannelRefundParametersDecoded>,
		pub state: SwapRequestState<T>,
	}

	#[frame_support::storage_alias]
	pub type SwapRequests<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, SwapRequestId, SwapRequest<T>>;
}

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let swap_request_count = old::SwapRequests::<T>::iter().count() as u64;
		Ok(swap_request_count.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		crate::SwapRequests::<T>::translate_values::<old::SwapRequest<T>, _>(|old_swap_request| {
			Some(SwapRequest {
				id: old_swap_request.id,
				input_asset: old_swap_request.input_asset,
				output_asset: old_swap_request.output_asset,
				state: match old_swap_request.state {
					old::SwapRequestState::UserSwap {
						ccm_deposit_metadata,
						output_address,
						dca_state,
						broker_fees,
					} => SwapRequestState::UserSwap {
						refund_params: old_swap_request.refund_params.map(
							|ChannelRefundParameters {
							     retry_duration,
							     refund_address,
							     min_price,
							 }| {
								RefundParametersExtended {
									retry_duration,
									refund_destination: RefundDestination::ExternalAddress(
										refund_address,
									),
									min_price,
								}
							},
						),
						output_action: SwapOutputAction::Egress {
							ccm_deposit_metadata,
							output_address,
						},
						dca_state,
						broker_fees,
					},
					old::SwapRequestState::NetworkFee => SwapRequestState::NetworkFee,
					old::SwapRequestState::IngressEgressFee => SwapRequestState::IngressEgressFee,
				},
			})
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_swap_request_count = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_swap_request_count = crate::SwapRequests::<T>::iter().count() as u64;

		assert_eq!(pre_swap_request_count, post_swap_request_count);
		Ok(())
	}
}
