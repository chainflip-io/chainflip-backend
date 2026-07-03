use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

pub trait HasTypeIntrospection: Sized {
	fn is_empty_type() -> bool;
	fn sample_all_shapes() -> Vec<Self>;
}

// -------------- primitives ---------------

#[duplicate::duplicate_item(Type; [()]; [bool]; [u8]; [u16]; [u32]; [u64]; [u128])]
impl HasTypeIntrospection for Type {
	fn is_empty_type() -> bool {
		false
	}
	fn sample_all_shapes() -> Vec<Self> {
		vec![Default::default()]
	}
}

impl<A> HasTypeIntrospection for sp_std::marker::PhantomData<A> {
	fn is_empty_type() -> bool {
		false
	}

	fn sample_all_shapes() -> Vec<Self> {
		vec![Default::default()]
	}
}

// -------------- containers ---------------

impl<A: HasTypeIntrospection> HasTypeIntrospection for Option<A> {
	fn is_empty_type() -> bool {
		false // because None is always constructible
	}

	fn sample_all_shapes() -> Vec<Self> {
		A::sample_all_shapes().into_iter().map(Some).chain([None]).collect()
	}
}

impl<A: HasTypeIntrospection> HasTypeIntrospection for Vec<A> {
	fn is_empty_type() -> bool {
		false // because vec![] is always constructible
	}

	fn sample_all_shapes() -> Vec<Self> {
		A::sample_all_shapes().into_iter().map(|a| vec![a]).chain([vec![]]).collect()
	}
}

impl<A: HasTypeIntrospection + Clone, B: HasTypeIntrospection> HasTypeIntrospection for (A, B) {
	fn is_empty_type() -> bool {
		A::is_empty_type() || B::is_empty_type()
	}

	fn sample_all_shapes() -> Vec<Self> {
		A::sample_all_shapes()
			.into_iter()
			.flat_map(move |a| {
				let a = a.clone();
				B::sample_all_shapes().into_iter().map(move |b| (a.clone(), b))
			})
			.collect()
	}
}

impl<A: HasTypeIntrospection + Ord + Clone, B: HasTypeIntrospection> HasTypeIntrospection
	for BTreeMap<A, B>
{
	fn is_empty_type() -> bool {
		false // because empty map is always constructible
	}

	fn sample_all_shapes() -> Vec<Self> {
		A::sample_all_shapes()
			.into_iter()
			.flat_map(move |a| {
				let a = a.clone();
				B::sample_all_shapes().into_iter().map(move |b| (a.clone(), b))
			})
			.map(|(a, b)| BTreeMap::from_iter([(a, b)]))
			.chain([BTreeMap::new()])
			.collect()
	}
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use crate::type_introspection::HasTypeIntrospection;

	#[test]
	fn test_containers() {
		assert_eq!(Option::<bool>::sample_all_shapes(), vec![Some(false), None]);
		assert_eq!(Vec::<bool>::sample_all_shapes(), vec![vec![false], vec![]]);
		assert_eq!(
			BTreeMap::<u8, bool>::sample_all_shapes()
				.into_iter()
				.map(|map| map.into_iter().collect::<Vec<_>>())
				.collect::<Vec<_>>(),
			vec![vec![(0, false)], vec![]]
		);
	}
}
