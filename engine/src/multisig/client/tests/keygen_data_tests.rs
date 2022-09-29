use std::collections::BTreeSet;

use cf_primitives::AuthorityCount;
use rand_legacy::SeedableRng;

use crate::multisig::{
    client::{
        common::{BroadcastVerificationMessage, CeremonyStageName, PreProcessStageDataCheck},
        keygen::{BlameResponse8, Complaints6, KeygenData, SecretShare5},
    },
    crypto::Rng,
    eth::Point,
};

use super::helpers::{gen_invalid_keygen_comm1, get_invalid_hash_comm};

/// ==========================
// Generate invalid keygen data with the given number of elements in its inner and outer collection(s)

pub fn gen_keygen_data_hash_comm1() -> KeygenData<Point> {
    let mut rng = Rng::from_seed([0; 32]);
    KeygenData::HashComm1(get_invalid_hash_comm(&mut rng))
}

pub fn gen_keygen_data_verify_hash_comm2(participant_count: AuthorityCount) -> KeygenData<Point> {
    let mut rng = Rng::from_seed([0; 32]);
    KeygenData::VerifyHashComm2(BroadcastVerificationMessage {
        data: (1..=participant_count)
            .map(|i| (i as AuthorityCount, Some(get_invalid_hash_comm(&mut rng))))
            .collect(),
    })
}

fn gen_keygen_data_coeff_comm3(participant_count: AuthorityCount) -> KeygenData<Point> {
    let mut rng = Rng::from_seed([0; 32]);
    KeygenData::CoeffComm3(gen_invalid_keygen_comm1(&mut rng, participant_count))
}

