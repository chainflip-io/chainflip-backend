use sp_std::collections::btree_map::BTreeMap;

pub trait Container {
	type With<A>;
}

impl<Key: Ord> Container for BTreeMap<Key, !> {
	type With<A> = BTreeMap<Key, A>;
}

pub trait HasKey<'k> {
	type Key: 'k;
}

impl<'k, Key: 'k> HasKey<'k> for BTreeMap<Key, !> {
	type Key = Key;
}
