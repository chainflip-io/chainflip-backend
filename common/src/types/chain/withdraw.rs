use super::{UniqueId, Validate};
use crate::types::{unique_id::GetUniqueId, Network};
use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use siphasher::sip::SipHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Withdraw {
    /// The identifier of the `WithdrawRequest` related to the withdraw.
    pub withdraw_request: u64,
    /// An array of identifiers of all the `Output` related to the withdraw (one for each coin type in the pool).
    pub outputs: [u64; 2],
    /// Event number used to sync the CFE and substrate node
    pub event_number: Option<u64>,
}

impl Validate for Withdraw {
    type Error = &'static str;

    fn validate(&self, _: Network) -> Result<(), Self::Error> {
        if self.outputs[0] == self.outputs[1] {
            return Err("Output states must be different");
        }

        Ok(())
    }
}

// Used as a key in the KV store of CFE
impl Hash for Withdraw {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.withdraw_request.hash(state);
        self.outputs.hash(state);
    }
}

impl GetUniqueId for Withdraw {
    type UniqueId = UniqueId;

    fn unique_id(&self) -> Self::UniqueId {
        let mut s = SipHasher::new();
        self.hash(&mut s);
        s.finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn withdraw() -> Withdraw {
        Withdraw {
            withdraw_request: 0,
            outputs: [0, 1],
            event_number: None,
        }
    }

    #[test]
    fn validates() {
        let withdraw = withdraw();
        assert!(withdraw.validate(Network::Mainnet).is_ok());

        let mut invalid = withdraw.clone();
        invalid.outputs[1] = invalid.outputs[0];
        assert_eq!(
            invalid.validate(Network::Mainnet).unwrap_err(),
            "Output states must be different"
        );
    }

    #[test]
    fn hash_withdraw() {
        let withdraw = withdraw();
        let mut s = SipHasher::new();
        withdraw.hash(&mut s);
        let hash = s.finish();

        assert_eq!(17029759267519039943, hash);
    }
}
