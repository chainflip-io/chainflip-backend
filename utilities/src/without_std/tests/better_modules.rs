#![cfg(test)]
#![allow(unused)]

pub trait T1 {
	type XY;
}

trait Good {
	type Bad;
}

cf_proc_macros::better_modules! {
	mod (A: T1) {
		type MyType = A::XY;
		type Bla = u16;
		struct ThisIsS {
			value: A::XY,
		}
		mod (B: Clone) where (B: Clone) {
			struct InnerWithBoth {
				a: A,
				b: B,
			}
			type MyVal = InnerWithBoth;
		}
		struct Without {
			value: bool,
		}
		impl Good for ThisIsS {
			type Bad = u8;
		}
	}
}
