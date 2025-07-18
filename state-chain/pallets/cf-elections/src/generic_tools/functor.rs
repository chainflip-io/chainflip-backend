pub use super::common_traits::*;
use crate::def_derive;

use sp_std::vec::Vec;

pub trait Functor {
	type Of<A: CommonTraits>: CommonTraits;
}

pub trait Transformation<F: Functor, G: Functor> {
	fn at<A: CommonTraits>(&self, input: F::Of<A>) -> G::Of<A>;
}

#[macro_export]
macro_rules! transform {
	(for $A:ident |$var:ident: $F:ty| -> $G:ty {
		$($expr:tt)*
	}) => {
		{
			struct LocalTransformation;
			impl Transformation<$F, $G> for LocalTransformation {
				fn at<$A: CommonTraits>(&self, $var: <$F as Functor>::Of<$A>) -> <$G as Functor>::Of<$A> {
					$($expr)*
				}
			}
			LocalTransformation
		}
	};
}
pub use transform;

// ----- vector -----
def_derive! {
	#[derive(TypeInfo)]
	pub struct VectorContainer;
}

impl Functor for VectorContainer {
	type Of<A: CommonTraits> = Vec<A>;
}

// ----- array -----
def_derive! {
	#[derive(TypeInfo)]
	pub struct Array<const N: usize, A: CommonTraits> {
		#[serde(with = "serde_arrays")]
		pub array: [A; N],
	}
}

def_derive! {
	pub struct ArrayContainer<const N: usize>;
}

impl<const N: usize> Functor for ArrayContainer<N> {
	type Of<A: CommonTraits> = Array<N, A>;
}

// ----- transformations -----
pub struct ArrayToVector;
impl<const N: usize> Transformation<ArrayContainer<N>, VectorContainer> for ArrayToVector {
	fn at<A: CommonTraits>(
		&self,
		input: <ArrayContainer<N> as Functor>::Of<A>,
	) -> <VectorContainer as Functor>::Of<A> {
		input.array.into()
	}
}
