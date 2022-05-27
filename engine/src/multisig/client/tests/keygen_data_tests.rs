use std::collections::BTreeSet;

use cf_traits::AuthorityCount;
use rand_legacy::SeedableRng;

use crate::multisig::{
    client::{
        common::BroadcastVerificationMessage,
        keygen::{BlameResponse6, Complaints4, KeygenData, SecretShare3},
    },
    crypto::Rng,
    eth::Point,
};

use super::helpers::{gen_invalid_keygen_comm1, get_invalid_hash_comm};

/// ==========================
// Generate invalid keygen data with the given number of elements in its inner and outer collection(s)

fn gen_keygen_data_verify_hash_comm2(length: AuthorityCount) -> KeygenData<Point> {
    let mut rng = Rng::from_seed([0; 32]);
    KeygenData::<Point>::VerifyHashComm2(BroadcastVerificationMessage {
        data: (0..length)
            .map(|i| (i as AuthorityCount, Some(get_invalid_hash_comm(&mut rng))))
            .collect(),
    })
}

fn gen_keygen_data_comm1(length: AuthorityCount) -> KeygenData<Point> {
    let mut rng = Rng::from_seed([0; 32]);
    KeygenData::<Point>::Comm1(gen_invalid_keygen_comm1(&mut rng, length))
}

fn gen_keygen_data_verify2(
    outer_len: AuthorityCount,
    inner_len: AuthorityCount,
) -> KeygenData<Point> {
    let mut rng = Rng::from_seed([0; 32]);
    KeygenData::<Point>::Verify2(BroadcastVerificationMessage {
        data: (0..outer_len)
            .map(|i| {
                (
                    i as AuthorityCount,
                    Some(gen_invalid_keygen_comm1(&mut rng, inner_len)),
                )
            })
            .collect(),
    })
}

fn gen_keygen_data_complaints4(length: AuthorityCount) -> KeygenData<Point> {
    KeygenData::<Point>::Complaints4(Complaints4(BTreeSet::from_iter(0..length)))
}

fn gen_keygen_data_verify_complaints5(
    outer_len: AuthorityCount,
    inner_len: AuthorityCount,
) -> KeygenData<Point> {
    KeygenData::<Point>::VerifyComplaints5(BroadcastVerificationMessage {
        data: (0..outer_len)
            .map(|i| {
                (
                    i as AuthorityCount,
                    Some(Complaints4(BTreeSet::from_iter(0..inner_len))),
                )
            })
            .collect(),
    })
}

fn gen_keygen_data_blame_response6(length: AuthorityCount) -> KeygenData<Point> {
    let mut rng = Rng::from_seed([0; 32]);
    KeygenData::<Point>::BlameResponse6(BlameResponse6(
        (0..length)
            .map(|i| (i, SecretShare3::create_random(&mut rng)))
            .collect(),
    ))
}

