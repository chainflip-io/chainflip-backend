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

use anyhow::bail;

pub use cf_chains::{address::AddressString, RefundParametersRpc};
use cf_chains::{Chain, ChainCrypto, ChannelRefundParameters, ForeignChain};
use cf_primitives::AffiliateShortId;
pub use cf_primitives::{AccountRole, Affiliates, Asset, BasisPoints, ChannelId, SemVer};
use cf_utilities::rpc::NumberOrHex;
pub use pallet_cf_swapping::AffiliateDetails;
pub use sp_core::{crypto::AccountId32, H256};
use sp_core::{
	serde::{Deserialize, Serialize},
	U256,
};
pub use state_chain_runtime::{
	chainflip::BlockUpdate,
	runtime_apis::{
		ChainAccounts, ChannelActionType, CustomRuntimeApi, TransactionScreeningEvents,
		VaultAddresses, VaultSwapDetails,
	},
};
use std::fmt;

pub type RefundParameters = ChannelRefundParameters<AddressString>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SwapDepositAddress {
	pub address: AddressString,
	pub issued_block: state_chain_runtime::BlockNumber,
	pub channel_id: ChannelId,
	pub source_chain_expiry_block: NumberOrHex,
	pub channel_opening_fee: U256,
	pub refund_parameters: RefundParametersRpc,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WithdrawFeesDetail {
	pub tx_hash: H256,
	pub egress_id: (ForeignChain, u64),
	pub egress_amount: U256,
	pub egress_fee: U256,
	pub destination_address: AddressString,
}

impl fmt::Display for WithdrawFeesDetail {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(
			f,
			"\
			Tx hash: {:?}\n\
			Egress id: {:?}\n\
			Egress amount: {}\n\
			Egress fee: {}\n\
			Destination address: {}\n\
			",
			self.tx_hash,
			self.egress_id,
			self.egress_amount,
			self.egress_fee,
			self.destination_address,
		)
	}
}

pub type TransactionInIdFor<C> = <<C as Chain>::ChainCrypto as ChainCrypto>::TransactionInId;

#[derive(Serialize, Deserialize)]
pub enum TransactionInId {
	Bitcoin(TransactionInIdFor<cf_chains::Bitcoin>),
	Ethereum(TransactionInIdFor<cf_chains::Ethereum>),
	Arbitrum(TransactionInIdFor<cf_chains::Arbitrum>),
	// other variants reserved for other chains.
}

#[derive(Serialize, Deserialize)]
pub enum GetOpenDepositChannelsQuery {
	All,
	Mine,
}

pub fn find_lowest_unused_short_id(
	used_ids: &[AffiliateShortId],
) -> anyhow::Result<AffiliateShortId> {
	let used_id_len = used_ids.len();
	if used_ids.is_empty() {
		Ok(AffiliateShortId::from(0))
	} else if used_id_len > u8::MAX as usize {
		bail!("No unused affiliate short IDs available")
	} else {
		let mut used_ids = used_ids.to_vec();
		used_ids.sort_unstable();
		Ok(AffiliateShortId::from(
			used_ids
				.iter()
				.enumerate()
				.find(|(index, assigned_id)| &AffiliateShortId::from(*index as u8) != *assigned_id)
				.map(|(index, _)| index)
				.unwrap_or(used_id_len) as u8,
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_find_lowest_unused_short_id() {
		fn test_lowest(used_ids: &mut Vec<AffiliateShortId>, expected: AffiliateShortId) {
			assert_eq!(find_lowest_unused_short_id(used_ids).unwrap(), expected);
			assert_eq!(
				used_ids.iter().find(|id| *id == &expected),
				None,
				"Should not overwrite existing IDs"
			);
			used_ids.push(expected);
		}

		let mut used_ids = vec![AffiliateShortId::from(1), AffiliateShortId::from(3)];
		test_lowest(&mut used_ids, AffiliateShortId::from(0));
		test_lowest(&mut used_ids, AffiliateShortId::from(2));
		test_lowest(&mut used_ids, AffiliateShortId::from(4));
		test_lowest(&mut used_ids, AffiliateShortId::from(5));
		let mut used_ids: Vec<AffiliateShortId> =
			(0..u8::MAX).map(AffiliateShortId::from).collect();
		test_lowest(&mut used_ids, AffiliateShortId::from(255));
		assert!(find_lowest_unused_short_id(&used_ids).is_err());
	}
}
