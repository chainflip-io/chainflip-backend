#[macro_export]
macro_rules! impl_mock_online {
	($account_id:ty) => {
		type Online = std::collections::HashMap<$account_id, bool>;

		thread_local! {
			pub static ONLINE: std::cell::RefCell<Online> = Default::default()
		}

		pub struct MockOnline;

		impl MockOnline {
			pub fn set_online(validator_id: &$account_id, online: bool) {
				ONLINE.with(|cell| cell.borrow_mut().insert(validator_id.clone(), online));
			}
		}

		impl IsOnline for MockOnline {
			type ValidatorId = $account_id;

			fn is_online(validator_id: &Self::ValidatorId) -> bool {
				ONLINE.with(|cell| {
					cell.borrow().get(validator_id).map(ToOwned::to_owned).unwrap_or(true)
				})
			}
		}
	};
}
