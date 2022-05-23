use std::collections::{BTreeMap, BTreeSet};

use cf_traits::AuthorityCount;
use serde::{Deserialize, Serialize};
use utilities::threshold_from_share_count;

use crate::multisig::{client::common::BroadcastVerificationMessage, crypto::ECPoint};

use super::keygen_frost::ShamirShare;

/// Data sent between parties over p2p for a keygen ceremony
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum KeygenData<P: ECPoint> {
    HashComm1(HashComm1),
    VerifyHashComm2(VerifyHashComm2),
    #[serde(bound = "")] // see https://github.com/serde-rs/serde/issues/1296
    Comm1(Comm1<P>),
    #[serde(bound = "")]
    Verify2(VerifyComm2<P>),
    #[serde(bound = "")]
    SecretShares3(SecretShare3<P>),
    Complaints4(Complaints4),
    VerifyComplaints5(VerifyComplaints5),
    #[serde(bound = "")]
    BlameResponse6(BlameResponse6<P>),
    #[serde(bound = "")]
    VerifyBlameResponses7(VerifyBlameResponses7<P>),
}

impl<P: ECPoint> std::fmt::Display for KeygenData<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = match self {
            KeygenData::HashComm1(hash_comm1) => hash_comm1.to_string(),
            KeygenData::VerifyHashComm2(verify2) => verify2.to_string(),
            KeygenData::Comm1(comm1) => comm1.to_string(),
            KeygenData::Verify2(verify2) => verify2.to_string(),
            KeygenData::SecretShares3(share3) => share3.to_string(),
            KeygenData::Complaints4(complaints4) => complaints4.to_string(),
            KeygenData::VerifyComplaints5(verify5) => verify5.to_string(),
            KeygenData::BlameResponse6(blame_response) => blame_response.to_string(),
            KeygenData::VerifyBlameResponses7(verify7) => verify7.to_string(),
        };
        write!(f, "KeygenData({})", inner)
    }
}