fn gen_keygen_data_verify_coeff_comm4(
    participant_count_outer: AuthorityCount,
    participant_count_inner: AuthorityCount,
) -> KeygenData<Point> {
    let mut rng = Rng::from_seed([0; 32]);
    KeygenData::VerifyCoeffComm4(BroadcastVerificationMessage {
        data: (1..=participant_count_outer)
            .map(|i| {
                (
                    i as AuthorityCount,
                    Some(gen_invalid_keygen_comm1(&mut rng, participant_count_inner)),
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
    participant_count_inner: AuthorityCount,
) -> KeygenData<Point> {
    KeygenData::VerifyComplaints7(BroadcastVerificationMessage {
        data: (1..=participant_count_outer)
            .map(|i| {
                (
                    i as AuthorityCount,
                    Some(Complaints6(BTreeSet::from_iter(
                        1..=participant_count_inner,
                    ))),
                )
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
    inner_len: AuthorityCount,
) -> KeygenData<Point> {
    let mut rng = Rng::from_seed([0; 32]);
    KeygenData::VerifyBlameResponses9(BroadcastVerificationMessage {
        data: (0..outer_len)
            .map(|i| {
                (
                    i as AuthorityCount,
                    // Create a nested collection with a changing size
                    Some(BlameResponse8(
                        (0..inner_len)
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
fn check_data_size_verify_hash_comm2() {
    let expected_len: AuthorityCount = 4;

    // Should pass with the correct data length
    assert!(gen_keygen_data_verify_hash_comm2(expected_len).data_size_is_valid(expected_len));

    // Should fail on sizes larger or smaller then expected
    assert!(!gen_keygen_data_verify_hash_comm2(expected_len + 1).data_size_is_valid(expected_len));
    assert!(!gen_keygen_data_verify_hash_comm2(expected_len - 1).data_size_is_valid(expected_len));
}

#[test]
fn check_data_size_coeff_comm3() {
    let expected_len: AuthorityCount = 4;

    assert!(gen_keygen_data_coeff_comm3(expected_len).data_size_is_valid(expected_len));

    // Should fail on sizes larger or smaller then expected
    assert!(!gen_keygen_data_coeff_comm3(expected_len + 1).data_size_is_valid(expected_len));
    assert!(!gen_keygen_data_coeff_comm3(expected_len - 1).data_size_is_valid(expected_len));
}

#[test]
fn check_data_size_verify_coeff_comm4() {
    let expected_len: AuthorityCount = 4;

    // Should pass when both collections are the correct size
    assert!(
        gen_keygen_data_verify_coeff_comm4(expected_len, expected_len)
            .data_size_is_valid(expected_len)
    );

    // The outer collection should fail if larger or smaller than expected
    assert!(
        !gen_keygen_data_verify_coeff_comm4(expected_len + 1, expected_len)
            .data_size_is_valid(expected_len)
    );
    assert!(
        !gen_keygen_data_verify_coeff_comm4(expected_len - 1, expected_len)
            .data_size_is_valid(expected_len)
    );

    // The nested collection should fail if larger or smaller than expected
    assert!(
        !gen_keygen_data_verify_coeff_comm4(expected_len, expected_len + 1)
            .data_size_is_valid(expected_len)
    );
    assert!(
        !gen_keygen_data_verify_coeff_comm4(expected_len, expected_len - 1)
            .data_size_is_valid(expected_len)
    );
}

#[test]
fn check_data_size_complaints6() {
    let expected_len: AuthorityCount = 4;

    assert!(gen_keygen_data_complaints6(expected_len).data_size_is_valid(expected_len));
    assert!(gen_keygen_data_complaints6(0).data_size_is_valid(expected_len));

    // Should fail on sizes larger then expected
    assert!(!gen_keygen_data_complaints6(expected_len + 1).data_size_is_valid(expected_len));
}

#[test]
fn check_data_size_verify_complaints7() {
    let expected_len: AuthorityCount = 4;

    // Should pass when both collections are the correct size
    assert!(
        gen_keygen_data_verify_complaints7(expected_len, expected_len)
            .data_size_is_valid(expected_len)
    );
    assert!(gen_keygen_data_verify_complaints7(expected_len, 0).data_size_is_valid(expected_len));

    // The outer collection should fail if larger or smaller than expected
    assert!(
        !gen_keygen_data_verify_complaints7(expected_len + 1, expected_len)
            .data_size_is_valid(expected_len)
    );
    assert!(
        !gen_keygen_data_verify_complaints7(expected_len - 1, expected_len)
            .data_size_is_valid(expected_len)
    );

    // The nested collection should fail if larger than expected
    assert!(
        !gen_keygen_data_verify_complaints7(expected_len, expected_len + 1)
            .data_size_is_valid(expected_len)
    );
}

#[test]
fn check_data_size_blame_response8() {
    let expected_len: AuthorityCount = 4;

    assert!(gen_keygen_data_blame_response8(expected_len).data_size_is_valid(expected_len));
    assert!(gen_keygen_data_blame_response8(0).data_size_is_valid(expected_len));

    // Should fail on sizes larger then expected
    assert!(!gen_keygen_data_blame_response8(expected_len + 1).data_size_is_valid(expected_len));
}

#[test]
fn check_data_size_verify_blame_responses9() {
    let expected_len: AuthorityCount = 4;

    // Should pass when both collections are the correct size
    assert!(
        gen_keygen_data_verify_blame_response9(expected_len, expected_len)
            .data_size_is_valid(expected_len)
    );
    assert!(
        gen_keygen_data_verify_blame_response9(expected_len, 0).data_size_is_valid(expected_len)
    );

    // The outer collection should fail if larger or smaller than expected
    assert!(
        !gen_keygen_data_verify_blame_response9(expected_len + 1, expected_len)
            .data_size_is_valid(expected_len)
    );
    assert!(
        !gen_keygen_data_verify_blame_response9(expected_len - 1, expected_len)
            .data_size_is_valid(expected_len)
    );

    // The nested collection should fail if larger than expected
    assert!(
        !gen_keygen_data_verify_blame_response9(expected_len, expected_len + 1)
            .data_size_is_valid(expected_len)
    );
}

#[test]
fn should_delay_correct_data_for_stage() {
    let default_length = 1;
    let stage_name = [
        CeremonyStageName::HashCommitments1,
        CeremonyStageName::VerifyHashCommitmentsBroadcast2,
        CeremonyStageName::CoefficientCommitments3,
        CeremonyStageName::VerifyCommitmentsBroadcast4,
        CeremonyStageName::SecretSharesStage5,
        CeremonyStageName::ComplaintsStage6,
        CeremonyStageName::VerifyComplaintsBroadcastStage7,
        CeremonyStageName::BlameResponsesStage8,
        CeremonyStageName::VerifyBlameResponsesBroadcastStage9,
    ];
    let stage_data = [
        gen_keygen_data_hash_comm1(),
        gen_keygen_data_verify_hash_comm2(default_length),
        gen_keygen_data_coeff_comm3(default_length),
        gen_keygen_data_verify_coeff_comm4(default_length, default_length),
        gen_keygen_secret_shares5(),
        gen_keygen_data_complaints6(default_length),
        gen_keygen_data_verify_complaints7(default_length, default_length),
        gen_keygen_data_blame_response8(default_length),
        gen_keygen_data_verify_blame_response9(default_length, default_length),
    ];

    for (stage_index, name) in stage_name.iter().enumerate() {
        for (data_index, data) in stage_data.iter().enumerate() {
            if stage_index + 1 == data_index {
                // Should delay the next stage data (stage_index + 1)
                assert!(KeygenData::should_delay(*name, data));
            } else {
                // Should not delay any other stage
                assert!(!KeygenData::should_delay(*name, data));
            }
        }
    }
}
