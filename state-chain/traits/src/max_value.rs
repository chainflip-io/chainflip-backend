pub trait MaxValue {
	const MAX: Self;
}

macro_rules! impl_max_value {
	($t:ty) => {
		impl MaxValue for $t {
			const MAX: $t = <$t>::MAX;
		}
	};
}

impl_max_value!(u8);
impl_max_value!(u16);
impl_max_value!(u32);
impl_max_value!(u64);
impl_max_value!(u128);
