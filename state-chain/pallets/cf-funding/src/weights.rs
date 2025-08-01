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


//! Autogenerated weights for pallet_cf_funding
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
// pallet_cf_funding
// --extrinsic
// *
// --output
// state-chain/pallets/cf-funding/src/weights.rs
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

/// Weight functions needed for pallet_cf_funding.
pub trait WeightInfo {
	fn funded() -> Weight;
	fn redeem() -> Weight;
	fn redeemed() -> Weight;
	fn redemption_expired() -> Weight;
	fn update_minimum_funding() -> Weight;
	fn update_redemption_tax() -> Weight;
	fn bind_redeem_address() -> Weight;
	fn update_restricted_addresses(a: u32, b: u32, c: u32, ) -> Weight;
	fn bind_executor_address() -> Weight;
	fn rebalance() -> Weight;
}

/// Weights for pallet_cf_funding using the Substrate node and recommended hardware.
pub struct PalletWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for PalletWeight<T> {
	/// Storage: `Funding::MinimumFunding` (r:1 w:0)
	/// Proof: `Funding::MinimumFunding` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Flip::OffchainFunds` (r:1 w:1)
	/// Proof: `Flip::OffchainFunds` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `Flip::Account` (r:1 w:1)
	/// Proof: `Flip::Account` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `Flip::TotalIssuance` (r:1 w:1)
	/// Proof: `Flip::TotalIssuance` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `Validator::CurrentAuthorities` (r:1 w:0)
	/// Proof: `Validator::CurrentAuthorities` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::Backups` (r:1 w:1)
	/// Proof: `Validator::Backups` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RestrictedAddresses` (r:1 w:0)
	/// Proof: `Funding::RestrictedAddresses` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `AccountRoles::AccountRoles` (r:0 w:1)
	/// Proof: `AccountRoles::AccountRoles` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn funded() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1139`
		//  Estimated: `4604`
		// Minimum execution time: 52_707_000 picoseconds.
		Weight::from_parts(53_103_000, 4604)
			.saturating_add(T::DbWeight::get().reads(7_u64))
			.saturating_add(T::DbWeight::get().writes(5_u64))
	}
	/// Storage: `Environment::RuntimeSafeMode` (r:1 w:0)
	/// Proof: `Environment::RuntimeSafeMode` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::CurrentRotationPhase` (r:1 w:0)
	/// Proof: `Validator::CurrentRotationPhase` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::CurrentEpochStartedAt` (r:1 w:0)
	/// Proof: `Validator::CurrentEpochStartedAt` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::RedemptionPeriodAsPercentage` (r:1 w:0)
	/// Proof: `Validator::RedemptionPeriodAsPercentage` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::BlocksPerEpoch` (r:1 w:0)
	/// Proof: `Validator::BlocksPerEpoch` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::PendingRedemptions` (r:1 w:1)
	/// Proof: `Funding::PendingRedemptions` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RestrictedBalances` (r:1 w:0)
	/// Proof: `Funding::RestrictedBalances` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::BoundExecutorAddress` (r:1 w:0)
	/// Proof: `Funding::BoundExecutorAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RedemptionTax` (r:1 w:0)
	/// Proof: `Funding::RedemptionTax` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::BoundRedeemAddress` (r:1 w:0)
	/// Proof: `Funding::BoundRedeemAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Flip::Account` (r:1 w:1)
	/// Proof: `Flip::Account` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `Flip::TotalIssuance` (r:1 w:1)
	/// Proof: `Flip::TotalIssuance` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `Validator::CurrentAuthorities` (r:1 w:0)
	/// Proof: `Validator::CurrentAuthorities` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::Backups` (r:1 w:1)
	/// Proof: `Validator::Backups` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `AccountRoles::AccountRoles` (r:1 w:0)
	/// Proof: `AccountRoles::AccountRoles` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Funding::RedemptionTTLSeconds` (r:1 w:0)
	/// Proof: `Funding::RedemptionTTLSeconds` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumStateChainGatewayAddress` (r:1 w:0)
	/// Proof: `Environment::EthereumStateChainGatewayAddress` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumSignatureNonce` (r:1 w:1)
	/// Proof: `Environment::EthereumSignatureNonce` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumChainId` (r:1 w:0)
	/// Proof: `Environment::EthereumChainId` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumKeyManagerAddress` (r:1 w:0)
	/// Proof: `Environment::EthereumKeyManagerAddress` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumBroadcaster::BroadcastIdCounter` (r:1 w:1)
	/// Proof: `EthereumBroadcaster::BroadcastIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumBroadcaster::PendingBroadcasts` (r:1 w:1)
	/// Proof: `EthereumBroadcaster::PendingBroadcasts` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
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
	/// Storage: `EvmThresholdSigner::SignerAndSignature` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::SignerAndSignature` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::PendingCeremonies` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::PendingCeremonies` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::RequestCallback` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::RequestCallback` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Flip::PendingRedemptionsReserve` (r:0 w:1)
	/// Proof: `Flip::PendingRedemptionsReserve` (`max_values`: None, `max_size`: Some(64), added: 2539, mode: `MaxEncodedLen`)
	fn redeem() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `3016`
		//  Estimated: `13906`
		// Minimum execution time: 170_101_000 picoseconds.
		Weight::from_parts(172_057_000, 13906)
			.saturating_add(T::DbWeight::get().reads(36_u64))
			.saturating_add(T::DbWeight::get().writes(15_u64))
	}
	/// Storage: `Funding::PendingRedemptions` (r:1 w:1)
	/// Proof: `Funding::PendingRedemptions` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Flip::PendingRedemptionsReserve` (r:1 w:1)
	/// Proof: `Flip::PendingRedemptionsReserve` (`max_values`: None, `max_size`: Some(64), added: 2539, mode: `MaxEncodedLen`)
	/// Storage: `Flip::OffchainFunds` (r:1 w:1)
	/// Proof: `Flip::OffchainFunds` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `Flip::Account` (r:1 w:1)
	/// Proof: `Flip::Account` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `Flip::TotalIssuance` (r:1 w:1)
	/// Proof: `Flip::TotalIssuance` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `AccountRoles::VanityNames` (r:1 w:0)
	/// Proof: `AccountRoles::VanityNames` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Reputation::LastHeartbeat` (r:0 w:1)
	/// Proof: `Reputation::LastHeartbeat` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Reputation::Reputations` (r:0 w:1)
	/// Proof: `Reputation::Reputations` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Reputation::OffenceTimeSlotTracker` (r:0 w:1)
	/// Proof: `Reputation::OffenceTimeSlotTracker` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `AccountRoles::AccountRoles` (r:0 w:1)
	/// Proof: `AccountRoles::AccountRoles` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RestrictedBalances` (r:0 w:1)
	/// Proof: `Funding::RestrictedBalances` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::BoundRedeemAddress` (r:0 w:1)
	/// Proof: `Funding::BoundRedeemAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::BoundExecutorAddress` (r:0 w:1)
	/// Proof: `Funding::BoundExecutorAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn redeemed() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1663`
		//  Estimated: `5128`
		// Minimum execution time: 133_980_000 picoseconds.
		Weight::from_parts(137_645_000, 5128)
			.saturating_add(T::DbWeight::get().reads(6_u64))
			.saturating_add(T::DbWeight::get().writes(12_u64))
	}
	/// Storage: `Funding::PendingRedemptions` (r:1 w:1)
	/// Proof: `Funding::PendingRedemptions` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Flip::PendingRedemptionsReserve` (r:1 w:1)
	/// Proof: `Flip::PendingRedemptionsReserve` (`max_values`: None, `max_size`: Some(64), added: 2539, mode: `MaxEncodedLen`)
	/// Storage: `Flip::Account` (r:1 w:1)
	/// Proof: `Flip::Account` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `Flip::TotalIssuance` (r:1 w:1)
	/// Proof: `Flip::TotalIssuance` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `Validator::CurrentAuthorities` (r:1 w:0)
	/// Proof: `Validator::CurrentAuthorities` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::Backups` (r:1 w:1)
	/// Proof: `Validator::Backups` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RestrictedAddresses` (r:1 w:0)
	/// Proof: `Funding::RestrictedAddresses` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn redemption_expired() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1429`
		//  Estimated: `4894`
		// Minimum execution time: 50_162_000 picoseconds.
		Weight::from_parts(50_897_000, 4894)
			.saturating_add(T::DbWeight::get().reads(7_u64))
			.saturating_add(T::DbWeight::get().writes(5_u64))
	}
	/// Storage: `Funding::RedemptionTax` (r:1 w:0)
	/// Proof: `Funding::RedemptionTax` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::MinimumFunding` (r:0 w:1)
	/// Proof: `Funding::MinimumFunding` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	fn update_minimum_funding() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `143`
		//  Estimated: `1628`
		// Minimum execution time: 9_830_000 picoseconds.
		Weight::from_parts(10_226_000, 1628)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `Funding::MinimumFunding` (r:1 w:0)
	/// Proof: `Funding::MinimumFunding` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RedemptionTax` (r:0 w:1)
	/// Proof: `Funding::RedemptionTax` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	fn update_redemption_tax() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `143`
		//  Estimated: `1628`
		// Minimum execution time: 10_475_000 picoseconds.
		Weight::from_parts(11_170_000, 1628)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `Funding::BoundRedeemAddress` (r:1 w:1)
	/// Proof: `Funding::BoundRedeemAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn bind_redeem_address() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `140`
		//  Estimated: `3605`
		// Minimum execution time: 12_868_000 picoseconds.
		Weight::from_parts(13_414_000, 3605)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `Funding::RestrictedBalances` (r:101 w:100)
	/// Proof: `Funding::RestrictedBalances` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RestrictedAddresses` (r:0 w:1)
	/// Proof: `Funding::RestrictedAddresses` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// The range of component `a` is `[1, 100]`.
	/// The range of component `b` is `[1, 100]`.
	/// The range of component `c` is `[1, 100]`.
	fn update_restricted_addresses(_a: u32, b: u32, c: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `170 + c * (92 ±0)`
		//  Estimated: `3633 + b * (9 ±1) + c * (2567 ±1)`
		// Minimum execution time: 338_355_000 picoseconds.
		Weight::from_parts(339_065_000, 3633)
			// Standard Error: 5_053_622
			.saturating_add(Weight::from_parts(196_472_300, 0).saturating_mul(b.into()))
			// Standard Error: 5_053_622
			.saturating_add(Weight::from_parts(191_697_168, 0).saturating_mul(c.into()))
			.saturating_add(T::DbWeight::get().reads((1_u64).saturating_mul(c.into())))
			.saturating_add(T::DbWeight::get().writes(1_u64))
			.saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(c.into())))
			.saturating_add(Weight::from_parts(0, 9).saturating_mul(b.into()))
			.saturating_add(Weight::from_parts(0, 2567).saturating_mul(c.into()))
	}
	/// Storage: `Funding::BoundExecutorAddress` (r:1 w:1)
	/// Proof: `Funding::BoundExecutorAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn bind_executor_address() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `140`
		//  Estimated: `3605`
		// Minimum execution time: 13_287_000 picoseconds.
		Weight::from_parts(13_695_000, 3605)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	fn rebalance() -> Weight {
		Weight::from_parts(0, 0)
	}
}

