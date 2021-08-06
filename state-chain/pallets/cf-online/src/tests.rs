mod tests {
	use crate::mock::*;
	use crate::*;
	use frame_support::{assert_noop, assert_ok};
	use cf_traits::mocks::{epoch_info, time_source};

	#[test]
	fn genesis() {
		new_test_ext().execute_with(|| {
			assert_eq!(1 + 1, 2)
		});
	}
}