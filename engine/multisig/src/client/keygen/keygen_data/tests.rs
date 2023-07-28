use std::collections::BTreeSet;

use cf_primitives::AuthorityCount;
use rand::SeedableRng;
use state_chain_runtime::constants::common::MAX_AUTHORITIES;

use crate::{
	bitcoin::BtcSigning,
	client::{
		common::{
			BroadcastVerificationMessage, DelayDeserialization, KeygenStageName,
			PreProcessStageDataCheck,
		},
		helpers::{gen_dummy_keygen_comm3, get_dummy_hash_comm},
		keygen::{BlameResponse8, Complaints6, KeygenData, PubkeyShares0, SecretShare5},
	},
	crypto::Rng,
	eth::{EthSigning, Point},
};

/// ==========================
// Generate invalid keygen data with the given number of elements in its inner and outer
// collection(s)

fn gen_keygen_data_pubkey_shares0(participant_count: AuthorityCount) -> KeygenData<Point> {
	let mut rng = Rng::from_seed([0; 32]);
	KeygenData::PubkeyShares0(PubkeyShares0(
		(1..=participant_count)
			.map(|i| (i as AuthorityCount, Point::random(&mut rng)))
			.collect(),
	))
}

pub fn gen_keygen_data_hash_comm1() -> KeygenData<Point> {
	let mut rng = Rng::from_seed([0; 32]);
	KeygenData::HashComm1(get_dummy_hash_comm(&mut rng))
}

pub fn gen_keygen_data_verify_hash_comm2(participant_count: AuthorityCount) -> KeygenData<Point> {
	let mut rng = Rng::from_seed([0; 32]);
	KeygenData::VerifyHashComm2(BroadcastVerificationMessage {
		data: (1..=participant_count)
			.map(|i| (i as AuthorityCount, Some(get_dummy_hash_comm(&mut rng))))
			.collect(),
	})
}

fn gen_keygen_data_coeff_comm3(participant_count: AuthorityCount) -> KeygenData<Point> {
	let mut rng = Rng::from_seed([0; 32]);
	KeygenData::CoeffComm3(DelayDeserialization::new(&gen_dummy_keygen_comm3::<Point>(
		&mut rng,
		participant_count,
	)))
}

fn gen_keygen_data_verify_coeff_comm4(
	participant_count_outer: AuthorityCount,
	participant_count_inner_first_half: AuthorityCount,
	participant_count_inner_second_half: AuthorityCount,
) -> KeygenData<Point> {
	let mut rng = Rng::from_seed([0; 32]);
	KeygenData::VerifyCoeffComm4(BroadcastVerificationMessage {
		data: (1..=participant_count_outer)
			.map(|i| {
				(
					i as AuthorityCount,
					Some(DelayDeserialization::new(&gen_dummy_keygen_comm3::<Point>(
						&mut rng,
						// Use a 2 different sizes for a more complex test
						if i <= participant_count_outer / 2 {
							participant_count_inner_first_half
						} else {
							participant_count_inner_second_half
						},
					))),
				)
			})
			.collect(),
	})
}

fn gen_keygen_secret_shares5() -> KeygenData<Point> {
	let mut rng = Rng::from_seed([0; 32]);
	KeygenData::SecretShares5(SecretShare5::create_random(&mut rng))
}

fn gen_keygen_data_complaints6(participant_count: AuthorityCount) -> KeygenData<Point> {
	KeygenData::Complaints6(Complaints6(BTreeSet::from_iter(1..=participant_count)))
}

fn gen_keygen_data_verify_complaints7(
	participant_count_outer: AuthorityCount,
	participant_count_inner_first_half: AuthorityCount,
	participant_count_inner_second_half: AuthorityCount,
) -> KeygenData<Point> {
	KeygenData::VerifyComplaints7(BroadcastVerificationMessage {
		data: (1..=participant_count_outer)
			.map(|i| {
				let inner_count = if i <= participant_count_outer / 2 {
					participant_count_inner_first_half
				} else {
					participant_count_inner_second_half
				};
				(i as AuthorityCount, Some(Complaints6(BTreeSet::from_iter(1..=inner_count))))
			})
			.collect(),
	})
}

