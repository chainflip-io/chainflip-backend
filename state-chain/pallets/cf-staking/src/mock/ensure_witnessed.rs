use super::*;
use frame_support::traits::EnsureOrigin;
use frame_system::{ensure_root, RawOrigin};

pub struct Mock;

impl EnsureOrigin<Origin> for Mock {
	type Success = ();

	fn try_origin(o: Origin) -> Result<Self::Success, Origin> {
		ensure_root(o).or(Err(RawOrigin::None.into()))
	}
}
