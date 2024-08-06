use super::*;
use crate::{vote_storage::VoteStorage, CorruptStorageError, SharedDataHash};
use cf_utilities::assert_ok;

#[test]
fn test_bitmap_vote_storage() {
	type BitmapStorageTest = Bitmap<u64>;

	let test_vote: <BitmapStorageTest as VoteStorage>::Vote = 67u64;
	let test_shared_data_hash = SharedDataHash::of(&test_vote);

	let partial_vote = BitmapStorageTest::vote_into_partial_vote(
		&test_vote,
		|shared_data: <BitmapStorageTest as VoteStorage>::SharedData| {
			SharedDataHash::of(&shared_data)
		},
	);
	assert_eq!(partial_vote, test_shared_data_hash);

	let vote_components =
		assert_ok!(BitmapStorageTest::partial_vote_into_components((), partial_vote));
	assert_eq!(vote_components.individual_component, None);
	assert_eq!(vote_components.bitmap_component, Some(test_shared_data_hash));

	let vote = assert_ok!(BitmapStorageTest::components_into_vote(
		VoteComponents {
			individual_component: None,
			bitmap_component: Some(test_shared_data_hash),
		},
		|shared_data_hash| {
			assert_eq!(shared_data_hash, test_shared_data_hash);
			Ok(Some(test_vote))
		},
	));
	assert_eq!(vote, Some(((), AuthorityVote::Vote(test_vote),)));

	let vote = assert_ok!(BitmapStorageTest::components_into_vote(
		VoteComponents {
			individual_component: None,
			bitmap_component: Some(test_shared_data_hash),
		},
		|shared_data_hash| {
			assert_eq!(shared_data_hash, test_shared_data_hash);
			Ok(None)
		},
	));
	assert_eq!(vote, Some(((), AuthorityVote::PartialVote(test_shared_data_hash),)));

	let vote = assert_ok!(BitmapStorageTest::components_into_vote(
		VoteComponents { individual_component: None, bitmap_component: None },
		|_shared_data_hash| {
			panic!();
		},
	));
	assert_eq!(vote, None);

	let vote = assert_ok!(BitmapStorageTest::components_into_vote(
		VoteComponents { individual_component: Some(((), ())), bitmap_component: None },
		|_shared_data_hash| {
			panic!();
		},
	));
	assert_eq!(vote, None);

	assert_eq!(
		BitmapStorageTest::visit_vote(
			test_vote,
			|shared_data: <BitmapStorageTest as VoteStorage>::SharedData| {
				assert_eq!(shared_data, test_vote);
				assert_eq!(SharedDataHash::of(&shared_data), test_shared_data_hash);
				Ok::<_, CorruptStorageError>(())
			}
		),
		Ok(())
	);

	BitmapStorageTest::visit_individual_component(&(), |_shared_data_hash| {
		panic!();
	});
}