fn gen_keygen_data_verify_blame_response7(
    outer_len: AuthorityCount,
    inner_len: AuthorityCount,
) -> KeygenData<Point> {
    let mut rng = Rng::from_seed([0; 32]);
    KeygenData::<Point>::VerifyBlameResponses7(BroadcastVerificationMessage {
        data: (0..outer_len)
            .map(|i| {
                (
                    i as AuthorityCount,
                    // Create a nested collection with a changing size
                    Some(BlameResponse6(
                        (0..inner_len)
                            .map(|i| (i, SecretShare3::create_random(&mut rng)))
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
    assert!(gen_keygen_data_verify_hash_comm2(expected_len).check_data_size(Some(expected_len)));

    // Should fail on sizes larger or smaller then expected
    assert!(
        !gen_keygen_data_verify_hash_comm2(expected_len + 1).check_data_size(Some(expected_len))
    );
    assert!(
        !gen_keygen_data_verify_hash_comm2(expected_len - 1).check_data_size(Some(expected_len))
    );
}

#[test]
fn check_data_size_comm1() {
    let expected_len: AuthorityCount = 4;

    assert!(gen_keygen_data_comm1(expected_len).check_data_size(Some(expected_len)));

    // Should fail on sizes larger or smaller then expected
    assert!(!gen_keygen_data_comm1(expected_len + 1).check_data_size(Some(expected_len)));
    assert!(!gen_keygen_data_comm1(expected_len - 1).check_data_size(Some(expected_len)));
}

#[test]
fn check_data_size_verify2() {
    let expected_len: AuthorityCount = 4;

    // Should pass when both collections are the correct size
    assert!(gen_keygen_data_verify2(expected_len, expected_len).check_data_size(Some(expected_len)));

    // The outer collection should fail if larger or smaller than expected
    assert!(!gen_keygen_data_verify2(expected_len + 1, expected_len)
        .check_data_size(Some(expected_len)));
    assert!(!gen_keygen_data_verify2(expected_len - 1, expected_len)
        .check_data_size(Some(expected_len)));

    // The nested collection should fail if larger or smaller than expected
    assert!(!gen_keygen_data_verify2(expected_len, expected_len + 1)
        .check_data_size(Some(expected_len)));
    assert!(!gen_keygen_data_verify2(expected_len, expected_len - 1)
        .check_data_size(Some(expected_len)));
}

#[test]
fn check_data_size_complaints4() {
    let expected_len: AuthorityCount = 4;

    assert!(gen_keygen_data_complaints4(expected_len).check_data_size(Some(expected_len)));
    assert!(gen_keygen_data_complaints4(0).check_data_size(Some(expected_len)));

    // Should fail on sizes larger then expected
    assert!(!gen_keygen_data_complaints4(expected_len + 1).check_data_size(Some(expected_len)));
}

#[test]
fn check_data_size_verify_complaints5() {
    let expected_len: AuthorityCount = 4;

    // Should pass when both collections are the correct size
    assert!(
        gen_keygen_data_verify_complaints5(expected_len, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(gen_keygen_data_verify_complaints5(expected_len, 0).check_data_size(Some(expected_len)));

    // The outer collection should fail if larger or smaller than expected
    assert!(
        !gen_keygen_data_verify_complaints5(expected_len + 1, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        !gen_keygen_data_verify_complaints5(expected_len - 1, expected_len)
            .check_data_size(Some(expected_len))
    );

    // The nested collection should fail if larger than expected
    assert!(
        !gen_keygen_data_verify_complaints5(expected_len, expected_len + 1)
            .check_data_size(Some(expected_len))
    );
}

#[test]
fn check_data_size_blame_response6() {
    let expected_len: AuthorityCount = 4;

    assert!(gen_keygen_data_blame_response6(expected_len).check_data_size(Some(expected_len)));
    assert!(gen_keygen_data_blame_response6(0).check_data_size(Some(expected_len)));

    // Should fail on sizes larger then expected
    assert!(!gen_keygen_data_blame_response6(expected_len + 1).check_data_size(Some(expected_len)));
}

#[test]
fn check_data_size_verify_blame_responses7() {
    let expected_len: AuthorityCount = 4;

    // Should pass when both collections are the correct size
    assert!(
        gen_keygen_data_verify_blame_response7(expected_len, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        gen_keygen_data_verify_blame_response7(expected_len, 0).check_data_size(Some(expected_len))
    );

    // The outer collection should fail if larger or smaller than expected
    assert!(
        !gen_keygen_data_verify_blame_response7(expected_len + 1, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        !gen_keygen_data_verify_blame_response7(expected_len - 1, expected_len)
            .check_data_size(Some(expected_len))
    );

    // The nested collection should fail if larger than expected
    assert!(
        !gen_keygen_data_verify_blame_response7(expected_len, expected_len + 1)
            .check_data_size(Some(expected_len))
    );
}

#[test]
#[should_panic]
fn check_data_size_should_panic_with_none_on_non_initial_stage() {
    gen_keygen_data_verify2(1, 1).check_data_size(None);
}
