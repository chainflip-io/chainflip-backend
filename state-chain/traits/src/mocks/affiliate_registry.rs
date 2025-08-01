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

use crate::AffiliateRegistry;
use cf_primitives::AffiliateShortId;
use frame_support::{parameter_types, sp_runtime::BoundedBTreeMap, traits::ConstU32};
use sp_std::collections::btree_map::BTreeMap;

parameter_types! {
	pub storage AffiliateMapping: BoundedBTreeMap<(u64, AffiliateShortId), u64, ConstU32<100>> = Default::default();
}

pub struct MockAffiliateRegistry {}

impl MockAffiliateRegistry {
	pub fn register_affiliate(
		broker_id: u64,
		affiliate_id: u64,
		affiliate_short_id: AffiliateShortId,
	) {
		let mapping = AffiliateMapping::get()
			.try_mutate(|mapping| {
				mapping.insert((broker_id, affiliate_short_id), affiliate_id);
			})
			.unwrap();

		AffiliateMapping::set(&mapping);
	}
}

impl AffiliateRegistry for MockAffiliateRegistry {
	type AccountId = u64;

	fn get_account_id(
		broker_id: &Self::AccountId,
		affiliate_short_id: AffiliateShortId,
	) -> Option<Self::AccountId> {
		AffiliateMapping::get().get(&(*broker_id, affiliate_short_id)).copied()
	}

	fn get_short_id(
		broker_id: &Self::AccountId,
		affiliate_id: &Self::AccountId,
	) -> Option<AffiliateShortId> {
		for ((broker, short_id), affiliate) in AffiliateMapping::get().iter() {
			if broker_id == broker && affiliate_id == affiliate {
				return Some(*short_id);
			}
		}
		None
	}

	fn reverse_mapping(broker_id: &Self::AccountId) -> BTreeMap<Self::AccountId, AffiliateShortId> {
		AffiliateMapping::get()
			.into_iter()
			.filter_map(|((map_broker_id, short_id), account_id)| {
				if *broker_id == map_broker_id {
					Some((account_id, short_id))
				} else {
					None
				}
			})
			.collect()
	}
}
