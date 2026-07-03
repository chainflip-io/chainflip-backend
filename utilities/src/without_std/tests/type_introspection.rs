#![cfg(test)]
#![allow(unused)]

use crate::{never::Never, type_introspection::HasTypeIntrospection};

// --------------- is_empty_type ----------------
#[test]
fn test_is_empty_type() {
	#![allow(clippy::bool_assert_comparison)]

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
