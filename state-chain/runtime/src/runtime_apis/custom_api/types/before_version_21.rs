// Copyright 2026 Chainflip Labs GmbH
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

/// The runtime offence shape before failed broadcasts became chain-specific.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Serialize, Deserialize)]
pub enum Offence {
	ParticipateSigningFailed,
	ParticipateKeygenFailed,
	FailedToBroadcastTransaction,
	MissedAuthorshipSlot,
	MissedHeartbeat,
	GrandpaEquivocation,
	ParticipateKeyHandoverFailed,
	FailedToWitnessInTime,
	FailedLivenessCheck(ForeignChain),
}

impl From<super::Offence> for Offence {
	fn from(offence: super::Offence) -> Self {
		match offence {
			super::Offence::ParticipateSigningFailed => Self::ParticipateSigningFailed,
			super::Offence::ParticipateKeygenFailed => Self::ParticipateKeygenFailed,
			super::Offence::FailedToBroadcastTransaction(_) => Self::FailedToBroadcastTransaction,
			super::Offence::MissedAuthorshipSlot => Self::MissedAuthorshipSlot,
			super::Offence::MissedHeartbeat => Self::MissedHeartbeat,
			super::Offence::GrandpaEquivocation => Self::GrandpaEquivocation,
			super::Offence::ParticipateKeyHandoverFailed => Self::ParticipateKeyHandoverFailed,
			super::Offence::FailedToWitnessInTime => Self::FailedToWitnessInTime,
			super::Offence::FailedLivenessCheck(chain) => Self::FailedLivenessCheck(chain),
		}
	}
}

impl Offence {
	fn into_current(self) -> super::Offence {
		match self {
			Self::ParticipateSigningFailed => super::Offence::ParticipateSigningFailed,
			Self::ParticipateKeygenFailed => super::Offence::ParticipateKeygenFailed,
			// The legacy offence did not identify its chain. Ethereum is only a placeholder when
			// presenting historical data through the current RPC type.
			Self::FailedToBroadcastTransaction =>
				super::Offence::FailedToBroadcastTransaction(ForeignChain::Ethereum),
			Self::MissedAuthorshipSlot => super::Offence::MissedAuthorshipSlot,
			Self::MissedHeartbeat => super::Offence::MissedHeartbeat,
			Self::GrandpaEquivocation => super::Offence::GrandpaEquivocation,
			Self::ParticipateKeyHandoverFailed => super::Offence::ParticipateKeyHandoverFailed,
			Self::FailedToWitnessInTime => super::Offence::FailedToWitnessInTime,
			Self::FailedLivenessCheck(chain) => super::Offence::FailedLivenessCheck(chain),
		}
	}
}

pub fn into_current_offences<T>(entries: Vec<(Offence, T)>) -> Vec<(super::Offence, T)> {
	entries
		.into_iter()
		.map(|(offence, value)| (offence.into_current(), value))
		.collect()
}

// Decode-only intermediate (converted to the current type via `From`); no serde needed.
//
// A delegator used to be able to delegate to at most one operator at a time, so
// `current_delegation_status`/`upcoming_delegation_status` were a single optional
// `DelegationInfo { operator, bid }` rather than a map of every operator it delegates to.
#[derive(Encode, Decode, TypeInfo, Clone, Default, Debug)]
pub struct RpcAccountInfoCommonItems<Balance> {
	pub account_id: Option<AccountId32>,
	pub vanity_name: VanityName,
	pub flip_balance: Balance,
	pub asset_balances: cf_chains::assets::any::AssetMap<Balance>,
	pub bond: Balance,
	pub estimated_redeemable_balance: Balance,
	pub bound_redeem_address: Option<EvmAddress>,
	pub restricted_balances: BTreeMap<EvmAddress, Balance>,
	pub current_delegation_status: Option<DelegationInfo<Balance>>,
	pub upcoming_delegation_status: Option<DelegationInfo<Balance>>,
}

impl<B: Default> From<RpcAccountInfoCommonItems<B>> for super::RpcAccountInfoCommonItems<B> {
	fn from(value: RpcAccountInfoCommonItems<B>) -> Self {
		Self {
			account_id: value.account_id,
			vanity_name: value.vanity_name,
			flip_balance: value.flip_balance,
			asset_balances: value.asset_balances,
			bond: value.bond,
			estimated_redeemable_balance: value.estimated_redeemable_balance,
			bound_redeem_address: value.bound_redeem_address,
			restricted_balances: value.restricted_balances,
			current_delegation_status: value
				.current_delegation_status
				.into_iter()
				.map(|d| (d.operator, d.bid))
				.collect(),
			upcoming_delegation_status: value
				.upcoming_delegation_status
				.into_iter()
				.map(|d| (d.operator, d.bid))
				.collect(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn legacy_offences_map_one_to_one_with_ethereum_as_placeholder() {
		assert_eq!(
			into_current_offences(sp_std::vec![
				(Offence::FailedToBroadcastTransaction, 1),
				(Offence::MissedHeartbeat, 2),
			]),
			sp_std::vec![
				(
					crate::chainflip::Offence::FailedToBroadcastTransaction(ForeignChain::Ethereum),
					1,
				),
				(crate::chainflip::Offence::MissedHeartbeat, 2),
			],
		);
	}
}
