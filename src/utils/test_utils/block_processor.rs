use crate::{quoter::BlockProcessor, side_chain::SideChainBlock};
use std::sync::Mutex;

pub struct TestBlockProcessor {
    pub last_processed_block_number: Option<u32>,
    pub recieved_blocks: Mutex<Vec<SideChainBlock>>,
    pub process_blocks_error: Mutex<Option<String>>,
}

impl TestBlockProcessor {
    pub fn new() -> Self {
        TestBlockProcessor {
            last_processed_block_number: None,
            recieved_blocks: Mutex::new(vec![]),
            process_blocks_error: Mutex::new(None),
        }
    }

    pub fn set_process_blocks_error(&self, error: Option<String>) {
        *self.process_blocks_error.lock().unwrap() = error;
    }
}

impl BlockProcessor for TestBlockProcessor {
    fn get_last_processed_block_number(&self) -> Option<u32> {
        return self.last_processed_block_number;
    }
    fn process_blocks(&self, blocks: Vec<SideChainBlock>) -> Result<(), String> {
        if let Some(error) = self.process_blocks_error.lock().unwrap().as_ref() {
            return Err(error.clone());
        }
        self.recieved_blocks.lock().unwrap().extend(blocks);
        return Ok(());
    }
}
