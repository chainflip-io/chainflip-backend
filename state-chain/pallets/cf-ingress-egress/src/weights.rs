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


//! Autogenerated weights for pallet_cf_ingress_egress
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2024-08-26, STEPS: `20`, REPEAT: `10`, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! WORST CASE MAP SIZE: `1000000`
//! HOSTNAME: `ip-172-31-10-39`, CPU: `Intel(R) Xeon(R) Platinum 8124M CPU @ 3.00GHz`
//! EXECUTION: , WASM-EXECUTION: Compiled, CHAIN: Some("dev-3"), DB CACHE: 1024

// Executed Command:
// ./chainflip-node
// benchmark
// pallet
// --pallet
// pallet_cf_ingress_egress
// --extrinsic
// *
// --output
// state-chain/pallets/cf-ingress-egress/src/weights.rs
// --steps=20
// --repeat=10
// --template=state-chain/chainflip-weight-template.hbs
// --chain=dev-3

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::ParityDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for pallet_cf_ingress_egress.
pub trait WeightInfo {
	fn disable_asset_egress() -> Weight;
	fn process_channel_deposit_full_witness() -> Weight;
	fn finalise_ingress(a: u32, ) -> Weight;
	fn vault_transfer_failed() -> Weight;
	fn ccm_broadcast_failed() -> Weight;
	fn vault_swap_request() -> Weight;
	fn boost_finalised() -> Weight;
	fn mark_transaction_for_rejection() -> Weight;
}

