use std::collections::BTreeMap;

#[derive(PartialEq, Eq, Debug)]
pub struct BTreeMultiSet<A>(pub BTreeMap<A, usize>);

impl<A> Default for BTreeMultiSet<A> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<A: Ord> BTreeMultiSet<A> {
	pub fn insert(&mut self, a: A) {
		*self.0.entry(a).or_insert(0) += 1;
	}
}

impl<A: Ord> FromIterator<A> for BTreeMultiSet<A> {
	fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
		let mut result = Self::default();
		for x in iter {
			result.insert(x);
		}
		result
	}
}
