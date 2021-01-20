use crate::{quoter::BlockProcessor, side_chain::SideChainBlock};

/// Test block processor
pub struct TestBlockProcessor {
    /// The last processed block
    pub last_processed_block_number: Option<u32>,
    /// The blocks received from process_blocks function
    pub recieved_blocks: Vec<SideChainBlock>,
    /// Error to return in process_blocks function
    pub process_blocks_error: Option<String>,
}

impl TestBlockProcessor {
    /// Create a new test block processor
    pub fn new() -> Self {
        TestBlockProcessor {
            last_processed_block_number: None,
            recieved_blocks: vec![],
            process_blocks_error: None,
        }
    }

    /// Set the process_blocks error
    pub fn set_process_blocks_error(&mut self, error: Option<String>) {
        self.process_blocks_error = error;
    }
}

impl BlockProcessor for TestBlockProcessor {
    fn get_last_processed_block_number(&self) -> Option<u32> {
        self.last_processed_block_number
    }
    fn process_blocks(&mut self, blocks: &[SideChainBlock]) -> Result<(), String> {
        if let Some(error) = self.process_blocks_error.as_ref() {
            return Err(error.clone());
        }
        self.recieved_blocks.extend_from_slice(blocks);
        Ok(())
    }
}
