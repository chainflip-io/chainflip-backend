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
    type Key;
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
