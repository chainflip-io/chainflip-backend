use cf_chains::{address::IntoForeignChainAddress, CcmChannelMetadataUnchecked};
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
use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::Config;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use super::*;
	use cf_chains::CcmAdditionalData;

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct CrossChainMessage<C: Chain> {
		pub egress_id: EgressId,
		pub asset: C::ChainAsset,
		pub amount: C::ChainAmount,
		pub destination_address: C::ChainAccount,
		pub message: CcmMessage,
		pub source_chain: ForeignChain,
		pub source_address: Option<ForeignChainAddress>,
		pub ccm_additional_data: CcmAdditionalData,
		pub gas_budget: GasAmount,
	}

	#[frame_support::storage_alias]
	pub type ScheduledEgressCcm<T: Config<I>, I: 'static> = StorageValue<
		Pallet<T, I>,
		Vec<CrossChainMessage<<T as Config<I>>::TargetChain>>,
		ValueQuery,
	>;
}

pub struct CcmAdditionalDataToCheckedMigration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade
	for CcmAdditionalDataToCheckedMigration<T, I>
{
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let ccms = old::ScheduledEgressCcm::<T, I>::get()
			.into_iter()
			.map(|ccm| ccm.egress_id)
			.collect::<BTreeSet<_>>();
		Ok(ccms.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		log::info!("🍩 Running migration for Ingress-Egress pallet: Updating CCM's additional data to decoded.");
		let _ = crate::ScheduledEgressCcm::<T, I>::translate::<
			Vec<old::CrossChainMessage<<T as Config<I>>::TargetChain>>,
			_,
		>(|maybe_old_ccms| {
			maybe_old_ccms.map(|old_ccms| {
				old_ccms
					.into_iter()
					.filter_map(|old_ccm| {
						match (CcmChannelMetadataUnchecked {
							message: old_ccm.message.clone(),
							gas_budget: old_ccm.gas_budget,
							ccm_additional_data: old_ccm.ccm_additional_data,
						}
						.to_checked(
							old_ccm.asset.into(),
							old_ccm.destination_address.clone().into_foreign_chain_address(),
						)) {
							Err(e) => {
								log::error!("❌ Ccm To Checked Migration for Ingress-Egress pallet failed. Egress id: {:?}, err: {:?}", old_ccm.egress_id, e);
								None
							},
							Ok(checked_ccm) => Some(CrossChainMessage {
								egress_id: old_ccm.egress_id,
								asset: old_ccm.asset,
								amount: old_ccm.amount,
								destination_address: old_ccm.destination_address.clone(),
								message: old_ccm.message,
								source_chain: old_ccm.source_chain,
								source_address: old_ccm.source_address,
								ccm_additional_data: checked_ccm.ccm_additional_data,
								gas_budget: old_ccm.gas_budget,
							})
						}
					})
					.collect::<Vec<_>>()
			})
		});
		log::info!("🍩 Migration for Ingress-Egress pallet complete.");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_ccms = BTreeSet::<EgressId>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_ccms = old::ScheduledEgressCcm::<T, I>::get()
			.into_iter()
			.map(|ccm| ccm.egress_id)
			.collect::<BTreeSet<_>>();

		assert_eq!(pre_ccms, post_ccms);
		Ok(())
	}
}
