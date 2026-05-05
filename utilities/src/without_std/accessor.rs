use sp_std::collections::btree_map::BTreeMap;

use crate::container::{Container, HasKey};

pub trait Accessor<'k, C: Container + HasKey<'k>> {
	fn access_key_values_from<'a, A: 'a>(
		&self,
		c: &'a C::With<A>,
	) -> impl Iterator<Item = (&'a C::Key, &'a A)>
	where
		'k: 'a;
}

pub struct SingleEntry<Key>(pub Key);

impl<'k, Key: Ord + Clone> Accessor<'k, BTreeMap<Key, !>> for SingleEntry<&'k Key> {
	fn access_key_values_from<'a, A: 'a>(
		&self,
		c: &'a BTreeMap<Key, A>,
	) -> impl Iterator<Item = (&'a Key, &'a A)>
	where
		'k: 'a,
	{
		c.get_key_value(self.0).into_iter()
	}
}

pub struct AllEntries();

impl<'k, Key: Ord + 'k> Accessor<'k, BTreeMap<Key, !>> for AllEntries {
	fn access_key_values_from<'a, A: 'a>(
		&self,
		c: &'a <BTreeMap<Key, !> as Container>::With<A>,
	) -> impl Iterator<Item = (&'a Key, &'a A)>
	where
		'k: 'a,
	{
		c.iter()
	}
}
