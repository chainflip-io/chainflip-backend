use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use warp::Filter;

use crate::side_chain::{ISideChain, SideChainTx};

use crate::common::coins::Coin;

use std::str::FromStr;

#[cfg(test)]
mod tests;

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
    #[serde(rename(serialize = "type"))]
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
#[derive(Debug, Serialize)]
struct QuoteQueryRequest {
    input_coin: Coin,
    input_return_address: String, // TODO
    #[serde(rename = "inputAddressID")]
    input_address_id: String,
    input_amount: String, // Amounts are strings,
    output_coin: Coin,    // TODO
    output_address: String,
    slippage_limit: f64,
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
    slippage_limit: f64,
}

#[derive(Debug, Deserialize, Serialize)]
struct ResponseError {
    code: u16,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct VaultResponseWrapper {
    success: bool,
    data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ResponseError>,
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

macro_rules! parse_field {
    ($v:ident, $field:literal) => {
        $v.get($field).ok_or(concat!("field missing: ", $field))
    };
}

macro_rules! parse_string_field {
    ($v:ident, $field:literal) => {
        parse_field!($v, $field)?
            .as_str()
            .ok_or(concat!("field must be a string: ", $field))
            .map(|x| x.to_owned())
    };
}

fn parse_quote_request(raw: serde_json::Value) -> Result<QuoteQueryRequest, &'static str> {
    let input_coin = parse_string_field!(raw, "inputCoin")?;
    let input_return_address = parse_string_field!(raw, "inputReturnAddress")?;
    let input_address_id = parse_string_field!(raw, "inputAddressID")?;
    let input_amount = parse_string_field!(raw, "inputAmount")?;
    let output_coin = parse_string_field!(raw, "outputCoin")?;
    let output_address = parse_string_field!(raw, "outputAddress")?;
    let slippage_limit = parse_field!(raw, "slippageLimit")?;
    let slippage_limit = slippage_limit
        .as_f64()
        .ok_or("field must be of type float: slippageLimit")?;

    let input_coin = Coin::from_str(&input_coin[..])?;
    let output_coin = Coin::from_str(&output_coin[..])?;

    Ok(QuoteQueryRequest {
        input_coin,
        input_return_address,
        input_address_id,
        input_amount,
        output_coin,
        output_address,
        slippage_limit,
    })
}

fn wrap_response<T>(res: Result<T, ResponseError>) -> VaultResponseWrapper
where
    T: Serialize,
{
    match res {
        Ok(res) => VaultResponseWrapper {
            success: true,
            data: serde_json::to_value(&res).expect("Could not construct json value"),
            error: None,
        },
        Err(err) => VaultResponseWrapper {
            success: true,
            data: serde_json::Value::Null,
            error: Some(ResponseError {
                code: err.code,
                message: err.message,
            }),
        },
    }
}

impl<S> APIServer<S>
where
    S: ISideChain + Send + 'static,
{
    /// Does the actual work for getting blocks from the
    /// database. (Does not actualy need to be async, but
    /// this way it will be easier to add async functions
    /// in the future).
    ///
    /// # Example Query
    /// `GET /v1/blocks?number=1&limit=50`.
    async fn get_blocks_inner(
        side_chain: Arc<Mutex<S>>,
        params: BlocksQueryParams,
    ) -> BlocksQueryResponse {
        println!("Hello! Params: {:?}", &params);

        let side_chain = side_chain.lock().unwrap();
        let total_blocks = side_chain.total_blocks();

        let BlocksQueryParams { number, limit } = params;

        if total_blocks == 0 || number >= total_blocks || limit == 0 {
            // Return an mpty response
            let res = BlocksQueryResponse {
                total_blocks,
                blocks: vec![],
            };
            return res;
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

        let res = BlocksQueryResponse {
            total_blocks,
            blocks,
        };

        res
    }

    /// Wrap get blocks response into a generic response object
    async fn get_blocks(
        side_chain: Arc<Mutex<S>>,
        params: BlocksQueryParams,
    ) -> VaultResponseWrapper {
        let res = APIServer::get_blocks_inner(side_chain, params).await;

        // /v1/blocks cannot fail
        wrap_response(Ok(res))
    }

    fn post_quote_inner(
        _side_chain: Arc<Mutex<S>>,
        params: QuoteQueryRequest,
    ) -> Result<QuoteQueryResponse, ResponseError> {
        Ok(QuoteQueryResponse {
            id: "TODO".to_owned(),
            created_at: 0,
            expires_at: 0,
            input_coin: params.input_coin.to_string(),
            input_address: "TODO".to_owned(),
            input_return_address: params.input_return_address,
            input_amount: params.input_amount,
            output_coin: params.output_coin.to_string(),
            output_address: params.output_address,
            estimated_output_amount: "TODO".to_owned(),
            slippage_limit: 0.0,
        })
    }

    async fn post_quote(
        side_chain: Arc<Mutex<S>>,
        params: serde_json::Value,
    ) -> VaultResponseWrapper {
        let params = parse_quote_request(params);

        let params = match params {
            Ok(params) => params,
            Err(err) => {
                let res_error = ResponseError {
                    code: 400,
                    message: err,
                };
                return wrap_response::<()>(Err(res_error));
            }
        };

        let res = APIServer::post_quote_inner(side_chain, params);

        wrap_response(res)
    }

    /// Top-level handler for quote (post) requests
    async fn process_quote(
        params: serde_json::Value,
        side_chain: Arc<Mutex<S>>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let server = Arc::clone(&side_chain);
        let res = APIServer::post_quote(server, params).await;
        Ok(serde_json::to_string_pretty(&res).unwrap())
    }

    /// Top-level handler for block (get) requests
    async fn process_blocks(
        params: BlocksQueryParams,
        side_chain: Arc<Mutex<S>>,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let server = Arc::clone(&side_chain);
        let res = APIServer::get_blocks(server, params).await;
        Ok(serde_json::to_string_pretty(&res).unwrap())
    }

    /// Starts an http server in the current thread and blocks
    pub fn serve(side_chain: Arc<Mutex<S>>) {
        let blocks = warp::path!("v1" / "blocks")
            .and(warp::query::<BlocksQueryParams>())
            .and(with_state(side_chain.clone()))
            .and_then(APIServer::process_blocks);

        let quotes = warp::path!("v1" / "quote")
            .and(warp::body::json())
            .and(with_state(side_chain.clone()))
            .and_then(APIServer::process_quote);

        let get = warp::get().and(blocks);
        let post = warp::post().and(quotes);

        let routes = get.or(post);

        let future = async { warp::serve(routes).run(([127, 0, 0, 1], 3030)).await };

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(future);
    }
}
