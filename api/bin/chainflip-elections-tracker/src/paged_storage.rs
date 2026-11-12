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

use std::collections::BTreeMap;

use codec::Decode;
use engine_sc_client::{
	base_rpc_api::BaseRpcApi,
	storage_api::{StorageDoubleMapAssociatedTypes, StorageMapAssociatedTypes},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sp_core::storage::{StorageData, StorageKey};
use state_chain_runtime::Hash;

const PAGE_SIZE: u32 = 256;

#[derive(Deserialize)]
struct StorageChanges {
	block: Hash,
	changes: Vec<(StorageKey, Option<StorageData>)>,
}

async fn request<T: DeserializeOwned>(
	rpc: &(impl BaseRpcApi + Sync),
	method: &str,
	params: impl Serialize,
) -> Result<T, String> {
	let params = serde_json::value::to_raw_value(&params).map_err(|e| e.to_string())?;
	let response = rpc.request_raw(method, Some(params)).await.map_err(|e| e.to_string())?;
	serde_json::from_str(response.get()).map_err(|e| format!("Invalid {method} response: {e}"))
}

async fn storage_pairs(
	rpc: &(impl BaseRpcApi + Sync),
	block_hash: Hash,
	prefix: StorageKey,
) -> Result<Vec<(StorageKey, StorageData)>, String> {
	let mut pairs = Vec::new();
	let mut start_key: Option<StorageKey> = None;
	loop {
		// Pin keys and values to the same block so new heads cannot change a scan mid-page.
		let keys: Vec<StorageKey> =
			request(rpc, "state_getKeysPaged", (&prefix, PAGE_SIZE, &start_key, block_hash))
				.await?;
		let Some(last_key) = keys.last() else { break };
		if start_key.as_ref().is_some_and(|start| last_key <= start) {
			return Err("Storage pagination did not advance".to_string());
		}
		start_key = Some(last_key.clone());
		let changes: Vec<StorageChanges> =
			request(rpc, "state_queryStorageAt", (&keys, block_hash)).await?;
		for change_set in changes {
			if change_set.block != block_hash {
				return Err("Storage values returned for a different block".to_string());
			}
			for (key, value) in change_set.changes {
				let value = value.ok_or_else(|| format!("Missing storage value for {key:?}"))?;
				pairs.push((key, value));
			}
		}
	}
	Ok(pairs)
}

pub async fn storage_map<SM: StorageMapAssociatedTypes>(
	rpc: &(impl BaseRpcApi + Sync),
	block_hash: Hash,
) -> Result<BTreeMap<SM::Key, SM::Value>, String>
where
	SM::Key: Ord,
{
	storage_pairs(rpc, block_hash, SM::_prefix_hash())
		.await?
		.into_iter()
		.map(|(key, data)| {
			let value = SM::Value::decode(&mut &data.0[..])
				.map_err(|e| format!("Invalid storage value: {e}"))?;
			Ok((SM::key_from_storage_key(&key), value))
		})
		.collect()
}

type DoubleMapEntry<SM> = (
	(<SM as StorageDoubleMapAssociatedTypes>::Key1, <SM as StorageDoubleMapAssociatedTypes>::Key2),
	<SM as StorageDoubleMapAssociatedTypes>::Value,
);

pub async fn storage_double_map<SM: StorageDoubleMapAssociatedTypes>(
	rpc: &(impl BaseRpcApi + Sync),
	block_hash: Hash,
) -> Result<Vec<DoubleMapEntry<SM>>, String> {
	storage_pairs(rpc, block_hash, SM::_prefix_hash())
		.await?
		.into_iter()
		.map(|(key, data)| {
			let value = SM::Value::decode(&mut &data.0[..])
				.map_err(|e| format!("Invalid storage value: {e}"))?;
			Ok((SM::keys_from_storage_key(&key), value))
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use engine_sc_client::base_rpc_api::MockBaseRpcApi;
	use pallet_cf_elections::{
		ElectionIdentifier,
		electoral_systems::composite::tuple_6_impls::{
			CompositeElectionIdentifierExtra, CompositeElectionProperties,
		},
		vote_storage::composite::tuple_6_impls::{
			CompositeIndividualComponent, CompositeVoteProperties,
		},
	};
	use serde_json::{Value, json};
	use state_chain_runtime::{BscInstance, Runtime};

	fn mock_rpc(responses: Vec<(&'static str, Value, Value)>) -> MockBaseRpcApi {
		let mut rpc = MockBaseRpcApi::new();
		let count = responses.len();
		let mut responses = responses.into_iter();
		rpc.expect_request_raw().times(count).returning(move |method, params| {
			let (expected_method, expected_params, response) = responses.next().unwrap();
			assert_eq!(method, expected_method);
			assert_eq!(
				serde_json::from_str::<Value>(params.unwrap().get()).unwrap(),
				expected_params
			);
			Ok(serde_json::value::to_raw_value(&response).unwrap())
		});
		rpc
	}

	fn single_page_rpc(prefix: StorageKey, key: StorageKey, data: Vec<u8>) -> MockBaseRpcApi {
		let block = Hash::repeat_byte(42);
		mock_rpc(vec![
			("state_getKeysPaged", json!([prefix, PAGE_SIZE, null, block]), json!([key])),
			(
				"state_queryStorageAt",
				json!([[key], block]),
				json!([{
					"block": block, "changes": [[key, StorageData(data)]],
				}]),
			),
			("state_getKeysPaged", json!([prefix, PAGE_SIZE, key, block]), json!([])),
		])
	}

	#[tokio::test]
	async fn reads_every_page_at_the_same_block_using_safe_methods() {
		let block = Hash::repeat_byte(42);
		let prefix = StorageKey(vec![1]);
		let keys = [StorageKey(vec![1, 1]), StorageKey(vec![1, 2]), StorageKey(vec![1, 3])];
		let values = [StorageData(vec![10]), StorageData(vec![20]), StorageData(vec![30])];
		let rpc = mock_rpc(vec![
			(
				"state_getKeysPaged",
				json!([prefix, PAGE_SIZE, null, block]),
				json!([keys[0], keys[1]]),
			),
			(
				"state_queryStorageAt",
				json!([[keys[0], keys[1]], block]),
				json!([{
					"block": block, "changes": [[keys[0], values[0]], [keys[1], values[1]]],
				}]),
			),
			("state_getKeysPaged", json!([prefix, PAGE_SIZE, keys[1], block]), json!([keys[2]])),
			(
				"state_queryStorageAt",
				json!([[keys[2]], block]),
				json!([{
					"block": block, "changes": [[keys[2], values[2]]],
				}]),
			),
			("state_getKeysPaged", json!([prefix, PAGE_SIZE, keys[2], block]), json!([])),
		]);
		assert_eq!(
			storage_pairs(&rpc, block, prefix).await.unwrap(),
			keys.into_iter().zip(values).collect::<Vec<_>>()
		);
	}

	#[tokio::test]
	async fn rejects_a_pagination_cursor_that_does_not_advance() {
		let block = Hash::repeat_byte(42);
		let key = StorageKey(vec![1, 1]);
		let prefix = StorageKey(vec![1]);
		let rpc = mock_rpc(vec![
			("state_getKeysPaged", json!([prefix, PAGE_SIZE, null, block]), json!([key])),
			(
				"state_queryStorageAt",
				json!([[key], block]),
				json!([{
					"block": block, "changes": [[key, StorageData(vec![10])]],
				}]),
			),
			("state_getKeysPaged", json!([prefix, PAGE_SIZE, key, block]), json!([key])),
		]);
		assert_eq!(
			storage_pairs(&rpc, block, prefix).await.unwrap_err(),
			"Storage pagination did not advance"
		);
	}

	#[tokio::test]
	async fn decodes_election_maps_and_individual_vote_double_maps() {
		type Properties = pallet_cf_elections::ElectionProperties<Runtime, BscInstance>;
		type Individuals = pallet_cf_elections::IndividualComponents<Runtime, BscInstance>;
		let block = Hash::repeat_byte(42);
		let election_id =
			ElectionIdentifier::new(1.into(), CompositeElectionIdentifierExtra::FF(()));
		let properties = CompositeElectionProperties::FF(123);
		let rpc = single_page_rpc(
			Properties::_prefix_hash(),
			Properties::_hashed_key_for(&election_id),
			properties.encode(),
		);
		assert_eq!(
			storage_map::<Properties>(&rpc, block).await.unwrap(),
			BTreeMap::from([(election_id, properties)])
		);

		let validator = [7; 32].into();
		let individual = (CompositeVoteProperties::FF(()), CompositeIndividualComponent::FF(()));
		let rpc = single_page_rpc(
			Individuals::_prefix_hash(),
			Individuals::_hashed_key_for(election_id.unique_monotonic(), &validator),
			individual.encode(),
		);
		assert_eq!(
			storage_double_map::<Individuals>(&rpc, block).await.unwrap(),
			vec![((*election_id.unique_monotonic(), validator), individual)]
		);
	}
}
