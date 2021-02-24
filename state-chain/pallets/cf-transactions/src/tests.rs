use crate::{mock::*, Error};
use frame_support::{assert_noop, assert_ok};

// TemplateModule, Origin and new_test_ext are from mock

#[test]
#[ignore = "To be implemented"]
fn it_stores_transactions() {
    new_test_ext().execute_with(|| {
        todo!()
        // assert_ok!(TemplateModule::set_swap_quote(Origin::signed(1)));
    });
}

#[test]
#[ignore = "To be implemented"]
fn it_throws_validation_errors() {
    new_test_ext().execute_with(|| todo!());
}
