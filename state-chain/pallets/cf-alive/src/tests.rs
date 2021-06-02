mod test {
    use crate::*;
    use crate::{mock::*};
    use frame_support::{assert_ok, assert_noop};
    fn last_event() -> mock::Event {
        frame_system::Pallet::<Test>::events().pop().expect("Event expected").event
    }

    #[test]
    fn something() {
        new_test_ext().execute_with(|| {
        });
    }
}