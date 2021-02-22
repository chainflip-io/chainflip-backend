use super::{UniqueId, Validate};
use crate::{
    string::*,
    types::{coin::Coin, unique_id::GetUniqueId, utf8::ByteString, AtomicAmount, Network},
    validation::validate_address,
};
use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use siphasher::sip::SipHasher;
use std::{
    hash::{Hash, Hasher},
    vec::Vec,
};

/// The parent of the output
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "id")]
pub enum OutputParent {
    SwapQuote(u64),
    DepositQuote(u64),
    WithdrawRequest(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    /// The id of the parent that was processed.
    pub parent: OutputParent,
    /// An array of identifiers of all the `Witness` that were processed.
    pub witnesses: Vec<u64>,
    /// An array of identifiers of all the `PoolChange` that were processed.
    pub pool_changes: Vec<u64>,
    /// The output coin
    pub coin: Coin,
    /// The receiving address the output
    pub address: ByteString,
    /// The amount that needs to be sent.
    pub amount: AtomicAmount,
    /// Event number used to sync the CFE and substrate node
    pub event_number: Option<u64>,
}

impl Validate for Output {
    type Error = &'static str;

    fn validate(&self, network: Network) -> Result<(), Self::Error> {
        validate_address(self.coin, network, &self.address.to_string())
            .map_err(|_| "Invalid address")?;

        if self.amount == 0 {
            return Err("Amount is zero");
        }

        Ok(())
    }
}

impl Output {
    /// Get the uuid of the parent
    pub fn parent_id(&self) -> u64 {
        match self.parent {
            OutputParent::SwapQuote(id) => id,
            OutputParent::DepositQuote(id) => id,
            OutputParent::WithdrawRequest(id) => id,
        }
    }
}

// Used as a key in the KV store of CFE
impl Hash for Output {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.coin.hash(state);
        self.parent_id().hash(state);
    }
}

impl GetUniqueId for Output {
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
    use crate::test::constants::ETH_ADDRESS;

    fn sent() -> Output {
        Output {
            coin: Coin::ETH,
            address: ETH_ADDRESS.into(),
            amount: 100,
            parent: OutputParent::DepositQuote(0),
            witnesses: vec![],
            pool_changes: vec![],
            event_number: None,
        }
    }

    #[test]
    fn validates() {
        let valid = sent();
        assert!(valid.validate(Network::Mainnet).is_ok());

        let mut data = sent();
        data.address = "Invalid".into();
        assert_eq!(
            data.validate(Network::Mainnet).unwrap_err(),
            "Invalid address"
        );

        let mut data = sent();
        data.amount = 0;
        assert_eq!(
            data.validate(Network::Mainnet).unwrap_err(),
            "Amount is zero"
        );
    }

    #[test]
    fn hash_output() {
        let output = sent();
        let mut s = SipHasher::new();
        output.hash(&mut s);
        let hash = s.finish();

        print!("hash: {}", hash);

        assert_eq!(13854094555763202650, hash);
    }
}
