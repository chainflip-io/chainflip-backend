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

/// Generate a some invalid keygen data of a specific variant
///     with the given number of elements in its inner and outer collections
fn gen_keygen_data_with_len(
    variant: usize,
    outer_len: AuthorityCount,
    inner_len: AuthorityCount,
) -> KeygenData<Point> {
    let mut rng = Rng::from_seed([0; 32]);

    match variant {
        2 => KeygenData::<Point>::VerifyHashComm2(BroadcastVerificationMessage {
            data: (0..outer_len)
                .map(|i| (i as AuthorityCount, Some(get_invalid_hash_comm(&mut rng))))
                .collect(),
        }),
        3 => KeygenData::<Point>::Comm1(gen_invalid_keygen_comm1(&mut rng, outer_len)),
        4 => KeygenData::<Point>::Verify2(BroadcastVerificationMessage {
            data: (0..outer_len)
                .map(|i| {
                    (
                        i as AuthorityCount,
                        // Create a nested collection with a changing size
                        Some(gen_invalid_keygen_comm1(&mut rng, inner_len)),
                    )
                })
                .collect(),
        }),
        6 => KeygenData::<Point>::Complaints4(Complaints4(BTreeSet::from_iter(0..outer_len))),
        7 => KeygenData::<Point>::VerifyComplaints5(BroadcastVerificationMessage {
            data: (0..outer_len)
                .map(|i| {
                    (
                        i as AuthorityCount,
                        // Create a nested collection with a changing size
                        Some(Complaints4(BTreeSet::from_iter(0..inner_len))),
                    )
                })
                .collect(),
        }),
        8 => KeygenData::<Point>::BlameResponse6(BlameResponse6(
            (0..outer_len)
                .map(|i| (i, SecretShare3::create_random(&mut rng)))
                .collect(),
        )),
        9 => KeygenData::<Point>::VerifyBlameResponses7(BroadcastVerificationMessage {
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
        }),
        _ => panic!("Invalid variant"),
    }
}

#[test]
fn check_data_size_verify_hash_comm2() {
    let expected_len: AuthorityCount = 4;
    let test_variant = 2;

    // Confirm that the data we are checking is the correct variant
    assert!(matches!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len),
        KeygenData::<Point>::VerifyHashComm2(_)
    ));

    // Should pass with the correct data length
    assert!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len)
            .check_data_size(Some(expected_len))
    );

    // Should fail on sizes larger or smaller then expected
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len + 1, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len - 1, expected_len)
            .check_data_size(Some(expected_len))
    );
}

#[test]
fn check_data_size_comm1() {
    let expected_len: AuthorityCount = 4;
    let test_variant = 3;

    assert!(matches!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len),
        KeygenData::<Point>::Comm1(_)
    ));

    assert!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len)
            .check_data_size(Some(expected_len))
    );

    // Should fail on sizes larger or smaller then expected
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len + 1, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len - 1, expected_len)
            .check_data_size(Some(expected_len))
    );
}

#[test]
fn check_data_size_verify2() {
    let expected_len: AuthorityCount = 4;
    let test_variant = 4;

    assert!(matches!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len),
        KeygenData::<Point>::Verify2(_)
    ));

    // Should pass when both collections are the correct size
    assert!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len)
            .check_data_size(Some(expected_len))
    );

    // The outer collection should fail if larger or smaller than expected
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len + 1, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len - 1, expected_len)
            .check_data_size(Some(expected_len))
    );

    // The nested collection should fail if larger or smaller than expected
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len, expected_len + 1)
            .check_data_size(Some(expected_len))
    );
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len, expected_len - 1)
            .check_data_size(Some(expected_len))
    );
}

#[test]
fn check_data_size_complaints4() {
    let expected_len: AuthorityCount = 4;
    let test_variant = 6;

    assert!(matches!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len),
        KeygenData::<Point>::Complaints4(_)
    ));

    assert!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        gen_keygen_data_with_len(test_variant, 0, expected_len).check_data_size(Some(expected_len))
    );

    // Should fail on sizes larger then expected
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len + 1, expected_len)
            .check_data_size(Some(expected_len))
    );
}

#[test]
fn check_data_size_verify_complaints5() {
    let expected_len: AuthorityCount = 4;
    let test_variant = 7;

    assert!(matches!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len),
        KeygenData::<Point>::VerifyComplaints5(_)
    ));

    // Should pass when both collections are the correct size
    assert!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        gen_keygen_data_with_len(test_variant, expected_len, 0).check_data_size(Some(expected_len))
    );

    // The outer collection should fail if larger or smaller than expected
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len + 1, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len - 1, expected_len)
            .check_data_size(Some(expected_len))
    );

    // The nested collection should fail if larger than expected
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len, expected_len + 1)
            .check_data_size(Some(expected_len))
    );
}

#[test]
fn check_data_size_blame_response6() {
    let expected_len: AuthorityCount = 4;
    let test_variant = 8;

    assert!(matches!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len),
        KeygenData::<Point>::BlameResponse6(_)
    ));

    assert!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        gen_keygen_data_with_len(test_variant, 0, expected_len).check_data_size(Some(expected_len))
    );

    // Should fail on sizes larger then expected
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len + 1, expected_len)
            .check_data_size(Some(expected_len))
    );
}

#[test]
fn check_data_size_verify_blame_responses7() {
    let expected_len: AuthorityCount = 4;
    let test_variant = 9;

    assert!(matches!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len),
        KeygenData::<Point>::VerifyBlameResponses7(_)
    ));

    // Should pass when both collections are the correct size
    assert!(
        gen_keygen_data_with_len(test_variant, expected_len, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        gen_keygen_data_with_len(test_variant, expected_len, 0).check_data_size(Some(expected_len))
    );

    // The outer collection should fail if larger or smaller than expected
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len + 1, expected_len)
            .check_data_size(Some(expected_len))
    );
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len - 1, expected_len)
            .check_data_size(Some(expected_len))
    );

    // The nested collection should fail if larger than expected
    assert!(
        !gen_keygen_data_with_len(test_variant, expected_len, expected_len + 1)
            .check_data_size(Some(expected_len))
    );
}

#[test]
#[should_panic]
fn check_data_size_should_panic_with_none_on_non_initial_stage() {
    let non_initial_stage_data = gen_keygen_data_with_len(2, 1, 1);

    assert!(!matches!(
        non_initial_stage_data,
        KeygenData::<Point>::HashComm1(_)
    ));

    non_initial_stage_data.check_data_size(None);
}
