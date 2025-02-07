use std::collections::BTreeMap;

pub type StateTree<K, V> = BTreeMap<Vec<K>, V>;

pub enum NodeDiff<V, W> {
	Left(V),
	Right(W),
	Both(V, W),
}

impl<V, W> NodeDiff<V, W> {
	pub fn get_left(&self) -> Option<&W> {
		match self {
			Left(_) => None,
			Right(a) => Some(a),
			Both(_, a) => Some(a),
		}
	}

	pub fn get_right(&self) -> Option<&W> {
		match self {
			Left(_) => None,
			Right(a) => Some(a),
			Both(_, a) => Some(a),
		}
	}
}

use NodeDiff::*;

pub fn diff<K: Ord, V, W>(a: StateTree<K, V>, b: StateTree<K, W>) -> StateTree<K, NodeDiff<V, W>> {
	zip_with(a, b, |v, w| match (v, w) {
		(None, None) => None,
		(None, Some(w)) => Some(Right(w)),
		(Some(v), None) => Some(Left(v)),
		(Some(v), Some(w)) => Some(Both(v, w)),
	})
}
pub fn fmap<K: Ord, V, W>(this: BTreeMap<K, V>, f: &impl Fn(V) -> W) -> BTreeMap<K, W> {
	this.into_iter().map(|(k, v)| (k, f(v))).collect()
}

// TODO! This has currently a hardcoded 10!
pub fn map_with_parent<K: Ord, V, W>(
	mut this: StateTree<K, V>,
	f: impl Fn(&Vec<K>, Option<&W>, V) -> W,
) -> StateTree<K, W> {
	let mut processed = BTreeMap::new();
	for length in 0..10 {
		for (key, value) in this.extract_if(|k, _| k.len() == length) {
			let p;
			if key.len() > 0 {
				let parent_key = &key[0..key.len() - 1];
				p = processed.get(parent_key);
			} else {
				p = None;
			}
			let v = f(&key, p, value);
			processed.insert(key, v);
		}
	}
	processed
}

pub fn get_key_name<'a, K: std::fmt::Display>(key: &'a Vec<K>) -> String {
	key.last().map(|x| format!("{x}")).unwrap_or("root".into())
}


fn zip_with<K: Ord, V, W, X>(
	x: BTreeMap<K, V>,
	mut y: BTreeMap<K, W>,
	f: impl Fn(Option<V>, Option<W>) -> Option<X>,
) -> BTreeMap<K, X> {
	let mut result = BTreeMap::new();
	for (k, v) in x.into_iter() {
		if let Some(x) = f(Some(v), y.remove(&k)) {
			result.insert(k, x);
		}
	}
	for (k, w) in y.into_iter() {
		if let Some(x) = f(None, Some(w)) {
			result.insert(k, x);
		}
	}
	result
}
