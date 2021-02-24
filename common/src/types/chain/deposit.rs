use super::{UniqueId, Validate};
use crate::{
    types::{coin::Coin, unique_id::GetUniqueId, AtomicAmount, Bytes, Network},
    validation::validate_staker_id,
};
use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use siphasher::sip::SipHasher;
use std::{
    collections::btree_set::BTreeSet,
    hash::{Hash, Hasher},
    vec::Vec,
};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deposit {
    /// The identifier of the `DepositQuote` related to the deposit.
    pub quote: u64,
    /// An array of identifiers of all the `Witness` related to the deposit.
    pub witnesses: Vec<u64>,
    /// The identifier of the `PoolChange` related to the deposit.
    pub pool_change: u64,
    /// The staker public key.
    pub staker_id: Bytes,
    /// The pool in which the deposit was made.
    pub pool: Coin,
    /// The base coin amount that was deposited.
    pub base_amount: AtomicAmount,
    /// The other coin amount that was deposited.
    pub other_amount: AtomicAmount,
    /// Event number used to sync the CFE and substrate node
    pub event_number: Option<u64>,
}

impl Validate for Deposit {
    type Error = &'static str;

    fn validate(&self, _: Network) -> Result<(), Self::Error> {
        if self.pool == Coin::BASE_COIN {
            return Err("Invalid pool coin");
        }

        if validate_staker_id(&self.staker_id).is_err() {
            return Err("Invalid staker id");
        }

        if self.base_amount == 0 && self.other_amount == 0 {
            return Err("No amount deposited");
        }

        if self.witnesses.is_empty() {
            return Err("No witnesses for deposit");
        }

        let witness_set: BTreeSet<u64> = self.witnesses.iter().cloned().collect();
        if witness_set.len() != self.witnesses.len() {
            return Err("Duplicate witness detected");
        }

        Ok(())
    }
}

// Used as a key in the KV store of CFE
impl Hash for Deposit {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.quote.hash(state);
        self.witnesses.hash(state);
        self.pool.hash(state);
        self.pool_change.hash(state);
    }
}

impl GetUniqueId for Deposit {
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
    use crate::test::constants::STAKER_ID;

    fn deposit() -> Deposit {
        Deposit {
            staker_id: STAKER_ID.to_vec(),
            pool: Coin::ETH,
            quote: 0,
            witnesses: vec![1],
            pool_change: 2,
            base_amount: 0,
            other_amount: 100,
            event_number: None,
        }
    }

    #[test]
    fn validates() {
        let valid = deposit();
        assert!(valid.validate(Network::Mainnet).is_ok());

        let mut data = deposit();
        data.pool = Coin::BASE_COIN;
        assert_eq!(
            data.validate(Network::Mainnet).unwrap_err(),
            "Invalid pool coin"
        );

        let mut data = deposit();
        data.staker_id = b"Invalid".to_vec();
        assert_eq!(
            data.validate(Network::Mainnet).unwrap_err(),
            "Invalid staker id"
        );

        let mut data = deposit();
        data.base_amount = 0;
        data.other_amount = 0;
        assert_eq!(
            data.validate(Network::Mainnet).unwrap_err(),
            "No amount deposited"
        );

        let mut data = deposit();
        data.witnesses = vec![];
        assert_eq!(
            data.validate(Network::Mainnet).unwrap_err(),
            "No witnesses for deposit"
        );

        let mut data = deposit();
        let id = 4;
        data.witnesses = vec![id, id];
        assert_eq!(
            data.validate(Network::Mainnet).unwrap_err(),
            "Duplicate witness detected"
        );
    }

    #[test]
    fn hash_deposit() {
        let deposit = deposit();
        let mut s = SipHasher::new();
        deposit.hash(&mut s);
        let hash = s.finish();

        assert_eq!(9252012390356796285, hash);
    }
}