/// Weights for pallet_cf_ingress_egress using the Substrate node and recommended hardware.
pub struct PalletWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for PalletWeight<T> {
	/// Storage: `EthereumIngressEgress::DisabledEgressAssets` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::DisabledEgressAssets` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn disable_asset_egress() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `206`
		//  Estimated: `3671`
		// Minimum execution time: 12_871_000 picoseconds.
		Weight::from_parts(13_340_000, 3671)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `EthereumIngressEgress::DepositChannelLookup` (r:1 w:0)
	/// Proof: `EthereumIngressEgress::DepositChannelLookup` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::DepositChannelPool` (r:1 w:0)
	/// Proof: `EthereumIngressEgress::DepositChannelPool` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::MinimumDeposit` (r:1 w:0)
	/// Proof: `EthereumIngressEgress::MinimumDeposit` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::ScheduledEgressFetchOrTransfer` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::ScheduledEgressFetchOrTransfer` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumChainTracking::FeeMultiplier` (r:1 w:0)
	/// Proof: `EthereumChainTracking::FeeMultiplier` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `EthereumChainTracking::CurrentChainState` (r:1 w:0)
	/// Proof: `EthereumChainTracking::CurrentChainState` (`max_values`: Some(1), `max_size`: Some(40), added: 535, mode: `MaxEncodedLen`)
	/// Storage: `AssetBalances::WithheldAssets` (r:1 w:1)
	/// Proof: `AssetBalances::WithheldAssets` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn process_channel_deposit_full_witness() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `624`
		//  Estimated: `4089`
		// Minimum execution time: 36_003_000 picoseconds.
		Weight::from_parts(36_638_000, 4089)
			.saturating_add(T::DbWeight::get().reads(7_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
	/// Storage: `EthereumIngressEgress::DepositChannelLookup` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::DepositChannelLookup` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// The range of component `a` is `[1, 100]`.
	fn finalise_ingress(a: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `340`
		//  Estimated: `3805`
		// Minimum execution time: 2_025_000 picoseconds.
		Weight::from_parts(7_534_981, 3805)
			// Standard Error: 5_738
			.saturating_add(Weight::from_parts(1_992_309, 0).saturating_mul(a.into()))
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `Validator::CurrentEpoch` (r:1 w:0)
	/// Proof: `Validator::CurrentEpoch` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumVaultAddress` (r:1 w:0)
	/// Proof: `Environment::EthereumVaultAddress` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumSignatureNonce` (r:1 w:1)
	/// Proof: `Environment::EthereumSignatureNonce` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumChainId` (r:1 w:0)
	/// Proof: `Environment::EthereumChainId` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumKeyManagerAddress` (r:1 w:0)
	/// Proof: `Environment::EthereumKeyManagerAddress` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumBroadcaster::BroadcastIdCounter` (r:1 w:1)
	/// Proof: `EthereumBroadcaster::BroadcastIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumChainTracking::CurrentChainState` (r:1 w:0)
	/// Proof: `EthereumChainTracking::CurrentChainState` (`max_values`: Some(1), `max_size`: Some(40), added: 535, mode: `MaxEncodedLen`)
	/// Storage: `EvmThresholdSigner::CurrentKeyEpoch` (r:1 w:0)
	/// Proof: `EvmThresholdSigner::CurrentKeyEpoch` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::Keys` (r:1 w:0)
	/// Proof: `EvmThresholdSigner::Keys` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::ThresholdSignatureRequestIdCounter` (r:1 w:1)
	/// Proof: `EvmThresholdSigner::ThresholdSignatureRequestIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::HistoricalAuthorities` (r:1 w:0)
	/// Proof: `Validator::HistoricalAuthorities` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Reputation::Suspensions` (r:4 w:0)
	/// Proof: `Reputation::Suspensions` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::CeremonyIdCounter` (r:1 w:1)
	/// Proof: `EvmThresholdSigner::CeremonyIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::ThresholdSignatureResponseTimeout` (r:1 w:0)
	/// Proof: `EvmThresholdSigner::ThresholdSignatureResponseTimeout` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::CeremonyRetryQueues` (r:1 w:1)
	/// Proof: `EvmThresholdSigner::CeremonyRetryQueues` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `CfeInterface::CfeEvents` (r:1 w:1)
	/// Proof: `CfeInterface::CfeEvents` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::FailedForeignChainCalls` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::FailedForeignChainCalls` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::SignerAndSignature` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::SignerAndSignature` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::PendingCeremonies` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::PendingCeremonies` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::RequestCallback` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::RequestCallback` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn vault_transfer_failed() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1800`
		//  Estimated: `12690`
		// Minimum execution time: 107_776_000 picoseconds.
		Weight::from_parts(109_294_000, 12690)
			.saturating_add(T::DbWeight::get().reads(20_u64))
			.saturating_add(T::DbWeight::get().writes(10_u64))
	}
	/// Storage: `Validator::CurrentEpoch` (r:1 w:0)
	/// Proof: `Validator::CurrentEpoch` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::FailedForeignChainCalls` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::FailedForeignChainCalls` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn ccm_broadcast_failed() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `552`
		//  Estimated: `4017`
		// Minimum execution time: 18_101_000 picoseconds.
		Weight::from_parts(18_571_000, 4017)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `Swapping::SwapRequestIdCounter` (r:1 w:1)
	/// Proof: `Swapping::SwapRequestIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Swapping::MaximumSwapAmount` (r:1 w:0)
	/// Proof: `Swapping::MaximumSwapAmount` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Swapping::SwapIdCounter` (r:1 w:1)
	/// Proof: `Swapping::SwapIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Swapping::SwapQueue` (r:1 w:1)
	/// Proof: `Swapping::SwapQueue` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Swapping::SwapRequests` (r:0 w:1)
	/// Proof: `Swapping::SwapRequests` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn vault_swap_request() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `103`
		//  Estimated: `3568`
		// Minimum execution time: 16_000_000 picoseconds.
		Weight::from_parts(17_000_000, 3568)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(4_u64))
	}
	/// Storage: `EthereumIngressEgress::DepositChannelLookup` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::DepositChannelLookup` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::DepositChannelPool` (r:1 w:0)
	/// Proof: `EthereumIngressEgress::DepositChannelPool` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::MinimumDeposit` (r:1 w:0)
	/// Proof: `EthereumIngressEgress::MinimumDeposit` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::ScheduledEgressFetchOrTransfer` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::ScheduledEgressFetchOrTransfer` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::BoostPools` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::BoostPools` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `AssetBalances::FreeBalances` (r:30 w:30)
	/// Proof: `AssetBalances::FreeBalances` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn boost_finalised() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `15997`
		//  Estimated: `91237`
		// Minimum execution time: 328_594_000 picoseconds.
		Weight::from_parts(330_581_000, 91237)
			.saturating_add(T::DbWeight::get().reads(35_u64))
			.saturating_add(T::DbWeight::get().writes(33_u64))
	}
	/// Storage: `EthereumIngressEgress::TransactionsMarkedForRejection` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::TransactionsMarkedForRejection` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::ReportExpiresAt` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::ReportExpiresAt` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn mark_transaction_for_rejection() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 0 picoseconds.
		Weight::from_parts(0, 0)
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	/// Storage: `EthereumIngressEgress::DisabledEgressAssets` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::DisabledEgressAssets` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn disable_asset_egress() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `206`
		//  Estimated: `3671`
		// Minimum execution time: 12_871_000 picoseconds.
		Weight::from_parts(13_340_000, 3671)
			.saturating_add(ParityDbWeight::get().reads(1_u64))
			.saturating_add(ParityDbWeight::get().writes(1_u64))
	}
	/// Storage: `EthereumIngressEgress::DepositChannelLookup` (r:1 w:0)
	/// Proof: `EthereumIngressEgress::DepositChannelLookup` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::DepositChannelPool` (r:1 w:0)
	/// Proof: `EthereumIngressEgress::DepositChannelPool` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::MinimumDeposit` (r:1 w:0)
	/// Proof: `EthereumIngressEgress::MinimumDeposit` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::ScheduledEgressFetchOrTransfer` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::ScheduledEgressFetchOrTransfer` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumChainTracking::FeeMultiplier` (r:1 w:0)
	/// Proof: `EthereumChainTracking::FeeMultiplier` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `EthereumChainTracking::CurrentChainState` (r:1 w:0)
	/// Proof: `EthereumChainTracking::CurrentChainState` (`max_values`: Some(1), `max_size`: Some(40), added: 535, mode: `MaxEncodedLen`)
	/// Storage: `AssetBalances::WithheldAssets` (r:1 w:1)
	/// Proof: `AssetBalances::WithheldAssets` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn process_channel_deposit_full_witness() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `624`
		//  Estimated: `4089`
		// Minimum execution time: 36_003_000 picoseconds.
		Weight::from_parts(36_638_000, 4089)
			.saturating_add(ParityDbWeight::get().reads(7_u64))
			.saturating_add(ParityDbWeight::get().writes(2_u64))
	}
	/// Storage: `EthereumIngressEgress::DepositChannelLookup` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::DepositChannelLookup` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// The range of component `a` is `[1, 100]`.
	fn finalise_ingress(a: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `340`
		//  Estimated: `3805`
		// Minimum execution time: 2_025_000 picoseconds.
		Weight::from_parts(7_534_981, 3805)
			// Standard Error: 5_738
			.saturating_add(Weight::from_parts(1_992_309, 0).saturating_mul(a.into()))
			.saturating_add(ParityDbWeight::get().reads(1_u64))
			.saturating_add(ParityDbWeight::get().writes(1_u64))
	}
	/// Storage: `Validator::CurrentEpoch` (r:1 w:0)
	/// Proof: `Validator::CurrentEpoch` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumVaultAddress` (r:1 w:0)
	/// Proof: `Environment::EthereumVaultAddress` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumSignatureNonce` (r:1 w:1)
	/// Proof: `Environment::EthereumSignatureNonce` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumChainId` (r:1 w:0)
	/// Proof: `Environment::EthereumChainId` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumKeyManagerAddress` (r:1 w:0)
	/// Proof: `Environment::EthereumKeyManagerAddress` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumBroadcaster::BroadcastIdCounter` (r:1 w:1)
	/// Proof: `EthereumBroadcaster::BroadcastIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumChainTracking::CurrentChainState` (r:1 w:0)
	/// Proof: `EthereumChainTracking::CurrentChainState` (`max_values`: Some(1), `max_size`: Some(40), added: 535, mode: `MaxEncodedLen`)
	/// Storage: `EvmThresholdSigner::CurrentKeyEpoch` (r:1 w:0)
	/// Proof: `EvmThresholdSigner::CurrentKeyEpoch` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::Keys` (r:1 w:0)
	/// Proof: `EvmThresholdSigner::Keys` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::ThresholdSignatureRequestIdCounter` (r:1 w:1)
	/// Proof: `EvmThresholdSigner::ThresholdSignatureRequestIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::HistoricalAuthorities` (r:1 w:0)
	/// Proof: `Validator::HistoricalAuthorities` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Reputation::Suspensions` (r:4 w:0)
	/// Proof: `Reputation::Suspensions` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::CeremonyIdCounter` (r:1 w:1)
	/// Proof: `EvmThresholdSigner::CeremonyIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::ThresholdSignatureResponseTimeout` (r:1 w:0)
	/// Proof: `EvmThresholdSigner::ThresholdSignatureResponseTimeout` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::CeremonyRetryQueues` (r:1 w:1)
	/// Proof: `EvmThresholdSigner::CeremonyRetryQueues` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `CfeInterface::CfeEvents` (r:1 w:1)
	/// Proof: `CfeInterface::CfeEvents` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::FailedForeignChainCalls` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::FailedForeignChainCalls` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::SignerAndSignature` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::SignerAndSignature` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::PendingCeremonies` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::PendingCeremonies` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::RequestCallback` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::RequestCallback` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn vault_transfer_failed() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1800`
		//  Estimated: `12690`
		// Minimum execution time: 107_776_000 picoseconds.
		Weight::from_parts(109_294_000, 12690)
			.saturating_add(ParityDbWeight::get().reads(20_u64))
			.saturating_add(ParityDbWeight::get().writes(10_u64))
	}
	/// Storage: `Validator::CurrentEpoch` (r:1 w:0)
	/// Proof: `Validator::CurrentEpoch` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::FailedForeignChainCalls` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::FailedForeignChainCalls` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn ccm_broadcast_failed() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `552`
		//  Estimated: `4017`
		// Minimum execution time: 18_101_000 picoseconds.
		Weight::from_parts(18_571_000, 4017)
			.saturating_add(ParityDbWeight::get().reads(2_u64))
			.saturating_add(ParityDbWeight::get().writes(1_u64))
	}
	/// Storage: `Swapping::SwapRequestIdCounter` (r:1 w:1)
	/// Proof: `Swapping::SwapRequestIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Swapping::MaximumSwapAmount` (r:1 w:0)
	/// Proof: `Swapping::MaximumSwapAmount` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Swapping::SwapIdCounter` (r:1 w:1)
	/// Proof: `Swapping::SwapIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Swapping::SwapQueue` (r:1 w:1)
	/// Proof: `Swapping::SwapQueue` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Swapping::SwapRequests` (r:0 w:1)
	/// Proof: `Swapping::SwapRequests` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn vault_swap_request() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `103`
		//  Estimated: `3568`
		// Minimum execution time: 16_000_000 picoseconds.
		Weight::from_parts(17_000_000, 3568)
			.saturating_add(ParityDbWeight::get().reads(4_u64))
			.saturating_add(ParityDbWeight::get().writes(4_u64))
	}
	/// Storage: `EthereumIngressEgress::DepositChannelLookup` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::DepositChannelLookup` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::DepositChannelPool` (r:1 w:0)
	/// Proof: `EthereumIngressEgress::DepositChannelPool` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::MinimumDeposit` (r:1 w:0)
	/// Proof: `EthereumIngressEgress::MinimumDeposit` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::ScheduledEgressFetchOrTransfer` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::ScheduledEgressFetchOrTransfer` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::BoostPools` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::BoostPools` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `AssetBalances::FreeBalances` (r:30 w:30)
	/// Proof: `AssetBalances::FreeBalances` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn boost_finalised() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `15997`
		//  Estimated: `91237`
		// Minimum execution time: 328_594_000 picoseconds.
		Weight::from_parts(330_581_000, 91237)
			.saturating_add(ParityDbWeight::get().reads(35_u64))
			.saturating_add(ParityDbWeight::get().writes(33_u64))
	}
	/// Storage: `EthereumIngressEgress::TransactionsMarkedForRejection` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::TransactionsMarkedForRejection` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumIngressEgress::ReportExpiresAt` (r:1 w:1)
	/// Proof: `EthereumIngressEgress::ReportExpiresAt` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn mark_transaction_for_rejection() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 0 picoseconds.
		Weight::from_parts(0, 0)
	}
}
