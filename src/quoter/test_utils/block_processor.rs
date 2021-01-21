use crate::{local_store::LocalEvent, quoter::EventProcessor};

/// Test block processor
pub struct TestEventProcessor {
    /// The last processed block
    pub last_processed_event_number: Option<u64>,
    /// The blocks received from process_blocks function
    // pub recieved_blocks: Vec<SideChainBlock>,
    /// Error to return in process_blocks function
    pub process_events_error: Option<String>,
}

impl TestEventProcessor {
    /// Create a new test block processor
    pub fn new() -> Self {
        TestEventProcessor {
            last_processed_event_number: None,
            // recieved_blocks: vec![],
            process_events_error: None,
        }
    }

    /// Set the process_blocks error
    pub fn set_process_events_error(&mut self, error: Option<String>) {
        self.process_events_error = error;
    }
}

impl EventProcessor for TestEventProcessor {
    fn get_last_processed_event_number(&self) -> Option<u64> {
        self.last_processed_event_number
    }

    fn process_events(&mut self, events: &[LocalEvent]) -> Result<(), String> {
        todo!();
    }
    // fn process_blocks(&mut self, blocks: &[SideChainBlock]) -> Result<(), String> {
    //     if let Some(error) = self.process_blocks_error.as_ref() {
    //         return Err(error.clone());
    //     }
    //     self.recieved_blocks.extend_from_slice(blocks);
    //     Ok(())
    // }
}
