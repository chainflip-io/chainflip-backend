// This (eventually) will be responsible for polling the "actual" state chain, and not the one that
// the centralised version used

use crate::local_store::GetEventNumber;

use super::vault_node::VaultNodeInterface;
use super::EventProcessor;
use std::sync::{Arc, Mutex};
use std::{thread, time};

/// Polls state chain for valid/confirmed events from the state chain
pub struct StateChainPoller<V, P>
where
    V: VaultNodeInterface + Send + Sync,
    P: EventProcessor + Send,
{
    api: Arc<V>,
    processor: Arc<Mutex<P>>,
    next_event_number: u64,
}

impl<V, P> StateChainPoller<V, P>
where
    V: VaultNodeInterface + Send + Sync + 'static,
    P: EventProcessor + Send + 'static,
{
    /// Create a new block poller
    pub fn new(api: Arc<V>, processor: Arc<Mutex<P>>) -> Self {
        let last_event_number = processor.lock().unwrap().get_last_processed_event_number();
        let next_event_number = if let Some(number) = last_event_number {
            number + 1
        } else {
            0
        };

        StateChainPoller {
            api,
            processor,
            next_event_number,
        }
    }

    /// Poll until we have reached the latest block.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * Any error occurs while trying to fetch blocks from the api after 3 retries.
    /// * Any error occurs while processing blocks.
    ///
    /// # Panics
    ///
    /// Panics if we detected any skipped events.
    /// This can happen if `VaultNodeInterface::get_events` returns partial data.
    pub async fn sync(&mut self) -> Result<(), String> {
        let mut error_count: u32 = 0;
        loop {
            match self.api.get_events(self.next_event_number, 50).await {
                Ok(events) => {
                    if events.is_empty() {
                        return Ok(());
                    }

                    let last_event_number: Option<u64> =
                        events.iter().map(|e| e.event_number().unwrap_or(0)).max();

                    // Validate the returned block numbers to make sure we didn't skip
                    // assumption: get_events(2, 4) will get us events 2,3,4,5
                    let expected_last_event_number =
                        self.next_event_number + (events.len() as u64) - 1;
                    if let Some(last_event_number) = last_event_number {
                        if last_event_number != expected_last_event_number {
                            error!("Expected last event number to be {} but got {}. We must've skipped an event!", last_event_number, expected_last_event_number);
                            panic!("StateChainPoller skipped events!");
                        }
                    }

                    // Pass events off to processor
                    self.processor.lock().unwrap().process_events(&events)?;

                    // Update our local value
                    if let Some(last_event_number) = last_event_number {
                        if last_event_number + 1 > self.next_event_number {
                            self.next_event_number = last_event_number + 1;
                        }
                    }

                    error_count = 0;
                }
                Err(e) => {
                    if error_count > 3 {
                        return Err(e);
                    } else {
                        error_count += 1
                    }
                }
            }
        }
    }

    /// Poll with a delay of `interval` each time.
    ///
    /// # Blocking
    ///
    /// This operation will block the thread it is called on.
    pub fn poll(&mut self, interval: time::Duration) {
        let future = async {
            loop {
                if let Err(e) = self.sync().await {
                    info!("Block Poller ran into an error while polling: {}", e);
                }

                // Wait for a while before fetching again
                thread::sleep(interval);
            }
        };
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(future);
    }
}

#[cfg(test)]
mod test {
    use chainflip_common::types::{coin::Coin, UUIDv4};

    use super::*;
    use crate::{
        local_store::LocalEvent,
        quoter::test_utils::{
            event_processor::TestEventProcessor, vault_node_api::TestVaultNodeAPI,
        },
        utils::test_utils::data::TestData,
    };

    struct TestVariables {
        poller: StateChainPoller<TestVaultNodeAPI, TestEventProcessor>,
        api: Arc<TestVaultNodeAPI>,
        processor: Arc<Mutex<TestEventProcessor>>,
    }

    fn setup() -> TestVariables {
        let api = Arc::new(TestVaultNodeAPI::new());
        let processor = Arc::new(Mutex::new(TestEventProcessor::new()));
        let poller = StateChainPoller::new(api.clone(), processor.clone());

        TestVariables {
            poller,
            api,
            processor,
        }
    }

    #[tokio::test]
    async fn test_sync_returns_when_no_blocks_returned() {
        let mut state = setup();
        assert!(state.api.get_events_return.lock().unwrap().is_empty());
        assert!(state.poller.sync().await.is_ok());
    }

    #[tokio::test]
    async fn test_sync_returns_error_if_api_failed() {
        let error = "APITestError".to_owned();
        let mut state = setup();
        state.api.set_get_events_error(Some(error.clone()));
        assert_eq!(state.poller.sync().await.unwrap_err(), error);
    }

    #[tokio::test]
    #[should_panic(expected = "StateChainPoller skipped events!")]
    async fn test_sync_panics_when_events_are_skipped() {
        let mut state = setup();
        state.api.add_events(vec![
            LocalEvent::Witness(TestData::witness(UUIDv4::new(), 100, Coin::ETH)),
            LocalEvent::Witness(TestData::witness(UUIDv4::new(), 123, Coin::BTC)),
        ]);
        state.poller.next_event_number = 1;
        state.poller.sync().await.unwrap();
    }

    #[tokio::test]
    async fn test_sync_updates_next_event_number_only_if_larger() -> Result<(), String> {
        let mut state = setup();
        state.api.add_events(vec![
            LocalEvent::Witness(TestData::witness_with_event_num(
                UUIDv4::new(),
                100,
                Coin::ETH,
                0,
            )),
            LocalEvent::Witness(TestData::witness_with_event_num(
                UUIDv4::new(),
                123,
                Coin::BTC,
                1,
            )),
        ]);

        state.poller.sync().await?;
        assert_eq!(state.poller.next_event_number, 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_loops_through_all_events() -> Result<(), String> {
        let mut state = setup();
        state.api.add_events(vec![
            LocalEvent::Witness(TestData::witness_with_event_num(
                UUIDv4::new(),
                100,
                Coin::ETH,
                0,
            )),
            LocalEvent::Witness(TestData::witness_with_event_num(
                UUIDv4::new(),
                123,
                Coin::BTC,
                1,
            )),
        ]);
        state
            .api
            .add_events(vec![LocalEvent::Witness(TestData::witness_with_event_num(
                UUIDv4::new(),
                100,
                Coin::ETH,
                2,
            ))]);

        state.poller.sync().await?;
        assert_eq!(state.poller.next_event_number, 3);
        assert_eq!(state.processor.lock().unwrap().recieved_events.len(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_passes_blocks_to_processor() -> Result<(), String> {
        let mut state = setup();
        state
            .api
            .add_events(vec![LocalEvent::Witness(TestData::witness_with_event_num(
                UUIDv4::new(),
                100,
                Coin::ETH,
                0,
            ))]);
        state.poller.sync().await?;
        assert_eq!(state.processor.lock().unwrap().recieved_events.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_returns_error_if_processor_failed() {
        let error = "ProcessorTestError".to_owned();
        let mut state = setup();

        state
            .api
            .add_events(vec![LocalEvent::Witness(TestData::witness(
                UUIDv4::new(),
                100,
                Coin::ETH,
            ))]);
        state
            .processor
            .lock()
            .unwrap()
            .set_process_events_error(Some(error.clone()));

        assert_eq!(state.poller.sync().await.unwrap_err(), error);
    }
}
