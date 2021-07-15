use crate::{mock::*, Error};
use frame_support::{assert_noop, assert_ok};

macro_rules! balance_totals {
	( $( $acct:literal ),+ ) => {
		(
			$(
				Flip::<Test>::total_balance_of(&$acct),
			)+
		)
	};
}
