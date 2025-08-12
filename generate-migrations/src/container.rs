
pub use super::common_traits::*;


#[macro_export]
macro_rules! def_derive {
	(#[no_serde] $($Definition:tt)*) => {
		#[derive(
			Debug, Clone, PartialEq, Eq, Encode, Decode,
		)]
		$($Definition)*
	};
	($($Definition:tt)*) => {
		#[derive(
			Debug, Clone, PartialEq, Eq, Encode, Decode,
		)]
		#[derive(Deserialize, Serialize)]
		#[serde(bound(deserialize = "", serialize = ""))]
		$($Definition)*
	};
}
pub use def_derive;

// ----- definition -----


use sp_std::vec::Vec;

pub trait Container {
	type Of<A: CommonTraits>: CommonTraits;
}

pub trait Transformation<F: Container, G: Container> {
	fn at<A: CommonTraits>(&self, input: F::Of<A>) -> G::Of<A>;
}

// ----- id -----

def_derive! {
    pub struct Id;
}

impl Container for Id {
    type Of<A: CommonTraits> = A;
}

// ----- vector -----
def_derive! {
	#[derive(TypeInfo)]
	pub struct VectorContainer;
}

impl Container for VectorContainer {
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

impl<const N: usize> Container for ArrayContainer<N> {
	type Of<A: CommonTraits> = Array<N, A>;
}

// ----- transformations -----
pub struct ArrayToVector;
impl<const N: usize> Transformation<ArrayContainer<N>, VectorContainer> for ArrayToVector {
	fn at<A: CommonTraits>(
		&self,
		input: <ArrayContainer<N> as Container>::Of<A>,
	) -> <VectorContainer as Container>::Of<A> {
		input.array.into()
	}
}
