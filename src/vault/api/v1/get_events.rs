use crate::{
    common::api::ResponseError,
    local_store::{ILocalStore, LocalEvent},
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Parameters for GET /v1/events request
#[derive(Debug, Deserialize, Serialize)]
pub struct EventsParams {
    number: Option<u32>,
    limit: Option<u32>,
}

impl EventsParams {
    /// Construct params from values
    pub fn new(number: u32, limit: u32) -> Self {
        EventsParams {
            number: Some(number),
            limit: Some(limit),
        }
    }
}

/// Typed representation of the response for /events
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct EventsQueryResponse {
    /// The total events
    pub total_events: u32,
    /// The current events
    pub events: Vec<LocalEvent>,
}

const DEFAULT_EVENT_NUMBER: u32 = 0;
/// Clients can request up to this number of events in one request
const MAX_EVENTS_IN_RESPONSE: u32 = 400;

/// Get the events
///
/// # Example Query
///
/// > GET /v1/events?number=0&limit=50
pub async fn get_events<L: ILocalStore>(
    params: EventsParams,
    local_store: Arc<Mutex<L>>,
) -> Result<EventsQueryResponse, ResponseError> {
    let EventsParams { number, limit } = params;

    let number = number.unwrap_or(DEFAULT_EVENT_NUMBER);
    let limit = limit.unwrap_or(MAX_EVENTS_IN_RESPONSE);

    let local_store = local_store.lock().unwrap();
    let total_events = local_store.total_events();

    if total_events == 0 || number >= total_events as u32 || limit <= 0 {
        // Return an empty response
        let res = EventsQueryResponse {
            total_events: 0,
            events: vec![],
        };
        return Ok(res);
    }

    // we would calculate the limit here and add it as a param to get_events

    let events = local_store.get_events(number as u64);

    println!("Events returned {}", events.len());
    let res = EventsQueryResponse {
        events,
        total_events: total_events as u32,
    };

    Ok(res)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{local_store::MemoryLocalStore, utils::test_utils::data::TestData};
    use chainflip_common::types::coin::Coin;

    /// Populate the chain with 2 events, request all 2
    #[tokio::test]
    async fn get_all_two_events() {
        let params = EventsParams::new(0, 2);

        let mut local_store = MemoryLocalStore::new();

        local_store
            .add_events(vec![
                TestData::witness(0, 100, Coin::ETH).into(),
                TestData::witness(1, 123, Coin::BTC).into(),
            ])
            .unwrap();

        let local_store = Arc::new(Mutex::new(local_store));

        let res = get_events(params, local_store)
            .await
            .expect("Expected success");

        assert_eq!(res.events.len(), 2);
        assert_eq!(res.total_events, 2);
    }

    #[tokio::test]
    async fn get_two_events_out_of_three() {
        let params = EventsParams::new(1, 2);

        let mut local_store = MemoryLocalStore::new();

        local_store
            .add_events(vec![
                TestData::swap_quote(Coin::ETH, Coin::LOKI).into(),
                TestData::witness(0, 100, Coin::ETH).into(),
                TestData::witness(1, 123, Coin::BTC).into(),
            ])
            .unwrap();
        let local_store = Arc::new(Mutex::new(local_store));

        let res = get_events(params, local_store)
            .await
            .expect("Expected success");
        assert_eq!(res.events.len(), 2);

        assert_eq!(res.total_events, 3);
    }

    #[tokio::test]
    async fn cap_too_big_limit() {
        let params = EventsParams::new(0, 1000);

        let mut local_store = MemoryLocalStore::new();

        local_store
            .add_events(vec![TestData::witness(0, 123, Coin::BTC).into()])
            .unwrap();

        let local_store = Arc::new(Mutex::new(local_store));

        let res = get_events(params, local_store)
            .await
            .expect("Expected success");

        assert_eq!(res.events.len(), 1);
        assert_eq!(res.total_events, 1);
    }

    #[tokio::test]
    #[ignore = "To be implemented"]
    async fn zero_limit() {
        let params = EventsParams::new(1, 0);
        let mut local_store = MemoryLocalStore::new();

        local_store
            .add_events(vec![
                TestData::witness(0, 123, Coin::BTC).into(),
                TestData::witness(1, 10, Coin::ETH).into(),
            ])
            .unwrap();

        let local_store = Arc::new(Mutex::new(local_store));

        let res = get_events(params, local_store)
            .await
            .expect("Expected success");

        assert_eq!(res.events.len(), 0);
        assert_eq!(res.total_events, 2);
    }

    #[tokio::test]
    #[ignore = "to be implemented"]
    async fn events_do_not_exist() {
        let params = EventsParams::new(100, 2);

        let mut local_store = MemoryLocalStore::new();

        local_store
            .add_events(vec![TestData::witness(0, 123, Coin::BTC).into()])
            .unwrap();

        let local_store = Arc::new(Mutex::new(local_store));

        let res = get_events(params, local_store)
            .await
            .expect("Expected success");

        assert_eq!(res.events.len(), 0);
        assert_eq!(res.total_events, 2);
    }
}
