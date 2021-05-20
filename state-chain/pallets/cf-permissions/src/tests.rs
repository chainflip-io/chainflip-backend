use super::*;
use crate::{Error, mock::*};
use frame_support::{assert_ok, assert_noop};

#[test]
fn answer() {
	new_test_ext().execute_with(|| {
		assert!(true)
	});
}
