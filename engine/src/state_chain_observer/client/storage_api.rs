use async_trait::async_trait;
use cf_primitives::SemVer;
use codec::{Decode, FullCodec};
use frame_support::{
	storage::{
		generator::StorageMap as StorageMapTrait,
		types::{QueryKindTrait, StorageDoubleMap, StorageMap, StorageValue},
	},
	traits::{Get, StorageInstance},
	ReversibleStorageHasher, StorageHasher,
};
use jsonrpsee::core::RpcResult;
use sp_core::storage::StorageKey;
use utilities::context;

use super::{CFE_VERSION, SUBSTRATE_BEHAVIOUR};

/// This trait extracts otherwise private type information about Substrate storage double maps
pub trait StorageDoubleMapAssociatedTypes {
	type Key1;
	type Key2;
	type Value: FullCodec;
	type QueryKind: QueryKindTrait<Self::Value, Self::OnEmpty>;
	type OnEmpty;

	fn _hashed_key_for(key1: &Self::Key1, key2: &Self::Key2) -> StorageKey;
}
impl<
		Prefix: StorageInstance,
		Hasher1: StorageHasher,
		Key1: FullCodec,
		Hasher2: StorageHasher,
		Key2: FullCodec,
		Value: FullCodec,
		QueryKind: QueryKindTrait<Value, OnEmpty>,
		OnEmpty: Get<QueryKind::Query> + 'static,
		MaxValues: Get<Option<u32>>,
	> StorageDoubleMapAssociatedTypes
	for StorageDoubleMap<Prefix, Hasher1, Key1, Hasher2, Key2, Value, QueryKind, OnEmpty, MaxValues>
{
	type Key1 = Key1;
	type Key2 = Key2;
	type Value = Value;
	type QueryKind = QueryKind;
	type OnEmpty = OnEmpty;

	fn _hashed_key_for(key1: &Self::Key1, key2: &Self::Key2) -> StorageKey {
		StorageKey(Self::hashed_key_for(key1, key2))
	}
}

/// This trait extracts otherwise private type information about Substrate storage maps
pub trait StorageMapAssociatedTypes {
	type Key: FullCodec;
	type Value: FullCodec;
	type QueryKind: QueryKindTrait<Self::Value, Self::OnEmpty>;
	type OnEmpty;

	fn _hashed_key_for(key: &Self::Key) -> StorageKey;

	fn _prefix_hash() -> StorageKey;

	fn key_from_storage_key(storage_key: &StorageKey) -> Self::Key;
}
impl<
		Prefix: StorageInstance,
		Hasher: ReversibleStorageHasher,
		Key: FullCodec,
		Value: FullCodec,
		QueryKind: QueryKindTrait<Value, OnEmpty>,
		OnEmpty: Get<QueryKind::Query> + 'static,
		MaxValues: Get<Option<u32>>,
	> StorageMapAssociatedTypes
	for StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
{
	type Key = Key;
	type Value = Value;
	type QueryKind = QueryKind;
	type OnEmpty = OnEmpty;

	fn _hashed_key_for(key: &Self::Key) -> StorageKey {
		StorageKey(Self::hashed_key_for(key))
	}

	fn _prefix_hash() -> StorageKey {
		StorageKey(Self::prefix_hash().to_vec())
	}

	fn key_from_storage_key(storage_key: &StorageKey) -> Self::Key {
		// This is effectively how the StorageMapPrefixIterator in substrate works
		// TODO: PR to substrate
		let raw_key_without_prefix = &storage_key.0[Self::prefix_hash().len()..];
		let reversed_bytes = Hasher::reverse(raw_key_without_prefix);
		Self::Key::decode(&mut &reversed_bytes[..]).unwrap()
	}
}

/// This trait extracts otherwise private type information about Substrate storage values
pub trait StorageValueAssociatedTypes {
	type Value: FullCodec;
	type QueryKind: QueryKindTrait<Self::Value, Self::OnEmpty>;
	type OnEmpty;

	fn _hashed_key() -> StorageKey;
}
impl<
		Prefix: StorageInstance,
		Value: FullCodec,
		QueryKind: QueryKindTrait<Value, OnEmpty>,
		OnEmpty: Get<QueryKind::Query> + 'static,
	> StorageValueAssociatedTypes for StorageValue<Prefix, Value, QueryKind, OnEmpty>
{
	type Value = Value;
	type QueryKind = QueryKind;
	type OnEmpty = OnEmpty;

	fn _hashed_key() -> StorageKey {
		StorageKey(Self::hashed_key().into())
	}
}