fn gen_keygen_data_blame_response8(participant_count: AuthorityCount) -> KeygenData<Point> {
	let mut rng = Rng::from_seed([0; 32]);
	KeygenData::BlameResponse8(BlameResponse8(
		(1..=participant_count)
			.map(|i| (i, SecretShare5::create_random(&mut rng)))
			.collect(),
	))
}

fn gen_keygen_data_verify_blame_response9(
	outer_len: AuthorityCount,
	inner_len_first_half: AuthorityCount,
	inner_len_second_half: AuthorityCount,
) -> KeygenData<Point> {
	let mut rng = Rng::from_seed([0; 32]);
	KeygenData::VerifyBlameResponses9(BroadcastVerificationMessage {
		data: (0..outer_len)
			.map(|i| {
				let inner_count =
					if i <= outer_len / 2 { inner_len_first_half } else { inner_len_second_half };
				(
					i as AuthorityCount,
					// Create a nested collection with a changing size
					Some(BlameResponse8(
						(0..inner_count)
							.map(|i| (i, SecretShare5::create_random(&mut rng)))
							.collect(),
					)),
				)
			})
			.collect(),
	})
}

// ==========================

#[test]
fn check_data_size_pubkey_shares0() {
	// This is currently equal to MAX_AUTHORITIES. If MAX_AUTHORITIES changes, this test will need
	// to be updated. But if MAX_AUTHORITIES is ever lowered, it will create an issue with rejecting
	// PubkeyShares0 messages when switching from the higher value to the lower value and will
	// therefore the data size check for PubkeyShares0 will need to be decoupled from
	// MAX_AUTHORITIES.
	let max_expected_len = 150;

	// Should pass with the correct data length
	assert!(gen_keygen_data_pubkey_shares0(max_expected_len)
		.is_initial_stage_data_size_valid::<BtcSigning>());

	// Should pass with an empty message (This is expected behaviour for non-sharing parties)
	assert!(gen_keygen_data_pubkey_shares0(0).is_initial_stage_data_size_valid::<BtcSigning>());

	// Should fail on sizes larger than expected
	assert!(!gen_keygen_data_pubkey_shares0(max_expected_len + 1)
		.is_initial_stage_data_size_valid::<BtcSigning>());
}

