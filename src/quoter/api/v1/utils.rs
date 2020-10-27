use crate::common::{api::ResponseError, coins::Coin};
use rand::Rng;
use reqwest::StatusCode;
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
};

pub fn generate_unique_input_address_id<R: Rng>(
    input_coin: Coin,
    input_id_cache: Arc<Mutex<HashMap<Coin, BTreeSet<String>>>>,
    rng: &mut R,
) -> Result<String, ResponseError> {
    let mut cache = input_id_cache.lock().unwrap();
    let used_ids = cache.entry(input_coin).or_insert(BTreeSet::new());

    // We can test this by passing a SeededRng
    let input_address_id = loop {
        let id = match input_coin {
            Coin::BTC => rng.gen_range(6, u64::MAX).to_string(),
            Coin::ETH => rng.gen_range(6, u64::MAX).to_string(),
            Coin::LOKI => {
                let random_bytes = rng.gen::<[u8; 8]>();
                hex::encode(random_bytes)
            }
            _ => {
                return Err(ResponseError::new(
                    StatusCode::BAD_REQUEST,
                    "Invalid input id",
                ))
            }
        };

        if !used_ids.contains(&id) {
            break id;
        }
    };

    // Add the id in the cache
    used_ids.insert(input_address_id.clone());

    Ok(input_address_id)
}
