use crate::{Error, mock::*, mock::dummy::pallet as pallet_dummy};
use frame_support::{assert_ok, assert_noop};

#[test]
fn witness_something() {
	new_test_ext().execute_with(|| {
		let call = Box::new(Call::Dummy(pallet_dummy::Call::<Test>::put_value(42)));
		assert_ok!(Witnesser::witness(Origin::signed(ALISSA), call));

		assert_eq!(pallet_dummy::Something::<Test>::get(), None);
	});
}
