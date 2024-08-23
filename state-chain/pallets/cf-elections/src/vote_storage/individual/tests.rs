use super::*;
use crate::{vote_storage::VoteStorage, CorruptStorageError, SharedDataHash};
use cf_utilities::assert_ok;

#[test]
fn test_individual_vote_storage() {
	type IndividualStorageTest = Individual<u8, (identity::Identity<u64>, shared::Shared<u64>)>;

	let test_properties = 7u8;
	let test_individual_vote = 19u64;
	let test_shared_vote = 67u64;
	let test_shared_vote_data =
		<IndividualStorageTest as VoteStorage>::SharedData::B(test_shared_vote);
	let test_shared_data_hash = SharedDataHash::of(&test_shared_vote_data);
	let test_vote: <IndividualStorageTest as VoteStorage>::Vote =
		(test_individual_vote, test_shared_vote);

	let partial_vote = IndividualStorageTest::vote_into_partial_vote(&test_vote);
	assert_eq!(partial_vote, (test_individual_vote, test_shared_data_hash),);

	let vote_components = assert_ok!(IndividualStorageTest::partial_vote_into_components(
		test_properties,
		partial_vote
	));
	assert_eq!(vote_components.individual_component, Some((test_properties, partial_vote)));
	assert_eq!(vote_components.bitmap_component, None);

	let vote = assert_ok!(IndividualStorageTest::components_into_authority_vote(
		VoteComponents {
			individual_component: Some((test_properties, partial_vote)),
			bitmap_component: None,
		},
		|shared_data_hash| {
			assert_eq!(shared_data_hash, test_shared_data_hash);
			Ok(Some(test_shared_vote_data.clone()))
		},
	));
	assert_eq!(vote, Some((test_properties, AuthorityVote::Vote(test_vote),)));

	let vote = assert_ok!(IndividualStorageTest::components_into_authority_vote(
		VoteComponents {
			individual_component: Some((test_properties, partial_vote)),
			bitmap_component: None,
		},
		|shared_data_hash| {
			assert_eq!(shared_data_hash, test_shared_data_hash);
			Ok(None)
		},
	));
	assert_eq!(vote, Some((test_properties, AuthorityVote::PartialVote(partial_vote),)));

	let vote = assert_ok!(IndividualStorageTest::components_into_authority_vote(
		VoteComponents { individual_component: None, bitmap_component: None },
		|_shared_data_hash| {
			panic!();
		},
	));
	assert_eq!(vote, None);

	let vote = assert_ok!(IndividualStorageTest::components_into_authority_vote(
		VoteComponents { individual_component: None, bitmap_component: Some(()) },
		|_shared_data_hash| {
			panic!();
		},
	));
	assert_eq!(vote, None);

	assert_eq!(
		IndividualStorageTest::visit_shared_data_in_vote(
			test_vote,
			|shared_data: <IndividualStorageTest as VoteStorage>::SharedData| {
				assert_eq!(shared_data, test_shared_vote_data);
				assert_eq!(SharedDataHash::of(&shared_data), test_shared_data_hash);
				Ok::<_, CorruptStorageError>(())
			}
		),
		Ok(())
	);

	IndividualStorageTest::visit_shared_data_references_in_individual_component(
		&partial_vote,
		|shared_data_hash| {
			assert_eq!(shared_data_hash, test_shared_data_hash);
		},
	);
}
