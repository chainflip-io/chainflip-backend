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

use subxt::{config::transaction_extensions, Config};

pub enum StateChainConfig {}

impl Config for StateChainConfig {
	// We cannot use our own Runtime's types for every associated type here, see comments below.
	type Hash = subxt::utils::H256;
	type AccountId = subxt::utils::AccountId32; // Requires EncodeAsType trait (which our AccountId doesn't)
	type Address = subxt::utils::MultiAddress<Self::AccountId, ()>; // Must be convertible from Self::AccountId
	type Signature = state_chain_runtime::Signature;
	type Hasher = subxt::config::substrate::BlakeTwo256;
	type Header = subxt::config::substrate::SubstrateHeader<u32, Self::Hasher>;
	type AssetId = u32; // Not used - we don't use pallet-assets
	type ExtrinsicParams = transaction_extensions::AnyOf<
		Self,
		(
			transaction_extensions::VerifySignature<Self>,
			transaction_extensions::CheckSpecVersion,
			transaction_extensions::CheckTxVersion,
			transaction_extensions::CheckNonce,
			transaction_extensions::CheckGenesis<Self>,
			transaction_extensions::CheckMortality<Self>,
			transaction_extensions::ChargeAssetTxPayment<Self>,
			transaction_extensions::ChargeTransactionPayment,
			transaction_extensions::CheckMetadataHash,
		),
	>;
}
