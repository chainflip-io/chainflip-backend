use std::collections::BTreeMap;

pub type StateTree<K, V> = BTreeMap<Vec<K>, V>;

pub enum NodeDiff<V, W> {
	Left(V),
	Right(W),
	Both(V, W),
}

use NodeDiff::*;

pub fn diff<K: Ord, V, W>(
	a: StateTree<K, V>,
	mut b: StateTree<K, W>,
) -> StateTree<K, NodeDiff<V, W>> {
	let mut result = BTreeMap::new();
	for (k, v) in a.into_iter() {
		if let Some(w) = b.remove(&k) {
			result.insert(k, Both(v, w));
		} else {
			result.insert(k, Left(v));
		}
	}
	for (k, w) in b.into_iter() {
		result.insert(k, Right(w));
	}
	result
}

pub fn fmap<K: Ord, V, W>(this: BTreeMap<K, V>, f: &impl Fn(V) -> W) -> BTreeMap<K, W> {
	this.into_iter().map(|(k, v)| (k, f(v))).collect()
}

pub fn map_with_parent<K: Ord, V, W>(
	mut this: StateTree<K, V>,
	f: impl Fn(&Vec<K>, Option<&W>, V) -> W,
) -> StateTree<K, W> {
	let max_key_length = this.keys().map(|key| key.len()).max().unwrap_or(0);
	let mut processed = BTreeMap::new();
	for length in 0..=max_key_length {
		for (key, value) in this.extract_if(|k, _| k.len() == length) {
			let p = if !key.is_empty() {
				let parent_key = &key[0..key.len() - 1];
				processed.get(parent_key)
			} else {
				None
			};
			let v = f(&key, p, value);
			processed.insert(key, v);
		}
	}
	processed
}

pub fn get_key_name<K: std::fmt::Display>(key: &[K]) -> String {
	key.last().map(|x| format!("{x}")).unwrap_or("root".into())
}
