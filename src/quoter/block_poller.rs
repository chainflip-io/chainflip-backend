use super::vault_node::VaultNodeInterface;
use super::BlockProcessor;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::{thread, time};

/// A struct which can poll for blocks
pub struct BlockPoller<V, P>
where
    V: VaultNodeInterface + Send + Sync,
    P: BlockProcessor + Send,
{
    api: Arc<V>,
    processor: Arc<Mutex<P>>,
    next_block_number: AtomicU32,
}

impl<V, P> BlockPoller<V, P>
where
    V: VaultNodeInterface + Send + Sync + 'static,
    P: BlockProcessor + Send + 'static,
{
    /// Create a new block poller
    pub fn new(api: Arc<V>, processor: Arc<Mutex<P>>) -> Self {
        let last_block_number = processor.lock().unwrap().get_last_processed_block_number();
        let next_block_number = if let Some(number) = last_block_number {
            number + 1
        } else {
            0
        };

        BlockPoller {
            api,
            processor,
            next_block_number: AtomicU32::new(next_block_number),
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
    /// Panics if we detected any skipped blocks.
    /// This can happen if `VaultNodeInterface::get_blocks` returns partial data.
    pub async fn sync(&self) -> Result<(), String> {
        let mut error_count = 0;
        loop {
            let next_block_number = self.next_block_number.load(Ordering::SeqCst);
            match self.api.get_blocks(next_block_number, 50).await {
                Ok(blocks) => {
                    if blocks.is_empty() {
                        return Ok(());
                    }

                    let last_block_number = blocks.iter().map(|b| b.id).max();

                    // Validate the returned block numbers to make sure we didn't skip
                    let expected_last_block_number = next_block_number + (blocks.len() as u32) - 1; // assumption: getBlock(2, 4) will get us blocks 2,3,4,5
                    if let Some(last_block_number) = last_block_number {
                        if last_block_number != expected_last_block_number {
                            error!("Expected last block number to be {} but got {}. We must've skipped block!", last_block_number, expected_last_block_number);
                            panic!("BlockPoller skipped blocks!");
                        }
                    }

                    // Pass blocks off to processor
                    self.processor.lock().unwrap().process_blocks(&blocks)?;

                    // Update our local value
                    if let Some(last_block_number) = last_block_number {
                        if last_block_number + 1 > next_block_number {
                            self.next_block_number
                                .store(last_block_number + 1, Ordering::SeqCst);
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
    pub fn poll(self, interval: time::Duration) {
        let future = async {
            loop {
                if let Err(e) = self.sync().await {
                    info!("Block Poller ran into an error while polling: {}", e);
                }

                // Wait for a while before fetching again
                thread::sleep(interval);
            }
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(future);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        quoter::test_utils::{
            block_processor::TestBlockProcessor, vault_node_api::TestVaultNodeAPI,
        },
        side_chain::SideChainBlock,
    };

    struct TestVariables {
        poller: BlockPoller<TestVaultNodeAPI, TestBlockProcessor>,
        api: Arc<TestVaultNodeAPI>,
        processor: Arc<Mutex<TestBlockProcessor>>,
    }

    fn setup() -> TestVariables {
        let api = Arc::new(TestVaultNodeAPI::new());
        let processor = Arc::new(Mutex::new(TestBlockProcessor::new()));
        let poller = BlockPoller::new(api.clone(), processor.clone());

        TestVariables {
            poller,
            api,
            processor,
        }
    }

    #[tokio::test]
    async fn test_sync_returns_when_no_blocks_returned() {
        let state = setup();
        assert!(state.api.get_blocks_return.lock().unwrap().is_empty());
        assert!(state.poller.sync().await.is_ok());
    }

    #[tokio::test]
    async fn test_sync_returns_error_if_api_failed() {
        let error = "APITestError".to_owned();
        let state = setup();
        state.api.set_get_blocks_error(Some(error.clone()));
        assert_eq!(state.poller.sync().await.unwrap_err(), error);
    }

    #[tokio::test]
    #[should_panic(expected = "BlockPoller skipped blocks!")]
    async fn test_sync_panics_when_blocks_are_skipped() {
        let state = setup();
        state.api.add_blocks(vec![
            SideChainBlock {
                id: 1,
                transactions: vec![],
            },
            SideChainBlock {
                id: 100,
                transactions: vec![],
            },
        ]);
        state.poller.next_block_number.store(1, Ordering::SeqCst);
        state.poller.sync().await.unwrap();
    }

    #[tokio::test]
    async fn test_sync_updates_next_block_number_only_if_larger() -> Result<(), String> {
        let state = setup();
        state.api.add_blocks(vec![
            SideChainBlock {
                id: 0,
                transactions: vec![],
            },
            SideChainBlock {
                id: 1,
                transactions: vec![],
            },
        ]);

        state.poller.sync().await?;
        assert_eq!(state.poller.next_block_number.load(Ordering::SeqCst), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_loops_through_all_blocks() -> Result<(), String> {
        let state = setup();
        state.api.add_blocks(vec![
            SideChainBlock {
                id: 0,
                transactions: vec![],
            },
            SideChainBlock {
                id: 1,
                transactions: vec![],
            },
        ]);
        state.api.add_blocks(vec![SideChainBlock {
            id: 2,
            transactions: vec![],
        }]);

        state.poller.sync().await?;
        assert_eq!(state.poller.next_block_number.load(Ordering::SeqCst), 3);
        assert_eq!(state.processor.lock().unwrap().recieved_blocks.len(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_passes_blocks_to_processor() -> Result<(), String> {
        let state = setup();
        state.api.add_blocks(vec![SideChainBlock {
            id: 0,
            transactions: vec![],
        }]);
        state.poller.sync().await?;
        assert_eq!(state.processor.lock().unwrap().recieved_blocks.len(), 1);
        assert_eq!(
            state
                .processor
                .lock()
                .unwrap()
                .recieved_blocks
                .get(0)
                .unwrap()
                .id,
            0
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_sync_returns_error_if_processor_failed() {
        let error = "ProcessorTestError".to_owned();
        let state = setup();

        state.api.add_blocks(vec![SideChainBlock {
            id: 0,
            transactions: vec![],
        }]);
        state
            .processor
            .lock()
            .unwrap()
            .set_process_blocks_error(Some(error.clone()));

        assert_eq!(state.poller.sync().await.unwrap_err(), error);
    }
}
