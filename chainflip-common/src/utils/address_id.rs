use crate::{
    string::{String, ToString},
    types::{addresses::OxenPaymentId, coin::Coin, Bytes},
};
use std::convert::{TryFrom, TryInto};

/// Convert a byte address id to string
pub fn to_string(coin: Coin, address_id: &[u8]) -> Result<String, &'static str> {
    match coin {
        Coin::BTC => {
            let val: [u8; 4] = address_id
                .try_into()
                .map_err(|_| "address id is not a valid integer")?;
            Ok(u32::from_be_bytes(val).to_string())
        }
        Coin::OXEN | Coin::ETH => Ok(hex::encode(&address_id)),
    }
}

/// Convert a string address id to bytes
pub fn to_bytes(coin: Coin, address_id: &str) -> Result<Bytes, &'static str> {
    match coin {
        Coin::BTC => {
            let id = address_id.parse::<u32>().map_err(|_| "Invalid integer")?;
            Ok(id.to_be_bytes().to_vec())
        }
        Coin::OXEN => OxenPaymentId::try_from(address_id.to_string())
            .map(|id| id.bytes())
            .map_err(|_| "Invalid hex string"),
        Coin::ETH => hex::decode(&address_id).map_err(|_| "Invalid hex string"),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    struct Data {
        coin: Coin,
        bytes: Bytes,
        string: &'static str,
    }

    fn data() -> Vec<Data> {
        vec![
            Data {
                coin: Coin::ETH,
                bytes: vec![
                    28, 142, 243, 136, 94, 84, 121, 2, 131, 195, 59, 126, 15, 247, 54, 134,
                ],
                string: "1c8ef3885e54790283c33b7e0ff73686",
            },
            Data {
                coin: Coin::BTC,
                bytes: 999u32.to_be_bytes().to_vec(),
                string: "999",
            },
            Data {
                coin: Coin::OXEN,
                bytes: vec![66, 15, 162, 155, 45, 154, 73, 245],
                string: "420fa29b2d9a49f5",
            },
        ]
    }

    #[test]
    fn converts_bytes_to_string() {
        for Data {
            coin,
            bytes,
            string,
        } in data()
        {
            let id = to_string(coin, &bytes);
            assert!(
                id.is_ok(),
                "Expected valid conversion of bytes to string from {}",
                coin
            );

            assert_eq!(&id.unwrap(), string);
        }
    }

    #[test]
    fn converts_string_to_bytes() {
        for Data {
            coin,
            bytes,
            string,
        } in data()
        {
            let id = to_bytes(coin, &string);
            assert!(
                id.is_ok(),
                "Expected valid conversion of string to bytes from {}",
                coin
            );

            assert_eq!(id.unwrap(), bytes);
        }
    }
}
