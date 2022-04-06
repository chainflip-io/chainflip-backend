mod keygen_data;
mod keygen_frost;
mod keygen_stages;

#[cfg(test)]
pub use keygen_frost::{generate_shares_and_commitment, DKGUnverifiedCommitment};

pub use keygen_data::{
    BlameResponse6, Comm1, Complaints4, HashComm1, KeygenData, SecretShare3, VerifyBlameResponses7,
    VerifyComm2, VerifyComplaints5, VerifyHashComm2,
};

pub use keygen_frost::HashContext;

pub use keygen_stages::{is_contract_compatible, HashCommitments1};

#[derive(Clone, Copy)] // TODO Doesn't need to derive Copy
pub struct KeygenOptions {
    /// This is intentionally private to ensure that the only
    /// way to unset this flag with via test-only constructor
    low_pubkey_only: bool,
}

impl Default for KeygenOptions {
    fn default() -> Self {
        Self {
            low_pubkey_only: true,
        }
    }
}

impl KeygenOptions {
    /// This should not be used in production as it could
    /// result in pubkeys incompatible with the KeyManager
    /// contract, but it is useful in tests that need to be
    /// deterministic and don't interact with the contract
    #[cfg(test)]
    pub fn allowing_high_pubkey() -> Self {
        Self {
            low_pubkey_only: false,
        }
    }
}
