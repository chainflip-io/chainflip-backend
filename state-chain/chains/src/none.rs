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

use super::*;
use cf_primitives::chains::assets::any::AssetMap;
use frame_support::traits::ConstBool;

/// A Chain that can't be constructed.
#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum NoneChain {}

impl Chain for NoneChain {
	const NAME: &'static str = "NONE";
	const GAS_ASSET: Self::ChainAsset = assets::any::Asset::Usdc;
	const WITNESS_PERIOD: Self::ChainBlockNumber = 1;
	type ChainCrypto = NoneChainCrypto;
	type ChainBlockNumber = u64;
	type ChainAmount = u64;
	type TransactionFee = u64;
	type TrackedData = ();
	type ChainAsset = assets::any::Asset;
	type ChainAccount = ForeignChainAddress;
	type DepositFetchId = ChannelId;
	type DepositChannelState = ();
	type DepositDetails = ();
	type Transaction = ();
	type TransactionMetadata = ();
	type ReplayProtectionParams = ();
	type ReplayProtection = ();
	type TransactionRef = ();
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = AssetMap<T>;

	fn reference_gas_asset_price_in_input_asset(
		_input_asset: Self::ChainAsset,
	) -> Self::ChainAmount {
		0
	}
}

impl FeeRefundCalculator<NoneChain> for () {
	fn return_fee_refund(
		&self,
		_fee_paid: <NoneChain as Chain>::TransactionFee,
	) -> <NoneChain as Chain>::ChainAmount {
		unimplemented!()
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoneChainCrypto;
impl ChainCrypto for NoneChainCrypto {
	const NAME: &'static str = "None";
	type UtxoChain = ConstBool<false>;
	type AggKey = ();
	type Payload = ();
	type ThresholdSignature = ();
	type TransactionInId = ();
	type TransactionOutId = ();
	type KeyHandoverIsRequired = ConstBool<false>;
	type GovKey = ();

	fn verify_threshold_signature(
		_agg_key: &Self::AggKey,
		_payload: &Self::Payload,
		_signature: &Self::ThresholdSignature,
	) -> bool {
		unimplemented!()
	}

	fn agg_key_to_payload(_agg_key: Self::AggKey, _for_handover: bool) -> Self::Payload {
		unimplemented!()
	}

	fn maybe_broadcast_barriers_on_rotation(
		_rotation_broadcast_id: BroadcastId,
	) -> Vec<BroadcastId> {
		unimplemented!()
	}
}
