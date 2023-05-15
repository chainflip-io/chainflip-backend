//! Serde doesn't implement serialization of arrays of size larger than 32.
//! This mostly copies parts of serde's implementation for fixed size arrays
//! so we can extend it to [u8; 33], for example. Currently, this only
//! implements ArrayVisitor<[u8;33]>, but this should be easy to extend to
//! arrays of other sizes. Note that the use of the macro seems to be important
//! for performance (presumably due to loop unrolling). Without it, I observed
//! a factor of 2-3 performance degradation.

macro_rules! array_impls {
    ($($len:expr => ($($n:tt)+))+) => {
        $(
            impl<'de> serde::de::Visitor<'de> for ArrayVisitor<[u8; $len]>
            {
                type Value = [u8; $len];

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str(concat!("an array of length ", $len))
                }

                #[inline]
                fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                where
                    A: serde::de::SeqAccess<'de>,
                {
                    Ok([$(
                        match seq.next_element() {
							Ok(val) => match val {
								Some(val) => val,
								None => return Err(serde::de::Error::invalid_length($n, &self)),
							},
							Err(e) => return Err(e),
                        }
					),+])
                }
            }
        )+
    }
}

pub(super) struct ArrayVisitor<A> {
	marker: std::marker::PhantomData<A>,
}

impl<A> ArrayVisitor<A> {
	pub(super) fn new() -> Self {
		ArrayVisitor { marker: std::marker::PhantomData }
	}
}

array_impls! {
	33 => (0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32)
}
