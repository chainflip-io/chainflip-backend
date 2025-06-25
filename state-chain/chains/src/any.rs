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

use crate::{
	address::ForeignChainAddress, none::NoneChainCrypto, Chain, DepositDetailsToTransactionInId,
	FeeRefundCalculator,
};
use codec::{FullCodec, MaxEncodedLen};
use frame_support::Parameter;
use sp_runtime::traits::Member;

use crate::benchmarking_value::BenchmarkValue;
use cf_primitives::{
	chains::{assets, AnyChain},
	AssetAmount, ChannelId,
};

impl Chain for AnyChain {
	const NAME: &'static str = "AnyChain";
	const GAS_ASSET: Self::ChainAsset = assets::any::Asset::Usdc;
	const WITNESS_PERIOD: u64 = 1;

	type ChainCrypto = NoneChainCrypto;
	type ChainBlockNumber = u64;
	type ChainAmount = AssetAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = ();
	type ChainAsset = assets::any::Asset;
	type ChainAssetMap<
		T: Member + Parameter + MaxEncodedLen + Copy + BenchmarkValue + FullCodec + Unpin,
	> = assets::any::AssetMap<T>;
	type ChainAccount = ForeignChainAddress;
	type DepositFetchId = ChannelId;
	type DepositChannelState = ();
	type DepositDetails = ();
	type Transaction = ();
	type TransactionMetadata = ();
	type TransactionRef = ();
	type ReplayProtectionParams = ();
	type ReplayProtection = ();

	fn reference_gas_asset_price_in_input_asset(
		_input_asset: Self::ChainAsset,
	) -> Self::ChainAmount {
		0
	}
}

impl FeeRefundCalculator<AnyChain> for () {
	fn return_fee_refund(
		&self,
		_fee_paid: <AnyChain as Chain>::TransactionFee,
	) -> <AnyChain as Chain>::ChainAmount {
		unimplemented!()
	}
}

impl DepositDetailsToTransactionInId<NoneChainCrypto> for () {}