impl<P: ECPoint> KeygenData<P> {
    /// Check that the number of elements in the data is correct
    pub fn check_data_size(&self, num_of_parties: Option<AuthorityCount>) -> bool {
        if let Some(num_of_parties) = num_of_parties {
            let num_of_parties = num_of_parties as usize;
            match self {
                // For messages that don't contain a collection (eg. HashComm1), we don't need to check the size.
                KeygenData::HashComm1(_) => true,
                KeygenData::VerifyHashComm2(message) => message.data.len() == num_of_parties,
                KeygenData::Comm1(message) => {
                    message.get_commitments_len()
                        == threshold_from_share_count(num_of_parties as u32) as usize + 1
                }
                KeygenData::Verify2(message) => {
                    for commitment in message.data.values().flatten() {
                        if commitment.get_commitments_len()
                            != threshold_from_share_count(num_of_parties as u32) as usize + 1
                        {
                            return false;
                        }
                    }

                    message.data.len() == num_of_parties
                }
                KeygenData::SecretShares3(_) => true,
                KeygenData::Complaints4(complaints) => {
                    // The complaints are optional, so we just check the max length
                    complaints.0.len() <= num_of_parties
                }
                KeygenData::VerifyComplaints5(message) => {
                    for complaints in message.data.values().flatten() {
                        if complaints.0.len() > num_of_parties {
                            return false;
                        }
                    }
                    message.data.len() == num_of_parties
                }
                KeygenData::BlameResponse6(blame_response) => {
                    // The blame response will only contain a subset, so we just check the max length
                    blame_response.0.len() <= num_of_parties
                }
                KeygenData::VerifyBlameResponses7(message) => {
                    for blame_response in message.data.values().flatten() {
                        if blame_response.0.len() > num_of_parties {
                            return false;
                        }
                    }
                    message.data.len() == num_of_parties
                }
            }
        } else {
            assert!(
                matches!(self, KeygenData::HashComm1(_)),
                "We should know the number of participants for any non-initial stage data"
            );
            true
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HashComm1(pub sp_core::H256);

pub type VerifyHashComm2 = BroadcastVerificationMessage<HashComm1>;

pub type Comm1<P> = super::keygen_frost::DKGUnverifiedCommitment<P>;

pub type VerifyComm2<P> = BroadcastVerificationMessage<Comm1<P>>;

/// Secret share of our locally generated secret calculated separately
/// for each party as the result of evaluating sharing polynomial (generated
/// during stage 1) at the corresponding signer's index
pub type SecretShare3<P> = ShamirShare<P>;

/// List of parties blamed for sending invalid secret shares
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Complaints4(pub BTreeSet<AuthorityCount>);

pub type VerifyComplaints5 = BroadcastVerificationMessage<Complaints4>;

/// For each party blaming a node, it responds with the corresponding (valid)
/// secret share. Unlike secret shares sent at the earlier stage, these shares
/// are verifiably broadcast, so sending an invalid share would result in the
/// node being slashed. Although the shares are meant to be secret, it is safe
/// to reveal/broadcast some them at this stage: a node's long-term secret can
/// only be recovered by collecting shares from all (N-1) nodes, which would
/// require collusion of N-1 nodes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlameResponse6<P: ECPoint>(
    #[serde(bound = "")] pub BTreeMap<AuthorityCount, ShamirShare<P>>,
);

pub type VerifyBlameResponses7<P> = BroadcastVerificationMessage<BlameResponse6<P>>;

derive_impls_for_enum_variants!(impl<P: ECPoint> for HashComm1, KeygenData::HashComm1, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyHashComm2, KeygenData::VerifyHashComm2, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for Comm1<P>, KeygenData::Comm1, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyComm2<P>, KeygenData::Verify2, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for ShamirShare<P>, KeygenData::SecretShares3, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for Complaints4, KeygenData::Complaints4, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyComplaints5, KeygenData::VerifyComplaints5, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for BlameResponse6<P>, KeygenData::BlameResponse6, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyBlameResponses7<P>, KeygenData::VerifyBlameResponses7, KeygenData<P>);

derive_display_as_type_name!(HashComm1);
derive_display_as_type_name!(VerifyHashComm2);
derive_display_as_type_name!(Comm1<P: ECPoint>);
derive_display_as_type_name!(VerifyComm2<P: ECPoint>);
derive_display_as_type_name!(ShamirShare<P: ECPoint>);
derive_display_as_type_name!(Complaints4);
derive_display_as_type_name!(VerifyComplaints5);
derive_display_as_type_name!(BlameResponse6<P: ECPoint>);
derive_display_as_type_name!(VerifyBlameResponses7<P: ECPoint>);

#[cfg(test)]
mod tests {
    use super::*;
    use rand_legacy::SeedableRng;

    use crate::multisig::{
        client::tests::{gen_invalid_keygen_comm1, get_invalid_hash_comm, ACCOUNT_IDS},
        crypto::Rng,
        eth::Point,
    };

    #[test]
    fn check_data_size_verify_hash_comm2() {
        let mut rng = Rng::from_seed([0; 32]);
        let data_to_check = KeygenData::<Point>::VerifyHashComm2(BroadcastVerificationMessage {
            data: (0..ACCOUNT_IDS.len())
                .map(|i| (i as AuthorityCount, Some(get_invalid_hash_comm(&mut rng))))
                .collect(),
        });

        // Should fail on sizes larger or smaller then expected
        assert!(data_to_check.check_data_size(Some(ACCOUNT_IDS.len() as AuthorityCount)));
        assert!(!data_to_check.check_data_size(Some((ACCOUNT_IDS.len() - 1) as AuthorityCount)));
        assert!(!data_to_check.check_data_size(Some((ACCOUNT_IDS.len() + 1) as AuthorityCount)));
    }

    #[test]
    fn check_data_size_comm1() {
        let mut rng = Rng::from_seed([0; 32]);
        let data_to_check = KeygenData::<Point>::Comm1(gen_invalid_keygen_comm1(&mut rng));

        // Should fail on sizes larger or smaller then expected
        assert!(data_to_check.check_data_size(Some(ACCOUNT_IDS.len() as AuthorityCount)));
        assert!(!data_to_check.check_data_size(Some((ACCOUNT_IDS.len() - 1) as AuthorityCount)));
        assert!(!data_to_check.check_data_size(Some((ACCOUNT_IDS.len() + 1) as AuthorityCount)));
    }

    #[test]
    fn check_data_size_verify2() {
        let mut rng = Rng::from_seed([0; 32]);
        let data_to_check = KeygenData::<Point>::Verify2(BroadcastVerificationMessage {
            data: (0..ACCOUNT_IDS.len())
                .map(|i| {
                    (
                        i as AuthorityCount,
                        Some(gen_invalid_keygen_comm1(&mut rng)),
                    )
                })
                .collect(),
        });

        // Should fail on sizes larger or smaller then expected
        assert!(data_to_check.check_data_size(Some(ACCOUNT_IDS.len() as AuthorityCount)));
        assert!(!data_to_check.check_data_size(Some((ACCOUNT_IDS.len() - 1) as AuthorityCount)));
        assert!(!data_to_check.check_data_size(Some((ACCOUNT_IDS.len() + 1) as AuthorityCount)));

        // TODO: is there a better way of testing the nested data size?

        // Create a nested collection that is too large
        let data_to_check = KeygenData::<Point>::Verify2(BroadcastVerificationMessage {
            data: (0..ACCOUNT_IDS.len() - 1)
                .map(|i| {
                    (
                        i as AuthorityCount,
                        Some(gen_invalid_keygen_comm1(&mut rng)),
                    )
                })
                .collect(),
        });

        // Should fail with incorrect size of nested collection
        assert!(!data_to_check.check_data_size(Some((ACCOUNT_IDS.len() - 1) as AuthorityCount)));

        // Create a nested collection that is too small
        let data_to_check = KeygenData::<Point>::Verify2(BroadcastVerificationMessage {
            data: (0..ACCOUNT_IDS.len() + 1)
                .map(|i| {
                    (
                        i as AuthorityCount,
                        Some(gen_invalid_keygen_comm1(&mut rng)),
                    )
                })
                .collect(),
        });

        // Should fail with incorrect size of nested collection
        assert!(!data_to_check.check_data_size(Some((ACCOUNT_IDS.len() + 1) as AuthorityCount)));
    }

    // TODO: check_data_size tests for the other KeygenData variants
}
