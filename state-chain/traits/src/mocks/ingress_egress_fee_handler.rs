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

use super::{MockPallet, MockPalletStorage};
use crate::IngressEgressFeeApi;
use cf_chains::Chain;

use sp_std::marker::PhantomData;

pub struct MockIngressEgressFeeHandler<C>(PhantomData<C>);

impl<C> MockPallet for MockIngressEgressFeeHandler<C> {
	const PREFIX: &'static [u8] = b"MockIngressEgressFeeHandler";
}

const WITHHELD_FEES: &[u8] = b"WITHHELD_FEES";

impl<C: Chain> MockIngressEgressFeeHandler<C> {
	pub fn withheld_assets(asset: C::ChainAsset) -> C::ChainAmount {
		let asset: cf_primitives::Asset = asset.into();
		Self::get_storage(WITHHELD_FEES, asset).unwrap_or_default()
	}
}

impl<C: Chain> IngressEgressFeeApi<C> for MockIngressEgressFeeHandler<C> {
	fn accrue_withheld_fee(asset: C::ChainAsset, fee: C::ChainAmount) {
		let asset: cf_primitives::Asset = asset.into();
		Self::mutate_storage::<cf_primitives::Asset, _, _, _, _>(
			WITHHELD_FEES,
			&asset,
			|fees: &mut Option<C::ChainAmount>| {
				*fees = Some(fees.unwrap_or_default() + fee);
			},
		);
	}
}
