use crate::{
    common::api::ResponseError, quoter::StateProvider, side_chain::SideChainTx,
    transactions::OutputTx,
};
use reqwest::StatusCode;
use serde::Deserialize;
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

/// Parameters for GET `transactions` endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsParams {
    /// The quote id
    pub quote_id: String,
}

/// Get all the transactions related to a quote
///
/// # Example Query
///
/// > GET /v1/transactions?quoteId=<quote-id>
pub async fn get_transactions<S>(
    params: TransactionsParams,
    state: Arc<Mutex<S>>,
) -> Result<Vec<SideChainTx>, ResponseError>
where
    S: StateProvider,
{
    let id = match Uuid::from_str(&params.quote_id) {
        Ok(id) => id,
        Err(_) => {
            return Err(ResponseError::new(
                StatusCode::BAD_REQUEST,
                "Invalid quote id",
            ))
        }
    };

    let state = state.lock().unwrap();

    let witnesses = state.get_witness_txs();
    let outputs = state.get_output_txs();
    let sent = state.get_output_sent_txs();
    let stakes = state.get_stake_txs();

    drop(state);

    // I know this is terribly inefficient but it'll have to do for now until we can clean it up :(

    let filtered_witnesses: Vec<SideChainTx> = witnesses
        .into_iter()
        .filter(|tx| tx.quote_id == id)
        .map(|tx| tx.into())
        .collect();

    let filtered_stake: Vec<SideChainTx> = stakes
        .into_iter()
        .filter(|tx| tx.quote_tx == id)
        .map(|tx| tx.into())
        .collect();

    let filtered_outputs: Vec<OutputTx> =
        outputs.into_iter().filter(|tx| tx.quote_tx == id).collect();
    let ids: Vec<Uuid> = filtered_outputs.iter().map(|tx| tx.id).collect();
    let filtered_outputs: Vec<SideChainTx> =
        filtered_outputs.into_iter().map(|tx| tx.into()).collect();

    let filtered_output_sent: Vec<SideChainTx> = sent
        .into_iter()
        .filter(|tx| ids.iter().find(|id| tx.output_txs.contains(id)).is_some())
        .map(|tx| tx.into())
        .collect();

    Ok([
        filtered_witnesses,
        filtered_stake,
        filtered_outputs,
        filtered_output_sent,
    ]
    .concat())
}