// Note 'static on the generics in this trait are only required for mockall to mock it
#[async_trait]
pub trait StorageApi {
	async fn storage_item<
		Value: codec::FullCodec + 'static,
		OnEmpty: 'static,
		QueryKind: QueryKindTrait<Value, OnEmpty> + 'static,
	>(
		&self,
		storage_key: StorageKey,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<<QueryKind as QueryKindTrait<Value, OnEmpty>>::Query>;

	async fn storage_value<StorageValue: StorageValueAssociatedTypes + 'static>(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<<StorageValue::QueryKind as QueryKindTrait<StorageValue::Value, StorageValue::OnEmpty>>::Query>;

	async fn storage_map_entry<StorageMap: StorageMapAssociatedTypes + 'static>(
		&self,
		block_hash: state_chain_runtime::Hash,
		key: &StorageMap::Key,
	) -> RpcResult<
		<StorageMap::QueryKind as QueryKindTrait<StorageMap::Value, StorageMap::OnEmpty>>::Query,
	>
	where
		StorageMap::Key: Sync;

	async fn storage_double_map_entry<StorageDoubleMap: StorageDoubleMapAssociatedTypes + 'static>(
		&self,
		block_hash: state_chain_runtime::Hash,
		key1: &StorageDoubleMap::Key1,
		key2: &StorageDoubleMap::Key2,
	) -> RpcResult<
		<StorageDoubleMap::QueryKind as QueryKindTrait<
			StorageDoubleMap::Value,
			StorageDoubleMap::OnEmpty,
		>>::Query,
	>
	where
		StorageDoubleMap::Key1: Sync,
		StorageDoubleMap::Key2: Sync;

	async fn storage_map<
		StorageMap: StorageMapAssociatedTypes + 'static,
		ReturnedIter: FromIterator<(<StorageMap as StorageMapAssociatedTypes>::Key, StorageMap::Value)> + 'static,
	>(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<ReturnedIter>;

	async fn storage_map_values<StorageMap: StorageMapAssociatedTypes + 'static>(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<Vec<StorageMap::Value>> {
		Ok(self
			.storage_map::<StorageMap, Vec<_>>(block_hash)
			.await?
			.into_iter()
			.map(|(_k, v)| v)
			.collect())
	}
}

#[async_trait]
impl<BaseRpcApi: super::base_rpc_api::BaseRpcApi + Send + Sync + 'static> StorageApi
	for BaseRpcApi
{
	#[track_caller]
	async fn storage_item<
		Value: codec::FullCodec + 'static,
		OnEmpty: 'static,
		QueryKind: QueryKindTrait<Value, OnEmpty> + 'static,
	>(
		&self,
		storage_key: StorageKey,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<<QueryKind as QueryKindTrait<Value, OnEmpty>>::Query> {
		Ok(QueryKind::from_optional_value_to_query(
			self.storage(block_hash, storage_key.clone())
				.await?
				.map(|data| context!(Value::decode(&mut &data.0[..])).expect(SUBSTRATE_BEHAVIOUR)),
		))
	}

	#[track_caller]
	async fn storage_value<StorageValue: StorageValueAssociatedTypes + 'static>(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<<StorageValue::QueryKind as QueryKindTrait<StorageValue::Value, StorageValue::OnEmpty>>::Query>{
		self.storage_item::<StorageValue::Value, StorageValue::OnEmpty, StorageValue::QueryKind>(
			StorageValue::_hashed_key(),
			block_hash,
		)
		.await
	}

	#[track_caller]
	async fn storage_map_entry<StorageMap: StorageMapAssociatedTypes + 'static>(
		&self,
		block_hash: state_chain_runtime::Hash,
		key: &StorageMap::Key,
	) -> RpcResult<
		<StorageMap::QueryKind as QueryKindTrait<StorageMap::Value, StorageMap::OnEmpty>>::Query,
	>
	where
		StorageMap::Key: Sync,
	{
		self.storage_item::<StorageMap::Value, StorageMap::OnEmpty, StorageMap::QueryKind>(
			StorageMap::_hashed_key_for(key),
			block_hash,
		)
		.await
	}

	#[track_caller]
	async fn storage_double_map_entry<StorageDoubleMap: StorageDoubleMapAssociatedTypes + 'static>(
		&self,
		block_hash: state_chain_runtime::Hash,
		key1: &StorageDoubleMap::Key1,
		key2: &StorageDoubleMap::Key2,
	) -> RpcResult<
		<StorageDoubleMap::QueryKind as QueryKindTrait<
			StorageDoubleMap::Value,
			StorageDoubleMap::OnEmpty,
		>>::Query,
	>
	where
		StorageDoubleMap::Key1: Sync,
		StorageDoubleMap::Key2: Sync,
	{
		self.storage_item::<StorageDoubleMap::Value, StorageDoubleMap::OnEmpty, StorageDoubleMap::QueryKind>(StorageDoubleMap::_hashed_key_for(key1, key2), block_hash).await
	}

	/// Gets all the storage pairs (key, value) of a StorageMap.
	/// NB: Because this is an unbounded operation, it requires the node to have
	/// the `--rpc-methods=unsafe` enabled.
	#[track_caller]
	async fn storage_map<
		StorageMap: StorageMapAssociatedTypes + 'static,
		ReturnedIter: FromIterator<(<StorageMap as StorageMapAssociatedTypes>::Key, StorageMap::Value)>,
	>(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<ReturnedIter> {
		Ok(self
			.storage_pairs(block_hash, StorageMap::_prefix_hash())
			.await?
			.into_iter()
			.map(|(storage_key, storage_data)| {
				(
					StorageMap::key_from_storage_key(&storage_key),
					context!(StorageMap::Value::decode(&mut &storage_data.0[..]))
						.expect(SUBSTRATE_BEHAVIOUR),
				)
			})
			.collect())
	}
}

