use chainflip_common::types::coin::Coin;
use std::convert::TryInto;

/// Convert an input address id to a string
pub fn input_address_id_to_string(
    coin: Coin,
    input_address_id: &[u8],
) -> Result<String, &'static str> {
    match coin {
        Coin::BTC | Coin::ETH => {
            let val: [u8; 4] = input_address_id
                .try_into()
                .map_err(|_| "Invalid input address id format")?;
            Ok(u32::from_be_bytes(val).to_string())
        }
        Coin::LOKI => Ok(hex::encode(&input_address_id)),
    }
}

/// Convert an input address id string to bytes
pub fn input_address_id_string_to_bytes(
    coin: Coin,
    input_address_id: &str,
) -> Result<Vec<u8>, &'static str> {
    match coin {
        Coin::BTC | Coin::ETH => {
            let id = input_address_id
                .parse::<u32>()
                .map_err(|_| "Invalid integer")?;
            Ok(id.to_be_bytes().to_vec())
        }
        Coin::LOKI => hex::decode(&input_address_id).map_err(|_| "Invalid hex string"),
    }
}