#[test]
fn check_data_size_verify_hash_comm2() {
	const PARTIES: AuthorityCount = 4;

	// Should pass with the correct data length
	assert!(
		gen_keygen_data_verify_hash_comm2(PARTIES).is_data_size_valid::<EthSigning>(PARTIES, None)
	);

	// Should fail on sizes larger or smaller than expected
	assert!(!gen_keygen_data_verify_hash_comm2(PARTIES + 1)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
	assert!(!gen_keygen_data_verify_hash_comm2(PARTIES - 1)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
}

#[test]
fn check_data_size_coeff_comm3() {
	const PARTIES: AuthorityCount = 4;

	assert!(gen_keygen_data_coeff_comm3(PARTIES).is_data_size_valid::<EthSigning>(PARTIES, None));

	assert!(!gen_keygen_data_coeff_comm3(MAX_AUTHORITIES + 1)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
}

#[test]
fn check_data_size_verify_coeff_comm4() {
	const PARTIES: AuthorityCount = 4;

	// Should pass when both collections are the correct size
	assert!(gen_keygen_data_verify_coeff_comm4(PARTIES, PARTIES, PARTIES)
		.is_data_size_valid::<EthSigning>(PARTIES, None));

	// Should fail if the outer collection is larger than expected
	assert!(!gen_keygen_data_verify_coeff_comm4(PARTIES + 1, PARTIES, PARTIES)
		.is_data_size_valid::<EthSigning>(PARTIES, None));

	// Should fail if the outer collection is smaller than expected
	assert!(!gen_keygen_data_verify_coeff_comm4(PARTIES - 1, PARTIES, PARTIES)
		.is_data_size_valid::<EthSigning>(PARTIES, None));

	// The nested collection should fail if any of its elements are larger than the max size
	assert!(!gen_keygen_data_verify_coeff_comm4(PARTIES, PARTIES, MAX_AUTHORITIES + 1)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
}

#[test]
fn check_data_size_complaints6() {
	const NUM_OF_PARTIES: AuthorityCount = 4;

	assert!(gen_keygen_data_complaints6(NUM_OF_PARTIES)
		.is_data_size_valid::<EthSigning>(NUM_OF_PARTIES, None));
	assert!(gen_keygen_data_complaints6(0).is_data_size_valid::<EthSigning>(NUM_OF_PARTIES, None));

	// Should fail on sizes larger than expected
	assert!(!gen_keygen_data_complaints6(NUM_OF_PARTIES + 1)
		.is_data_size_valid::<EthSigning>(NUM_OF_PARTIES, None));
}

#[test]
fn check_data_size_verify_complaints7() {
	const PARTIES: AuthorityCount = 4;

	// Should pass when both collections are the correct size
	assert!(gen_keygen_data_verify_complaints7(PARTIES, PARTIES, PARTIES)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
	assert!(gen_keygen_data_verify_complaints7(PARTIES, PARTIES, 0)
		.is_data_size_valid::<EthSigning>(PARTIES, None));

	// The outer collection should fail if larger or smaller than expected
	assert!(!gen_keygen_data_verify_complaints7(PARTIES + 1, PARTIES, PARTIES)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
	assert!(!gen_keygen_data_verify_complaints7(PARTIES - 1, PARTIES, PARTIES)
		.is_data_size_valid::<EthSigning>(PARTIES, None));

	// The nested collection should fail if larger than expected
	assert!(!gen_keygen_data_verify_complaints7(PARTIES, PARTIES, PARTIES + 1)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
	assert!(gen_keygen_data_verify_complaints7(PARTIES, PARTIES, PARTIES - 1)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
}

#[test]
fn check_data_size_blame_response8() {
	const PARTIES: AuthorityCount = 4;

	assert!(
		gen_keygen_data_blame_response8(PARTIES).is_data_size_valid::<EthSigning>(PARTIES, None)
	);
	assert!(gen_keygen_data_blame_response8(0).is_data_size_valid::<EthSigning>(PARTIES, None));

	// Should fail on sizes larger than expected
	assert!(!gen_keygen_data_blame_response8(PARTIES + 1)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
}

#[test]
fn check_data_size_verify_blame_responses9() {
	const PARTIES: AuthorityCount = 4;

	// Should pass when both collections are the correct size
	assert!(gen_keygen_data_verify_blame_response9(PARTIES, PARTIES, PARTIES)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
	assert!(gen_keygen_data_verify_blame_response9(PARTIES, PARTIES, 0)
		.is_data_size_valid::<EthSigning>(PARTIES, None));

	// The outer collection should fail if larger or smaller than expected
	assert!(!gen_keygen_data_verify_blame_response9(PARTIES + 1, PARTIES, PARTIES)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
	assert!(!gen_keygen_data_verify_blame_response9(PARTIES - 1, PARTIES, PARTIES)
		.is_data_size_valid::<EthSigning>(PARTIES, None));

	// The nested collection should fail if larger than expected
	assert!(!gen_keygen_data_verify_blame_response9(PARTIES, PARTIES, PARTIES + 1)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
	assert!(gen_keygen_data_verify_blame_response9(PARTIES, PARTIES, PARTIES - 1)
		.is_data_size_valid::<EthSigning>(PARTIES, None));
}

#[test]
fn should_delay_correct_data_for_stage() {
	use strum::IntoEnumIterator;

	let default_length = 1;

	let stage_names: Vec<KeygenStageName> = KeygenStageName::iter().collect();

	let stage_data = [
		gen_keygen_data_pubkey_shares0(default_length),
		gen_keygen_data_hash_comm1(),
		gen_keygen_data_verify_hash_comm2(default_length),
		gen_keygen_data_coeff_comm3(default_length),
		gen_keygen_data_verify_coeff_comm4(default_length, default_length, default_length),
		gen_keygen_secret_shares5(),
		gen_keygen_data_complaints6(default_length),
		gen_keygen_data_verify_complaints7(default_length, default_length, default_length),
		gen_keygen_data_blame_response8(default_length),
		gen_keygen_data_verify_blame_response9(default_length, default_length, default_length),
	];

	assert_eq!(stage_names.len(), stage_data.len());

	for (stage_index, name) in stage_names.into_iter().enumerate() {
		for (data_index, data) in stage_data.iter().enumerate() {
			if stage_index + 1 == data_index {
				// Should delay the next stage data (stage_index + 1)
				assert!(KeygenData::should_delay(name, data));
			} else {
				// Should not delay any other stage
				assert!(!KeygenData::should_delay(name, data));
			}
		}
	}
}
