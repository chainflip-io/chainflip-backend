use sp_std::{vec, vec::Vec};

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

#[cfg(test)]
mod tests {
	use crate::{never::Never, type_introspection::HasTypeIntrospection};

	#[derive(Copy, Clone, Debug, PartialEq, Eq)]
	enum BinaryShape {
		First,
		Second,
	}

	impl HasTypeIntrospection for BinaryShape {
		fn is_empty_type() -> bool {
			false
		}

		fn sample_all_shapes() -> Vec<Self> {
			vec![Self::First, Self::Second]
		}
	}

	#[derive(Copy, Clone, Debug, PartialEq, Eq)]
	enum TernaryShape {
		First,
		Second,
		Third,
	}

	impl HasTypeIntrospection for TernaryShape {
		fn is_empty_type() -> bool {
			false
		}

		fn sample_all_shapes() -> Vec<Self> {
			vec![Self::First, Self::Second, Self::Third]
		}
	}

	#[derive(Copy, Clone, Debug, PartialEq, Eq, cf_proc_macros::HasTypeIntrospection)]
	struct NamedProduct {
		binary: BinaryShape,
		ternary: TernaryShape,
	}

	#[derive(Copy, Clone, Debug, PartialEq, Eq, cf_proc_macros::HasTypeIntrospection)]
	struct TupleProduct(BinaryShape, TernaryShape);

	#[derive(Copy, Clone, Debug, PartialEq, Eq, cf_proc_macros::HasTypeIntrospection)]
	enum SumShape {
		Unit,
		Tuple(BinaryShape, TernaryShape),
		Named { ternary: TernaryShape },
		EmptyTuple(BinaryShape, Never),
		EmptyNamed { never: Never },
	}

	#[test]
	fn structs_sample_the_cartesian_product_of_field_shapes() {
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
