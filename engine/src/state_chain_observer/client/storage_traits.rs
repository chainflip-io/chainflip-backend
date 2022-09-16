use codec::FullCodec;
use frame_support::storage::generator::StorageMap as StorageMapTrait;
use frame_support::{
    storage::types::{QueryKindTrait, StorageDoubleMap, StorageMap, StorageValue},
    traits::{Get, StorageInstance},
    ReversibleStorageHasher, StorageHasher,
};
use sp_core::storage::StorageKey;

// A method to safely extract type information about Substrate storage maps (As the Key and Value types are not available)
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

// A method to safely extract type information about Substrate storage maps (As the Key and Value types are not available)
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
        StorageKey(Self::prefix_hash())
    }

    fn key_from_storage_key(storage_key: &StorageKey) -> Self::Key {
        // This is effectively how the StorageMapPrefixIterator in substrate works
        // TODO: PR to substrate
        let raw_key_without_prefix = &storage_key.0[Self::prefix_hash().len()..];
        let reversed_bytes = Hasher::reverse(raw_key_without_prefix);
        Self::Key::decode(&mut &reversed_bytes[..]).unwrap()
    }
}

// A method to safely extract type information about Substrate storage values (As the Key and Value types are not available)
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
        assert!(test_map_storage_key_and_back::<BlakeStorageMap, _>(
            H256::from([0x1; 32])
        ));

        assert!(test_map_storage_key_and_back::<TwoxStorageMap, _>(42));

        assert!(test_map_storage_key_and_back::<IdentityStorageMap, _>(
            Asset::Eth
        ));
    }
}