// For backwards compatibility and tests
impl WeightInfo for () {
	/// Storage: `Funding::MinimumFunding` (r:1 w:0)
	/// Proof: `Funding::MinimumFunding` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Flip::OffchainFunds` (r:1 w:1)
	/// Proof: `Flip::OffchainFunds` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `Flip::Account` (r:1 w:1)
	/// Proof: `Flip::Account` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `Flip::TotalIssuance` (r:1 w:1)
	/// Proof: `Flip::TotalIssuance` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `Validator::CurrentAuthorities` (r:1 w:0)
	/// Proof: `Validator::CurrentAuthorities` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::Backups` (r:1 w:1)
	/// Proof: `Validator::Backups` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RestrictedAddresses` (r:1 w:0)
	/// Proof: `Funding::RestrictedAddresses` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `AccountRoles::AccountRoles` (r:0 w:1)
	/// Proof: `AccountRoles::AccountRoles` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn funded() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1139`
		//  Estimated: `4604`
		// Minimum execution time: 52_707_000 picoseconds.
		Weight::from_parts(53_103_000, 4604)
			.saturating_add(ParityDbWeight::get().reads(7_u64))
			.saturating_add(ParityDbWeight::get().writes(5_u64))
	}
	/// Storage: `Environment::RuntimeSafeMode` (r:1 w:0)
	/// Proof: `Environment::RuntimeSafeMode` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::CurrentRotationPhase` (r:1 w:0)
	/// Proof: `Validator::CurrentRotationPhase` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::CurrentEpochStartedAt` (r:1 w:0)
	/// Proof: `Validator::CurrentEpochStartedAt` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::RedemptionPeriodAsPercentage` (r:1 w:0)
	/// Proof: `Validator::RedemptionPeriodAsPercentage` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::BlocksPerEpoch` (r:1 w:0)
	/// Proof: `Validator::BlocksPerEpoch` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::PendingRedemptions` (r:1 w:1)
	/// Proof: `Funding::PendingRedemptions` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RestrictedBalances` (r:1 w:0)
	/// Proof: `Funding::RestrictedBalances` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::BoundExecutorAddress` (r:1 w:0)
	/// Proof: `Funding::BoundExecutorAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RedemptionTax` (r:1 w:0)
	/// Proof: `Funding::RedemptionTax` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::BoundRedeemAddress` (r:1 w:0)
	/// Proof: `Funding::BoundRedeemAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Flip::Account` (r:1 w:1)
	/// Proof: `Flip::Account` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `Flip::TotalIssuance` (r:1 w:1)
	/// Proof: `Flip::TotalIssuance` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `Validator::CurrentAuthorities` (r:1 w:0)
	/// Proof: `Validator::CurrentAuthorities` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::Backups` (r:1 w:1)
	/// Proof: `Validator::Backups` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `AccountRoles::AccountRoles` (r:1 w:0)
	/// Proof: `AccountRoles::AccountRoles` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Funding::RedemptionTTLSeconds` (r:1 w:0)
	/// Proof: `Funding::RedemptionTTLSeconds` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumStateChainGatewayAddress` (r:1 w:0)
	/// Proof: `Environment::EthereumStateChainGatewayAddress` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumSignatureNonce` (r:1 w:1)
	/// Proof: `Environment::EthereumSignatureNonce` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumChainId` (r:1 w:0)
	/// Proof: `Environment::EthereumChainId` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Environment::EthereumKeyManagerAddress` (r:1 w:0)
	/// Proof: `Environment::EthereumKeyManagerAddress` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumBroadcaster::BroadcastIdCounter` (r:1 w:1)
	/// Proof: `EthereumBroadcaster::BroadcastIdCounter` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `EthereumBroadcaster::PendingBroadcasts` (r:1 w:1)
	/// Proof: `EthereumBroadcaster::PendingBroadcasts` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
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
	/// Storage: `EvmThresholdSigner::SignerAndSignature` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::SignerAndSignature` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::PendingCeremonies` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::PendingCeremonies` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `EvmThresholdSigner::RequestCallback` (r:0 w:1)
	/// Proof: `EvmThresholdSigner::RequestCallback` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Flip::PendingRedemptionsReserve` (r:0 w:1)
	/// Proof: `Flip::PendingRedemptionsReserve` (`max_values`: None, `max_size`: Some(64), added: 2539, mode: `MaxEncodedLen`)
	fn redeem() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `3016`
		//  Estimated: `13906`
		// Minimum execution time: 170_101_000 picoseconds.
		Weight::from_parts(172_057_000, 13906)
			.saturating_add(ParityDbWeight::get().reads(36_u64))
			.saturating_add(ParityDbWeight::get().writes(15_u64))
	}
	/// Storage: `Funding::PendingRedemptions` (r:1 w:1)
	/// Proof: `Funding::PendingRedemptions` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Flip::PendingRedemptionsReserve` (r:1 w:1)
	/// Proof: `Flip::PendingRedemptionsReserve` (`max_values`: None, `max_size`: Some(64), added: 2539, mode: `MaxEncodedLen`)
	/// Storage: `Flip::OffchainFunds` (r:1 w:1)
	/// Proof: `Flip::OffchainFunds` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `Flip::Account` (r:1 w:1)
	/// Proof: `Flip::Account` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `Flip::TotalIssuance` (r:1 w:1)
	/// Proof: `Flip::TotalIssuance` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `AccountRoles::VanityNames` (r:1 w:0)
	/// Proof: `AccountRoles::VanityNames` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Reputation::LastHeartbeat` (r:0 w:1)
	/// Proof: `Reputation::LastHeartbeat` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Reputation::Reputations` (r:0 w:1)
	/// Proof: `Reputation::Reputations` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Reputation::OffenceTimeSlotTracker` (r:0 w:1)
	/// Proof: `Reputation::OffenceTimeSlotTracker` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `AccountRoles::AccountRoles` (r:0 w:1)
	/// Proof: `AccountRoles::AccountRoles` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RestrictedBalances` (r:0 w:1)
	/// Proof: `Funding::RestrictedBalances` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::BoundRedeemAddress` (r:0 w:1)
	/// Proof: `Funding::BoundRedeemAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::BoundExecutorAddress` (r:0 w:1)
	/// Proof: `Funding::BoundExecutorAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn redeemed() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1663`
		//  Estimated: `5128`
		// Minimum execution time: 133_980_000 picoseconds.
		Weight::from_parts(137_645_000, 5128)
			.saturating_add(ParityDbWeight::get().reads(6_u64))
			.saturating_add(ParityDbWeight::get().writes(12_u64))
	}
	/// Storage: `Funding::PendingRedemptions` (r:1 w:1)
	/// Proof: `Funding::PendingRedemptions` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Flip::PendingRedemptionsReserve` (r:1 w:1)
	/// Proof: `Flip::PendingRedemptionsReserve` (`max_values`: None, `max_size`: Some(64), added: 2539, mode: `MaxEncodedLen`)
	/// Storage: `Flip::Account` (r:1 w:1)
	/// Proof: `Flip::Account` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `Flip::TotalIssuance` (r:1 w:1)
	/// Proof: `Flip::TotalIssuance` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `Validator::CurrentAuthorities` (r:1 w:0)
	/// Proof: `Validator::CurrentAuthorities` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Validator::Backups` (r:1 w:1)
	/// Proof: `Validator::Backups` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RestrictedAddresses` (r:1 w:0)
	/// Proof: `Funding::RestrictedAddresses` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn redemption_expired() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1429`
		//  Estimated: `4894`
		// Minimum execution time: 50_162_000 picoseconds.
		Weight::from_parts(50_897_000, 4894)
			.saturating_add(ParityDbWeight::get().reads(7_u64))
			.saturating_add(ParityDbWeight::get().writes(5_u64))
	}
	/// Storage: `Funding::RedemptionTax` (r:1 w:0)
	/// Proof: `Funding::RedemptionTax` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::MinimumFunding` (r:0 w:1)
	/// Proof: `Funding::MinimumFunding` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	fn update_minimum_funding() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `143`
		//  Estimated: `1628`
		// Minimum execution time: 9_830_000 picoseconds.
		Weight::from_parts(10_226_000, 1628)
			.saturating_add(ParityDbWeight::get().reads(1_u64))
			.saturating_add(ParityDbWeight::get().writes(1_u64))
	}
	/// Storage: `Funding::MinimumFunding` (r:1 w:0)
	/// Proof: `Funding::MinimumFunding` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RedemptionTax` (r:0 w:1)
	/// Proof: `Funding::RedemptionTax` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	fn update_redemption_tax() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `143`
		//  Estimated: `1628`
		// Minimum execution time: 10_475_000 picoseconds.
		Weight::from_parts(11_170_000, 1628)
			.saturating_add(ParityDbWeight::get().reads(1_u64))
			.saturating_add(ParityDbWeight::get().writes(1_u64))
	}
	/// Storage: `Funding::BoundRedeemAddress` (r:1 w:1)
	/// Proof: `Funding::BoundRedeemAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn bind_redeem_address() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `140`
		//  Estimated: `3605`
		// Minimum execution time: 12_868_000 picoseconds.
		Weight::from_parts(13_414_000, 3605)
			.saturating_add(ParityDbWeight::get().reads(1_u64))
			.saturating_add(ParityDbWeight::get().writes(1_u64))
	}
	/// Storage: `Funding::RestrictedBalances` (r:101 w:100)
	/// Proof: `Funding::RestrictedBalances` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Funding::RestrictedAddresses` (r:0 w:1)
	/// Proof: `Funding::RestrictedAddresses` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// The range of component `a` is `[1, 100]`.
	/// The range of component `b` is `[1, 100]`.
	/// The range of component `c` is `[1, 100]`.
	fn update_restricted_addresses(_a: u32, b: u32, c: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `170 + c * (92 ±0)`
		//  Estimated: `3633 + b * (9 ±1) + c * (2567 ±1)`
		// Minimum execution time: 338_355_000 picoseconds.
		Weight::from_parts(339_065_000, 3633)
			// Standard Error: 5_053_622
			.saturating_add(Weight::from_parts(196_472_300, 0).saturating_mul(b.into()))
			// Standard Error: 5_053_622
			.saturating_add(Weight::from_parts(191_697_168, 0).saturating_mul(c.into()))
			.saturating_add(ParityDbWeight::get().reads((1_u64).saturating_mul(c.into())))
			.saturating_add(ParityDbWeight::get().writes(1_u64))
			.saturating_add(ParityDbWeight::get().writes((1_u64).saturating_mul(c.into())))
			.saturating_add(Weight::from_parts(0, 9).saturating_mul(b.into()))
			.saturating_add(Weight::from_parts(0, 2567).saturating_mul(c.into()))
	}
	/// Storage: `Funding::BoundExecutorAddress` (r:1 w:1)
	/// Proof: `Funding::BoundExecutorAddress` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn bind_executor_address() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `140`
		//  Estimated: `3605`
		// Minimum execution time: 13_287_000 picoseconds.
		Weight::from_parts(13_695_000, 3605)
			.saturating_add(ParityDbWeight::get().reads(1_u64))
			.saturating_add(ParityDbWeight::get().writes(1_u64))
	}
	fn rebalance() -> Weight {
		Weight::from_parts(0, 0)
	}
}