#[async_trait]
impl<
		BaseRpcApi: super::base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: Send + Sync + 'static,
	> StorageApi for super::StateChainClient<SignedExtrinsicClient, BaseRpcApi>
{
	#[track_caller]
	async fn storage_item<
		Value: codec::FullCodec + 'static,
		OnEmpty: 'static,
		QueryKind: QueryKindTrait<Value, OnEmpty> + 'static,
	>(
		&self,
		storage_key: StorageKey,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<<QueryKind as QueryKindTrait<Value, OnEmpty>>::Query> {
		self.base_rpc_client
			.storage_item::<Value, OnEmpty, QueryKind>(storage_key, block_hash)
			.await
	}

	#[track_caller]
	async fn storage_value<StorageValue: StorageValueAssociatedTypes + 'static>(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<<StorageValue::QueryKind as QueryKindTrait<StorageValue::Value, StorageValue::OnEmpty>>::Query>{
		self.base_rpc_client.storage_value::<StorageValue>(block_hash).await
	}

	#[track_caller]
	async fn storage_map_entry<StorageMap: StorageMapAssociatedTypes + 'static>(
		&self,
		block_hash: state_chain_runtime::Hash,
		key: &StorageMap::Key,
	) -> RpcResult<
		<StorageMap::QueryKind as QueryKindTrait<StorageMap::Value, StorageMap::OnEmpty>>::Query,
	>
	where
		StorageMap::Key: Sync,
	{
		self.base_rpc_client.storage_map_entry::<StorageMap>(block_hash, key).await
	}

	#[track_caller]
	async fn storage_double_map_entry<StorageDoubleMap: StorageDoubleMapAssociatedTypes + 'static>(
		&self,
		block_hash: state_chain_runtime::Hash,
		key1: &StorageDoubleMap::Key1,
		key2: &StorageDoubleMap::Key2,
	) -> RpcResult<
		<StorageDoubleMap::QueryKind as QueryKindTrait<
			StorageDoubleMap::Value,
			StorageDoubleMap::OnEmpty,
		>>::Query,
	>
	where
		StorageDoubleMap::Key1: Sync,
		StorageDoubleMap::Key2: Sync,
	{
		self.base_rpc_client
			.storage_double_map_entry::<StorageDoubleMap>(block_hash, key1, key2)
			.await
	}

	#[track_caller]
	async fn storage_map<
		StorageMap: StorageMapAssociatedTypes + 'static,
		ReturnedIter: FromIterator<(<StorageMap as StorageMapAssociatedTypes>::Key, StorageMap::Value)> + 'static,
	>(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<ReturnedIter> {
		self.base_rpc_client.storage_map::<StorageMap, _>(block_hash).await
	}
}

#[async_trait]
pub trait CheckBlockCompatibility: StorageApi {
	async fn check_block_compatibility(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<Result<(), SemVer>> {
		let block_version = self
			.storage_value::<pallet_cf_environment::CurrentReleaseVersion<state_chain_runtime::Runtime>>(
				block_hash,
			)
			.await?;
		Ok(if CFE_VERSION.is_compatible_with(block_version) { Ok(()) } else { Err(block_version) })
	}
}
#[async_trait]
impl<T: StorageApi + Send + Sync + 'static> CheckBlockCompatibility for T {}

#[cfg(test)]
mod tests {

	use cf_primitives::Asset;
	use frame_support::{storage_alias, Blake2_128Concat, Identity, Twox64Concat};
	use sp_core::H256;

	use super::*;

	fn test_map_storage_key_and_back<
		StorageMap: StorageMapAssociatedTypes<Key = K>,
		K: PartialEq,
	>(
		key: K,
	) -> bool {
		let storage_key = StorageMap::_hashed_key_for(&key);

		let key_from_storage_key = StorageMap::key_from_storage_key(&storage_key);
		// encode so we don't need PartialEq
		key == key_from_storage_key
	}

	#[storage_alias]
	type BlakeStorageMap = StorageMap<Test, Blake2_128Concat, H256, ()>;

	#[storage_alias]
	type TwoxStorageMap = StorageMap<Test, Twox64Concat, u32, ()>;

	#[storage_alias]
	type IdentityStorageMap = StorageMap<Test, Identity, Asset, ()>;

	#[test]
	fn test_fake_storage_keys() {
		// Blake2_128Concat
		assert!(test_map_storage_key_and_back::<BlakeStorageMap, _>(H256::from([0x1; 32])));

		assert!(test_map_storage_key_and_back::<TwoxStorageMap, _>(42));

		assert!(test_map_storage_key_and_back::<IdentityStorageMap, _>(Asset::Eth));
	}
}
