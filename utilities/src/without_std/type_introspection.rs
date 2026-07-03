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

#[cfg(test)]
mod derive_macro_tests {
	#![allow(unused)]

	use crate::{never::Never, type_introspection::HasTypeIntrospection};

	// --------------- is_empty_type ----------------
	#[test]
	fn test_is_empty_type() {
		#[derive(cf_proc_macros::HasTypeIntrospection)]
		enum NotEmpty0 {
			Var0 { a: bool },
		}

		#[derive(cf_proc_macros::HasTypeIntrospection)]
		enum NotEmpty1 {
			Var0,
		}

		#[derive(cf_proc_macros::HasTypeIntrospection)]
		enum NotEmpty2 {
			Other { a: u16 },
			MyBranch { a: u8, b: Never },
		}

		assert_eq!(NotEmpty0::is_empty_type(), false);
		assert_eq!(NotEmpty1::is_empty_type(), false);
		assert_eq!(NotEmpty2::is_empty_type(), false);

		#[derive(cf_proc_macros::HasTypeIntrospection)]
		enum Empty0 {}

		#[derive(cf_proc_macros::HasTypeIntrospection)]
		enum Empty1 {
			Var0 { a: Never },
		}

		#[derive(cf_proc_macros::HasTypeIntrospection)]
		enum Empty2 {
			Var0 { a: Never },
			Var1 { a: u8, b: Never },
		}

		assert_eq!(Empty0::is_empty_type(), true);
		assert_eq!(Empty1::is_empty_type(), true);
		assert_eq!(Empty2::is_empty_type(), true);
	}

	// --------------- sample_all_shapes ----------------

	#[derive(PartialEq, Debug, cf_proc_macros::HasTypeIntrospection)]
	enum Empty {}

	#[derive(PartialEq, Debug, cf_proc_macros::HasTypeIntrospection)]
	struct Singleton {}

	#[derive(PartialEq, Debug, cf_proc_macros::HasTypeIntrospection)]
	enum BinaryShape {
		First,
		Second,
	}

	#[derive(PartialEq, Debug, cf_proc_macros::HasTypeIntrospection)]
	enum TernaryShape {
		First,
		Second,
		Third,
	}
	#[derive(PartialEq, Debug, cf_proc_macros::HasTypeIntrospection)]
	struct NamedProduct {
		binary: BinaryShape,
		ternary: TernaryShape,
	}

	#[derive(PartialEq, Debug, cf_proc_macros::HasTypeIntrospection)]
	struct TupleProduct(BinaryShape, TernaryShape);

	#[derive(PartialEq, Debug, cf_proc_macros::HasTypeIntrospection)]
	enum SumShape {
		Unit,
		Tuple(BinaryShape, TernaryShape),
		Named { ternary: TernaryShape },
		EmptyTuple(BinaryShape, Never),
		EmptyNamed { never: Never },
	}

	#[test]
	fn structs_sample_the_cartesian_product_of_field_shapes() {
		assert_eq!(Singleton::sample_all_shapes(), vec![Singleton {}]);
		assert_eq!(
			NamedProduct::sample_all_shapes(),
			vec![
				NamedProduct { binary: BinaryShape::First, ternary: TernaryShape::First },
				NamedProduct { binary: BinaryShape::First, ternary: TernaryShape::Second },
				NamedProduct { binary: BinaryShape::First, ternary: TernaryShape::Third },
				NamedProduct { binary: BinaryShape::Second, ternary: TernaryShape::First },
				NamedProduct { binary: BinaryShape::Second, ternary: TernaryShape::Second },
				NamedProduct { binary: BinaryShape::Second, ternary: TernaryShape::Third },
			],
		);

		assert_eq!(
			TupleProduct::sample_all_shapes(),
			vec![
				TupleProduct(BinaryShape::First, TernaryShape::First),
				TupleProduct(BinaryShape::First, TernaryShape::Second),
				TupleProduct(BinaryShape::First, TernaryShape::Third),
				TupleProduct(BinaryShape::Second, TernaryShape::First),
				TupleProduct(BinaryShape::Second, TernaryShape::Second),
				TupleProduct(BinaryShape::Second, TernaryShape::Third),
			],
		);
	}

	#[test]
	fn enums_sample_the_sum_of_variant_shapes() {
		assert_eq!(Empty::sample_all_shapes(), vec![]);

		assert_eq!(
			SumShape::sample_all_shapes(),
			vec![
				SumShape::Unit,
				SumShape::Tuple(BinaryShape::First, TernaryShape::First),
				SumShape::Tuple(BinaryShape::First, TernaryShape::Second),
				SumShape::Tuple(BinaryShape::First, TernaryShape::Third),
				SumShape::Tuple(BinaryShape::Second, TernaryShape::First),
				SumShape::Tuple(BinaryShape::Second, TernaryShape::Second),
				SumShape::Tuple(BinaryShape::Second, TernaryShape::Third),
				SumShape::Named { ternary: TernaryShape::First },
				SumShape::Named { ternary: TernaryShape::Second },
				SumShape::Named { ternary: TernaryShape::Third },
			],
		);
	}
}
