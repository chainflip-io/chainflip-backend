use crate::DeregistrationCheck;
use codec::{Decode, Encode};
use sp_std::marker::PhantomData;

use super::{MockPallet, MockPalletStorage};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct MockDeregistrationCheck<Id>(PhantomData<Id>);

impl<Id> MockPallet for MockDeregistrationCheck<Id> {
	const PREFIX: &'static [u8] = b"cf-mocks//DeregistrationCheck";
}

const SHOULD_FAIL: &[u8] = b"SHOULD_FAIL";

impl<Id: Encode + Decode> MockDeregistrationCheck<Id> {
	pub fn set_should_fail(account_id: &Id, should_fail: bool) {
		if should_fail {
			<Self as MockPalletStorage>::put_storage(SHOULD_FAIL, account_id, ());
		} else {
			Self::take_storage::<_, Id>(SHOULD_FAIL, account_id);
		}
	}
	fn should_fail(account_id: &Id) -> bool {
		<Self as MockPalletStorage>::get_storage::<_, ()>(SHOULD_FAIL, account_id).is_some()
	}
}

impl<Id: Encode + Decode> DeregistrationCheck for MockDeregistrationCheck<Id> {
	type AccountId = Id;
	type Error = &'static str;

	fn check(account_id: &Self::AccountId) -> Result<(), Self::Error> {
		if Self::should_fail(account_id) {
			Err("Cannot deregister.")
		} else {
			Ok(())
		}
	}
}
