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
    collections::btree_set::BTreeSet,
    hash::{Hash, Hasher},
    vec::Vec,
};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSent {
    /// An array of identifiers of all the `Outputs` related to this.
    pub outputs: Vec<u64>,
    /// The coin type that was sent.
    pub coin: Coin,
    /// The address that coins were sent to.
    pub address: ByteString,
    /// The amount that was sent.
    pub amount: AtomicAmount,
    /// The fee that was taken.
    pub fee: AtomicAmount,
    /// The utf-8 encoded bytes of the output transaction id or hash on the actual blockchain
    pub transaction_id: ByteString,
    /// Event number used to sync the CFE and substrate node
    pub event_number: Option<u64>,
}

impl Validate for OutputSent {
    type Error = &'static str;

    fn validate(&self, network: Network) -> Result<(), Self::Error> {
        validate_address(self.coin, network, &self.address.to_string())
            .map_err(|_| "Invalid address")?;

        if self.outputs.is_empty() {
            return Err("No outputs");
        }

        let outputs_set: BTreeSet<u64> = self.outputs.iter().cloned().collect();
        if outputs_set.len() != self.outputs.len() {
            return Err("Duplicate outputs detected");
        }

        if self.transaction_id.to_string().is_empty() {
            return Err("Invalid transaction id");
        }

        if self.amount == 0 && self.fee == 0 {
            return Err("Amount and Fee are zero");
        }

        Ok(())
    }
}

// Used as a key in the KV store of CFE
impl Hash for OutputSent {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.coin.hash(state);
        self.transaction_id.hash(state);
    }
}

impl GetUniqueId for OutputSent {
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

    fn sent() -> OutputSent {
        OutputSent {
            outputs: vec![0],
            coin: Coin::ETH,
            address: ETH_ADDRESS.into(),
            amount: 100,
            fee: 0,
            transaction_id: "id".into(),
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
        data.outputs = vec![];
        assert_eq!(data.validate(Network::Mainnet).unwrap_err(), "No outputs");

        let mut data = sent();
        let id = 0;
        data.outputs = vec![id, id];
        assert_eq!(
            data.validate(Network::Mainnet).unwrap_err(),
            "Duplicate outputs detected"
        );

        let mut data = sent();
        data.transaction_id = "".into();
        assert_eq!(
            data.validate(Network::Mainnet).unwrap_err(),
            "Invalid transaction id"
        );

        let mut data = sent();
        data.amount = 0;
        data.fee = 0;
        assert_eq!(
            data.validate(Network::Mainnet).unwrap_err(),
            "Amount and Fee are zero"
        );
    }

    #[test]
    fn hash_output_sent() {
        let output_sent = sent();
        let mut s = SipHasher::new();
        output_sent.hash(&mut s);
        let hash = s.finish();

        assert_eq!(1376835125943705973, hash);
    }
}
