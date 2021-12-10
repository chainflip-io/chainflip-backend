use crate::chainflip::{get_random_index, select_signer};
use cf_traits::IsOnline;
use sp_std::cell::RefCell;
// use std::ops::Range;

/// Generates a set of validators with the SignerId = index + 1
fn validator_set(len: usize) -> Vec<(u64, ())> {
	let mut id: u64 = 0;
	(0..len)
		.map(|_| {
			id += 1;
			(id, ())
		})
		.collect::<Vec<_>>()
}

thread_local! {
	// Switch to control the mock
	pub static ONLINE: RefCell<bool>  = RefCell::new(true);
}

struct MockIsOnline;
impl IsOnline for MockIsOnline {
	type ValidatorId = u64;

	fn is_online(_validator_id: &Self::ValidatorId) -> bool {
		ONLINE.with(|cell| cell.borrow().clone())
	}
}

#[test]
fn test_get_random_index() {
	assert!(get_random_index(vec![1, 6, 7, 4, 6, 7, 8], 5) < 5);
	assert!(get_random_index(vec![0, 0, 0], 5) < 5);
	assert!(get_random_index(vec![180, 200, 240], 10) < 10);
}

#[test]
fn test_select_signer() {
	// Expect Some validator
	assert_eq!(
		select_signer::<u64, MockIsOnline>(
			vec![(4, ()), (6, ()), (7, ()), (9, ())],
			vec![2, 5, 7, 3]
		),
		Some(9)
	);
	// Expect a validator in a set of 150 validators
	assert_eq!(
		select_signer::<u64, MockIsOnline>(validator_set(150), String::from("seed").into_bytes()),
		Some(45)
	);
	// Expect an comparable big change in the value
	// distribution for an small input seed change
	assert_eq!(
		select_signer::<u64, MockIsOnline>(validator_set(150), String::from("seeed").into_bytes()),
		Some(53)
	);
	// Expect an reasonable SignerId for an bigger input seed
	assert_eq!(
		select_signer::<u64, MockIsOnline>(
			validator_set(150),
			String::from("west1_north_south_east:_berlin_zonk").into_bytes(),
		),
		Some(97)
	);
	// Switch the mock to simulate an situation where all
	// validators are offline
	ONLINE.with(|cell| *cell.borrow_mut() = false);
	// Expect the select_signer function to return None
	// if there is currently no online validator
	assert_eq!(
		select_signer::<u64, MockIsOnline>(
			vec![(14, ()), (3, ()), (2, ()), (6, ())],
			vec![2, 5, 9, 3]
		),
		None
	);
}
