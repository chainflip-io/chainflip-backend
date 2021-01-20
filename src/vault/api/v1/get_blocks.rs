use crate::{
    common::api::ResponseError,
    local_store::{ISideChain, SideChainBlock},
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Parameters for GET /v1/blocks request
#[derive(Debug, Deserialize, Serialize)]
pub struct BlocksQueryParams {
    number: Option<u32>,
    limit: Option<u32>,
}

impl BlocksQueryParams {
    /// Construct params from values
    pub fn new(number: u32, limit: u32) -> Self {
        BlocksQueryParams {
            number: Some(number),
            limit: Some(limit),
        }
    }
}

/// Typed representation of the response for /blocks
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct BlocksQueryResponse {
    /// The total blocks
    pub total_blocks: u32,
    /// The current blocks
    pub blocks: Vec<SideChainBlock>,
}

const DEFAULT_BLOCK_NUMBER: u32 = 0;
/// Clients can request up to this number of blocks in one request
const MAX_BLOCKS_IN_RESPONSE: u32 = 50;

/// Get the side chain blocks
///
/// # Example Query
///
/// > GET /v1/blocks?number=0&limit=50
pub async fn get_blocks<S: ISideChain>(
    params: BlocksQueryParams,
    side_chain: Arc<Mutex<S>>,
) -> Result<BlocksQueryResponse, ResponseError> {
    let BlocksQueryParams { number, limit } = params;

    let number = number.unwrap_or(DEFAULT_BLOCK_NUMBER);
    let limit = limit.unwrap_or(MAX_BLOCKS_IN_RESPONSE);

    let side_chain = side_chain.lock().unwrap();
    let total_blocks = side_chain.total_blocks();

    if total_blocks == 0 || number >= total_blocks || limit <= 0 {
        // Return an empty response
        let res = BlocksQueryResponse {
            total_blocks,
            blocks: vec![],
        };
        return Ok(res);
    }

    let limit = std::cmp::min(limit, MAX_BLOCKS_IN_RESPONSE);

    let last_valid_idx = total_blocks.saturating_sub(1);

    let last_requested_idx = number.saturating_add(limit).saturating_sub(1);

    let last_idx = std::cmp::min(last_valid_idx, last_requested_idx);

    let mut blocks = Vec::with_capacity(limit as usize);

    // TODO: optimise this for a range of blocks?
    for idx in number..=last_idx {
        // We already checked the boundaries, so just asserting here:
        let block = side_chain.get_block(idx).expect("Failed to get block");
        blocks.push(block.clone());
    }

    let res = BlocksQueryResponse {
        total_blocks,
        blocks,
    };

    Ok(res)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{local_store::MemorySideChain, utils::test_utils::data::TestData};
    use chainflip_common::types::coin::Coin;

    /// Populate the chain with 2 blocks, request all 2
    #[tokio::test]
    async fn get_all_two_blocks() {
        let params = BlocksQueryParams::new(0, 2);

        let mut side_chain = MemorySideChain::new();

        side_chain.add_block(vec![]).unwrap();
        side_chain.add_block(vec![]).unwrap();

        let side_chain = Arc::new(Mutex::new(side_chain));

        let res = get_blocks(params, side_chain)
            .await
            .expect("Expected success");

        assert_eq!(res.blocks.len(), 2);
        assert_eq!(res.total_blocks, 2);
    }

    #[tokio::test]
    async fn get_two_blocks_out_of_three() {
        let params = BlocksQueryParams::new(0, 2);

        let mut side_chain = MemorySideChain::new();

        side_chain.add_block(vec![]).unwrap();

        let tx = TestData::swap_quote(Coin::ETH, Coin::LOKI);

        side_chain.add_block(vec![tx.clone().into()]).unwrap();
        side_chain.add_block(vec![]).unwrap();

        let side_chain = Arc::new(Mutex::new(side_chain));

        let res = get_blocks(params, side_chain)
            .await
            .expect("Expected success");

        assert_eq!(res.blocks.len(), 2);
        assert_eq!(res.blocks[1].transactions.len(), 1);
        assert_eq!(res.total_blocks, 3);
    }

    #[tokio::test]
    async fn cap_too_big_limit() {
        let params = BlocksQueryParams::new(1, 100);

        let mut side_chain = MemorySideChain::new();

        side_chain.add_block(vec![]).unwrap();
        side_chain.add_block(vec![]).unwrap();

        let side_chain = Arc::new(Mutex::new(side_chain));

        let res = get_blocks(params, side_chain)
            .await
            .expect("Expected success");

        assert_eq!(res.blocks.len(), 1);
        assert_eq!(res.total_blocks, 2);
    }

    #[tokio::test]
    async fn zero_limit() {
        let params = BlocksQueryParams::new(1, 0);
        let mut side_chain = MemorySideChain::new();

        side_chain.add_block(vec![]).unwrap();
        side_chain.add_block(vec![]).unwrap();

        let side_chain = Arc::new(Mutex::new(side_chain));

        let res = get_blocks(params, side_chain)
            .await
            .expect("Expected success");

        assert_eq!(res.blocks.len(), 0);
        assert_eq!(res.total_blocks, 2);
    }

    #[tokio::test]
    async fn blocks_do_not_exist() {
        let params = BlocksQueryParams::new(100, 2);

        let mut side_chain = MemorySideChain::new();

        side_chain.add_block(vec![]).unwrap();
        side_chain.add_block(vec![]).unwrap();

        let side_chain = Arc::new(Mutex::new(side_chain));

        let res = get_blocks(params, side_chain)
            .await
            .expect("Expected success");

        assert_eq!(res.blocks.len(), 0);
        assert_eq!(res.total_blocks, 2);
    }
}
