use siphasher::sip::SipHasher;

use std::hash::{Hash, Hasher};

use super::{UniqueId, Validate};
use crate::{
    string::*,
    types::{coin::Coin, unique_id::GetUniqueId, utf8::ByteString, Bytes, Network, Timestamp},
    validation::{validate_address, validate_address_id, validate_staker_id},
};
use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositQuote {
    /// Creation timestamp
    pub timestamp: Timestamp,
    /// The staker public key
    pub staker_id: Bytes,
    /// The pool coin which we want to deposit.
    ///
    /// If you wanted to deposit to the ETH/BASE pool then `coin` would be ETH.
    pub pool: Coin,
    /// The address in which the user will deposit coins
    pub coin_input_address: ByteString,
    /// The information used to derive `coin_input_address`
    pub coin_input_address_id: Bytes,
    /// The address to refund coins in the case of a failed deposit
    pub coin_return_address: ByteString,
    /// The address in which the user will deposit base coins
    pub base_input_address: ByteString,
    /// The information used to derive `base_input_address`
    pub base_input_address_id: Bytes,
    /// The address to refund base coins in the case of a failed deposit
    pub base_return_address: ByteString,
    /// event number
    pub event_number: Option<u64>,
}

impl Validate for DepositQuote {
    type Error = &'static str;

    fn validate(&self, network: Network) -> Result<(), Self::Error> {
        if validate_staker_id(&self.staker_id).is_err() {
            return Err("Invalid staker id");
        }

        if self.pool == Coin::BASE_COIN {
            return Err("Invalid pool coin");
        }

        validate_address(self.pool, network, &self.coin_input_address.to_string())
            .map_err(|_| "Invalid coin input address")?;
        validate_address_id(self.pool, &self.coin_input_address_id)
            .map_err(|_| "Invalid coin input address id")?;
        validate_address(self.pool, network, &self.coin_return_address.to_string())
            .map_err(|_| "Invalid coin return address")?;

        validate_address(
            Coin::BASE_COIN,
            network,
            &self.base_input_address.to_string(),
        )
        .map_err(|_| "Invalid base input address")?;
        validate_address_id(Coin::BASE_COIN, &self.base_input_address_id)
            .map_err(|_| "Invalid base input address id")?;
        validate_address(
            Coin::BASE_COIN,
            network,
            &self.base_return_address.to_string(),
        )
        .map_err(|_| "Invalid base return address")?;

        Ok(())
    }
}

// Used as a key in the KV store of CFE
impl Hash for DepositQuote {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pool.hash(state);
        self.staker_id.hash(state);
        self.coin_input_address.hash(state);
    }
}

impl GetUniqueId for DepositQuote {
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
    use crate::test::constants::{ETH_ADDRESS, OXEN_ADDRESS, OXEN_PAYMENT_ID, STAKER_ID};

    fn quote() -> DepositQuote {
        DepositQuote {
            timestamp: Timestamp(0),
            staker_id: STAKER_ID.to_vec(),
            pool: Coin::ETH,
            coin_input_address: ETH_ADDRESS.into(),
            coin_input_address_id: vec![0; 32],
            coin_return_address: ETH_ADDRESS.into(),
            base_input_address: OXEN_ADDRESS.into(),
            base_input_address_id: OXEN_PAYMENT_ID.bytes(),
            base_return_address: OXEN_ADDRESS.into(),
            event_number: None,
        }
    }

    #[test]
    fn validates() {
        let deposit = quote();
        assert!(deposit.validate(Network::Mainnet).is_ok());

        let mut deposit = quote();
        deposit.staker_id = vec![];
        assert_eq!(
            deposit.validate(Network::Mainnet).unwrap_err(),
            "Invalid staker id"
        );

        let mut deposit = quote();
        deposit.pool = Coin::BASE_COIN;
        assert_eq!(
            deposit.validate(Network::Mainnet).unwrap_err(),
            "Invalid pool coin"
        );

        let mut deposit = quote();
        deposit.coin_input_address = "invalid".into();
        assert_eq!(
            deposit.validate(Network::Mainnet).unwrap_err(),
            "Invalid coin input address"
        );

        let mut deposit = quote();
        deposit.coin_input_address_id = b"invalid".to_vec();
        assert_eq!(
            deposit.validate(Network::Mainnet).unwrap_err(),
            "Invalid coin input address id"
        );

        let mut deposit = quote();
        deposit.coin_return_address = "invalid".into();
        assert_eq!(
            deposit.validate(Network::Mainnet).unwrap_err(),
            "Invalid coin return address"
        );

        let mut deposit = quote();
        deposit.base_input_address = "invalid".into();
        assert_eq!(
            deposit.validate(Network::Mainnet).unwrap_err(),
            "Invalid base input address"
        );

        let mut deposit = quote();
        deposit.base_input_address_id = b"invalid".to_vec();
        assert_eq!(
            deposit.validate(Network::Mainnet).unwrap_err(),
            "Invalid base input address id"
        );

        let mut deposit = quote();
        deposit.base_return_address = "invalid".into();
        assert_eq!(
            deposit.validate(Network::Mainnet).unwrap_err(),
            "Invalid base return address"
        );
    }

    #[test]
    fn hash_deposit_quote() {
        let deposit_quote = quote();
        let mut s = SipHasher::new();
        deposit_quote.hash(&mut s);
        let hash = s.finish();

        assert_eq!(2357315162032783535, hash);
    }
}
