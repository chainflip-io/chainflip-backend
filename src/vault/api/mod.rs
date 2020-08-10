use serde::{Deserialize, Serialize};
use warp::Filter;

pub struct APIServer {}

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
    timestamp: u32,  // TODO: are we using seconds since epoch?
    tx_type: String, // NOTE: can we use enum here?
    info: TransactionInfo,
}

#[derive(Debug, Deserialize, Serialize)]
struct BlockResponseEntry {
    number: u32,
    timestamp: u32, // TODO: are we using seconds since epoch?
    transactions: Vec<TransactionResponseEntry>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Serialize)]
struct BlocksQueryResponse {
    totalBlocks: u32,
    blockNumber: u32,
    blockLimit: u32,
    blocks: Vec<BlockResponseEntry>,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Serialize)]
struct QuoteQueryReuqest {
    inputCoin: String,          // TODO
    inputReturnAddress: String, // TODO
    inputAddressID: u32,
    inputAmount: String, // Amounts are strings,
    outputCoin: String,  // TODO
    outputAddress: String,
    slippageLimit: u32,
}

impl APIServer {
    fn get_blocks(params: BlocksQueryParams) -> String {
        format!("Hello! Params: {:?}", &params)
    }

    fn post_quote(params: QuoteQueryReuqest) -> String {
        format!("TODO:/get_quote. Params: {:?}", params)
    }

    pub fn serve() {
        let blocks = warp::path!("v1" / "blocks")
            .and(warp::query::<BlocksQueryParams>())
            .map(APIServer::get_blocks);
        let quotes = warp::path!("v1" / "quote")
            .and(warp::body::json())
            .map(APIServer::post_quote);

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

    #[test]
    fn get_blocks() {
        let params = BlocksQueryParams {
            number: 0,
            limit: 50,
        };

        let _res = APIServer::get_blocks(params);

        // TODO
    }

    #[test]
    fn post_quote() {
        // inputCoin: String, // TODO
        // inputReturnAddress: String, // TODO
        // inputAddressID: u32,
        // inputAmount: String, // Amounts are strings,
        // outputCoin: String, // TODO
        // outputAddress: String,
        // slippageLimit: u32,

        let params = QuoteQueryReuqest {
            inputCoin: String::from("LOKI"),
            inputReturnAddress: String::from("Some address"),
            inputAddressID: 0,
            inputAmount: String::from("100000"),
            outputCoin: String::from("BTC"),
            outputAddress: String::from("Some other Address"),
            slippageLimit: 1,
        };

        let _res = APIServer::post_quote(params);

        // TODO
    }
}
