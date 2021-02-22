use super::{UniqueId, Validate};
use crate::types::{coin::Coin, unique_id::GetUniqueId, Network};
use codec::{Decode, Encode};

use serde::{Deserialize, Serialize};
use siphasher::sip::SipHasher;
use std::hash::{Hash, Hasher};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolChange {
    /// id of the pool - randomly generated on creation
    id: u128,
    /// The coin associated with the pool
    pub pool: Coin,
    /// The depth change in atomic value of the `coin` in the pool
    pub depth_change: i128,
    /// The depth change in atomic value of the BASE coin in the pool
    pub base_depth_change: i128,
    /// Event number used to sync the CFE and substrate node
    pub event_number: Option<u64>,
}

impl PoolChange {
    pub fn new(
        pool: Coin,
        depth_change: i128,
        base_depth_change: i128,
        event_number: Option<u64>,
    ) -> Self {
        let id = Uuid::new_v4().as_u128();
        PoolChange {
            id,
            pool,
            depth_change,
            base_depth_change,
            event_number,
        }
    }
}

impl Validate for PoolChange {
    type Error = &'static str;

    fn validate(&self, _: Network) -> Result<(), Self::Error> {
        if self.pool == Coin::BASE_COIN {
            return Err("Invalid pool coin");
        }

        if self.depth_change == 0 && self.base_depth_change == 0 {
            return Err("Pool depths unchanged");
        }

        Ok(())
    }
}

// The deterministic uniqueness of an ID across all nodes isn't as important for PoolChange
// as it is for other events, since this is just used for internal accounting purposes.
// As long as the depth changes and pool are correct that's what matters
// not the deterministic ID.
// Used as a key in the KV store of CFE
impl Hash for PoolChange {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.pool.hash(state);
        self.depth_change.hash(state);
        self.base_depth_change.hash(state);
        self.event_number.hash(state);
    }
}

impl GetUniqueId for PoolChange {
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

    #[test]
    fn validates() {
        let change = PoolChange::new(Coin::ETH, 0, -100, None);
        assert!(change.validate(Network::Mainnet).is_ok());

        let mut invalid_pool = change.clone();
        invalid_pool.pool = Coin::BASE_COIN;
        assert_eq!(
            invalid_pool.validate(Network::Mainnet).unwrap_err(),
            "Invalid pool coin"
        );

        let mut no_change = change.clone();
        no_change.depth_change = 0;
        no_change.base_depth_change = 0;
        assert_eq!(
            no_change.validate(Network::Mainnet).unwrap_err(),
            "Pool depths unchanged"
        );
    }
}
