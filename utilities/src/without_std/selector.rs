use sp_std::{boxed::Box, collections::btree_map::BTreeMap};

/// Can be used to control which values are extracted from a key-value map.
///
/// Useful when e.g. processing rpc methods, where passing a parameter is
/// a request for this particular key but passing nothing is meant to return all
/// all the information for all keys.
pub enum Select<'k, Key: 'k> {
	Single(&'k Key),
	All(),
}

impl<'k, Key: Ord + 'k> Select<'k, Key> {
	pub fn with_values_from_btree_map<'a: 'k, A: 'a>(
		&self,
		container: &'a BTreeMap<Key, A>,
	) -> Box<dyn Iterator<Item = (&'a Key, &'a A)> + 'a> {
		match self {
			Select::Single(key) => Box::new(container.get_key_value(key).into_iter()),
			Select::All() => Box::new(container.iter()),
		}
	}
}
