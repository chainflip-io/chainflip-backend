use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use warp::Filter;

use crate::side_chain::{ISideChain, SideChainTx};

#[derive(Debug, Deserialize, Serialize)]
struct BlocksQueryParams {
    number: u32,
    limit: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct TransactionInfo {}

#[derive(Debug, Deserialize, Serialize)]
struct TransactionResponseEntry {
    id: u32,
    timestamp: u64, // TODO: milliseconds since epoch (the actual values should be within the safe range for javascript)
    tx_type: String, // NOTE: can we use enum here?
    info: TransactionInfo,
}

impl From<SideChainTx> for TransactionResponseEntry {
    fn from(_tx: SideChainTx) -> Self {
        TransactionResponseEntry {
            id: 0,
            timestamp: 0,
            tx_type: "TODO".to_owned(),
            info: TransactionInfo {},
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct BlockResponseEntry {
    id: u32,
    transactions: Vec<TransactionResponseEntry>,
}

/// Typed representation of the response for /blocks
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub struct BlocksQueryResponse {
    total_blocks: u32,
    blocks: Vec<BlockResponseEntry>,
}

#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
struct QuoteQueryRequest {
    input_coin: String,           // TODO
    input_return_address: String, // TODO
    input_address_id: String,
    input_amount: String, // Amounts are strings,
    output_coin: String,  // TODO
    output_address: String,
    slippage_limit: f32,
}

#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
struct QuoteQueryResponse {
    id: String,      // unique id
    created_at: u64, // milliseconds from epoch
    expires_at: u64, // milliseconds from epoch
    input_coin: String,
    input_address: String,        // Generated on the server,
    input_return_address: String, // User specified address,
    input_amount: String,
    output_coin: String,
    output_address: String,
    estimated_output_amount: String, // Generated on the server. Quoted amount.
    slippage_limit: f32,
}

/// Clients can request up to this number of blocks in one request
const MAX_BLOCKS_IN_RESPONSE: u32 = 50;

/// Unused
pub struct APIServer<S>
where
    S: ISideChain + Send + 'static,
{
    /// Unused
    _side_chain: Arc<Mutex<S>>,
}

fn with_state<S>(
    side_chain: Arc<Mutex<S>>,
) -> impl Filter<Extract = (Arc<Mutex<S>>,), Error = std::convert::Infallible> + Clone
where
    S: ISideChain + Send + 'static,
{
    warp::any().map(move || side_chain.clone())
}

impl<S> APIServer<S>
where
    S: ISideChain + Send + 'static,
{
    /// GET /v1/blocks?number=1&limit=50
    fn get_blocks(side_chain: Arc<Mutex<S>>, params: BlocksQueryParams) -> BlocksQueryResponse {
        println!("Hello! Params: {:?}", &params);

        let side_chain = side_chain.lock().unwrap();
        let total_blocks = side_chain.total_blocks();

        let BlocksQueryParams { number, limit } = params;

        if total_blocks == 0 || number >= total_blocks || limit == 0 {
            // Return an mpty response
            return BlocksQueryResponse {
                total_blocks,
                blocks: vec![],
            };
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

            let transactions = block
                .txs
                .iter()
                .map(|tx| tx.clone().into())
                .collect::<Vec<_>>();

            let block = BlockResponseEntry {
                id: block.id,
                transactions,
            };
            blocks.push(block);
        }

        BlocksQueryResponse {
            total_blocks,
            blocks,
        }
    }

    // serde_json::to_string(&res).unwrap()

    fn post_quote(_side_chain: Arc<Mutex<S>>, _params: QuoteQueryRequest) -> QuoteQueryResponse {
        todo!();
    }

    /// Starts an http server in the current thread and blocks
    pub fn serve(side_chain: Arc<Mutex<S>>) {
        let blocks = warp::path!("v1" / "blocks")
            .and(warp::query::<BlocksQueryParams>())
            .and(with_state(side_chain.clone()))
            .map(|params, side_chain| {
                let server = Arc::clone(&side_chain);
                APIServer::get_blocks(server, params)
            })
            .map(|res| serde_json::to_string(&res).unwrap());

        let quotes = warp::path!("v1" / "quote")
            .and(warp::body::json())
            .and(with_state(side_chain.clone()))
            .map(|params, side_chain| {
                let server = Arc::clone(&side_chain);
                APIServer::post_quote(server, params)
            })
            .map(|res| serde_json::to_string(&res).unwrap());

        let get = warp::get().and(blocks);
        let post = warp::post().and(quotes);

        let routes = get.or(post);

        let future = async { warp::serve(routes).run(([127, 0, 0, 1], 3030)).await };

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(future);
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use crate::side_chain::FakeSideChain;

    #[test]
    /// Populate the chain with 2 blocks, request all 2
    fn get_all_two_blocks() {
        let params = BlocksQueryParams {
            number: 0,
            limit: 2,
        };

        let mut side_chain = FakeSideChain::new();

        side_chain.add_block(vec![]).unwrap();
        side_chain.add_block(vec![]).unwrap();

        let side_chain = Arc::new(Mutex::new(side_chain));

        let res = APIServer::get_blocks(side_chain, params);

        assert_eq!(res.blocks.len(), 2);
        assert_eq!(res.total_blocks, 2);
    }

    #[test]
    fn get_two_blocks_out_of_three() {
        use crate::utils::test_utils;

        let params = BlocksQueryParams {
            number: 0,
            limit: 2,
        };

        let mut side_chain = FakeSideChain::new();

        side_chain.add_block(vec![]).unwrap();

        let tx = test_utils::create_fake_quote_tx();

        side_chain.add_block(vec![tx.clone().into()]).unwrap();
        side_chain.add_block(vec![]).unwrap();

        let side_chain = Arc::new(Mutex::new(side_chain));

        let res = APIServer::get_blocks(side_chain, params);

        assert_eq!(res.blocks.len(), 2);
        assert_eq!(res.blocks[1].transactions.len(), 1);
        assert_eq!(res.total_blocks, 3);
    }

    #[test]
    fn cap_too_big_limit() {
        let params = BlocksQueryParams {
            number: 1,
            limit: 100,
        };

        let mut side_chain = FakeSideChain::new();

        side_chain.add_block(vec![]).unwrap();
        side_chain.add_block(vec![]).unwrap();

        let side_chain = Arc::new(Mutex::new(side_chain));

        let res = APIServer::get_blocks(side_chain, params);

        assert_eq!(res.blocks.len(), 1);
        assert_eq!(res.total_blocks, 2);
    }

    #[test]
    fn zero_limit() {
        let params = BlocksQueryParams {
            number: 1,
            limit: 0,
        };

        let mut side_chain = FakeSideChain::new();

        side_chain.add_block(vec![]).unwrap();
        side_chain.add_block(vec![]).unwrap();

        let side_chain = Arc::new(Mutex::new(side_chain));

        let res = APIServer::get_blocks(side_chain, params);

        assert_eq!(res.blocks.len(), 0);
        assert_eq!(res.total_blocks, 2);
    }

    #[test]
    fn blocks_do_not_exist() {
        let params = BlocksQueryParams {
            number: 100,
            limit: 2,
        };

        let mut side_chain = FakeSideChain::new();

        side_chain.add_block(vec![]).unwrap();
        side_chain.add_block(vec![]).unwrap();

        let side_chain = Arc::new(Mutex::new(side_chain));

        let res = APIServer::get_blocks(side_chain, params);

        assert_eq!(res.blocks.len(), 0);
        assert_eq!(res.total_blocks, 2);
    }

    #[ignore]
    #[test]
    fn post_quote() {
        let params = QuoteQueryRequest {
            input_coin: String::from("LOKI"),
            input_return_address: String::from("Some address"),
            input_address_id: "0".to_owned(),
            input_amount: String::from("100000"),
            output_coin: String::from("BTC"),
            output_address: String::from("Some other Address"),
            slippage_limit: 1.0,
        };

        let side_chain = FakeSideChain::new();
        let side_chain = Arc::new(Mutex::new(side_chain));

        let _res = APIServer::post_quote(side_chain, params);

        todo!();
    }
}
