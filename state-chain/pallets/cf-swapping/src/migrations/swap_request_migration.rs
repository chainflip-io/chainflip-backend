use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::Config;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use super::*;
	use cf_chains::{CcmDepositMetadata, ChannelRefundParameters, ForeignChainAddress};
	use cf_primitives::{Asset, AssetAmount, Beneficiaries, SwapId};
	use frame_support::Twox64Concat;

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
		pub refund_params: Option<ChannelRefundParameters>,
		pub state: SwapRequestState<T>,
	}

	#[frame_support::storage_alias]
	pub type SwapRequests<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, SwapRequestId, SwapRequest<T>>;
}

pub struct SwapRequestMigration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for SwapRequestMigration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok((old::SwapRequests::<T>::iter().count() as u64).encode())
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
						ccm_deposit_metadata: ccm
							.map(|old_ccm_state| old_ccm_state.ccm_deposit_metadata),
						output_address,
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
