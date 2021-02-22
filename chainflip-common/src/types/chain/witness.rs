use super::{UniqueId, Validate};
use crate::types::{coin::Coin, unique_id::GetUniqueId, utf8::ByteString, AtomicAmount, Network};
use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use siphasher::sip::SipHasher;
use std::{
    hash::{Hash, Hasher},
    str::FromStr,
};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Witness {
    /// The identifier of the quote related to the witness.
    pub quote: u64,
    /// The utf-8 encoded bytes of the input transaction id or hash on the actual blockchain
    pub transaction_id: ByteString,
    /// The transaction block number on the actual blockchain
    pub transaction_block_number: u64,
    /// The input transaction index in the block
    pub transaction_index: u64,
    /// The amount that was sent.
    pub amount: AtomicAmount,
    /// The type of coin that was sent.
    pub coin: Coin,
    /// Event number used to sync the CFE and substrate node
    pub event_number: Option<u64>,
}

impl Witness {
    pub fn id_from(coin: &str, txid: &str) -> UniqueId {
        let coin = Coin::from_str(coin).unwrap();
        let pseudo_witness = Self {
            quote: 0,
            transaction_id: ByteString::from(txid),
            transaction_block_number: 0,
            transaction_index: 0,
            amount: 0,
            coin,
            event_number: None,
        };

        pseudo_witness.unique_id()
    }
}

impl Validate for Witness {
    type Error = &'static str;

    fn validate(&self, _: Network) -> Result<(), Self::Error> {
        if self.amount == 0 {
            return Err("Zero amount specified");
        }

        Ok(())
    }
}

// Used as a key in the KV store of CFE
impl Hash for Witness {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.coin.hash(state);
        self.transaction_id.hash(state);
    }
}

impl GetUniqueId for Witness {
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

    fn witness() -> Witness {
        Witness {
            quote: 0,
            transaction_id: "id".into(),
            transaction_block_number: 0,
            transaction_index: 0,
            amount: 100,
            coin: Coin::ETH,
            event_number: None,
        }
    }

    #[test]
    fn validates() {
        let valid = witness();
        assert!(valid.validate(Network::Mainnet).is_ok());

        let mut data = witness();
        data.amount = 0;
        assert_eq!(
            data.validate(Network::Mainnet).unwrap_err(),
            "Zero amount specified"
        );
    }

    #[test]
    fn unique_id_witness() {
        let witness = witness();
        let id = witness.unique_id();

        assert_eq!(1376835125943705973, id);
    }

    #[test]
    fn id_from_generates_same_id_as_unique_id() {
        let coin = "btc";
        let txid = "123";
        let id = Witness::id_from(coin, txid);
        let witness = Witness {
            quote: 0,
            transaction_id: ByteString::from(txid),
            transaction_block_number: 0,
            transaction_index: 0,
            amount: 1232,
            coin: Coin::from_str("btc").unwrap(),
            event_number: None,
        };

        let id_from_w = witness.unique_id();
        assert_eq!(id, id_from_w);
        assert_eq!(id, 4120909342546681490);
    }
}
