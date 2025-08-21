use sp_core::{bounded::BoundedVec, Get};

pub fn map_bounded_vec<S: Get<u32>, T0, T1>(
	xs: BoundedVec<T0, S>,
	f: impl Fn(T0) -> T1,
) -> BoundedVec<T1, S> {
	BoundedVec::truncate_from(xs.into_iter().map(f).collect())
}
